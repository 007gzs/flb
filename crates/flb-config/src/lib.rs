use std::net::SocketAddr;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Settings {
    pub admin_listen: SocketAddr,
    pub http_listen: String,
    pub https_listen: String,
    pub data_dir: PathBuf,
    pub www_dir: PathBuf,
    pub acme_staging: bool,
}

impl Settings {
    pub fn config_path(&self) -> PathBuf {
        self.data_dir.join("config.json")
    }

    pub fn acme_dir(&self) -> PathBuf {
        self.data_dir.join("letsencrypt")
    }

    pub fn acme_account_path(&self) -> PathBuf {
        self.acme_dir().join("acme-account.json")
    }
}
