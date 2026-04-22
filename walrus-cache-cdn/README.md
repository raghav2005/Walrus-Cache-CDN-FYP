# walrus-cache-cdn

This directory contains the Cargo project for Walrus-Cache-CDN.

The main setup, requirements, default configuration, quick-start commands, and
dissertation notes now live in the repository root README:

- [../README.md](../README.md)

Useful local documents in this directory are:

- [DEMO_RUNBOOK.md](DEMO_RUNBOOK.md)
- [evaluation_output/README.md](evaluation_output/README.md)

The source tree itself is organised as follows:

| Path | Purpose |
|---|---|
| `src/main.rs` | CLI entry point and command dispatch |
| `src/walrus.rs` | Walrus aggregator/publisher client and cached retrieval path |
| `src/cache.rs` | RocksDB cache, RAM hot tier, admission logic, invalidation |
| `src/indexer.rs` | Simulated and live event ingestion |
| `src/metrics.rs` | Prometheus metrics registry and HTTP exporter |
| `src/config.rs` | Environment-driven configuration |
| `src/eval.rs` | Synthetic evaluation suite, plotting support, and tests |
