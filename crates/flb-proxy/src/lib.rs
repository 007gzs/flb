use flb_config::Settings;
use flb_log::FileLogger;
use flb_plugin::PluginRegistry;
use flb_store::Store;
use std::sync::Arc;

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
}

impl ProxyState {
    pub fn new(
        store: Arc<Store>,
        acme_http01: Arc<dashmap::DashMap<String, String>>,
        settings: Arc<Settings>,
        access_log: FileLogger,
    ) -> Self {
        Self {
            store,
            acme_http01,
            plugins: Arc::new(PluginRegistry::new()),
            settings,
            access_log,
        }
    }
}
