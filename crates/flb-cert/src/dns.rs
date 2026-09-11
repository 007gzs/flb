use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DnsError {
    #[error("{0}")]
    Message(String),
    #[error("http: {0}")]
    Http(#[from] reqwest::Error),
}

#[derive(Clone)]
pub struct DnsCredentials {
    pub access_key: String,
    pub access_secret: String,
}

pub async fn upsert_txt(
    kind: flb_core::DnsProviderKind,
    creds: &DnsCredentials,
    domain_name: &str,
    value: &str,
) -> Result<TxtHandle, DnsError> {
    let (zone, rr) = split_zone_and_rr(domain_name);
    match kind {
        flb_core::DnsProviderKind::Aliyun | flb_core::DnsProviderKind::Wanwang => {
            aliyun::upsert_txt(creds, &zone, &rr, value).await
        }
        flb_core::DnsProviderKind::Godaddy => godaddy::upsert_txt(creds, &zone, &rr, value).await,
    }
}

pub async fn delete_txt(handle: &TxtHandle) -> Result<(), DnsError> {
    match handle {
        TxtHandle::Aliyun(h) => aliyun::delete_txt(h).await,
        TxtHandle::Godaddy(h) => godaddy::delete_txt(h).await,
    }
}

#[derive(Clone)]
pub enum TxtHandle {
    Aliyun(AliyunHandle),
    Godaddy(GodaddyHandle),
}

#[derive(Clone)]
pub struct AliyunHandle {
    creds: DnsCredentials,
    record_id: String,
}

#[derive(Clone)]
pub struct GodaddyHandle {
    creds: DnsCredentials,
    zone: String,
    rr: String,
}

pub fn challenge_fqdn(domain: &str) -> String {
    let stripped = domain.strip_prefix("*.").unwrap_or(domain);
    format!("_acme-challenge.{stripped}")
}

fn split_zone_and_rr(fqdn: &str) -> (String, String) {
    let fqdn = fqdn.trim_end_matches('.').to_ascii_lowercase();
    let labels: Vec<&str> = fqdn.split('.').collect();
    let zone_labels = root_zone_labels(&labels);
    let zone = labels[labels.len().saturating_sub(zone_labels)..].join(".");
    let rr = if labels.len() > zone_labels {
        labels[..labels.len() - zone_labels].join(".")
    } else {
        "@".into()
    };
    (zone, rr)
}

fn root_zone_labels(labels: &[&str]) -> usize {
    const COMPOUND: &[&str] = &[
        "com.cn", "net.cn", "org.cn", "gov.cn", "ac.cn", "com.hk", "co.uk", "org.uk", "com.tw",
        "co.jp",
    ];
    if labels.len() >= 2 {
        let last_two = format!("{}.{}", labels[labels.len() - 2], labels[labels.len() - 1]);
        if COMPOUND.contains(&last_two.as_str()) {
            return 3.min(labels.len());
        }
    }
    2.min(labels.len())
}

mod aliyun {
    use super::{AliyunHandle, DnsCredentials, DnsError, TxtHandle, percent_encode};
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD;
    use chrono::Utc;
    use hmac::{Hmac, Mac};
    use serde::Deserialize;
    use sha1::Sha1;
    use std::collections::BTreeMap;
    use uuid::Uuid;

    type HmacSha1 = Hmac<Sha1>;

    #[derive(Deserialize)]
    struct AddResp {
        #[serde(rename = "RecordId")]
        record_id: Option<String>,
        #[serde(rename = "Message")]
        message: Option<String>,
        #[serde(rename = "Code")]
        code: Option<String>,
    }

    pub async fn upsert_txt(
        creds: &DnsCredentials,
        zone: &str,
        rr: &str,
        value: &str,
    ) -> Result<TxtHandle, DnsError> {
        let mut params = BTreeMap::new();
        params.insert("Action", "AddDomainRecord".into());
        params.insert("DomainName", zone.to_string());
        params.insert("RR", rr.to_string());
        params.insert("Type", "TXT".into());
        params.insert("Value", value.to_string());
        params.insert("TTL", "600".into());
        let body: AddResp = call(creds, params).await?;
        if let Some(id) = body.record_id {
            return Ok(TxtHandle::Aliyun(AliyunHandle {
                creds: creds.clone(),
                record_id: id,
            }));
        }
        Err(DnsError::Message(
            body.message
                .or(body.code)
                .unwrap_or_else(|| "阿里云/万网添加解析失败".into()),
        ))
    }

    pub async fn delete_txt(handle: &AliyunHandle) -> Result<(), DnsError> {
        let mut params = BTreeMap::new();
        params.insert("Action", "DeleteDomainRecord".into());
        params.insert("RecordId", handle.record_id.clone());
        let _: serde_json::Value = call(&handle.creds, params).await?;
        Ok(())
    }

    async fn call<T: serde::de::DeserializeOwned>(
        creds: &DnsCredentials,
        mut params: BTreeMap<&str, String>,
    ) -> Result<T, DnsError> {
        params.insert("Format", "JSON".into());
        params.insert("Version", "2015-01-09".into());
        params.insert("AccessKeyId", creds.access_key.clone());
        params.insert("SignatureMethod", "HMAC-SHA1".into());
        params.insert(
            "Timestamp",
            Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        );
        params.insert("SignatureVersion", "1.0".into());
        params.insert("SignatureNonce", Uuid::new_v4().to_string());

        let canonical = params
            .iter()
            .map(|(k, v)| format!("{}={}", percent_encode(k), percent_encode(v)))
            .collect::<Vec<_>>()
            .join("&");
        let string_to_sign = format!("GET&%2F&{}", percent_encode(&canonical));
        let mut mac = HmacSha1::new_from_slice(format!("{}&", creds.access_secret).as_bytes())
            .map_err(|e| DnsError::Message(e.to_string()))?;
        mac.update(string_to_sign.as_bytes());
        let signature = STANDARD.encode(mac.finalize().into_bytes());
        let url = format!(
            "https://alidns.aliyuncs.com/?{}&Signature={}",
            canonical,
            percent_encode(&signature)
        );
        let resp = reqwest::Client::new()
            .get(url)
            .timeout(std::time::Duration::from_secs(20))
            .send()
            .await?
            .error_for_status()?
            .json::<T>()
            .await?;
        Ok(resp)
    }
}

mod godaddy {
    use super::{DnsCredentials, DnsError, GodaddyHandle, TxtHandle};

    pub async fn upsert_txt(
        creds: &DnsCredentials,
        zone: &str,
        rr: &str,
        value: &str,
    ) -> Result<TxtHandle, DnsError> {
        let url = format!(
            "https://api.godaddy.com/v1/domains/{}/records/TXT/{}",
            zone, rr
        );
        let body = serde_json::json!([{ "data": value, "ttl": 600 }]);
        reqwest::Client::new()
            .put(url)
            .header(
                "Authorization",
                format!("sso-key {}:{}", creds.access_key, creds.access_secret),
            )
            .header("Content-Type", "application/json")
            .json(&body)
            .timeout(std::time::Duration::from_secs(20))
            .send()
            .await?
            .error_for_status()?;
        Ok(TxtHandle::Godaddy(GodaddyHandle {
            creds: creds.clone(),
            zone: zone.to_string(),
            rr: rr.to_string(),
        }))
    }

    pub async fn delete_txt(handle: &GodaddyHandle) -> Result<(), DnsError> {
        let url = format!(
            "https://api.godaddy.com/v1/domains/{}/records/TXT/{}",
            handle.zone, handle.rr
        );
        reqwest::Client::new()
            .delete(url)
            .header(
                "Authorization",
                format!(
                    "sso-key {}:{}",
                    handle.creds.access_key, handle.creds.access_secret
                ),
            )
            .timeout(std::time::Duration::from_secs(20))
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }
}

fn percent_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

pub async fn wait_propagation() {
    tokio::time::sleep(Duration::from_secs(15)).await;
}
