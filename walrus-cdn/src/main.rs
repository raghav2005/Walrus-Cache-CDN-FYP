mod cache;
mod config;
mod eval;
mod indexer;
mod metrics;
mod sharding;
mod walrus;

use anyhow::Result;
use clap::{ArgAction, ArgGroup, Parser, Subcommand};
use config::{AppConfig, IndexerMode};
use dotenvy::dotenv;
use futures::StreamExt;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tracing::info;
use tracing_subscriber::EnvFilter;
use walrus::WalrusClient;

#[derive(Parser)]
#[command(name = "walrus-cdn")]
#[command(about = "Walrus CDN prototype: indexer + HTTP retrieval")]
struct Cli {
    #[command(subcommand)]
    cmd: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Print events from chosen source (simulated or sui)
    ///
    /// By default this exits after `--limit` events. Use `--follow` to keep tailing.
    Index {
        /// no. of events to pull before exiting (simulated ignores this)
        #[arg(long, default_value_t = 2)]
        limit: usize,
        /// Keep running and print an updated feed until Ctrl+C
        #[arg(long, default_value_t = false)]
        follow: bool,
        /// Demo-friendly compact view: print only key lifecycle events in one line each
        #[arg(long, default_value_t = false)]
        demo_view: bool,
    },
    /// Download a blob by its Walrus blob ID
    Get {
        #[arg(long)]
        id: String,
        #[arg(long)]
        out: Option<String>,
    },
    /// Download a blob by Sui object ID
    GetByObject {
        #[arg(long)]
        object_id: String,
        #[arg(long)]
        out: Option<String>,
    },
    /// Upload bytes via a publisher (local or public); prints blobId
    Put {
        #[arg(long)]
        data: Option<String>,
        #[arg(long)]
        file: Option<String>,
        #[arg(long)]
        epochs: Option<u64>,
        #[arg(long, default_value_t = false)]
        permanent: bool,
    },
    /// Simple load generator: replay blob IDs from a file
    Bench {
        #[arg(long)]
        file: String,
        #[arg(long, default_value_t = 1000)]
        requests: usize,
        #[arg(long, default_value_t = 16)]
        conc: usize,
    },
    /// Force cache invalidation for demos (by blob id, tag, or epoch)
    #[command(group(
        ArgGroup::new("target")
            .required(true)
            .multiple(false)
            .args(["id", "tag", "epoch_leq"])
    ))]
    CachePurge {
        /// Purge one blob from cache
        #[arg(long)]
        id: Option<String>,
        /// Purge all blobs with this tag (e.g. module:walrus)
        #[arg(long)]
        tag: Option<String>,
        /// Purge all blobs whose end_epoch <= this value
        #[arg(long = "epoch-leq")]
        epoch_leq: Option<u64>,
    },
    /// Set/overwrite cached blob end-epoch metadata for deterministic epoch-purge demos
    CacheSetEpoch {
        #[arg(long)]
        id: String,
        #[arg(long = "end-epoch")]
        end_epoch: u64,
        #[arg(long, default_value_t = true, action = ArgAction::Set)]
        deletable: bool,
    },
    /// Run deterministic cache demo in one process so metrics remain consistent
    DemoFlow {
        #[arg(long, default_value = "prof-demo")]
        data: String,
        #[arg(long, default_value = "/tmp/prof-demo.bin")]
        out: String,
        /// Run without pressing Enter between steps
        #[arg(long, default_value_t = false)]
        auto: bool,
        /// Seconds to wait between steps in --auto mode
        #[arg(long, default_value_t = 6)]
        step_sleep_secs: u64,
    },
    /// Run dissertation-grade synthetic evaluation suite and export CSV/JSON artifacts
    Evaluate {
        /// Output directory for summary.csv, warmup_curve.csv, evaluation.json, README.md
        #[arg(long, default_value = "./evaluation_output")]
        out_dir: String,
        /// Reproducibility seed
        #[arg(long, default_value_t = 42)]
        seed: u64,
        /// Scale factor (1=default, 2=2x workload, ...)
        #[arg(long, default_value_t = 1)]
        scale: usize,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with_target(false)
        .compact()
        .init();

    let cfg = AppConfig::from_env();

    // Start /metrics
    let maddr = cfg.metrics_addr.clone();
    tokio::spawn(async move {
        if let Err(e) = metrics::serve(&maddr).await {
            eprintln!("metrics server error: {e}");
        }
    });

    // Build one shared cache (optional)
    let cache_opt: Option<Arc<Mutex<cache::RocksCache>>> = if cfg.cache_enabled {
        let admission = cfg.cache_admission;
        let adaptsize_k = if cfg.cache_adaptsize {
            Some(128_000.0)
        } else {
            None
        };
        Some(Arc::new(Mutex::new(cache::RocksCache::open(
            &cfg.cache_db_path,
            Some(cfg.cache_block_cache_mb * 1024 * 1024),
            admission,
            adaptsize_k,
        )?)))
    } else {
        None
    };

    // expose to indexer for purge/update side-effects
    if let Some(c) = &cache_opt {
        let _ = indexer::CACHE_HANDLE.set(c.clone());
    }

    let cli = Cli::parse();

    match cli.cmd {
        Commands::Index {
            limit,
            follow,
            demo_view,
        } => run_index(cfg, limit, follow, demo_view).await?,
        Commands::Get { id, out } => {
            let client = {
                let base = WalrusClient::new(&cfg.aggregator_base, cfg.publisher_base.as_deref())?;
                if let Some(c) = &cache_opt {
                    base.with_cache(c.clone())
                } else {
                    base
                }
            };
            if let Some(n) = sharding::pick_node(&id, &cfg.cache_nodes) {
                tracing::debug!("(shard) {} -> {}", id, n);
            }
            let bytes = client.get_blob_by_id(&id).await?;
            write_output(&bytes, out.as_deref())?;
        }
        Commands::GetByObject { object_id, out } => {
            let client = WalrusClient::new(&cfg.aggregator_base, cfg.publisher_base.as_deref())?;
            let bytes = client.get_blob_by_object_id(&object_id).await?;
            write_output(&bytes, out.as_deref())?;
        }
        Commands::Put {
            data,
            file,
            epochs,
            permanent,
        } => {
            let client = {
                let base = WalrusClient::new(&cfg.aggregator_base, cfg.publisher_base.as_deref())?;
                if let Some(c) = &cache_opt {
                    base.with_cache(c.clone())
                } else {
                    base
                }
            };
            let bytes = if let Some(path) = file {
                std::fs::read(path)?
            } else {
                data.unwrap_or_else(|| "hello walrus".to_string())
                    .into_bytes()
            };
            let blob_id = client.put_blob(bytes, epochs, Some(permanent)).await?;
            println!("{blob_id}");
        }
        Commands::Bench {
            file,
            requests,
            conc,
        } => {
            use rand::{SeedableRng, rngs::StdRng, seq::SliceRandom};
            let ids: Vec<String> = std::fs::read_to_string(&file)?
                .lines()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            anyhow::ensure!(!ids.is_empty(), "no blob IDs in {}", file);
            let client = {
                let base = WalrusClient::new(&cfg.aggregator_base, cfg.publisher_base.as_deref())?;
                if let Some(c) = &cache_opt {
                    base.with_cache(c.clone())
                } else {
                    base
                }
            };
            let client = Arc::new(client);
            let mut rng = StdRng::seed_from_u64(0xC0FFEE);
            let chosen: Vec<String> = (0..requests)
                .map(|_| ids.choose(&mut rng).unwrap().clone())
                .collect();
            let start = Instant::now();
            let mut tasks = Vec::new();
            for chunk in chosen.chunks((requests + conc - 1) / conc) {
                let client = client.clone();
                let chunk = chunk.to_owned();
                tasks.push(tokio::spawn(async move {
                    for id in chunk {
                        let _ = client.get_blob_by_id(&id).await;
                    }
                }));
            }
            for t in tasks {
                let _ = t.await;
            }
            let elapsed = start.elapsed().as_millis();
            println!("bench_done reqs={} elapsed_ms={}", requests, elapsed);
        }
        Commands::CachePurge { id, tag, epoch_leq } => {
            let cache = cache_opt
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("cache is disabled (set CACHE_ENABLED=true)"))?;
            let mut cache = cache
                .lock()
                .map_err(|_| anyhow::anyhow!("cache lock poisoned"))?;

            if let Some(blob_id) = id {
                cache.purge_by_blob_id(&blob_id)?;
                println!("cache_purge blob_id={blob_id}");
            } else if let Some(tag_name) = tag {
                cache.purge_by_tag(&tag_name)?;
                println!("cache_purge tag={tag_name}");
            } else if let Some(epoch) = epoch_leq {
                cache.purge_epoch_leq(epoch)?;
                println!("cache_purge epoch_leq={epoch}");
            }
        }
        Commands::CacheSetEpoch {
            id,
            end_epoch,
            deletable,
        } => {
            let cache = cache_opt
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("cache is disabled (set CACHE_ENABLED=true)"))?;
            let cache = cache
                .lock()
                .map_err(|_| anyhow::anyhow!("cache lock poisoned"))?;
            cache.update_meta_epoch(&id, end_epoch, Some(deletable), None)?;
            println!("cache_set_epoch blob_id={id} end_epoch={end_epoch} deletable={deletable}");
        }
        Commands::DemoFlow {
            data,
            out,
            auto,
            step_sleep_secs,
        } => {
            run_demo_flow(&cfg, cache_opt.clone(), data, out, !auto, step_sleep_secs).await?;
        }
        Commands::Evaluate {
            out_dir,
            seed,
            scale,
        } => {
            let output = eval::run_evaluation_suite(scale, seed);
            eval::write_evaluation_artifacts(std::path::Path::new(&out_dir), &output, scale, seed)?;
            println!(
                "evaluation_done out_dir={} rows={} warmup_rows={} seed={} scale={}",
                out_dir,
                output.rows.len(),
                output.warmup.len(),
                seed,
                scale
            );
        }
    }
    Ok(())
}

fn write_output(bytes: &[u8], out: Option<&str>) -> Result<()> {
    use std::io::Write;
    match out {
        Some(path) => {
            std::fs::write(path, bytes)?;
            println!("Wrote {} bytes to {}", bytes.len(), path);
        }
        None => {
            let mut stdout = std::io::stdout().lock();
            stdout.write_all(bytes)?;
        }
    }
    Ok(())
}

fn event_kind_compact(kind: &indexer::EventKind) -> String {
    use indexer::EventKind::*;
    match kind {
        BlobRegistered => "BlobRegistered".to_string(),
        BlobCertified {
            end_epoch,
            deletable,
        } => format!("BlobCertified end_epoch={end_epoch} deletable={deletable}"),
        BlobDeleted => "BlobDeleted".to_string(),
        InvalidBlobId => "InvalidBlobId".to_string(),
        EpochChangeStart { epoch } => format!("EpochChangeStart epoch={epoch}"),
        EpochChangeDone { epoch } => format!("EpochChangeDone epoch={epoch}"),
        ShardsReceived => "ShardsReceived".to_string(),
        EpochParametersSelected => "EpochParametersSelected".to_string(),
        ShardRecoveryStart => "ShardRecoveryStart".to_string(),
        Extended {
            old_end_epoch,
            new_end_epoch,
        } => format!("Extended old={old_end_epoch} new={new_end_epoch}"),
        Expired => "Expired".to_string(),
    }
}

fn is_demo_event(kind: &indexer::EventKind) -> bool {
    use indexer::EventKind::*;
    matches!(
        kind,
        BlobRegistered
            | BlobCertified { .. }
            | BlobDeleted
            | InvalidBlobId
            | Extended { .. }
            | Expired
            | EpochChangeDone { .. }
    )
}

fn print_index_event(prefix: &str, ev: Result<indexer::WalrusEvent>, demo_view: bool) {
    if !demo_view {
        println!("{prefix} EVENT: {ev:#?}");
        return;
    }

    match ev {
        Ok(e) => {
            if !is_demo_event(&e.kind) {
                return;
            }
            let kind = event_kind_compact(&e.kind);
            let blob = e.blob_id.as_deref().unwrap_or("-");
            let obj = e.object_id.as_deref().unwrap_or("-");
            let tx = e.tx_digest.as_deref().unwrap_or("-");
            println!("{prefix} kind={kind} blob_id={blob} object_id={obj} tx={tx}");
        }
        Err(err) => {
            eprintln!("{prefix} error={err}");
        }
    }
}

async fn run_index(cfg: AppConfig, limit: usize, follow: bool, demo_view: bool) -> Result<()> {
    use indexer::{EventSource, SimulatedEventSource};
    info!(
        "Indexer mode: {:?} (demo_view={})",
        cfg.indexer_mode, demo_view
    );

    match cfg.indexer_mode {
        IndexerMode::Simulated => {
            let mut src = SimulatedEventSource::new();
            let mut s = src.stream();
            let mut seen = 0usize;
            while let Some(ev) = s.next().await {
                print_index_event("SIMULATED", ev, demo_view);
                seen += 1;
                if !follow && seen >= limit {
                    break;
                }
            }
        }
        IndexerMode::Sui => {
            #[cfg(feature = "sui-live")]
            {
                use indexer::sui_source::SuiEventSource;
                let pkg = match cfg.walrus_pkg_id.clone() {
                    Some(p) => p,
                    None => {
                        eprintln!(
                            "Missing WALRUS_PACKAGE_ID.\n\
                             Get it from the walrus repo Move.lock (testnet-contracts/wal/Move.lock) \
                             and set WALRUS_PACKAGE_ID + WALRUS_EVENT_MODULE=walrus."
                        );
                        return Ok(());
                    }
                };
                let module = cfg
                    .walrus_event_module
                    .clone()
                    .unwrap_or_else(|| "walrus".to_string());
                let mut src = SuiEventSource::new(cfg.sui_rpc_url.clone(), pkg, module);
                let mut s = src.stream();
                let mut seen = 0usize;
                let mut last_heartbeat = std::time::Instant::now();
                while let Some(ev) = s.next().await {
                    print_index_event("SUI", ev, demo_view);
                    seen += 1;

                    // lightweight heartbeat so you know the program is still alive even if events are sparse
                    if last_heartbeat.elapsed().as_secs() >= 10 {
                        tracing::info!(
                            "indexer heartbeat: seen_events={} (follow={})",
                            seen,
                            follow
                        );
                        last_heartbeat = std::time::Instant::now();
                    }

                    if !follow && seen >= limit {
                        break;
                    }
                }
                if !follow && seen == 0 {
                    eprintln!(
                        "No events observed. \
                         Tips: (1) verify WALRUS_PACKAGE_ID and WALRUS_EVENT_MODULE, \
                         (2) try `cargo run -- put --data \"hi\" --epochs 1` to emit fresh events."
                    );
                }
            }
            #[cfg(not(feature = "sui-live"))]
            {
                eprintln!("Rebuild with --features sui-live to enable Sui indexing");
            }
        }
    }
    Ok(())
}

fn pause_step(next_action: &str, observe: &str, manual: bool, step_sleep_secs: u64) {
    println!();
    println!("============================================================");
    println!("Next step: {next_action}");
    println!("Watch for: {observe}");
    println!("============================================================");
    if manual {
        println!("Press Enter to execute this step...");
        let mut line = String::new();
        let _ = std::io::stdin().read_line(&mut line);
    } else {
        println!(
            "[pause] auto mode: waiting {}s before executing...",
            step_sleep_secs
        );
        std::thread::sleep(std::time::Duration::from_secs(step_sleep_secs));
    }
}

async fn run_demo_flow(
    cfg: &AppConfig,
    cache_opt: Option<Arc<Mutex<cache::RocksCache>>>,
    data: String,
    out: String,
    manual: bool,
    step_sleep_secs: u64,
) -> Result<()> {
    let client = {
        let base = WalrusClient::new(&cfg.aggregator_base, cfg.publisher_base.as_deref())?;
        if let Some(c) = &cache_opt {
            base.with_cache(c.clone())
        } else {
            base
        }
    };

    pause_step(
        "Upload a new blob (put) and print its blob ID.",
        "metrics(9003): usually no hit/miss change yet; this creates demo input blob",
        manual,
        step_sleep_secs,
    );
    println!("[1/8] put blob");
    let put = client
        .put_blob_with_details(data.into_bytes(), Some(1), Some(false))
        .await?;
    let blob_id = put.blob_id.clone();
    println!("blob_id={blob_id}");
    if let Some(object_id) = &put.object_id {
        println!("object_id={object_id}");
    }
    if let Some(tx_digest) = &put.tx_digest {
        println!("tx_digest={tx_digest}");
    }
    if let Ok(path) = std::env::var("DEMO_BLOB_FILE") {
        if !path.trim().is_empty() {
            if let Some(parent) = std::path::Path::new(&path).parent() {
                if !parent.as_os_str().is_empty() {
                    let _ = std::fs::create_dir_all(parent);
                }
            }
            let mut track = format!("blob_id={}\n", blob_id);
            if let Some(object_id) = &put.object_id {
                track.push_str(&format!("object_id={}\n", object_id));
            }
            if let Some(tx_digest) = &put.tx_digest {
                track.push_str(&format!("tx_digest={}\n", tx_digest));
            }
            let _ = std::fs::write(&path, track);
            println!("demo_blob_file={path}");
        }
    }
    println!("[1 complete] Blob uploaded. Check metrics pane for cache counters baseline.");

    pause_step(
        "Run first get for this blob (expect cache_miss then cache_store).",
        "logs: cache_miss + cache_store | metrics: cache_origin_misses++, cache_puts++, bytes_written++",
        manual,
        step_sleep_secs,
    );
    println!("[2/8] get #1 (expect cache_miss then cache_store)");
    let bytes = client.get_blob_by_id(&blob_id).await?;
    write_output(&bytes, Some(&out))?;
    println!(
        "[2 complete] First get done. Expect cache_origin_misses/cache_puts/cache_bytes_* to increase."
    );

    pause_step(
        "Run second get for same blob (expect cache_hit source=disk or source=ram).",
        "logs: cache_hit source=disk/ram | metrics: cache_disk_hits++ or cache_ram_hits++",
        manual,
        step_sleep_secs,
    );
    println!("[3/8] get #2 (expect cache_hit source=disk or source=ram)");
    let bytes = client.get_blob_by_id(&blob_id).await?;
    write_output(&bytes, Some(&out))?;
    println!("[3 complete] Second get done. Expect cache_disk_hits or cache_ram_hits to increase.");

    pause_step(
        "Purge this blob by blob ID (deterministic invalidation).",
        "metrics: cache_purges++",
        manual,
        step_sleep_secs,
    );
    println!("[4/8] purge by blob id");
    let cache = cache_opt
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("cache is disabled (set CACHE_ENABLED=true)"))?;
    cache
        .lock()
        .map_err(|_| anyhow::anyhow!("cache lock poisoned"))?
        .purge_by_blob_id(&blob_id)?;
    println!("cache_purge blob_id={blob_id}");
    println!("[4 complete] Blob-id purge done. Expect cache_purges to increase.");

    pause_step(
        "Get the blob again after purge (expect miss/store cycle again).",
        "logs: cache_miss + cache_store | metrics: cache_origin_misses++, cache_puts++",
        manual,
        step_sleep_secs,
    );
    println!("[5/8] get #3 after id purge (expect cache_miss again)");
    let bytes = client.get_blob_by_id(&blob_id).await?;
    write_output(&bytes, Some(&out))?;
    println!("[5 complete] Post-purge get done. Expect another miss/store cycle.");

    pause_step(
        "Purge by tag module:walrus (tag-based invalidation).",
        "metrics: cache_purges++",
        manual,
        step_sleep_secs,
    );
    println!("[6/8] purge by tag module:walrus");
    cache
        .lock()
        .map_err(|_| anyhow::anyhow!("cache lock poisoned"))?
        .purge_by_tag("module:walrus")?;
    println!("cache_purge tag=module:walrus");
    println!("[6 complete] Tag purge done. Expect cache_purges to increase again.");

    pause_step(
        "Set epoch metadata then purge by epoch threshold (epoch-based invalidation).",
        "metrics: cache_purges++ after epoch threshold purge",
        manual,
        step_sleep_secs,
    );
    println!("[7/8] set epoch metadata and purge by epoch");
    {
        let cache = cache
            .lock()
            .map_err(|_| anyhow::anyhow!("cache lock poisoned"))?;
        cache.update_meta_epoch(&blob_id, 123, Some(true), None)?;
    }
    cache
        .lock()
        .map_err(|_| anyhow::anyhow!("cache lock poisoned"))?
        .purge_epoch_leq(123)?;
    println!("cache_set_epoch blob_id={blob_id} end_epoch=123 deletable=true");
    println!("cache_purge epoch_leq=123");
    println!(
        "[7 complete] Epoch-based purge done. This demonstrates deterministic invalidation by epoch threshold."
    );

    pause_step(
        "Finish demo and print artifact path.",
        "final check: hits/misses/purges counters should all be non-zero on 9003",
        manual,
        step_sleep_secs,
    );
    println!("[8/8] done");
    println!("artifact={out}");

    Ok(())
}
