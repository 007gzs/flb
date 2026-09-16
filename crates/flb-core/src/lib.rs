use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub fn new_id() -> String {
    Uuid::new_v4().to_string()
}

pub fn now_rfc3339() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Certificate {
    pub id: String,
    pub name: String,
    pub cert_pem: String,
    pub key_pem: String,
    #[serde(default)]
    pub not_after: Option<String>,
    #[serde(default)]
    pub auto_issued: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DnsProviderKind {
    Aliyun,
    Wanwang,
    Godaddy,
    Tencent,
    Xinnet,
    Cloudflare,
    Amazon,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DnsProvider {
    pub id: String,
    pub name: String,
    pub kind: DnsProviderKind,
    pub access_key: String,
    pub access_secret: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CertMode {
    Manual,
    Acme,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AcmeChallenge {
    Http01,
    Dns01,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Domain {
    pub id: String,
    pub name: String,
    pub mode: CertMode,
    #[serde(default)]
    pub cert_id: Option<String>,
    #[serde(default)]
    pub challenge: Option<AcmeChallenge>,
    #[serde(default)]
    pub dns_provider_id: Option<String>,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub last_error: Option<String>,
    #[serde(default)]
    pub expires_at: Option<String>,
    pub created_at: String,
}

impl Domain {
    pub fn is_wildcard(&self) -> bool {
        self.name.starts_with("*.") || self.name.contains("*")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UpstreamProtocol {
    #[default]
    Http,
    Https,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpstreamServer {
    pub address: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(default = "default_weight")]
    pub weight: u32,
    #[serde(default)]
    pub protocol: Option<UpstreamProtocol>,
    #[serde(default)]
    pub verify_tls: Option<bool>,
}

fn default_weight() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Upstream {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub protocol: UpstreamProtocol,
    #[serde(default)]
    pub verify_tls: bool,
    #[serde(default)]
    pub sni: Option<String>,
    #[serde(default)]
    pub servers: Vec<UpstreamServer>,
}

impl UpstreamServer {
    pub fn protocol_or(&self, fallback: UpstreamProtocol) -> UpstreamProtocol {
        self.protocol.unwrap_or(fallback)
    }

    pub fn verify_tls_or(&self, fallback: bool) -> bool {
        self.verify_tls.unwrap_or(fallback)
    }

    pub fn effective_port(&self, fallback: UpstreamProtocol) -> u16 {
        if let Some(port) = self.port.filter(|p| *p != 0) {
            return port;
        }
        match self.protocol_or(fallback) {
            UpstreamProtocol::Https => 443,
            UpstreamProtocol::Http => 80,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MatchType {
    Prefix,
    Regex,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeaderRewrite {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Route {
    pub id: String,
    #[serde(alias = "prefix")]
    pub pattern: String,
    pub match_type: MatchType,
    #[serde(default)]
    pub rewrite_uri: Option<String>,
    #[serde(default)]
    pub methods: Vec<String>,
    #[serde(default)]
    pub request_headers: Vec<HeaderRewrite>,
    #[serde(default)]
    pub response_headers: Vec<HeaderRewrite>,
    pub upstream_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Host {
    pub id: String,
    pub hostname: String,
    #[serde(default = "default_protocol")]
    pub protocol: String,
    #[serde(default)]
    pub https_enabled: bool,
    #[serde(default)]
    pub cert_id: Option<String>,
    #[serde(default)]
    pub force_https: bool,
    pub default_upstream_id: String,
    #[serde(default)]
    pub routes: Vec<Route>,
}

pub fn parse_hostnames(raw: &str) -> Vec<String> {
    let mut names = Vec::new();
    for part in raw.split(|c: char| matches!(c, ',' | ';' | '\n' | '\r') || c.is_whitespace()) {
        let name = part.trim().to_ascii_lowercase();
        if name.is_empty() {
            continue;
        }
        if !names.iter().any(|existing| existing == &name) {
            names.push(name);
        }
    }
    if names.is_empty() {
        vec![String::new()]
    } else {
        names
    }
}

pub fn join_hostnames(names: &[String]) -> String {
    if names.len() == 1 && names[0].is_empty() {
        String::new()
    } else {
        names.join(", ")
    }
}

fn default_protocol() -> String {
    "http".into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StreamProtocol {
    Tcp,
    Udp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamConfig {
    pub id: String,
    pub name: String,
    pub listen_port: u16,
    pub protocol: StreamProtocol,
    pub target_ip: String,
    pub target_port: u16,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigData {
    #[serde(default)]
    pub certificates: Vec<Certificate>,
    #[serde(default)]
    pub dns_providers: Vec<DnsProvider>,
    #[serde(default)]
    pub domains: Vec<Domain>,
    #[serde(default)]
    pub upstreams: Vec<Upstream>,
    #[serde(default)]
    pub hosts: Vec<Host>,
    #[serde(default)]
    pub streams: Vec<StreamConfig>,
}

impl ConfigData {
    pub fn cert(&self, id: &str) -> Option<&Certificate> {
        self.certificates.iter().find(|c| c.id == id)
    }

    pub fn dns_provider(&self, id: &str) -> Option<&DnsProvider> {
        self.dns_providers.iter().find(|c| c.id == id)
    }

    pub fn domain(&self, id: &str) -> Option<&Domain> {
        self.domains.iter().find(|c| c.id == id)
    }

    pub fn upstream(&self, id: &str) -> Option<&Upstream> {
        self.upstreams.iter().find(|c| c.id == id)
    }

    pub fn host(&self, id: &str) -> Option<&Host> {
        self.hosts.iter().find(|c| c.id == id)
    }

    pub fn stream(&self, id: &str) -> Option<&StreamConfig> {
        self.streams.iter().find(|c| c.id == id)
    }
}
