# Walrus CDN Demo Runbook

This runbook is structured for **4 terminals** so you can clearly show indexing, metrics, cache hits, and invalidation.

<details>
  <summary><strong>Table of Contents</strong></summary>

- [One-command `tmux` launcher (recommended)](#one-command-tmux-launcher-recommended)
- [Stop/cleanup command](#stopcleanup-command)
- [Terminal 1 — Walrus mode (testnet public)](#terminal-1--walrus-mode-testnet-public)
- [Terminal 2 — Continuous indexer feed (never quits until Ctrl+C)](#terminal-2--continuous-indexer-feed-never-quits-until-ctrlc)
- [Terminal 3 — Live metrics view (clear cache proof)](#terminal-3--live-metrics-view-clear-cache-proof)
- [Terminal 4 — Deterministic cache flow (put/get/hit/purge/epoch-purge)](#terminal-4--deterministic-cache-flow-putgethitpurgeepoch-purge)
- [Tests](#tests)
- [Dissertation evaluation suite (extensive)](#dissertation-evaluation-suite-extensive)

</details>

## One-command `tmux` launcher (recommended)

```bash
./scripts/demo_tmux.sh
```

Optional custom session/mode:

```bash
./scripts/demo_tmux.sh my-session testnet-public
```

This opens 4 panes and starts all demo roles automatically.

By default, pane 4 is now **manual**: it waits for Enter before each action and explains what the next Enter will do.

Optional auto-timed pacing (seconds between cache-flow steps):

```bash
DEMO_STEP_MODE=auto STEP_SLEEP=10 ./scripts/demo_t4_cache_flow.sh "prof-demo"
```

## Stop/cleanup command

```bash
./scripts/demo_tmux_stop.sh
```

Optional custom session name:

```bash
./scripts/demo_tmux_stop.sh my-session
```

When to run it:
- After your professor demo is finished.
- Before starting a fresh demo if ports/locks are busy.
- Any time a previous tmux demo session is still running.

## Terminal 1 — Walrus mode (testnet public)

```bash
./scripts/demo_t1_mode.sh testnet-public
```

## Terminal 2 — Continuous indexer feed (never quits until Ctrl+C)

```bash
./scripts/demo_t2_indexer_follow.sh
```

- This uses `index --follow`.
- In demo mode, it also uses `--demo-view` by default (compact lifecycle-focused output).
- It also highlights lines that match the active demo blob from pane 4 using a `[DEMO-BLOB ...]` label.
- Matching now uses multiple identifiers when available (`blob_id`, `object_id`, `tx_digest`) to handle representation differences.
- To switch back to full verbose event dump:

```bash
INDEX_DEMO_VIEW=false ./scripts/demo_t2_indexer_follow.sh
```

- You can also run directly:

```bash
cargo run --features sui-live -- index --follow --demo-view
```
- It runs on `METRICS_ADDR=127.0.0.1:9002` and `CACHE_DB_PATH=./walrus_cache_cdn_indexer`.

If no `[DEMO-BLOB ...]` lines appear, common causes are:
- The upload and indexer are not observing the same network/package configuration.
- Event visibility lag on public testnet.
- Provider response missing `object_id`/`tx_digest` fields (blob-id-only matching may be less reliable).

## Terminal 3 — Live metrics view (clear cache proof)

```bash
./scripts/demo_t3_metrics_watch.sh
```

This now watches both the indexer metrics port and demo-flow metrics port.
If a port is not up yet, the pane shows `[waiting]` for that address.

Watch these counters change live:
- `cache_origin_misses`
- `cache_disk_hits`
- `cache_ram_hits`
- `cache_puts`
- `cache_purges`
- `cache_bytes_served`
- `cache_bytes_written`
- `request_latency_ms{source=...}`

## Terminal 4 — Deterministic cache flow (put/get/hit/purge/epoch-purge)

```bash
./scripts/demo_t4_cache_flow.sh "prof-demo"
```

This now runs `demo-flow` in a **single process** so cache metrics are cumulative and accurate during the step-through.

This demonstrates, in order:
1. Cache miss + store
2. Cache hit
3. Invalidate by blob id
4. Miss after invalidation
5. Invalidate by tag (`module:walrus`)
6. Set epoch metadata
7. Invalidate by epoch threshold

## Tests

```bash
cargo test
cargo test --features sui-live
```

## Dissertation evaluation suite (extensive)

Run the full synthetic benchmark matrix and export machine-readable artifacts:

```bash
cargo run -- evaluate --out-dir ./evaluation_output --seed 42 --scale 1
```

This command now also generates a broad set of PNG plots automatically under `evaluation_output/plots/`.
It additionally creates a curated shortlist under `evaluation_output/plots/must_include/` for direct dissertation insertion.
The policy frontier uses `avg_latency_vs_hit_rate` as the canonical tradeoff chart for the write-up.

One-command run + plots:

```bash
./scripts/eval_dissertation.sh ./evaluation_output
```

The evaluation currently includes:
- baseline vs CDN (A/B toggle)
- cold -> warm sanity + warm-up curve
- admission policy comparison (WTinyLFU vs LRU vs None)
- scan resistance
- size-aware admission (AdaptSize-lite)
- event-driven invalidation (id/tag/epoch)
- stress + concurrency scaling sweep
- disk-only survival/recovery (via unit test)

Output files:
- `evaluation_output/summary.csv`
- `evaluation_output/warmup_curve.csv`
- `evaluation_output/evaluation.json`
- `evaluation_output/figure_index.csv` (claim + caption mapping for each key figure)
- `evaluation_output/figure_index.md` (table version for direct dissertation drafting)
- `evaluation_output/evaluation_interpretation.md` (auto-generated results narrative + utility checks)
- `evaluation_output/graph_explanations.md` (explanation + correctness checklist for each graph family)
- `evaluation_output/plots/*.png` (generated natively by Rust)
- `evaluation_output/plots/must_include/*.png` (core recommended dissertation figures)
