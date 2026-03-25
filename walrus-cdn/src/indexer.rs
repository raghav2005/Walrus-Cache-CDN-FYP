use anyhow::Result;
use futures::Stream;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::{pin::Pin, time::Duration};
use tokio::time::sleep;

// Global cache handle for event-driven invalidation.
pub static CACHE_HANDLE: OnceCell<std::sync::Arc<std::sync::Mutex<crate::cache::RocksCache>>> =
    OnceCell::new();

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventKind {
    // walrus blob lifecycle (on-chain) events
    BlobRegistered,
    BlobCertified {
        end_epoch: u64,
        deletable: bool,
    },
    BlobDeleted,
    InvalidBlobId,

    // sui / system epoch signals
    EpochChangeStart {
        epoch: u64,
    },
    EpochChangeDone {
        epoch: u64,
    },
    ShardsReceived,
    EpochParametersSelected,
    ShardRecoveryStart,

    // synthetic (derived) signals
    Extended {
        old_end_epoch: u64,
        new_end_epoch: u64,
    },
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalrusEvent {
    pub kind: EventKind,
    pub blob_id: Option<String>,
    pub object_id: Option<String>,
    pub end_epoch: Option<u64>,
    pub tx_digest: Option<String>,
    pub event_seq: Option<String>,
    pub timestamp_ms: Option<u64>,
}

pub trait EventSource: Send {
    fn stream(&mut self) -> Pin<Box<dyn Stream<Item = Result<WalrusEvent>> + Send + '_>>;
}

/* ---------------------- simulated source ---------------------- */
pub struct SimulatedEventSource;

impl SimulatedEventSource {
    pub fn new() -> Self {
        Self
    }
}

impl EventSource for SimulatedEventSource {
    fn stream(&mut self) -> Pin<Box<dyn Stream<Item = Result<WalrusEvent>> + Send + '_>> {
        let s = async_stream::try_stream! {
            let samples = vec![
                WalrusEvent {
                    kind: EventKind::BlobRegistered,
                    blob_id: Some("M4hsZGQ1oCktdzeg...".into()),
                    object_id: Some("0xabc...".into()),
                    end_epoch: None,
                    tx_digest: Some("A1...".into()),
                    event_seq: Some("0".into()),
                    timestamp_ms: Some(1_700_000_000_000),
                },
                WalrusEvent {
                    kind: EventKind::BlobCertified { end_epoch: 50, deletable: true },
                    blob_id: Some("M4hsZGQ1oCktdzeg...".into()),
                    object_id: Some("0xabc...".into()),
                    end_epoch: Some(50),
                    tx_digest: Some("A1...".into()),
                    event_seq: Some("1".into()),
                    timestamp_ms: Some(1_700_000_100_000),
                },
                WalrusEvent {
                    kind: EventKind::BlobCertified { end_epoch: 80, deletable: true },
                    blob_id: Some("M4hsZGQ1oCktdzeg...".into()),
                    object_id: Some("0xabc...".into()),
                    end_epoch: Some(80),
                    tx_digest: Some("A1...".into()),
                    event_seq: Some("2".into()),
                    timestamp_ms: Some(1_700_000_200_000),
                },
                WalrusEvent {
                    kind: EventKind::EpochChangeDone { epoch: 81 },
                    blob_id: None, object_id: None, end_epoch: None,
                    tx_digest: None, event_seq: None, timestamp_ms: Some(1_700_000_300_000),
                },
            ];

            let mut last_end_epoch: HashMap<String, u64> = HashMap::new();

            for ev in samples {
                if let (EventKind::BlobCertified { end_epoch: new, .. }, Some(bid)) =
                    (&ev.kind, ev.blob_id.clone())
                {
                    if let Some(old) = last_end_epoch.insert(bid.clone(), *new) {
                        if *new > old {
                            yield WalrusEvent {
                                kind: EventKind::Extended { old_end_epoch: old, new_end_epoch: *new },
                                blob_id: Some(bid), object_id: None, end_epoch: Some(*new),
                                tx_digest: ev.tx_digest.clone(), event_seq: ev.event_seq.clone(),
                                timestamp_ms: ev.timestamp_ms,
                            };
                        }
                    }
                }

                if let EventKind::EpochChangeDone { epoch } = ev.kind {
                    let current_epoch = epoch;
                    for (bid, ee) in last_end_epoch.clone() {
                        if ee <= current_epoch {
                            yield WalrusEvent {
                                kind: EventKind::Expired,
                                blob_id: Some(bid), object_id: None, end_epoch: Some(ee),
                                tx_digest: None, event_seq: None, timestamp_ms: ev.timestamp_ms,
                            };
                        }
                    }
                }

                yield ev;
                sleep(Duration::from_millis(150)).await;
            }
        };
        Box::pin(s)
    }
}

/* ---------------------- sui-backed source ---------------------- */
#[cfg(feature = "sui-live")]
pub mod sui_source {
    use super::{EventKind, EventSource, WalrusEvent};
    use anyhow::{Result, anyhow};
    use async_stream::try_stream;
    use futures::Stream;
    use serde_json::Value;
    use std::{collections::HashMap, pin::Pin};
    use sui_sdk::{
        SuiClientBuilder,
        rpc_types::{EventFilter, EventPage},
        types::{Identifier, event::EventID},
    };

    fn mk_move_module_filter(pkg: &str, module: &str) -> anyhow::Result<EventFilter> {
        Ok(EventFilter::MoveModule {
            package: pkg.parse()?,
            module: Identifier::new(module)?,
        })
    }
    fn mk_move_event_module_filter(pkg: &str, module: &str) -> anyhow::Result<EventFilter> {
        Ok(EventFilter::MoveEventModule {
            package: pkg.parse()?,
            module: Identifier::new(module)?,
        })
    }

    pub struct SuiEventSource {
        rpc_url: String,
        pkg_id: String,
        module: String,
        cursor: Option<EventID>,
        last_end_epoch: HashMap<String, u64>,
        current_epoch: u64,
    }

    impl SuiEventSource {
        pub fn new(rpc_url: String, pkg_id: String, module: String) -> Self {
            Self {
                rpc_url,
                pkg_id,
                module,
                cursor: None,
                last_end_epoch: HashMap::new(),
                current_epoch: 0,
            }
        }

        fn get_u64(v: &Value, keys: &[&str]) -> Option<u64> {
            for k in keys {
                if let Some(n) = v.get(*k).and_then(|x| x.as_u64()) {
                    return Some(n);
                }
                if let Some(s) = v.get(*k).and_then(|x| x.as_str()) {
                    if let Ok(n) = s.parse::<u64>() {
                        return Some(n);
                    }
                }
            }
            None
        }
        fn get_bool(v: &Value, keys: &[&str]) -> Option<bool> {
            for k in keys {
                if let Some(b) = v.get(*k).and_then(|x| x.as_bool()) {
                    return Some(b);
                }
                if let Some(s) = v.get(*k).and_then(|x| x.as_str()) {
                    if let Ok(b) = s.parse::<bool>() {
                        return Some(b);
                    }
                }
            }
            None
        }
        fn get_str(v: &Value, keys: &[&str]) -> Option<String> {
            for k in keys {
                if let Some(s) = v.get(*k).and_then(|x| x.as_str()) {
                    return Some(s.to_string());
                }
            }
            None
        }

        fn decode(type_str: &str, parsed: &Value) -> Result<WalrusEvent> {
            if type_str.ends_with("::BlobRegistered") || type_str.contains("BlobRegistered") {
                return Ok(WalrusEvent {
                    kind: EventKind::BlobRegistered,
                    blob_id: Self::get_str(parsed, &["blob_id", "blobId"]),
                    object_id: Self::get_str(parsed, &["object_id", "objectId"]),
                    end_epoch: None,
                    tx_digest: None,
                    event_seq: None,
                    timestamp_ms: None,
                });
            }
            if type_str.ends_with("::BlobCertified") || type_str.contains("BlobCertified") {
                let end_epoch = Self::get_u64(parsed, &["end_epoch", "endEpoch"])
                    .ok_or_else(|| anyhow!("BlobCertified missing end_epoch"))?;
                let deletable = Self::get_bool(parsed, &["deletable"]).unwrap_or(true);
                return Ok(WalrusEvent {
                    kind: EventKind::BlobCertified {
                        end_epoch,
                        deletable,
                    },
                    blob_id: Self::get_str(parsed, &["blob_id", "blobId"]),
                    object_id: Self::get_str(parsed, &["object_id", "objectId"]),
                    end_epoch: Some(end_epoch),
                    tx_digest: None,
                    event_seq: None,
                    timestamp_ms: None,
                });
            }
            if type_str.ends_with("::BlobDeleted") || type_str.contains("BlobDeleted") {
                return Ok(WalrusEvent {
                    kind: EventKind::BlobDeleted,
                    blob_id: Self::get_str(parsed, &["blob_id", "blobId"]),
                    object_id: Self::get_str(parsed, &["object_id", "objectId"]),
                    end_epoch: None,
                    tx_digest: None,
                    event_seq: None,
                    timestamp_ms: None,
                });
            }
            if type_str.contains("InvalidBlobID") || type_str.contains("InvalidBlobId") {
                return Ok(WalrusEvent {
                    kind: EventKind::InvalidBlobId,
                    blob_id: Self::get_str(parsed, &["blob_id", "blobId"]),
                    object_id: None,
                    end_epoch: None,
                    tx_digest: None,
                    event_seq: None,
                    timestamp_ms: None,
                });
            }
            if type_str.contains("EpochChangeStart") {
                let epoch = Self::get_u64(parsed, &["epoch"]).unwrap_or_default();
                return Ok(WalrusEvent {
                    kind: EventKind::EpochChangeStart { epoch },
                    blob_id: None,
                    object_id: None,
                    end_epoch: None,
                    tx_digest: None,
                    event_seq: None,
                    timestamp_ms: None,
                });
            }
            if type_str.contains("EpochChangeDone") {
                let epoch = Self::get_u64(parsed, &["epoch"]).unwrap_or_default();
                return Ok(WalrusEvent {
                    kind: EventKind::EpochChangeDone { epoch },
                    blob_id: None,
                    object_id: None,
                    end_epoch: None,
                    tx_digest: None,
                    event_seq: None,
                    timestamp_ms: None,
                });
            }
            if type_str.contains("ShardsReceived") {
                return Ok(WalrusEvent {
                    kind: EventKind::ShardsReceived,
                    blob_id: None,
                    object_id: None,
                    end_epoch: None,
                    tx_digest: None,
                    event_seq: None,
                    timestamp_ms: None,
                });
            }
            if type_str.contains("EpochParametersSelected") {
                return Ok(WalrusEvent {
                    kind: EventKind::EpochParametersSelected,
                    blob_id: None,
                    object_id: None,
                    end_epoch: None,
                    tx_digest: None,
                    event_seq: None,
                    timestamp_ms: None,
                });
            }
            if type_str.contains("ShardRecoveryStart") {
                return Ok(WalrusEvent {
                    kind: EventKind::ShardRecoveryStart,
                    blob_id: None,
                    object_id: None,
                    end_epoch: None,
                    tx_digest: None,
                    event_seq: None,
                    timestamp_ms: None,
                });
            }
            Err(anyhow!("Unrecognized event type: {type_str}"))
        }

        async fn query_page(
            client: &sui_sdk::SuiClient,
            filter: EventFilter,
            cursor: Option<EventID>,
            limit: Option<usize>,
            descending: bool,
        ) -> Result<EventPage> {
            Ok(client
                .event_api()
                .query_events(filter, cursor, limit, descending)
                .await?)
        }
    }

    impl EventSource for SuiEventSource {
        fn stream(&mut self) -> Pin<Box<dyn Stream<Item = Result<WalrusEvent>> + Send + '_>> {
            let rpc = self.rpc_url.clone();
            let pkg = self.pkg_id.clone();

            let mut module_candidates: Vec<String> = std::env::var("WALRUS_EVENT_MODULES")
                .unwrap_or_else(|_| "events,blob,walrus".to_string())
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            if !self.module.is_empty() && !module_candidates.contains(&self.module) {
                module_candidates.insert(0, self.module.clone());
            }

            let s = try_stream! {
                let client = SuiClientBuilder::default().build(&rpc).await?;
                tracing::info!("SuiEventSource connected: rpc={}, pkg={}, modules={:?}", rpc, pkg, module_candidates);

                let mut cursor = if std::env::var("INDEXER_START_AT_HEAD").map(|v| v.eq_ignore_ascii_case("true")).unwrap_or(false) {
                    tracing::info!("INDEXER_START_AT_HEAD=true: tailing from head.");
                    None
                } else {
                    tracing::info!("INDEXER_START_AT_HEAD not set: will backfill page(s) then tail.");
                    None
                };

                loop {
                    let mut got_any = false;

                    for m in &module_candidates {
                        let f1 = mk_move_event_module_filter(&pkg, m)?;
                        let p1 = Self::query_page(&client, f1.clone(), cursor.clone(), Some(50), true).await?;
                        tracing::info!("MoveEventModule({}, {}) -> {} events, next_cursor={:?}", pkg, m, p1.data.len(), p1.next_cursor);

                        let mut pages: Vec<EventPage> = Vec::new();
                        if !p1.data.is_empty() { pages.push(p1); }
                        else {
                            let f2 = mk_move_module_filter(&pkg, m)?;
                            let p2 = Self::query_page(&client, f2.clone(), cursor.clone(), Some(50), true).await?;
                            tracing::info!("MoveModule({}, {}) -> {} events, next_cursor={:?}", pkg, m, p2.data.len(), p2.next_cursor);
                            if !p2.data.is_empty() { pages.push(p2); }
                        }

                        for page in pages {
                            got_any |= !page.data.is_empty();
                            self.cursor = page.next_cursor;

                            if let Some(first) = page.data.first() {
                                tracing::info!("first: type={}, tx={}, seq={}, ts_ms={:?}", first.type_, first.id.tx_digest, first.id.event_seq, first.timestamp_ms);
                            }
                            if let Some(last) = page.data.last() {
                                tracing::info!("last:  type={}, tx={}, seq={}, ts_ms={:?}", last.type_, last.id.tx_digest, last.id.event_seq, last.timestamp_ms);
                            }

                            for ev in page.data {
                                let type_str = ev.type_.to_string();
                                let mut wev = Self::decode(&type_str, &ev.parsed_json)?;
                                wev.tx_digest = Some(ev.id.tx_digest.to_string());
                                wev.event_seq = Some(ev.id.event_seq.to_string());
                                wev.timestamp_ms = ev.timestamp_ms;

                                // synth "Extended"
                                if let (super::EventKind::BlobCertified { end_epoch: new, .. }, Some(bid)) = (&wev.kind, wev.blob_id.clone()) {
                                    if let Some(old) = self.last_end_epoch.insert(bid.clone(), *new) {
                                        if *new > old {
                                            tracing::debug!("Synth EXTENDED for {}: {} -> {}", bid, old, new);
                                            yield WalrusEvent {
                                                kind: super::EventKind::Extended { old_end_epoch: old, new_end_epoch: *new },
                                                blob_id: Some(bid.clone()), object_id: None, end_epoch: Some(*new),
                                                tx_digest: wev.tx_digest.clone(), event_seq: wev.event_seq.clone(),
                                                timestamp_ms: wev.timestamp_ms,
                                            };
                                        }
                                    }
                                }

                                // === cache side-effects ===
                                if let Some(cache) = super::CACHE_HANDLE.get() {
                                    use super::EventKind::*;
                                    let mut cache = cache.lock().unwrap();
                                    match &wev.kind {
                                        BlobDeleted | InvalidBlobId => {
                                            if let Some(bid) = &wev.blob_id {
                                                let _ = cache.purge_by_blob_id(bid);
                                                tracing::info!("cache purge: {}", bid);
                                            }
                                        }
                                        Expired => {
                                            if let Some(bid) = &wev.blob_id {
                                                let _ = cache.purge_by_blob_id(bid);
                                                tracing::info!("cache expire purge: {}", bid);
                                            }
                                        }
                                        BlobCertified { end_epoch, deletable } => {
                                            if let Some(bid) = &wev.blob_id {
                                                let _ = cache.update_meta_epoch(bid, *end_epoch, Some(*deletable), wev.timestamp_ms);
                                            }
                                        }
                                        Extended { new_end_epoch, .. } => {
                                            if let Some(bid) = &wev.blob_id {
                                                let _ = cache.update_meta_epoch(bid, *new_end_epoch, None, wev.timestamp_ms);
                                            }
                                        }
                                        _ => {}
                                    }
                                }

                                yield wev;
                            }
                        }
                    }

                    // synthesise expiries on epoch change tick
                    let sys = client.governance_api().get_latest_sui_system_state().await?;
                    let epoch_now = sys.epoch as u64;
                    if epoch_now > self.current_epoch {
                        self.current_epoch = epoch_now;
                        if let Some(cache) = super::CACHE_HANDLE.get() {
                            let mut cache = cache.lock().unwrap();
                            let _ = cache.purge_epoch_leq(epoch_now);
                        }
                    }

                    if !got_any {
                        tracing::warn!("No Walrus events found via {:?}. If on testnet, upload a blob or recheck WALRUS_PACKAGE_ID.", module_candidates);
                    }

                    tokio::time::sleep(std::time::Duration::from_millis(
                        std::env::var("INDEXER_POLL_MS").ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(1500)
                    )).await;

                    cursor = self.cursor.take();
                }
            };
            Box::pin(s)
        }
    }
}
