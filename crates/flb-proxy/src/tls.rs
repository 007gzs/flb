use crate::ProxyState;
use async_trait::async_trait;
use flb_core::parse_hostnames;
use flb_router::{host_matches, host_specificity};
use parking_lot::RwLock;
use pingora::listeners::TlsAccept;
use pingora::tls::ext::{ssl_use_certificate, ssl_use_private_key};
use pingora::tls::pkey::{PKey, Private};
use pingora::tls::ssl::{NameType, SslRef};
use pingora::tls::x509::X509;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::warn;

#[derive(Clone)]
struct ParsedCert {
    certs: Vec<X509>,
    key: PKey<Private>,
}

struct Cache {
    generation: u64,
    by_id: HashMap<String, ParsedCert>,
    host_cert: Vec<(String, String)>,
}

pub struct DynamicCert {
    state: Arc<ProxyState>,
    cache: RwLock<Cache>,
    fallback: ParsedCert,
}

impl DynamicCert {
    pub fn new(state: Arc<ProxyState>) -> Box<Self> {
        Box::new(Self {
            state,
            cache: RwLock::new(Cache {
                generation: 0,
                by_id: HashMap::new(),
                host_cert: Vec::new(),
            }),
            fallback: make_fallback().expect("generate fallback TLS certificate"),
        })
    }

    fn refresh(&self) {
        let generation = self.state.store.generation();
        {
            let cache = self.cache.read();
            if cache.generation == generation && !cache.by_id.is_empty() {
                return;
            }
        }
        let snapshot = self.state.store.snapshot();
        let mut by_id = HashMap::new();
        for cert in &snapshot.certificates {
            match parse_cert(&cert.cert_pem, &cert.key_pem) {
                Ok(parsed) => {
                    by_id.insert(cert.id.clone(), parsed);
                }
                Err(err) => warn!(id = %cert.id, error = %err, "skip invalid certificate"),
            }
        }
        let mut host_cert = Vec::new();
        for host in &snapshot.hosts {
            if host.https_enabled
                && let Some(id) = &host.cert_id
            {
                for pattern in parse_hostnames(&host.hostname) {
                    host_cert.push((pattern, id.clone()));
                }
            }
        }
        for domain in &snapshot.domains {
            if let Some(id) = &domain.cert_id {
                host_cert.push((domain.name.clone(), id.clone()));
            }
        }
        *self.cache.write() = Cache {
            generation,
            by_id,
            host_cert,
        };
    }

    fn pick(&self, sni: Option<&str>) -> ParsedCert {
        self.refresh();
        let cache = self.cache.read();
        let sni = sni.unwrap_or("");
        let mut best: Option<(&str, u32)> = None;
        for (pattern, id) in &cache.host_cert {
            if host_matches(pattern, sni) {
                let score = host_specificity(pattern);
                if best.is_none_or(|(_, s)| score > s) {
                    best = Some((id, score));
                }
            }
        }
        if let Some((id, _)) = best
            && let Some(parsed) = cache.by_id.get(id)
        {
            return parsed.clone();
        }
        cache
            .by_id
            .values()
            .next()
            .cloned()
            .unwrap_or_else(|| self.fallback.clone())
    }
}

#[async_trait]
impl TlsAccept for DynamicCert {
    async fn certificate_callback(&self, ssl: &mut SslRef) {
        let sni = ssl.servername(NameType::HOST_NAME).map(str::to_owned);
        let parsed = self.pick(sni.as_deref());
        if let Some(leaf) = parsed.certs.first()
            && let Err(err) = ssl_use_certificate(ssl, leaf)
        {
            warn!(error = %err, "failed to set TLS certificate");
            return;
        }
        for extra in parsed.certs.iter().skip(1) {
            if let Err(err) = pingora::tls::ext::ssl_add_chain_cert(ssl, extra) {
                warn!(error = %err, "failed to add TLS chain certificate");
            }
        }
        if let Err(err) = ssl_use_private_key(ssl, &parsed.key) {
            warn!(error = %err, "failed to set TLS private key");
        }
    }
}

fn make_fallback() -> Result<ParsedCert, String> {
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

fn parse_cert(cert_pem: &str, key_pem: &str) -> Result<ParsedCert, String> {
    let mut certs = Vec::new();
    for block in split_pem(cert_pem, "CERTIFICATE") {
        let cert = X509::from_pem(block.as_bytes()).map_err(|e| e.to_string())?;
        certs.push(cert);
    }
    if certs.is_empty() {
        return Err("certificate pem is empty".into());
    }
    let key = PKey::private_key_from_pem(key_pem.as_bytes()).map_err(|e| e.to_string())?;
    Ok(ParsedCert { certs, key })
}

fn split_pem(pem: &str, label: &str) -> Vec<String> {
    let begin = format!("-----BEGIN {label}-----");
    let end = format!("-----END {label}-----");
    let mut out = Vec::new();
    let mut rest = pem;
    while let Some(start) = rest.find(&begin) {
        let slice = &rest[start..];
        if let Some(stop) = slice.find(&end) {
            out.push(format!("{}{end}\n", &slice[..stop]));
            rest = &slice[stop + end.len()..];
        } else {
            break;
        }
    }
    out
}
