use anyhow::Result;
use bytes::Bytes;
use rand::{Rng, thread_rng};
use rocksdb::{
    BlockBasedOptions, Cache as RdbCache, ColumnFamilyDescriptor, DB, DBWithThreadMode, Direction,
    IteratorMode, Options, SingleThreaded, WriteOptions,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    path::Path,
    time::Instant,
};

use crate::metrics::*;

const DEFAULT_BLOCK_CACHE_BYTES: usize = 512 * 1024 * 1024;
const WINDOW_LRU_CAP: usize = 512;
const MAX_ADMIT_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug)]
pub enum AdmissionMode {
    WTinyLFU,
    Lru,
    None,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlobMeta {
    pub size: u64,
    pub end_epoch: Option<u64>,
    pub ts_ms: Option<u64>,
    pub deletable: Option<bool>,
    pub tags: Vec<String>,
}

pub enum CacheGet {
    RamHit(Bytes),
    DiskHit(Bytes),
    Miss,
}

pub struct RocksCache {
    db: DBWithThreadMode<SingleThreaded>,
    window_lru: VecDeque<(String, Bytes)>,
    disk_hits: HashMap<String, u8>,

    sketch: CountMin8,
    admission: AdmissionMode,
    adaptsize_k: Option<f64>,
}

impl RocksCache {
    pub fn open(
        path: impl AsRef<Path>,
        block_cache_bytes: Option<usize>,
        admission: AdmissionMode,
        adaptsize_k: Option<f64>,
    ) -> Result<Self> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);
        opts.set_max_open_files(1024);
        opts.optimize_for_point_lookup(1024);

        let mut table = BlockBasedOptions::default();
        let cap = block_cache_bytes.unwrap_or(DEFAULT_BLOCK_CACHE_BYTES);
        let lru: RdbCache = RdbCache::new_lru_cache(cap);
        table.set_block_cache(&lru);
        table.set_bloom_filter(10.0, false);
        table.set_cache_index_and_filter_blocks(true);
        opts.set_block_based_table_factory(&table);

        let cf_blobs = ColumnFamilyDescriptor::new("blobs", Options::default());
        let cf_meta = ColumnFamilyDescriptor::new("meta", Options::default());
        let cf_tags = ColumnFamilyDescriptor::new("tags", Options::default());

        let db = DB::open_cf_descriptors(&opts, path, vec![cf_blobs, cf_meta, cf_tags])?;

        Ok(Self {
            db,
            window_lru: VecDeque::with_capacity(WINDOW_LRU_CAP),
            disk_hits: HashMap::new(),
            sketch: CountMin8::new(1 << 14, 4),
            admission,
            adaptsize_k,
        })
    }

    #[inline]
    fn cf_blobs(&self) -> &rocksdb::ColumnFamily {
        self.db.cf_handle("blobs").expect("missing cf: blobs")
    }

    #[inline]
    fn cf_meta(&self) -> &rocksdb::ColumnFamily {
        self.db.cf_handle("meta").expect("missing cf: meta")
    }

    #[inline]
    fn cf_tags(&self) -> &rocksdb::ColumnFamily {
        self.db.cf_handle("tags").expect("missing cf: tags")
    }

    #[inline]
    fn sketch_add(&mut self, k: &str) {
        self.sketch.add(k);
    }
    #[inline]
    fn sketch_est(&self, k: &str) -> u32 {
        self.sketch.estimate(k)
    }

    fn admit(&mut self, key: &str, size: usize, victim_freq: u32) -> bool {
        match self.admission {
            AdmissionMode::None => size <= MAX_ADMIT_BYTES,
            AdmissionMode::Lru => size <= MAX_ADMIT_BYTES && self.adaptsize_pass(size),
            AdmissionMode::WTinyLFU => {
                let cand = self.sketch_est(key);
                size <= MAX_ADMIT_BYTES && cand >= victim_freq && self.adaptsize_pass(size)
            }
        }
    }

    fn adaptsize_pass(&self, size: usize) -> bool {
        if let Some(k) = self.adaptsize_k {
            let p = (k / (size as f64)).clamp(0.0, 1.0);
            return thread_rng().gen_bool(p);
        }
        true
    }

    pub fn get(&mut self, blob_id: &str) -> Result<CacheGet> {
        let t0 = Instant::now();
        self.sketch_add(blob_id);

        if let Some((_, v)) = self.window_lru.iter().find(|(k, _)| k == blob_id) {
            CACHE_RAM_HITS.inc();
            CACHE_BYTES_SERVED.inc_by(v.len() as u64);
            REQUEST_LATENCY_MS
                .with_label_values(&["ram"])
                .observe(t0.elapsed().as_millis() as f64);
            return Ok(CacheGet::RamHit(v.clone()));
        }

        if let Some(v) = self.db.get_cf(self.cf_blobs(), blob_id.as_bytes())? {
            let b = Bytes::from(v);
            CACHE_DISK_HITS.inc();
            CACHE_BYTES_SERVED.inc_by(b.len() as u64);
            REQUEST_LATENCY_MS
                .with_label_values(&["disk"])
                .observe(t0.elapsed().as_millis() as f64);
            let e = self.disk_hits.entry(blob_id.to_string()).or_default();
            *e = e.saturating_add(1);
            if *e >= 2 || self.sketch_est(blob_id) >= 3 {
                self.promote(blob_id, &b);
                self.disk_hits.remove(blob_id);
            }
            return Ok(CacheGet::DiskHit(b));
        }

        Ok(CacheGet::Miss)
    }

    fn promote(&mut self, blob_id: &str, bytes: &Bytes) {
        if self.window_lru.len() >= WINDOW_LRU_CAP {
            self.window_lru.pop_back();
        }
        self.window_lru
            .push_front((blob_id.to_string(), bytes.clone()));
    }

    pub fn put_if_admit(&mut self, blob_id: &str, bytes: Bytes, meta: &BlobMeta) -> Result<bool> {
        let size = bytes.len();
        let victim_freq = self
            .window_lru
            .back()
            .map(|(k, _)| self.sketch_est(k))
            .unwrap_or(0);
        let admit = self.admit(blob_id, size, victim_freq);

        self.write_kv(blob_id, &bytes, meta)?;
        CACHE_PUTS.inc();
        CACHE_BYTES_WRITTEN.inc_by(bytes.len() as u64);

        if admit {
            self.promote(blob_id, &bytes);
        }

        if !meta.tags.is_empty() {
            self.add_tags(blob_id, &meta.tags)?;
        }
        Ok(admit)
    }

    pub fn purge_by_blob_id(&mut self, blob_id: &str) -> Result<()> {
        self.db.delete_cf(self.cf_blobs(), blob_id.as_bytes())?;
        self.db.delete_cf(self.cf_meta(), blob_id.as_bytes())?;
        // delete tag entries with suffix |blob_id
        let entries = self.scan_prefix(self.cf_tags(), b"tag|");
        for (k, _) in entries {
            if k.ends_with(format!("|{blob_id}").as_bytes()) {
                self.db.delete_cf(self.cf_tags(), &k)?;
            }
        }
        if let Some(pos) = self.window_lru.iter().position(|(k, _)| k == blob_id) {
            self.window_lru.remove(pos);
        }
        CACHE_PURGES.inc();
        Ok(())
    }

    pub fn add_tags(&self, blob_id: &str, tags: &[String]) -> Result<()> {
        let mut wo = WriteOptions::default();
        wo.set_sync(false);
        for t in tags {
            let key = format!("tag|{t}|{blob_id}");
            self.db
                .put_cf_opt(self.cf_tags(), key.as_bytes(), &[], &wo)?;
        }
        Ok(())
    }

    pub fn purge_by_tag(&mut self, tag: &str) -> Result<()> {
        let prefix = format!("tag|{tag}|");
        let pfx = prefix.as_bytes().to_vec();
        let entries = self.scan_prefix(self.cf_tags(), &pfx);
        for (k, _) in entries {
            if let Some(bid) = k
                .split(|&b| b == b'|')
                .last()
                .and_then(|s| std::str::from_utf8(s).ok())
            {
                let _ = self.purge_by_blob_id(bid);
            }
            self.db.delete_cf(self.cf_tags(), &k)?;
        }
        CACHE_PURGES.inc();
        Ok(())
    }

    pub fn purge_epoch_leq(&mut self, epoch: u64) -> Result<()> {
        let prefix = b"tag|epoch:";
        let entries = self.scan_prefix(self.cf_tags(), prefix);
        for (k, _) in entries {
            let s = std::str::from_utf8(&k).unwrap_or("");
            // tag|epoch:{n}|{blob}
            if let Some(rest) = s.strip_prefix("tag|epoch:") {
                if let Some((n, bid)) = rest.split_once('|') {
                    if let Ok(v) = n.parse::<u64>() {
                        if v <= epoch {
                            let _ = self.purge_by_blob_id(bid);
                            self.db.delete_cf(self.cf_tags(), &k)?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub fn update_meta_epoch(
        &self,
        blob_id: &str,
        end_epoch: u64,
        deletable: Option<bool>,
        ts_ms: Option<u64>,
    ) -> Result<()> {
        let existing = self.db.get_cf(self.cf_meta(), blob_id.as_bytes())?;
        let mut meta = if let Some(v) = existing {
            serde_json::from_slice::<BlobMeta>(&v).unwrap_or(BlobMeta {
                size: 0,
                end_epoch: None,
                ts_ms: None,
                deletable: None,
                tags: vec![],
            })
        } else {
            BlobMeta {
                size: 0,
                end_epoch: None,
                ts_ms: None,
                deletable: None,
                tags: vec![],
            }
        };
        meta.end_epoch = Some(end_epoch);
        meta.deletable = deletable.or(meta.deletable);
        meta.ts_ms = ts_ms.or(meta.ts_ms);
        if !meta.tags.iter().any(|t| t.starts_with("epoch:")) {
            meta.tags.push(format!("epoch:{end_epoch}"));
        }
        let v = serde_json::to_vec(&meta)?;
        self.db.put_cf(self.cf_meta(), blob_id.as_bytes(), v)?;
        self.add_tags(blob_id, &meta.tags)?;
        Ok(())
    }

    fn write_kv(&self, blob_id: &str, bytes: &Bytes, meta: &BlobMeta) -> Result<()> {
        let mut wo = WriteOptions::default();
        wo.set_sync(false);
        self.db
            .put_cf_opt(self.cf_blobs(), blob_id.as_bytes(), bytes, &wo)?;
        let v = serde_json::to_vec(meta)?;
        self.db
            .put_cf_opt(self.cf_meta(), blob_id.as_bytes(), &v, &wo)?;
        Ok(())
    }

    fn scan_prefix(&self, cf: &rocksdb::ColumnFamily, prefix: &[u8]) -> Vec<(Vec<u8>, Vec<u8>)> {
        let mut out = Vec::new();
        let mut it = self
            .db
            .iterator_cf(cf, IteratorMode::From(prefix, Direction::Forward));
        while let Some(Ok((k, v))) = it.next() {
            if !k.starts_with(prefix) {
                break;
            }
            out.push((k.to_vec(), v.to_vec()));
        }
        out
    }
}

/* --------- Count-Min Sketch (8-bit counters) --------- */
struct CountMin8 {
    width: usize,
    depth: usize,
    table: Vec<u8>,
    salts: Vec<u64>,
}
impl CountMin8 {
    fn new(width: usize, depth: usize) -> Self {
        assert!(width.is_power_of_two());
        let table = vec![0u8; width * depth];
        let salts = (0..depth)
            .map(|i| 0x9e3779b97f4a7c15u64 ^ (i as u64).wrapping_mul(0xBF58476D1CE4E5B9))
            .collect();
        Self {
            width,
            depth,
            table,
            salts,
        }
    }
    #[inline]
    fn idx(&self, h: u64, row: usize) -> usize {
        ((h ^ self.salts[row]) as usize) & (self.width - 1)
    }
    #[inline]
    fn hash(s: &str) -> u64 {
        use ahash::AHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = AHasher::default();
        s.hash(&mut hasher);
        hasher.finish()
    }
    fn add(&mut self, key: &str) {
        let h = Self::hash(key);
        for r in 0..self.depth {
            let i = self.idx(h, r);
            let cell = &mut self.table[r * self.width + i];
            if *cell < u8::MAX {
                *cell += 1;
            }
        }
    }
    fn estimate(&self, key: &str) -> u32 {
        let h = Self::hash(key);
        let mut m = u8::MAX;
        for r in 0..self.depth {
            let i = self.idx(h, r);
            m = m.min(self.table[r * self.width + i]);
        }
        m as u32
    }
}
