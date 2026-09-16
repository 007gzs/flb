use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DnsError {
    #[error("{0}")]
    Message(String),
    #[error("http: {0}")]
    Http(#[from] reqwest::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
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
        flb_core::DnsProviderKind::Tencent => tencent::upsert_txt(creds, &zone, &rr, value).await,
        flb_core::DnsProviderKind::Xinnet => xinnet::upsert_txt(creds, &zone, &rr, value).await,
        flb_core::DnsProviderKind::Cloudflare => {
            cloudflare::upsert_txt(creds, &zone, &rr, value).await
        }
        flb_core::DnsProviderKind::Amazon => amazon::upsert_txt(creds, &zone, &rr, value).await,
    }
}

pub async fn delete_txt(handle: &TxtHandle) -> Result<(), DnsError> {
    match handle {
        TxtHandle::Aliyun(h) => aliyun::delete_txt(h).await,
        TxtHandle::Godaddy(h) => godaddy::delete_txt(h).await,
        TxtHandle::Tencent(h) => tencent::delete_txt(h).await,
        TxtHandle::Xinnet(h) => xinnet::delete_txt(h).await,
        TxtHandle::Cloudflare(h) => cloudflare::delete_txt(h).await,
        TxtHandle::Amazon(h) => amazon::delete_txt(h).await,
    }
}

#[derive(Clone)]
pub enum TxtHandle {
    Aliyun(AliyunHandle),
    Godaddy(GodaddyHandle),
    Tencent(TencentHandle),
    Xinnet(XinnetHandle),
    Cloudflare(CloudflareHandle),
    Amazon(AmazonHandle),
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

#[derive(Clone)]
pub struct TencentHandle {
    creds: DnsCredentials,
    domain: String,
    record_id: u64,
}

#[derive(Clone)]
pub struct XinnetHandle {
    creds: DnsCredentials,
    domain: String,
    record_id: String,
}

#[derive(Clone)]
pub struct CloudflareHandle {
    creds: DnsCredentials,
    zone_id: String,
    record_id: String,
}

#[derive(Clone)]
pub struct AmazonHandle {
    creds: DnsCredentials,
    hosted_zone_id: String,
    name: String,
    value: String,
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

mod tencent {
    use super::{DnsCredentials, DnsError, TencentHandle, TxtHandle, hex_encode, hmac_sha256};
    use chrono::Utc;
    use serde::Deserialize;
    use serde_json::{Value, json};
    use sha2::{Digest, Sha256};

    const HOST: &str = "dnspod.tencentcloudapi.com";
    const SERVICE: &str = "dnspod";
    const VERSION: &str = "2021-03-23";
    const SIGNED_HEADERS: &str = "content-type;host;x-tc-action";

    #[derive(Deserialize)]
    struct Envelope {
        #[serde(rename = "Response")]
        response: ResponseBody,
    }

    #[derive(Deserialize)]
    struct ResponseBody {
        #[serde(rename = "Error")]
        error: Option<ApiError>,
        #[serde(rename = "RecordId")]
        record_id: Option<u64>,
    }

    #[derive(Deserialize)]
    struct ApiError {
        #[serde(rename = "Code")]
        code: String,
        #[serde(rename = "Message")]
        message: String,
    }

    pub async fn upsert_txt(
        creds: &DnsCredentials,
        zone: &str,
        rr: &str,
        value: &str,
    ) -> Result<TxtHandle, DnsError> {
        let payload = json!({
            "Domain": zone,
            "SubDomain": rr,
            "RecordType": "TXT",
            "RecordLine": "默认",
            "Value": value,
            "TTL": 600
        });
        let body: Envelope = call(creds, "CreateRecord", &payload).await?;
        if let Some(err) = body.response.error {
            return Err(DnsError::Message(format!(
                "腾讯云 {} {}",
                err.code, err.message
            )));
        }
        let record_id = body
            .response
            .record_id
            .ok_or_else(|| DnsError::Message("腾讯云未返回记录 ID".into()))?;
        Ok(TxtHandle::Tencent(TencentHandle {
            creds: creds.clone(),
            domain: zone.to_string(),
            record_id,
        }))
    }

    pub async fn delete_txt(handle: &TencentHandle) -> Result<(), DnsError> {
        let payload = json!({
            "Domain": handle.domain,
            "RecordId": handle.record_id
        });
        let body: Envelope = call(&handle.creds, "DeleteRecord", &payload).await?;
        if let Some(err) = body.response.error {
            return Err(DnsError::Message(format!(
                "腾讯云 {} {}",
                err.code, err.message
            )));
        }
        Ok(())
    }

    async fn call(
        creds: &DnsCredentials,
        action: &str,
        payload: &Value,
    ) -> Result<Envelope, DnsError> {
        let payload_str = serde_json::to_string(payload)?;
        let timestamp = Utc::now().timestamp();
        let date = Utc::now().format("%Y-%m-%d").to_string();
        let hashed_payload = sha256_hex(payload_str.as_bytes());
        let canonical_headers = format!(
            "content-type:application/json; charset=utf-8\nhost:{HOST}\nx-tc-action:{}\n",
            action.to_ascii_lowercase()
        );
        let canonical_request =
            format!("POST\n/\n\n{canonical_headers}\n{SIGNED_HEADERS}\n{hashed_payload}");
        let credential_scope = format!("{date}/{SERVICE}/tc3_request");
        let string_to_sign = format!(
            "TC3-HMAC-SHA256\n{timestamp}\n{credential_scope}\n{}",
            sha256_hex(canonical_request.as_bytes())
        );
        let secret_date = hmac_sha256(
            format!("TC3{}", creds.access_secret).as_bytes(),
            date.as_bytes(),
        )?;
        let secret_service = hmac_sha256(&secret_date, SERVICE.as_bytes())?;
        let secret_signing = hmac_sha256(&secret_service, b"tc3_request")?;
        let signature = hex_encode(&hmac_sha256(&secret_signing, string_to_sign.as_bytes())?);
        let authorization = format!(
            "TC3-HMAC-SHA256 Credential={}/{credential_scope}, SignedHeaders={SIGNED_HEADERS}, Signature={signature}",
            creds.access_key
        );
        let resp = reqwest::Client::new()
            .post(format!("https://{HOST}/"))
            .header("Content-Type", "application/json; charset=utf-8")
            .header("Host", HOST)
            .header("X-TC-Action", action)
            .header("X-TC-Timestamp", timestamp.to_string())
            .header("X-TC-Version", VERSION)
            .header("Authorization", authorization)
            .body(payload_str)
            .timeout(std::time::Duration::from_secs(20))
            .send()
            .await?
            .error_for_status()?
            .json::<Envelope>()
            .await?;
        Ok(resp)
    }

    fn sha256_hex(data: &[u8]) -> String {
        hex_encode(&Sha256::digest(data))
    }
}

mod xinnet {
    use super::{DnsCredentials, DnsError, TxtHandle, XinnetHandle, hex_encode, hmac_sha256};
    use chrono::Utc;
    use serde_json::{Value, json};

    const BASE: &str = "https://apiv2.xinnet.com";

    pub async fn upsert_txt(
        creds: &DnsCredentials,
        zone: &str,
        rr: &str,
        value: &str,
    ) -> Result<TxtHandle, DnsError> {
        let queried = post(creds, "/api/dns/queryDomain", json!({ "domainName": zone })).await?;
        let domain_name = queried
            .pointer("/data/name")
            .and_then(Value::as_str)
            .unwrap_or(zone)
            .to_string();
        let record_name = if rr == "@" {
            domain_name.clone()
        } else {
            format!("{rr}.{domain_name}")
        };
        let created = post(
            creds,
            "/api/dns/create",
            json!({
                "domainName": domain_name,
                "recordName": record_name,
                "type": "TXT",
                "value": value,
                "line": "默认",
                "ttl": 600,
                "mx": 0,
                "status": 0
            }),
        )
        .await?;
        let mut record_id = extract_id(&created);
        if record_id.is_none() {
            let unique = post(
                creds,
                "/api/dns/queryRecordsUnique",
                json!({
                    "domainName": domain_name,
                    "recordName": record_name,
                    "type": "TXT",
                    "value": value,
                    "line": "默认"
                }),
            )
            .await?;
            record_id = extract_id(&unique);
        }
        let record_id = record_id.ok_or_else(|| {
            DnsError::Message(
                created
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("新网添加解析失败")
                    .to_string(),
            )
        })?;
        Ok(TxtHandle::Xinnet(XinnetHandle {
            creds: creds.clone(),
            domain: domain_name,
            record_id,
        }))
    }

    pub async fn delete_txt(handle: &XinnetHandle) -> Result<(), DnsError> {
        let _ = post(
            &handle.creds,
            "/api/dns/delete",
            json!({
                "recordId": handle.record_id,
                "domainName": handle.domain
            }),
        )
        .await?;
        Ok(())
    }

    fn extract_id(value: &Value) -> Option<String> {
        let data = value.get("data")?;
        if let Some(id) = data.get("recordId").or_else(|| data.get("id")) {
            return match id {
                Value::String(s) if !s.is_empty() => Some(s.clone()),
                Value::Number(n) => Some(n.to_string()),
                _ => None,
            };
        }
        None
    }

    async fn post(creds: &DnsCredentials, path: &str, payload: Value) -> Result<Value, DnsError> {
        let body = serde_json::to_string(&payload)?;
        let timestamp = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
        let url_path = format!("{path}/");
        let string_to_sign = format!("HMAC-SHA256\n{timestamp}\nPOST\n{url_path}\n{body}");
        let signature = hex_encode(&hmac_sha256(
            creds.access_secret.as_bytes(),
            string_to_sign.as_bytes(),
        )?);
        let authorization = format!(
            "HMAC-SHA256 Access={}, Signature={signature}",
            creds.access_key
        );
        let resp = reqwest::Client::new()
            .post(format!("{BASE}{path}"))
            .header("timestamp", timestamp)
            .header("authorization", authorization)
            .header("Content-Type", "application/json")
            .body(body)
            .timeout(std::time::Duration::from_secs(20))
            .send()
            .await?
            .error_for_status()?
            .json::<Value>()
            .await?;
        let code = resp.get("code").and_then(|c| {
            c.as_str()
                .map(str::to_string)
                .or_else(|| c.as_i64().map(|n| n.to_string()))
        });
        if matches!(code.as_deref(), Some("0") | Some("200") | None) {
            return Ok(resp);
        }
        Err(DnsError::Message(
            resp.get("message")
                .and_then(Value::as_str)
                .unwrap_or("新网接口调用失败")
                .to_string(),
        ))
    }
}

mod cloudflare {
    use super::{CloudflareHandle, DnsCredentials, DnsError, TxtHandle};
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct CfResp<T> {
        success: bool,
        result: Option<T>,
        errors: Option<Vec<CfError>>,
    }

    #[derive(Deserialize)]
    struct CfError {
        message: Option<String>,
    }

    #[derive(Deserialize)]
    struct Zone {
        id: String,
    }

    #[derive(Deserialize)]
    struct Record {
        id: String,
    }

    pub async fn upsert_txt(
        creds: &DnsCredentials,
        zone: &str,
        rr: &str,
        value: &str,
    ) -> Result<TxtHandle, DnsError> {
        let zones: CfResp<Vec<Zone>> = get(
            creds,
            &format!("https://api.cloudflare.com/client/v4/zones?name={zone}"),
        )
        .await?;
        ensure_ok(&zones)?;
        let zone_id = zones
            .result
            .into_iter()
            .flatten()
            .next()
            .map(|z| z.id)
            .ok_or_else(|| DnsError::Message(format!("Cloudflare 未找到域名 {zone}")))?;
        let name = if rr == "@" {
            zone.to_string()
        } else {
            format!("{rr}.{zone}")
        };
        let created: CfResp<Record> = post(
            creds,
            &format!("https://api.cloudflare.com/client/v4/zones/{zone_id}/dns_records"),
            serde_json::json!({
                "type": "TXT",
                "name": name,
                "content": value,
                "ttl": 60
            }),
        )
        .await?;
        ensure_ok(&created)?;
        let record_id = created
            .result
            .map(|r| r.id)
            .ok_or_else(|| DnsError::Message("Cloudflare 未返回记录 ID".into()))?;
        Ok(TxtHandle::Cloudflare(CloudflareHandle {
            creds: creds.clone(),
            zone_id,
            record_id,
        }))
    }

    pub async fn delete_txt(handle: &CloudflareHandle) -> Result<(), DnsError> {
        let url = format!(
            "https://api.cloudflare.com/client/v4/zones/{}/dns_records/{}",
            handle.zone_id, handle.record_id
        );
        let resp: CfResp<serde_json::Value> = delete(&handle.creds, &url).await?;
        ensure_ok(&resp)
    }

    fn ensure_ok<T>(resp: &CfResp<T>) -> Result<(), DnsError> {
        if resp.success {
            return Ok(());
        }
        let msg = resp
            .errors
            .as_ref()
            .and_then(|e| e.first())
            .and_then(|e| e.message.clone())
            .unwrap_or_else(|| "Cloudflare 接口调用失败".into());
        Err(DnsError::Message(msg))
    }

    fn apply_auth(
        mut req: reqwest::RequestBuilder,
        creds: &DnsCredentials,
    ) -> reqwest::RequestBuilder {
        if creds.access_secret.trim().is_empty() {
            req = req.bearer_auth(&creds.access_key);
        } else {
            req = req
                .header("X-Auth-Email", &creds.access_key)
                .header("X-Auth-Key", &creds.access_secret);
        }
        req
    }

    async fn get<T: serde::de::DeserializeOwned>(
        creds: &DnsCredentials,
        url: &str,
    ) -> Result<T, DnsError> {
        let resp = apply_auth(reqwest::Client::new().get(url), creds)
            .timeout(std::time::Duration::from_secs(20))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(resp)
    }

    async fn post<T: serde::de::DeserializeOwned>(
        creds: &DnsCredentials,
        url: &str,
        body: serde_json::Value,
    ) -> Result<T, DnsError> {
        let resp = apply_auth(reqwest::Client::new().post(url), creds)
            .json(&body)
            .timeout(std::time::Duration::from_secs(20))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(resp)
    }

    async fn delete<T: serde::de::DeserializeOwned>(
        creds: &DnsCredentials,
        url: &str,
    ) -> Result<T, DnsError> {
        let resp = apply_auth(reqwest::Client::new().delete(url), creds)
            .timeout(std::time::Duration::from_secs(20))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(resp)
    }
}

mod amazon {
    use super::{AmazonHandle, DnsCredentials, DnsError, TxtHandle, hex_encode, hmac_sha256};
    use chrono::Utc;
    use sha2::{Digest, Sha256};

    const HOST: &str = "route53.amazonaws.com";
    const SERVICE: &str = "route53";
    const REGION: &str = "us-east-1";

    pub async fn upsert_txt(
        creds: &DnsCredentials,
        zone: &str,
        rr: &str,
        value: &str,
    ) -> Result<TxtHandle, DnsError> {
        let name = record_name(zone, rr);
        let hosted_zone_id = find_zone(creds, zone).await?;
        change(creds, &hosted_zone_id, "UPSERT", &name, value).await?;
        Ok(TxtHandle::Amazon(AmazonHandle {
            creds: creds.clone(),
            hosted_zone_id,
            name,
            value: value.to_string(),
        }))
    }

    pub async fn delete_txt(handle: &AmazonHandle) -> Result<(), DnsError> {
        change(
            &handle.creds,
            &handle.hosted_zone_id,
            "DELETE",
            &handle.name,
            &handle.value,
        )
        .await
    }

    fn record_name(zone: &str, rr: &str) -> String {
        if rr == "@" {
            format!("{zone}.")
        } else {
            format!("{rr}.{zone}.")
        }
    }

    async fn find_zone(creds: &DnsCredentials, zone: &str) -> Result<String, DnsError> {
        let xml = signed(
            creds,
            "GET",
            "/2013-04-01/hostedzone",
            "",
            b"",
            "application/xml",
        )
        .await?;
        let wanted = format!("{}.", zone.trim_end_matches('.').to_ascii_lowercase());
        let mut best: Option<(usize, String)> = None;
        for (id, name) in parse_zones(&xml) {
            let name = name.to_ascii_lowercase();
            if (wanted == name || wanted.ends_with(&format!(".{name}")))
                && best.as_ref().is_none_or(|(len, _)| name.len() > *len)
            {
                best = Some((name.len(), id));
            }
        }
        best.map(|(_, id)| id)
            .ok_or_else(|| DnsError::Message(format!("Route53 未找到托管区 {zone}")))
    }

    fn parse_zones(xml: &str) -> Vec<(String, String)> {
        let mut out = Vec::new();
        let mut rest = xml;
        while let Some(start) = rest.find("<HostedZone>") {
            let Some(end) = rest[start..].find("</HostedZone>") else {
                break;
            };
            let block = &rest[start..start + end];
            if let (Some(id), Some(name)) = (xml_text(block, "Id"), xml_text(block, "Name")) {
                let id = id.rsplit('/').next().unwrap_or(&id).to_string();
                out.push((id, name));
            }
            rest = &rest[start + end + 13..];
        }
        out
    }

    fn xml_text(block: &str, tag: &str) -> Option<String> {
        let open = format!("<{tag}>");
        let close = format!("</{tag}>");
        let start = block.find(&open)? + open.len();
        let end = block[start..].find(&close)? + start;
        Some(block[start..end].trim().to_string())
    }

    async fn change(
        creds: &DnsCredentials,
        zone_id: &str,
        action: &str,
        name: &str,
        value: &str,
    ) -> Result<(), DnsError> {
        let quoted = format!("\"{}\"", xml_escape(value));
        let body = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<ChangeResourceRecordSetsRequest xmlns="https://route53.amazonaws.com/doc/2013-04-01/">
  <ChangeBatch>
    <Changes>
      <Change>
        <Action>{action}</Action>
        <ResourceRecordSet>
          <Name>{name}</Name>
          <Type>TXT</Type>
          <TTL>60</TTL>
          <ResourceRecords>
            <ResourceRecord>
              <Value>{quoted}</Value>
            </ResourceRecord>
          </ResourceRecords>
        </ResourceRecordSet>
      </Change>
    </Changes>
  </ChangeBatch>
</ChangeResourceRecordSetsRequest>"#
        );
        let path = format!("/2013-04-01/hostedzone/{zone_id}/rrset");
        let xml = signed(creds, "POST", &path, "", body.as_bytes(), "application/xml").await?;
        if xml.contains("<Error>") || xml.contains("<Code>") && xml.contains("Error") {
            let msg = xml_text(&xml, "Message").unwrap_or_else(|| "Route53 接口调用失败".into());
            return Err(DnsError::Message(msg));
        }
        Ok(())
    }

    fn xml_escape(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    }

    async fn signed(
        creds: &DnsCredentials,
        method: &str,
        path: &str,
        query: &str,
        payload: &[u8],
        content_type: &str,
    ) -> Result<String, DnsError> {
        let now = Utc::now();
        let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
        let date = now.format("%Y%m%d").to_string();
        let payload_hash = hex_encode(&Sha256::digest(payload));
        let canonical_headers = if method == "POST" {
            format!(
                "content-type:{content_type}\nhost:{HOST}\nx-amz-content-sha256:{payload_hash}\nx-amz-date:{amz_date}\n"
            )
        } else {
            format!("host:{HOST}\nx-amz-content-sha256:{payload_hash}\nx-amz-date:{amz_date}\n")
        };
        let signed_headers = if method == "POST" {
            "content-type;host;x-amz-content-sha256;x-amz-date"
        } else {
            "host;x-amz-content-sha256;x-amz-date"
        };
        let canonical_request = format!(
            "{method}\n{path}\n{query}\n{canonical_headers}\n{signed_headers}\n{payload_hash}"
        );
        let credential_scope = format!("{date}/{REGION}/{SERVICE}/aws4_request");
        let string_to_sign = format!(
            "AWS4-HMAC-SHA256\n{amz_date}\n{credential_scope}\n{}",
            hex_encode(&Sha256::digest(canonical_request.as_bytes()))
        );
        let k_date = hmac_sha256(
            format!("AWS4{}", creds.access_secret).as_bytes(),
            date.as_bytes(),
        )?;
        let k_region = hmac_sha256(&k_date, REGION.as_bytes())?;
        let k_service = hmac_sha256(&k_region, SERVICE.as_bytes())?;
        let k_signing = hmac_sha256(&k_service, b"aws4_request")?;
        let signature = hex_encode(&hmac_sha256(&k_signing, string_to_sign.as_bytes())?);
        let authorization = format!(
            "AWS4-HMAC-SHA256 Credential={}/{credential_scope}, SignedHeaders={signed_headers}, Signature={signature}",
            creds.access_key
        );
        let http_method = if method == "POST" {
            reqwest::Method::POST
        } else {
            reqwest::Method::GET
        };
        let url = if query.is_empty() {
            format!("https://{HOST}{path}")
        } else {
            format!("https://{HOST}{path}?{query}")
        };
        let mut req = reqwest::Client::new()
            .request(http_method, url)
            .header("Host", HOST)
            .header("X-Amz-Date", &amz_date)
            .header("X-Amz-Content-Sha256", &payload_hash)
            .header("Authorization", authorization);
        if method == "POST" {
            req = req
                .header("Content-Type", content_type)
                .body(payload.to_vec());
        }
        let resp = req
            .timeout(std::time::Duration::from_secs(20))
            .send()
            .await?;
        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() {
            let msg = xml_text(&text, "Message").unwrap_or(text);
            return Err(DnsError::Message(msg));
        }
        Ok(text)
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

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn hmac_sha256(key: &[u8], message: &[u8]) -> Result<Vec<u8>, DnsError> {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    let mut mac =
        Hmac::<Sha256>::new_from_slice(key).map_err(|e| DnsError::Message(e.to_string()))?;
    mac.update(message);
    Ok(mac.finalize().into_bytes().to_vec())
}

pub async fn wait_propagation() {
    tokio::time::sleep(Duration::from_secs(15)).await;
}
