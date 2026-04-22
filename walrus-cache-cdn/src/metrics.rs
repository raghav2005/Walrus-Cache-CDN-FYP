use axum::{Router, routing::get};
use once_cell::sync::Lazy;
use prometheus::{Encoder, HistogramOpts, HistogramVec, IntCounter, Registry, TextEncoder};
use std::{net::SocketAddr, time::Duration};

pub static REGISTRY: Lazy<Registry> = Lazy::new(Registry::new);

pub static CACHE_RAM_HITS: Lazy<IntCounter> = Lazy::new(|| {
    let c = IntCounter::new("cache_ram_hits", "RAM-window cache hits").unwrap();
    REGISTRY.register(Box::new(c.clone())).ok();
    c
});

pub static CACHE_DISK_HITS: Lazy<IntCounter> = Lazy::new(|| {
    let c = IntCounter::new("cache_disk_hits", "RocksDB cache hits").unwrap();
    REGISTRY.register(Box::new(c.clone())).ok();
    c
});

pub static CACHE_ORIGIN_MISSES: Lazy<IntCounter> = Lazy::new(|| {
    let c = IntCounter::new("cache_origin_misses", "Origin fetches due to cache miss").unwrap();
    REGISTRY.register(Box::new(c.clone())).ok();
    c
});

pub static CACHE_PURGES: Lazy<IntCounter> = Lazy::new(|| {
    let c = IntCounter::new("cache_purges", "Cache purges (explicit or expiry)").unwrap();
    REGISTRY.register(Box::new(c.clone())).ok();
    c
});

pub static CACHE_PUTS: Lazy<IntCounter> = Lazy::new(|| {
    let c = IntCounter::new("cache_puts", "Objects written to cache").unwrap();
    REGISTRY.register(Box::new(c.clone())).ok();
    c
});

pub static CACHE_BYTES_SERVED: Lazy<IntCounter> = Lazy::new(|| {
    let c = IntCounter::new("cache_bytes_served", "Total bytes served (any source)").unwrap();
    REGISTRY.register(Box::new(c.clone())).ok();
    c
});

pub static CACHE_BYTES_WRITTEN: Lazy<IntCounter> = Lazy::new(|| {
    let c = IntCounter::new("cache_bytes_written", "Bytes written into cache").unwrap();
    REGISTRY.register(Box::new(c.clone())).ok();
    c
});

pub static REQUEST_LATENCY_MS: Lazy<HistogramVec> = Lazy::new(|| {
    let opts =
        HistogramOpts::new("request_latency_ms", "Get latency by source (ms)").buckets(vec![
            1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0,
        ]);
    let h = HistogramVec::new(opts, &["source"]).unwrap();
    REGISTRY.register(Box::new(h.clone())).ok();
    h
});

pub async fn serve(addr: &str) -> anyhow::Result<()> {
    let app = Router::new().route("/metrics", get(handler));
    let addr: SocketAddr = addr.parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app.into_make_service())
        .with_graceful_shutdown(async {
            loop {
                tokio::time::sleep(Duration::from_secs(3600)).await;
            }
        })
        .await?;
    Ok(())
}

async fn handler() -> String {
    let encoder = TextEncoder::new();
    let metric_families = REGISTRY.gather();
    let mut buf = Vec::new();
    encoder.encode(&metric_families, &mut buf).unwrap();
    String::from_utf8(buf).unwrap_or_default()
}
