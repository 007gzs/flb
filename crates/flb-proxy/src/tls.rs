use crate::ProxyState;
use flb_core::parse_hostnames;
use flb_router::{host_specificity, strip_host_port};
use parking_lot::RwLock;
use pingora::tls::sign::CertifiedKey;
use pingora::tls::{
    ClientHello, CryptoProvider, ResolvesServerCert, install_default_crypto_provider,
};
use std::collections::HashMap;
use std::io::Cursor;
use std::sync::Arc;
use tracing::warn;

struct Cache {
    generation: u64,
    exact: HashMap<String, Arc<CertifiedKey>>,
    wildcards: Vec<(String, Arc<CertifiedKey>, u32)>,
    default: Arc<CertifiedKey>,
}

pub struct DynamicCert {
    state: Arc<ProxyState>,
    cache: RwLock<Cache>,
    fallback: Arc<CertifiedKey>,
}

impl std::fmt::Debug for DynamicCert {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DynamicCert").finish_non_exhaustive()
    }
}

impl DynamicCert {
    pub fn new(state: Arc<ProxyState>) -> Arc<Self> {
        install_default_crypto_provider();
        let fallback = Arc::new(make_fallback().expect("generate fallback TLS certificate"));
        Arc::new(Self {
            state,
            cache: RwLock::new(Cache {
                generation: 0,
                exact: HashMap::new(),
                wildcards: Vec::new(),
                default: fallback.clone(),
            }),
            fallback,
        })
    }

    fn refresh(&self) {
        let generation = self.state.store.generation();
        {
            let cache = self.cache.read();
            if cache.generation == generation && cache.generation != 0 {
                return;
            }
        }
        let snapshot = self.state.store.snapshot();
        let mut by_id = HashMap::new();
        for cert in &snapshot.certificates {
            match parse_cert(&cert.cert_pem, &cert.key_pem) {
                Ok(parsed) => {
                    by_id.insert(cert.id.clone(), Arc::new(parsed));
                }
                Err(err) => warn!(id = %cert.id, error = %err, "skip invalid certificate"),
            }
        }
        let mut exact: HashMap<String, (Arc<CertifiedKey>, u32)> = HashMap::new();
        let mut wildcards = Vec::new();
        let mut default: Option<(Arc<CertifiedKey>, u32)> = None;
        for host in &snapshot.hosts {
            if host.https_enabled
                && let Some(id) = &host.cert_id
                && let Some(cert) = by_id.get(id)
            {
                for pattern in parse_hostnames(&host.hostname) {
                    index_pattern(&mut exact, &mut wildcards, &mut default, &pattern, cert);
                }
            }
        }
        for domain in &snapshot.domains {
            if let Some(id) = &domain.cert_id
                && let Some(cert) = by_id.get(id)
            {
                index_pattern(&mut exact, &mut wildcards, &mut default, &domain.name, cert);
            }
        }
        wildcards.sort_by_key(|a| std::cmp::Reverse(a.2));
        *self.cache.write() = Cache {
            generation,
            exact: exact.into_iter().map(|(k, (c, _))| (k, c)).collect(),
            wildcards,
            default: default
                .map(|(c, _)| c)
                .or_else(|| by_id.into_values().next())
                .unwrap_or_else(|| self.fallback.clone()),
        };
    }

    fn pick(&self, sni: Option<&str>) -> Arc<CertifiedKey> {
        self.refresh();
        let cache = self.cache.read();
        let stripped = strip_host_port(sni.unwrap_or(""));
        let lowered;
        let key: &str = if stripped.bytes().all(|b| !b.is_ascii_uppercase()) {
            stripped
        } else {
            lowered = stripped.to_ascii_lowercase();
            &lowered
        };
        if let Some(cert) = cache.exact.get(key) {
            return cert.clone();
        }
        for (suffix, cert, _) in &cache.wildcards {
            if key != suffix
                && key
                    .strip_suffix(suffix.as_str())
                    .is_some_and(|prefix| prefix.ends_with('.'))
            {
                return cert.clone();
            }
        }
        cache.default.clone()
    }
}

impl ResolvesServerCert for DynamicCert {
    fn resolve(&self, client_hello: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        Some(self.pick(client_hello.server_name()))
    }
}

fn index_pattern(
    exact: &mut HashMap<String, (Arc<CertifiedKey>, u32)>,
    wildcards: &mut Vec<(String, Arc<CertifiedKey>, u32)>,
    default: &mut Option<(Arc<CertifiedKey>, u32)>,
    pattern: &str,
    cert: &Arc<CertifiedKey>,
) {
    let score = host_specificity(pattern);
    if pattern.is_empty() {
        if default.as_ref().is_none_or(|(_, s)| score > *s) {
            *default = Some((cert.clone(), score));
        }
    } else if let Some(suffix) = pattern.strip_prefix("*.") {
        wildcards.push((suffix.to_ascii_lowercase(), cert.clone(), score));
    } else {
        let key = pattern.to_ascii_lowercase();
        match exact.get(&key) {
            Some(&(_, s)) if s >= score => {}
            _ => {
                exact.insert(key, (cert.clone(), score));
            }
        }
    }
}

fn make_fallback() -> Result<CertifiedKey, String> {
    let mut params = rcgen::CertificateParams::new(vec!["localhost".into(), "flb.local".into()])
        .map_err(|e| e.to_string())?;
    params.distinguished_name = rcgen::DistinguishedName::new();
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, "FLB default");
    let key_pair = rcgen::KeyPair::generate().map_err(|e| e.to_string())?;
    let cert = params.self_signed(&key_pair).map_err(|e| e.to_string())?;
    parse_cert(&cert.pem(), &key_pair.serialize_pem())
}

fn parse_cert(cert_pem: &str, key_pem: &str) -> Result<CertifiedKey, String> {
    let certs = rustls_pemfile::certs(&mut Cursor::new(cert_pem.as_bytes()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    if certs.is_empty() {
        return Err("certificate pem is empty".into());
    }
    let key = rustls_pemfile::private_key(&mut Cursor::new(key_pem.as_bytes()))
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "private key pem is empty".to_string())?;
    install_default_crypto_provider();
    let provider = CryptoProvider::get_default()
        .ok_or_else(|| "rustls crypto provider is not installed".to_string())?;
    CertifiedKey::from_der(certs, key, provider).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::parse_cert;

    #[test]
    fn parses_self_signed_pem() {
        let mut params = rcgen::CertificateParams::new(vec!["example.test".into()]).unwrap();
        params.distinguished_name = rcgen::DistinguishedName::new();
        let key_pair = rcgen::KeyPair::generate().unwrap();
        let cert = params.self_signed(&key_pair).unwrap();
        let parsed = parse_cert(&cert.pem(), &key_pair.serialize_pem()).unwrap();
        assert_eq!(parsed.cert.len(), 1);
    }
}
