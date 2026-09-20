use dashmap::DashMap;
use flb_config::Settings;
use flb_log::FileLogger;
use flb_plugin::PluginRegistry;
use flb_router::HostIndex;
use flb_store::Store;
use parking_lot::RwLock;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Semaphore;

mod http;
mod stream;
mod tls;

pub use http::run_http_proxy;
pub use stream::run_streams;

#[derive(Clone)]
pub struct ProxyState {
    pub store: Arc<Store>,
    pub acme_http01: Arc<dashmap::DashMap<String, String>>,
    pub plugins: Arc<PluginRegistry>,
    pub settings: Arc<Settings>,
    pub access_log: FileLogger,
    host_index: Arc<RwLock<(u64, Arc<HostIndex>)>>,
    resolved: Arc<DashMap<String, SocketAddr>>,
    inflight: Arc<Semaphore>,
}

impl ProxyState {
    pub fn new(
        store: Arc<Store>,
        acme_http01: Arc<dashmap::DashMap<String, String>>,
        settings: Arc<Settings>,
        access_log: FileLogger,
    ) -> Self {
        let generation = store.generation();
        let index = Arc::new(flb_router::build_host_index(&store.snapshot().hosts));
        let inflight = Semaphore::new(settings.max_inflight.max(1));
        Self {
            store,
            acme_http01,
            plugins: Arc::new(PluginRegistry::new()),
            settings,
            access_log,
            host_index: Arc::new(RwLock::new((generation, index))),
            resolved: Arc::new(DashMap::new()),
            inflight: Arc::new(inflight),
        }
    }

    fn host_index(&self) -> Arc<HostIndex> {
        let generation = self.store.generation();
        {
            let cache = self.host_index.read();
            if cache.0 == generation {
                return cache.1.clone();
            }
        }
        let snapshot = self.store.snapshot();
        let index = Arc::new(flb_router::build_host_index(&snapshot.hosts));
        *self.host_index.write() = (generation, index.clone());
        self.resolved.clear();
        index
    }
}
