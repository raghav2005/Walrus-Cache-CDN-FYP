use anyhow::{Context, Result, bail};
use reqwest::Client;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tracing::info;
use url::Url;

use crate::cache::{BlobMeta, CacheGet, RocksCache};
use crate::metrics::{
    CACHE_BYTES_SERVED, CACHE_BYTES_WRITTEN, CACHE_ORIGIN_MISSES, REQUEST_LATENCY_MS,
};

pub struct PutBlobResult {
    pub blob_id: String,
    pub object_id: Option<String>,
    pub tx_digest: Option<String>,
}

#[derive(Clone)]
pub struct WalrusClient {
    http: Client,
    base: Url,
    publisher: Option<Url>,
    pub cache: Option<Arc<Mutex<RocksCache>>>,
}

impl WalrusClient {
    pub fn new(aggregator_base: &str, publisher_base: Option<&str>) -> Result<Self> {
        let base = Url::parse(aggregator_base)
            .with_context(|| format!("Invalid WALRUS_AGGREGATOR_URL: {aggregator_base}"))?;
        let publisher = match publisher_base {
            Some(p) => {
                Some(Url::parse(p).with_context(|| format!("Invalid WALRUS_PUBLISHER_URL: {p}"))?)
            }
            None => None,
        };
        Ok(Self {
            http: Client::builder().build()?,
            base,
            publisher,
            cache: None,
        })
    }

    pub fn with_cache(mut self, cache: Arc<Mutex<RocksCache>>) -> Self {
        self.cache = Some(cache);
        self
    }

    // GET /v1/blobs/{blobId} -> Vec<u8>
    pub async fn get_blob_by_id(&self, blob_id: &str) -> Result<Vec<u8>> {
        let t0 = Instant::now();

        if let Some(c) = &self.cache {
            match c.lock().unwrap().get(blob_id)? {
                CacheGet::RamHit(b) => {
                    info!("cache_hit source=ram blob_id={} bytes={}", blob_id, b.len());
                    return Ok(b.to_vec());
                }
                CacheGet::DiskHit(b) => {
                    info!(
                        "cache_hit source=disk blob_id={} bytes={}",
                        blob_id,
                        b.len()
                    );
                    return Ok(b.to_vec());
                }
                CacheGet::Miss => {
                    info!("cache_miss blob_id={}", blob_id);
                }
            }
        }

        let url = self.base.join(&format!("v1/blobs/{blob_id}"))?;
        let res = self
            .http
            .get(url)
            .send()
            .await?
            .error_for_status()
            .context("Aggregator returned an error")?;
        let body = res.bytes().await?;
        CACHE_ORIGIN_MISSES.inc();
        CACHE_BYTES_SERVED.inc_by(body.len() as u64);
        REQUEST_LATENCY_MS
            .with_label_values(&["origin"])
            .observe(t0.elapsed().as_millis() as f64);

        if let Some(c) = &self.cache {
            let meta = BlobMeta {
                size: body.len() as u64,
                end_epoch: None,
                ts_ms: None,
                deletable: None,
                tags: vec!["module:walrus".into()],
            };
            let admitted = c
                .lock()
                .unwrap()
                .put_if_admit(blob_id, body.clone(), &meta)?;
            info!(
                "cache_store blob_id={} bytes={} admitted={}",
                blob_id,
                body.len(),
                admitted
            );
            CACHE_BYTES_WRITTEN.inc_by(body.len() as u64);
        }
        Ok(body.to_vec())
    }

    // GET /v1/blobs/by-object-id/{object-id} -> Vec<u8>
    // left uncached by default - can add a separate object-id cache if needed
    pub async fn get_blob_by_object_id(&self, object_id: &str) -> Result<Vec<u8>> {
        let url = self
            .base
            .join(&format!("v1/blobs/by-object-id/{object_id}"))?;
        let res = self
            .http
            .get(url)
            .send()
            .await?
            .error_for_status()
            .context("Aggregator returned an error")?;
        Ok(res.bytes().await?.to_vec())
    }

    // PUT /v1/blobs[...] -> returns blobId
    pub async fn put_blob(
        &self,
        data: impl Into<Vec<u8>>,
        epochs: Option<u64>,
        permanent: Option<bool>,
    ) -> Result<String> {
        Ok(self
            .put_blob_with_details(data, epochs, permanent)
            .await?
            .blob_id)
    }

    // PUT /v1/blobs[...] -> returns blobId + optional object/tx identifiers when present
    pub async fn put_blob_with_details(
        &self,
        data: impl Into<Vec<u8>>,
        epochs: Option<u64>,
        permanent: Option<bool>,
    ) -> Result<PutBlobResult> {
        let Some(pub_base) = &self.publisher else {
            bail!("No publisher configured. Set WALRUS_PUBLISHER_URL to use uploads.");
        };
        let mut url = pub_base.join("v1/blobs")?;
        {
            let mut qp = url.query_pairs_mut();
            if let Some(e) = epochs {
                qp.append_pair("epochs", &e.to_string());
            }
            if let Some(perma) = permanent {
                qp.append_pair("permanent", if perma { "true" } else { "false" });
            }
        }

        let body_vec: Vec<u8> = data.into();
        let res = self
            .http
            .put(url)
            .body(body_vec)
            .send()
            .await?
            .error_for_status()
            .context("Publisher returned an error")?;
        let v: serde_json::Value = res.json().await?;
        let blob_id = v
            .pointer("/newlyCreated/blobObject/blobId")
            .or_else(|| v.pointer("/alreadyCertified/blobObject/blobId"))
            .and_then(|x| x.as_str())
            .ok_or_else(|| anyhow::anyhow!("blobId not found in publisher response: {v}"))?
            .to_string();

        let object_id = v
            .pointer("/newlyCreated/blobObject/objectId")
            .or_else(|| v.pointer("/newlyCreated/blobObject/id"))
            .or_else(|| v.pointer("/alreadyCertified/blobObject/objectId"))
            .or_else(|| v.pointer("/alreadyCertified/blobObject/id"))
            .or_else(|| v.pointer("/blobObject/objectId"))
            .or_else(|| v.pointer("/blobObject/id"))
            .and_then(|x| x.as_str())
            .map(|s| s.to_string());

        let tx_digest = v
            .pointer("/newlyCreated/txDigest")
            .or_else(|| v.pointer("/alreadyCertified/txDigest"))
            .or_else(|| v.pointer("/txDigest"))
            .and_then(|x| x.as_str())
            .map(|s| s.to_string());

        Ok(PutBlobResult {
            blob_id,
            object_id,
            tx_digest,
        })
    }
}
