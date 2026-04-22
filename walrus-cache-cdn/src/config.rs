use crate::cache::AdmissionMode;
use std::env;

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub enum IndexerMode {
    Simulated,
    Sui,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub enum Network {
    Localnet,
    Testnet,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct AppConfig {
    pub network: Network,
    pub aggregator_base: String,
    pub publisher_base: Option<String>,
    pub indexer_mode: IndexerMode,
    pub sui_rpc_url: String,
    pub walrus_pkg_id: Option<String>,
    pub walrus_event_module: Option<String>,

    // cache / metrics / sharding
    pub cache_enabled: bool,
    pub cache_db_path: String,
    pub cache_block_cache_mb: usize,
    pub cache_admission: AdmissionMode,
    pub cache_adaptsize: bool,
    pub metrics_addr: String,
    pub cache_nodes: Vec<String>,
}

impl AppConfig {
    pub fn from_env() -> Self {
        let network = match env::var("NETWORK")
            .unwrap_or_else(|_| "testnet".into())
            .to_lowercase()
            .as_str()
        {
            "localnet" => Network::Localnet,
            _ => Network::Testnet,
        };

        let aggregator_base = env::var("WALRUS_AGGREGATOR_URL").unwrap_or_else(|_| {
            "https://aggregator.walrus-testnet.walrus.space".into()
        });
        let publisher_base = match env::var("WALRUS_PUBLISHER_URL") {
            Ok(v) if !v.trim().is_empty() => Some(v),
            _ => Some("https://publisher.walrus-testnet.walrus.space".into()),
        };

        let indexer_mode = match env::var("INDEXER_MODE")
            .unwrap_or_else(|_| "simulated".into())
            .to_lowercase()
            .as_str()
        {
            "sui" => IndexerMode::Sui,
            _ => IndexerMode::Simulated,
        };

        let sui_rpc_url = env::var("SUI_RPC_URL").unwrap_or_else(|_| match network {
            Network::Localnet => "http://127.0.0.1:9000".into(),
            Network::Testnet => "https://fullnode.testnet.sui.io:443".into(),
        });

        let walrus_pkg_id = env::var("WALRUS_PACKAGE_ID").ok();
        let walrus_event_module = env::var("WALRUS_EVENT_MODULE").ok();

        let cache_enabled = env::var("CACHE_ENABLED")
            .map(|v| v.eq_ignore_ascii_case("true"))
            .unwrap_or(true);
        let cache_db_path =
            env::var("CACHE_DB_PATH").unwrap_or_else(|_| "./walrus_cache_cdn".into());
        let cache_block_cache_mb: usize = env::var("CACHE_BLOCK_CACHE_MB")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(512);
        let cache_admission = match env::var("CACHE_ADMISSION")
            .unwrap_or_else(|_| "wtinylfu".into())
            .to_lowercase()
            .as_str()
        {
            "lru" => AdmissionMode::Lru,
            "none" => AdmissionMode::None,
            _ => AdmissionMode::WTinyLFU,
        };
        let cache_adaptsize = env::var("CACHE_ADAPTSIZE")
            .map(|v| v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        let metrics_addr = env::var("METRICS_ADDR").unwrap_or_else(|_| "127.0.0.1:9001".into());
        let cache_nodes = env::var("CACHE_NODES")
            .ok()
            .map(|s| {
                s.split(',')
                    .map(|x| x.trim().to_string())
                    .filter(|x| !x.is_empty())
                    .collect()
            })
            .unwrap_or_else(|| vec![]);

        Self {
            network,
            aggregator_base,
            publisher_base,
            indexer_mode,
            sui_rpc_url,
            walrus_pkg_id,
            walrus_event_module,
            cache_enabled,
            cache_db_path,
            cache_block_cache_mb,
            cache_admission,
            cache_adaptsize,
            metrics_addr,
            cache_nodes,
        }
    }
}
