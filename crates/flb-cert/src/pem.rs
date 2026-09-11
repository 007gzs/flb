use chrono::{TimeZone, Utc};
use thiserror::Error;
use x509_parser::prelude::*;

#[derive(Debug, Error)]
pub enum PemError {
    #[error("{0}")]
    Message(String),
}

pub fn cert_not_after(cert_pem: &str) -> Result<String, PemError> {
    let (_, pem) = parse_x509_pem(cert_pem.as_bytes())
        .map_err(|e| PemError::Message(format!("无法解析证书 PEM: {e}")))?;
    let (_, cert) = X509Certificate::from_der(&pem.contents)
        .map_err(|e| PemError::Message(format!("无法解析证书: {e}")))?;
    let ts = cert.validity().not_after.timestamp();
    let dt = Utc
        .timestamp_opt(ts, 0)
        .single()
        .ok_or_else(|| PemError::Message("证书过期时间无效".into()))?;
    Ok(dt.to_rfc3339())
}

pub fn expires_within_days(not_after: &str, days: i64) -> bool {
    let Ok(dt) = chrono::DateTime::parse_from_rfc3339(not_after) else {
        return true;
    };
    dt.signed_duration_since(chrono::Utc::now()).num_days() <= days
}

pub fn looks_like_pem(label: &str, value: &str) -> bool {
    value.contains("BEGIN") && value.contains(label)
}
