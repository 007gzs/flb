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
    pub admin_user: String,
    pub admin_password: String,
    pub jwt_secret: String,
}

impl Settings {
    pub fn config_path(&self) -> PathBuf {
        self.data_dir.join("config.yaml")
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.data_dir.join("logs")
    }

    pub fn access_log_path(&self) -> PathBuf {
        self.logs_dir().join("access.log")
    }

    pub fn error_log_path(&self) -> PathBuf {
        self.logs_dir().join("error.log")
    }

    pub fn audit_log_path(&self) -> PathBuf {
        self.logs_dir().join("flb.log")
    }

    pub fn acme_dir(&self) -> PathBuf {
        self.data_dir.join("letsencrypt")
    }

    pub fn acme_account_path(&self) -> PathBuf {
        self.acme_dir().join("acme-account.json")
    }
}
