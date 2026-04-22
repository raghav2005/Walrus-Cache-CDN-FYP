# Walrus-Cache-CDN-FYP

Walrus-Cache-CDN is a prototype that places a cache-enabled,
CDN-like edge layer in front of Walrus blob retrieval endpoints. The repository
contains the Rust implementation, demo/evaluation scripts, and generated evaluation
artefacts aligned to the current workspace.

<details>
  <summary><strong>Table of Contents</strong></summary>

- [What You Can Run Immediately After Cloning](#what-you-can-run-immediately-after-cloning)
- [Requirements](#requirements)
- [Repository Layout](#repository-layout)
- [Build and Test](#build-and-test)
- [Default Configuration and Presets](#default-configuration-and-presets)
- [Main Commands](#main-commands)
- [Demo Workflow](#demo-workflow)
- [Evaluation Workflow](#evaluation-workflow)
- [Metrics](#metrics)
- [Package-Level Docs](#package-level-docs)
- [Known Scope Limits](#known-scope-limits)

</details>

## What You Can Run Immediately After Cloning

The repository now provides safe built-in defaults for a first run:

- `INDEXER_MODE=simulated`, so indexing works without live Sui setup
- public Walrus testnet aggregator/publisher endpoints are configured by default
- the Rust toolchain is pinned in `rust-toolchain.toml`
- an explicit example config is provided in `walrus-cache-cdn/.env.example`

The canonical repository is:

- `https://github.com/raghav2005/Walrus-Cache-CDN-FYP`

That means the quickest smoke tests after cloning are:

```bash
cd walrus-cache-cdn
cargo run -- index --limit 4
cargo run -- evaluate --out-dir ./evaluation_output --seed 42 --scale 1
```

If you want a real network retrieval smoke test and have internet access:

```bash
cd walrus-cache-cdn
cargo run -- get --id "$(head -n 1 ids.txt)" --out /tmp/walrus-sample.bin
```

## Requirements

The project was checked against the following tool versions in this workspace:

| Requirement | Version / Notes |
|---|---|
| Rust toolchain | `1.91.0` |
| Cargo | `1.91.0` |
| Rust edition | `2024` |
| Python | `3.12.2` (optional, mainly for helper scripts) |
| `tmux` | optional, for the multi-pane demo launcher |
| C/C++ build tooling | required by the `rocksdb` crate during compilation |
| Network access | optional for simulated mode, required for public Walrus/Sui use |

The Cargo dependency set is locked in `walrus-cache-cdn/Cargo.lock`. Key crate
versions include `axum 0.7`, `reqwest 0.12`, `rocksdb 0.22`, `tokio 1`, and
`prometheus 0.13`, with optional live indexing via `sui-sdk`.

## Repository Layout

| Path | Purpose |
|---|---|
| `walrus-cache-cdn/` | Rust project for the prototype |
| `walrus-cache-cdn/src/` | implementation modules |
| `walrus-cache-cdn/scripts/` | demo and evaluation helper scripts |
| `walrus-cache-cdn/evaluation_output/` | generated evaluation summaries and plots |
| `walrus-cache-cdn/DEMO_RUNBOOK.md` | four-terminal demo walkthrough |
| `dissertation.pdf` | current compiled dissertation snapshot |
| `dissertation_appendices.tex` | appendix material aligned to this workspace |

## Build and Test

```bash
cd walrus-cache-cdn
cargo build
cargo test
```

Build with live Sui indexing support:

```bash
cd walrus-cache-cdn
cargo build --features sui-live
cargo test --features sui-live
```

## Default Configuration and Presets

The executable reads configuration from environment variables. If a local
`walrus-cache-cdn/.env` file is present, it is loaded automatically through `dotenvy`,
but a fresh clone also works without one because the runtime defaults were
updated to be clone-safe.

Useful preset files:

- `walrus-cache-cdn/.env.example` - documented example configuration
- `walrus-cache-cdn/.env.testnet-public` - live public testnet configuration
- `walrus-cache-cdn/.env.testnet-local` - local Walrus endpoints on testnet settings
- `walrus-cache-cdn/.env.localnet` - local Sui/localnet-oriented preset

If you want an explicit local config file, create it with:

```bash
cd walrus-cache-cdn
cp .env.example .env
```

Important environment variables:

| Variable | Meaning | Default |
|---|---|---|
| `NETWORK` | `localnet` or `testnet` | `testnet` |
| `WALRUS_AGGREGATOR_URL` | Walrus read endpoint | `https://aggregator.walrus-testnet.walrus.space` |
| `WALRUS_PUBLISHER_URL` | Walrus upload endpoint | `https://publisher.walrus-testnet.walrus.space` |
| `INDEXER_MODE` | `simulated` or `sui` | `simulated` |
| `SUI_RPC_URL` | Sui RPC endpoint | network-dependent |
| `WALRUS_PACKAGE_ID` | package id used for live event filtering | unset |
| `WALRUS_EVENT_MODULE` | preferred live event module name | unset |
| `WALRUS_EVENT_MODULES` | fallback module candidates for live indexing | `events,blob,walrus` |
| `INDEXER_POLL_MS` | polling interval for live indexing | `1500` |
| `INDEXER_START_AT_HEAD` | tail from head in live mode | `false` unless set |
| `CACHE_ENABLED` | enable/disable cache path | `true` |
| `CACHE_DB_PATH` | RocksDB directory | `./walrus_cache_cdn` |
| `CACHE_BLOCK_CACHE_MB` | RocksDB block cache size | `512` |
| `CACHE_ADMISSION` | `wtinylfu`, `lru`, or `none` | `wtinylfu` |
| `CACHE_ADAPTSIZE` | enable probabilistic size-aware gating | `false` |
| `METRICS_ADDR` | listen address for `/metrics` | `127.0.0.1:9001` |
| `CACHE_NODES` | comma-separated nodes for sharding experiments | unset |

## Main Commands

Run these from `walrus-cache-cdn/`:

| Command | Purpose |
|---|---|
| `cargo run -- index --limit 4` | run the simulated event source and print a short lifecycle trace |
| `cargo run -- index --follow --demo-view` | follow lifecycle events continuously |
| `cargo run -- get --id <blob_id>` | retrieve a blob by Walrus blob ID |
| `cargo run -- get-by-object --object-id <object_id>` | retrieve a blob by Sui object ID |
| `cargo run -- put --data "hello"` | upload inline data through the configured publisher |
| `cargo run -- put --file ./file.bin --epochs 5` | upload a file with an epoch horizon |
| `cargo run -- cache-purge --id <blob_id>` | purge one cached blob |
| `cargo run -- cache-purge --tag module:walrus` | purge a cached tag group |
| `cargo run -- cache-purge --epoch-leq 55` | purge cached entries up to an epoch threshold |
| `cargo run -- cache-set-epoch --id <blob_id> --end-epoch 55` | set deterministic epoch metadata |
| `cargo run -- demo-flow --data "prof-demo"` | run the deterministic miss/hit/invalidate walkthrough |
| `cargo run -- evaluate --out-dir ./evaluation_output --seed 42 --scale 1` | run the dissertation evaluation suite |

There are also two sample blob IDs in `walrus-cache-cdn/ids.txt` for convenience in
smoke testing.

## Demo Workflow

Launch the full four-pane demo:

```bash
cd walrus-cache-cdn
./scripts/demo_tmux.sh
```

Stop the session:

```bash
cd walrus-cache-cdn
./scripts/demo_tmux_stop.sh
```

The detailed walkthrough is in
[walrus-cache-cdn/DEMO_RUNBOOK.md](walrus-cache-cdn/DEMO_RUNBOOK.md).

## Evaluation Workflow

Run the evaluation directly:

```bash
cd walrus-cache-cdn
cargo run -- evaluate --out-dir ./evaluation_output --seed 42 --scale 1
```

Or use the helper script:

```bash
cd walrus-cache-cdn
./scripts/eval_dissertation.sh ./evaluation_output
```

Generated artefacts include:

- `walrus-cache-cdn/evaluation_output/summary.csv`
- `walrus-cache-cdn/evaluation_output/warmup_curve.csv`
- `walrus-cache-cdn/evaluation_output/evaluation.json`
- `walrus-cache-cdn/evaluation_output/figure_index.md`
- `walrus-cache-cdn/evaluation_output/evaluation_interpretation.md`
- `walrus-cache-cdn/evaluation_output/plots/`
- `walrus-cache-cdn/evaluation_output/plots/must_include/`

## Metrics

When the binary starts it also serves Prometheus-style metrics. By default:

```text
http://127.0.0.1:9001/metrics
```

Important metrics include:

- `cache_ram_hits`
- `cache_disk_hits`
- `cache_origin_misses`
- `cache_purges`
- `cache_puts`
- `cache_bytes_served`
- `cache_bytes_written`
- `request_latency_ms{source=...}`

## Package-Level Docs

The root README is intended to be the main entry point. The crate-local
[walrus-cache-cdn/README.md](walrus-cache-cdn/README.md) is now just a short package
supplement.

## Known Scope Limits

This is a dissertation prototype rather than a production CDN. In particular:

- the cache is primarily single-node
- live event ingestion is optional and feature-gated
- `WTinyLFU` is intentionally lightweight rather than a full industrial
  implementation
- object-ID retrieval is supported, but blob-ID retrieval is the main cached path
