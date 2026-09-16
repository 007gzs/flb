use flb_core::{Certificate, ConfigData, DnsProvider, Domain, Host, StreamConfig, Upstream};
use parking_lot::RwLock;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("io: {0}")]
    Io(#[from] io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("yaml: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("{0}")]
    Message(String),
}

pub type Result<T> = std::result::Result<T, StoreError>;

pub struct Store {
    path: PathBuf,
    data: RwLock<ConfigData>,
    generation: AtomicU64,
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let yaml_path = path.as_ref().to_path_buf();
        let json_path = yaml_path.with_extension("json");
        if let Some(parent) = yaml_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let data = if yaml_path.exists() {
            load_from_path(&yaml_path)?
        } else if json_path.exists() {
            let data = load_from_path(&json_path)?;
            persist_yaml(&yaml_path, &data)?;
            data
        } else {
            let empty = ConfigData::default();
            persist_yaml(&yaml_path, &empty)?;
            empty
        };
        Ok(Self {
            path: yaml_path,
            data: RwLock::new(data),
            generation: AtomicU64::new(1),
        })
    }

    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Relaxed)
    }

    pub fn snapshot(&self) -> ConfigData {
        self.data.read().clone()
    }

    fn persist(&self, data: &ConfigData) -> Result<()> {
        persist_yaml(&self.path, data)?;
        self.generation.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    pub fn replace(&self, data: ConfigData) -> Result<()> {
        self.persist(&data)?;
        *self.data.write() = data;
        Ok(())
    }

    pub fn update<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&mut ConfigData) -> Result<T>,
    {
        let mut guard = self.data.write();
        let result = f(&mut guard)?;
        self.persist(&guard)?;
        Ok(result)
    }

    pub fn upsert_certificate(&self, item: Certificate) -> Result<Certificate> {
        self.update(|data| {
            if let Some(existing) = data.certificates.iter_mut().find(|c| c.id == item.id) {
                *existing = item.clone();
            } else {
                data.certificates.push(item.clone());
            }
            Ok(item)
        })
    }

    pub fn delete_certificate(&self, id: &str) -> Result<()> {
        self.update(|data| {
            if data
                .domains
                .iter()
                .any(|d| d.cert_id.as_deref() == Some(id))
                || data.hosts.iter().any(|h| h.cert_id.as_deref() == Some(id))
            {
                return Err(StoreError::Message("证书仍被域名或主机引用".into()));
            }
            let before = data.certificates.len();
            data.certificates.retain(|c| c.id != id);
            if before == data.certificates.len() {
                return Err(StoreError::Message("证书不存在".into()));
            }
            Ok(())
        })
    }

    pub fn upsert_dns_provider(&self, item: DnsProvider) -> Result<DnsProvider> {
        self.update(|data| {
            if let Some(existing) = data.dns_providers.iter_mut().find(|c| c.id == item.id) {
                *existing = item.clone();
            } else {
                data.dns_providers.push(item.clone());
            }
            Ok(item)
        })
    }

    pub fn delete_dns_provider(&self, id: &str) -> Result<()> {
        self.update(|data| {
            if data
                .domains
                .iter()
                .any(|d| d.dns_provider_id.as_deref() == Some(id))
            {
                return Err(StoreError::Message("DNS 提供商仍被域名引用".into()));
            }
            let before = data.dns_providers.len();
            data.dns_providers.retain(|c| c.id != id);
            if before == data.dns_providers.len() {
                return Err(StoreError::Message("DNS 提供商不存在".into()));
            }
            Ok(())
        })
    }

    pub fn upsert_domain(&self, item: Domain) -> Result<Domain> {
        self.update(|data| {
            if let Some(existing) = data.domains.iter_mut().find(|c| c.id == item.id) {
                *existing = item.clone();
            } else {
                data.domains.push(item.clone());
            }
            Ok(item)
        })
    }

    pub fn delete_domain(&self, id: &str) -> Result<()> {
        self.update(|data| {
            let before = data.domains.len();
            data.domains.retain(|c| c.id != id);
            if before == data.domains.len() {
                return Err(StoreError::Message("域名不存在".into()));
            }
            Ok(())
        })
    }

    pub fn upsert_upstream(&self, item: Upstream) -> Result<Upstream> {
        self.update(|data| {
            if let Some(existing) = data.upstreams.iter_mut().find(|c| c.id == item.id) {
                *existing = item.clone();
            } else {
                data.upstreams.push(item.clone());
            }
            Ok(item)
        })
    }

    pub fn delete_upstream(&self, id: &str) -> Result<()> {
        self.update(|data| {
            if data.hosts.iter().any(|h| {
                h.default_upstream_id == id || h.routes.iter().any(|r| r.upstream_id == id)
            }) {
                return Err(StoreError::Message("服务组仍被主机或路由引用".into()));
            }
            let before = data.upstreams.len();
            data.upstreams.retain(|c| c.id != id);
            if before == data.upstreams.len() {
                return Err(StoreError::Message("服务组不存在".into()));
            }
            Ok(())
        })
    }

    pub fn upsert_host(&self, item: Host) -> Result<Host> {
        self.update(|data| {
            if let Some(existing) = data.hosts.iter_mut().find(|c| c.id == item.id) {
                *existing = item.clone();
            } else {
                data.hosts.push(item.clone());
            }
            Ok(item)
        })
    }

    pub fn delete_host(&self, id: &str) -> Result<()> {
        self.update(|data| {
            let before = data.hosts.len();
            data.hosts.retain(|c| c.id != id);
            if before == data.hosts.len() {
                return Err(StoreError::Message("主机不存在".into()));
            }
            Ok(())
        })
    }

    pub fn upsert_stream(&self, item: StreamConfig) -> Result<StreamConfig> {
        self.update(|data| {
            if data
                .streams
                .iter()
                .any(|s| s.listen_port == item.listen_port && s.id != item.id)
            {
                return Err(StoreError::Message(format!(
                    "监听端口 {} 已被占用",
                    item.listen_port
                )));
            }
            if let Some(existing) = data.streams.iter_mut().find(|c| c.id == item.id) {
                *existing = item.clone();
            } else {
                data.streams.push(item.clone());
            }
            Ok(item)
        })
    }

    pub fn delete_stream(&self, id: &str) -> Result<()> {
        self.update(|data| {
            let before = data.streams.len();
            data.streams.retain(|c| c.id != id);
            if before == data.streams.len() {
                return Err(StoreError::Message("数据流不存在".into()));
            }
            Ok(())
        })
    }
}

fn load_from_path(path: &Path) -> Result<ConfigData> {
    let raw = fs::read_to_string(path)?;
    if raw.trim().is_empty() {
        return Ok(ConfigData::default());
    }
    if path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("json"))
    {
        Ok(serde_json::from_str(&raw)?)
    } else {
        Ok(serde_yaml::from_str(&raw)?)
    }
}

fn persist_yaml(path: &Path, data: &ConfigData) -> Result<()> {
    atomic_write(path, serde_yaml::to_string(data)?.as_bytes())?;
    Ok(())
}

fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = path.with_extension("yaml.tmp");
    {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    fs::rename(tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use flb_core::{Certificate, now_rfc3339};

    #[test]
    fn roundtrip_certificate() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join("config.yaml")).unwrap();
        let cert = Certificate {
            id: "c1".into(),
            name: "demo".into(),
            cert_pem: "CERT".into(),
            key_pem: "KEY".into(),
            not_after: None,
            auto_issued: false,
            created_at: now_rfc3339(),
        };
        store.upsert_certificate(cert.clone()).unwrap();
        assert_eq!(store.snapshot().certificates.len(), 1);
        store.delete_certificate("c1").unwrap();
        assert!(store.snapshot().certificates.is_empty());
        assert!(dir.path().join("config.yaml").exists());
    }

    #[test]
    fn migrates_json_to_yaml() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("config.json"),
            r#"{"certificates":[],"dnsProviders":[],"domains":[],"upstreams":[],"hosts":[],"streams":[]}"#,
        )
        .unwrap();
        let store = Store::open(dir.path().join("config.yaml")).unwrap();
        assert!(dir.path().join("config.yaml").exists());
        assert!(store.snapshot().certificates.is_empty());
    }
}
