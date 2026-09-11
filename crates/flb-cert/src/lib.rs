pub mod dns;
pub mod pem;

use crate::dns::{DnsCredentials, TxtHandle};
use crate::pem::cert_not_after;
use dashmap::DashMap;
use flb_core::{AcmeChallenge, CertMode, Certificate, Domain, new_id, now_rfc3339};
use flb_store::Store;
use instant_acme::{
    Account, AccountCredentials, ChallengeType, Identifier, LetsEncrypt, NewAccount, NewOrder,
    OrderStatus,
};
use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::Mutex;
use tracing::{info, warn};

#[derive(Debug, Error)]
pub enum CertError {
    #[error("{0}")]
    Message(String),
    #[error("acme: {0}")]
    Acme(#[from] instant_acme::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("store: {0}")]
    Store(#[from] flb_store::StoreError),
    #[error("dns: {0}")]
    Dns(#[from] dns::DnsError),
    #[error("pem: {0}")]
    Pem(#[from] pem::PemError),
    #[error("rcgen: {0}")]
    Rcgen(#[from] rcgen::Error),
}

pub struct AcmeService {
    directory_url: String,
    account_path: PathBuf,
    pub http01: Arc<DashMap<String, String>>,
    account: Mutex<Option<Account>>,
}

impl AcmeService {
    pub fn new(account_path: PathBuf, staging: bool) -> Self {
        let directory_url = if staging {
            LetsEncrypt::Staging.url().to_string()
        } else {
            LetsEncrypt::Production.url().to_string()
        };
        Self {
            directory_url,
            account_path,
            http01: Arc::new(DashMap::new()),
            account: Mutex::new(None),
        }
    }

    async fn account(&self) -> Result<Account, CertError> {
        let mut guard = self.account.lock().await;
        if let Some(account) = guard.as_ref() {
            return Ok(account.clone());
        }
        if self.account_path.exists() {
            let raw = std::fs::read_to_string(&self.account_path)?;
            let credentials: AccountCredentials = serde_json::from_str(&raw)?;
            let account = Account::from_credentials(credentials).await?;
            *guard = Some(account.clone());
            return Ok(account);
        }
        if let Some(parent) = self.account_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let (account, credentials) = Account::create(
            &NewAccount {
                contact: &[],
                terms_of_service_agreed: true,
                only_return_existing: false,
            },
            &self.directory_url,
            None,
        )
        .await?;
        std::fs::write(&self.account_path, serde_json::to_vec_pretty(&credentials)?)?;
        *guard = Some(account.clone());
        Ok(account)
    }

    pub async fn issue_or_renew(
        &self,
        store: &Store,
        domain_id: &str,
    ) -> Result<Domain, CertError> {
        let snapshot = store.snapshot();
        let mut domain = snapshot
            .domain(domain_id)
            .cloned()
            .ok_or_else(|| CertError::Message("域名不存在".into()))?;
        if domain.mode != CertMode::Acme {
            return Err(CertError::Message("该域名未启用自动证书".into()));
        }
        let challenge = domain
            .challenge
            .ok_or_else(|| CertError::Message("未选择验证方式".into()))?;
        if domain.is_wildcard() && challenge == AcmeChallenge::Http01 {
            return Err(CertError::Message("泛域名不支持 HTTP 验证".into()));
        }

        domain.status = "issuing".into();
        domain.last_error = None;
        store.upsert_domain(domain.clone())?;

        match self.issue_inner(store, &domain, challenge).await {
            Ok((cert_pem, key_pem, not_after)) => {
                let cert_id = domain.cert_id.clone().unwrap_or_else(new_id);
                let cert = Certificate {
                    id: cert_id.clone(),
                    name: format!("{} (Let's Encrypt)", domain.name),
                    cert_pem,
                    key_pem,
                    not_after: Some(not_after.clone()),
                    auto_issued: true,
                    created_at: now_rfc3339(),
                };
                store.upsert_certificate(cert)?;
                domain.cert_id = Some(cert_id);
                domain.status = "issued".into();
                domain.expires_at = Some(not_after);
                domain.last_error = None;
                store.upsert_domain(domain.clone())?;
                info!(domain = %domain.name, "certificate issued");
                Ok(domain)
            }
            Err(err) => {
                domain.status = "failed".into();
                domain.last_error = Some(err.to_string());
                let _ = store.upsert_domain(domain.clone());
                Err(err)
            }
        }
    }

    async fn issue_inner(
        &self,
        store: &Store,
        domain: &Domain,
        challenge_kind: AcmeChallenge,
    ) -> Result<(String, String, String), CertError> {
        let account = self.account().await?;
        let identifiers = [Identifier::Dns(domain.name.clone())];
        let mut order = account
            .new_order(&NewOrder {
                identifiers: &identifiers,
            })
            .await?;

        let authorizations = order.authorizations().await?;
        let mut dns_handles: Vec<TxtHandle> = Vec::new();
        let mut http_tokens: Vec<String> = Vec::new();

        let wanted = match challenge_kind {
            AcmeChallenge::Http01 => ChallengeType::Http01,
            AcmeChallenge::Dns01 => ChallengeType::Dns01,
        };

        for authz in &authorizations {
            let challenge = authz
                .challenges
                .iter()
                .find(|c| c.r#type == wanted)
                .ok_or_else(|| CertError::Message("ACME 未提供所选验证方式".into()))?;
            let key_auth = order.key_authorization(challenge);
            match challenge_kind {
                AcmeChallenge::Http01 => {
                    self.http01
                        .insert(challenge.token.clone(), key_auth.as_str().to_string());
                    http_tokens.push(challenge.token.clone());
                }
                AcmeChallenge::Dns01 => {
                    let provider_id = domain
                        .dns_provider_id
                        .as_deref()
                        .ok_or_else(|| CertError::Message("DNS 验证需要选择 DNS 提供商".into()))?;
                    let snapshot = store.snapshot();
                    let provider = snapshot
                        .dns_provider(provider_id)
                        .cloned()
                        .ok_or_else(|| CertError::Message("DNS 提供商不存在".into()))?;
                    let fqdn = dns::challenge_fqdn(&domain.name);
                    let handle = dns::upsert_txt(
                        provider.kind,
                        &DnsCredentials {
                            access_key: provider.access_key,
                            access_secret: provider.access_secret,
                        },
                        &fqdn,
                        &key_auth.dns_value(),
                    )
                    .await?;
                    dns_handles.push(handle);
                }
            }
            order.set_challenge_ready(&challenge.url).await?;
        }

        if challenge_kind == AcmeChallenge::Dns01 {
            dns::wait_propagation().await;
        }

        let result = self.finalize_order(&mut order, &domain.name).await;
        for token in http_tokens {
            self.http01.remove(&token);
        }
        for handle in dns_handles {
            if let Err(err) = dns::delete_txt(&handle).await {
                warn!(error = %err, "failed to clean ACME DNS record");
            }
        }
        result
    }

    async fn finalize_order(
        &self,
        order: &mut instant_acme::Order,
        domain: &str,
    ) -> Result<(String, String, String), CertError> {
        let mut tries = 0;
        loop {
            let state = order.refresh().await?;
            match state.status {
                OrderStatus::Ready => break,
                OrderStatus::Invalid => {
                    return Err(CertError::Message("ACME 订单无效，验证失败".into()));
                }
                OrderStatus::Pending | OrderStatus::Processing => {
                    tries += 1;
                    if tries > 30 {
                        return Err(CertError::Message("等待 ACME 验证超时".into()));
                    }
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
                _ => break,
            }
        }

        let names = vec![domain.to_string()];
        let mut params = CertificateParams::new(names)?;
        params.distinguished_name = DistinguishedName::new();
        params
            .distinguished_name
            .push(DnType::CommonName, domain.to_string());
        let key_pair = KeyPair::generate()?;
        let csr = params.serialize_request(&key_pair)?;
        order.finalize(csr.der().as_ref()).await?;

        let mut tries = 0;
        let cert_pem = loop {
            if let Some(pem) = order.certificate().await? {
                break pem;
            }
            tries += 1;
            if tries > 20 {
                return Err(CertError::Message("等待签发证书超时".into()));
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        };
        let key_pem = key_pair.serialize_pem();
        let not_after = cert_not_after(&cert_pem)?;
        Ok((cert_pem, key_pem, not_after))
    }

    pub async fn renew_due(&self, store: &Store) {
        let snapshot = store.snapshot();
        for domain in snapshot.domains {
            if domain.mode != CertMode::Acme {
                continue;
            }
            let due = domain
                .expires_at
                .as_deref()
                .map(|exp| pem::expires_within_days(exp, 30))
                .unwrap_or(true);
            if !due {
                continue;
            }
            info!(domain = %domain.name, "attempting scheduled renewal");
            if let Err(err) = self.issue_or_renew(store, &domain.id).await {
                warn!(domain = %domain.name, error = %err, "renewal failed");
            }
        }
    }
}

pub fn parse_uploaded_not_after(cert_pem: &str) -> Option<String> {
    cert_not_after(cert_pem).ok()
}

pub fn validate_manual_pem(cert_pem: &str, key_pem: &str) -> Result<Option<String>, CertError> {
    if !pem::looks_like_pem("CERTIFICATE", cert_pem) {
        return Err(CertError::Message("公钥必须是 PEM 证书".into()));
    }
    if !pem::looks_like_pem("PRIVATE KEY", key_pem)
        && !pem::looks_like_pem("RSA PRIVATE KEY", key_pem)
    {
        return Err(CertError::Message("私钥必须是 PEM 格式".into()));
    }
    Ok(parse_uploaded_not_after(cert_pem))
}

#[cfg(test)]
mod tests {
    use super::dns::challenge_fqdn;
    use super::pem::looks_like_pem;

    #[test]
    fn challenge_name() {
        assert_eq!(challenge_fqdn("example.com"), "_acme-challenge.example.com");
        assert_eq!(
            challenge_fqdn("*.example.com"),
            "_acme-challenge.example.com"
        );
    }

    #[test]
    fn pem_detect() {
        assert!(looks_like_pem(
            "CERTIFICATE",
            "-----BEGIN CERTIFICATE-----\nMII\n-----END CERTIFICATE-----"
        ));
    }
}
