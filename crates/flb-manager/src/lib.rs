mod schedule;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use flb_cert::{AcmeService, validate_manual_pem};
use flb_config::Settings;
use flb_core::{
    AcmeChallenge, CertMode, Certificate, DnsProvider, DnsProviderKind, Domain, HeaderRewrite,
    Host, MatchType, Route, StreamConfig, StreamProtocol, Upstream, UpstreamProtocol,
    UpstreamServer, new_id, now_rfc3339,
};
use flb_store::Store;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};
use tracing::error;

#[derive(Clone)]
pub struct AppState {
    pub store: Arc<Store>,
    pub acme: Arc<AcmeService>,
}

pub async fn run(
    settings: Settings,
    store: Arc<Store>,
    acme: Arc<AcmeService>,
) -> Result<(), std::io::Error> {
    crate::schedule::spawn_renewal(store.clone(), acme.clone());
    let state = AppState { store, acme };
    let api = Router::new()
        .route("/health", get(health))
        .route("/stats", get(stats))
        .route("/certs", get(list_certs).post(create_cert))
        .route(
            "/certs/{id}",
            get(get_cert).put(update_cert).delete(delete_cert),
        )
        .route("/dns-providers", get(list_dns).post(create_dns))
        .route(
            "/dns-providers/{id}",
            get(get_dns).put(update_dns).delete(delete_dns),
        )
        .route("/domains", get(list_domains).post(create_domain))
        .route(
            "/domains/{id}",
            get(get_domain).put(update_domain).delete(delete_domain),
        )
        .route("/domains/{id}/renew", post(renew_domain))
        .route("/upstreams", get(list_upstreams).post(create_upstream))
        .route(
            "/upstreams/{id}",
            get(get_upstream)
                .put(update_upstream)
                .delete(delete_upstream),
        )
        .route("/hosts", get(list_hosts).post(create_host))
        .route(
            "/hosts/{id}",
            get(get_host).put(update_host).delete(delete_host),
        )
        .route("/streams", get(list_streams).post(create_stream))
        .route(
            "/streams/{id}",
            get(get_stream).put(update_stream).delete(delete_stream),
        );

    let mut app = Router::new()
        .nest("/api", api)
        .layer(CorsLayer::permissive())
        .with_state(state);

    if settings.www_dir.exists() {
        let index = settings.www_dir.join("index.html");
        let static_files = ServeDir::new(&settings.www_dir)
            .append_index_html_on_directories(true)
            .fallback(ServeFile::new(index));
        app = app.fallback_service(static_files);
    }

    let listener = tokio::net::TcpListener::bind(settings.admin_listen).await?;
    tracing::info!(addr = %settings.admin_listen, "admin server listening");
    axum::serve(listener, app).await?;
    Ok(())
}

#[derive(Serialize)]
struct Health {
    status: &'static str,
}

async fn health() -> Json<Health> {
    Json(Health { status: "ok" })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Stats {
    certs: usize,
    dns_providers: usize,
    domains: usize,
    upstreams: usize,
    hosts: usize,
    streams: usize,
}

async fn stats(State(state): State<AppState>) -> Json<Stats> {
    let data = state.store.snapshot();
    Json(Stats {
        certs: data.certificates.len(),
        dns_providers: data.dns_providers.len(),
        domains: data.domains.len(),
        upstreams: data.upstreams.len(),
        hosts: data.hosts.len(),
        streams: data.streams.len(),
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CertInput {
    name: String,
    cert_pem: String,
    key_pem: String,
}

async fn list_certs(State(state): State<AppState>) -> Json<Vec<Certificate>> {
    Json(state.store.snapshot().certificates)
}

async fn get_cert(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Certificate>> {
    state
        .store
        .snapshot()
        .cert(&id)
        .cloned()
        .map(Json)
        .ok_or_else(|| ApiError::not_found("证书不存在"))
}

async fn create_cert(
    State(state): State<AppState>,
    Json(input): Json<CertInput>,
) -> ApiResult<Json<Certificate>> {
    let not_after = validate_manual_pem(&input.cert_pem, &input.key_pem)
        .map_err(|e| ApiError::bad(e.to_string()))?;
    let item = Certificate {
        id: new_id(),
        name: require_name(&input.name)?,
        cert_pem: input.cert_pem,
        key_pem: input.key_pem,
        not_after,
        auto_issued: false,
        created_at: now_rfc3339(),
    };
    Ok(Json(
        state.store.upsert_certificate(item).map_err(store_err)?,
    ))
}

async fn update_cert(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<CertInput>,
) -> ApiResult<Json<Certificate>> {
    let existing = state
        .store
        .snapshot()
        .cert(&id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("证书不存在"))?;
    let not_after = validate_manual_pem(&input.cert_pem, &input.key_pem)
        .map_err(|e| ApiError::bad(e.to_string()))?;
    let item = Certificate {
        id,
        name: require_name(&input.name)?,
        cert_pem: input.cert_pem,
        key_pem: input.key_pem,
        not_after,
        auto_issued: existing.auto_issued,
        created_at: existing.created_at,
    };
    Ok(Json(
        state.store.upsert_certificate(item).map_err(store_err)?,
    ))
}

async fn delete_cert(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    state.store.delete_certificate(&id).map_err(store_err)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DnsInput {
    name: String,
    kind: DnsProviderKind,
    access_key: String,
    access_secret: String,
}

async fn list_dns(State(state): State<AppState>) -> Json<Vec<DnsProvider>> {
    Json(state.store.snapshot().dns_providers)
}

async fn get_dns(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<DnsProvider>> {
    state
        .store
        .snapshot()
        .dns_provider(&id)
        .cloned()
        .map(Json)
        .ok_or_else(|| ApiError::not_found("DNS 提供商不存在"))
}

async fn create_dns(
    State(state): State<AppState>,
    Json(input): Json<DnsInput>,
) -> ApiResult<Json<DnsProvider>> {
    let item = DnsProvider {
        id: new_id(),
        name: require_name(&input.name)?,
        kind: input.kind,
        access_key: require_name(&input.access_key)?,
        access_secret: require_name(&input.access_secret)?,
    };
    Ok(Json(
        state.store.upsert_dns_provider(item).map_err(store_err)?,
    ))
}

async fn update_dns(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<DnsInput>,
) -> ApiResult<Json<DnsProvider>> {
    if state.store.snapshot().dns_provider(&id).is_none() {
        return Err(ApiError::not_found("DNS 提供商不存在"));
    }
    let item = DnsProvider {
        id,
        name: require_name(&input.name)?,
        kind: input.kind,
        access_key: require_name(&input.access_key)?,
        access_secret: require_name(&input.access_secret)?,
    };
    Ok(Json(
        state.store.upsert_dns_provider(item).map_err(store_err)?,
    ))
}

async fn delete_dns(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    state.store.delete_dns_provider(&id).map_err(store_err)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DomainInput {
    name: String,
    mode: CertMode,
    cert_id: Option<String>,
    challenge: Option<AcmeChallenge>,
    dns_provider_id: Option<String>,
}

fn validate_domain_input(input: &DomainInput) -> ApiResult<()> {
    require_name(&input.name)?;
    if input.name.starts_with("*.") && input.challenge == Some(AcmeChallenge::Http01) {
        return Err(ApiError::bad("泛域名不支持 HTTP 验证，请使用 DNS 验证"));
    }
    match input.mode {
        CertMode::Manual => {
            if input.cert_id.as_deref().unwrap_or("").is_empty() {
                return Err(ApiError::bad("手动模式需要选择证书"));
            }
        }
        CertMode::Acme => {
            if input.challenge.is_none() {
                return Err(ApiError::bad("自动证书需要选择验证方式"));
            }
            if input.challenge == Some(AcmeChallenge::Dns01)
                && input.dns_provider_id.as_deref().unwrap_or("").is_empty()
            {
                return Err(ApiError::bad("DNS 验证需要选择 DNS 提供商"));
            }
        }
    }
    Ok(())
}

async fn list_domains(State(state): State<AppState>) -> Json<Vec<Domain>> {
    Json(state.store.snapshot().domains)
}

async fn get_domain(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Domain>> {
    state
        .store
        .snapshot()
        .domain(&id)
        .cloned()
        .map(Json)
        .ok_or_else(|| ApiError::not_found("域名不存在"))
}

async fn create_domain(
    State(state): State<AppState>,
    Json(input): Json<DomainInput>,
) -> ApiResult<Json<Domain>> {
    validate_domain_input(&input)?;
    let item = Domain {
        id: new_id(),
        name: input.name.trim().to_ascii_lowercase(),
        mode: input.mode,
        cert_id: input.cert_id,
        challenge: input.challenge,
        dns_provider_id: input.dns_provider_id,
        status: if input.mode == CertMode::Acme {
            "pending".into()
        } else {
            "manual".into()
        },
        last_error: None,
        expires_at: None,
        created_at: now_rfc3339(),
    };
    if item.mode == CertMode::Manual
        && let Some(cert) = item
            .cert_id
            .as_ref()
            .and_then(|id| state.store.snapshot().cert(id).cloned())
    {
        let mut item = item.clone();
        item.expires_at = cert.not_after;
        let saved = state.store.upsert_domain(item).map_err(store_err)?;
        return Ok(Json(saved));
    }
    let saved = state.store.upsert_domain(item.clone()).map_err(store_err)?;
    if saved.mode == CertMode::Acme {
        spawn_issue(state.clone(), saved.id.clone());
    }
    Ok(Json(saved))
}

async fn update_domain(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<DomainInput>,
) -> ApiResult<Json<Domain>> {
    validate_domain_input(&input)?;
    let existing = state
        .store
        .snapshot()
        .domain(&id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("域名不存在"))?;
    let mut item = existing.clone();
    item.name = input.name.trim().to_ascii_lowercase();
    item.mode = input.mode;
    item.cert_id = input.cert_id;
    item.challenge = input.challenge;
    item.dns_provider_id = input.dns_provider_id;
    if item.mode == CertMode::Manual {
        item.status = "manual".into();
        item.expires_at = item.cert_id.as_ref().and_then(|cid| {
            state
                .store
                .snapshot()
                .cert(cid)
                .and_then(|c| c.not_after.clone())
        });
    }
    let saved = state.store.upsert_domain(item).map_err(store_err)?;
    Ok(Json(saved))
}

async fn delete_domain(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    state.store.delete_domain(&id).map_err(store_err)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn renew_domain(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Domain>> {
    let domain = state
        .store
        .snapshot()
        .domain(&id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("域名不存在"))?;
    if domain.mode != CertMode::Acme {
        return Err(ApiError::bad("仅自动证书支持续签"));
    }
    match state.acme.issue_or_renew(&state.store, &id).await {
        Ok(domain) => Ok(Json(domain)),
        Err(err) => {
            error!(error = %err, "renew failed");
            Err(ApiError::bad(err.to_string()))
        }
    }
}

fn spawn_issue(state: AppState, id: String) {
    tokio::spawn(async move {
        if let Err(err) = state.acme.issue_or_renew(&state.store, &id).await {
            error!(id = %id, error = %err, "auto issue failed");
        }
    });
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpstreamInput {
    name: String,
    #[serde(default)]
    protocol: Option<UpstreamProtocol>,
    #[serde(default)]
    verify_tls: bool,
    sni: Option<String>,
    servers: Vec<UpstreamServer>,
}

fn validate_upstream(input: &UpstreamInput) -> ApiResult<()> {
    require_name(&input.name)?;
    if input.servers.is_empty() {
        return Err(ApiError::bad("至少添加一个后端地址"));
    }
    for server in &input.servers {
        if server.address.trim().is_empty() {
            return Err(ApiError::bad("后端地址不能为空"));
        }
    }
    Ok(())
}

fn normalize_upstream(id: String, input: UpstreamInput) -> Upstream {
    let mut servers = input.servers;
    for server in &mut servers {
        if server.port == Some(0) {
            server.port = None;
        }
    }
    let protocol = input.protocol.unwrap_or_else(|| {
        servers
            .first()
            .map(|s| s.protocol_or(UpstreamProtocol::Http))
            .unwrap_or(UpstreamProtocol::Http)
    });
    let verify_tls = servers.iter().any(|s| {
        s.protocol_or(protocol) == UpstreamProtocol::Https && s.verify_tls_or(input.verify_tls)
    });
    Upstream {
        id,
        name: input.name,
        protocol,
        verify_tls,
        sni: empty_to_none(input.sni),
        servers,
    }
}

async fn list_upstreams(State(state): State<AppState>) -> Json<Vec<Upstream>> {
    Json(state.store.snapshot().upstreams)
}

async fn get_upstream(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Upstream>> {
    state
        .store
        .snapshot()
        .upstream(&id)
        .cloned()
        .map(Json)
        .ok_or_else(|| ApiError::not_found("服务组不存在"))
}

async fn create_upstream(
    State(state): State<AppState>,
    Json(input): Json<UpstreamInput>,
) -> ApiResult<Json<Upstream>> {
    validate_upstream(&input)?;
    let item = normalize_upstream(new_id(), input);
    Ok(Json(state.store.upsert_upstream(item).map_err(store_err)?))
}

async fn update_upstream(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<UpstreamInput>,
) -> ApiResult<Json<Upstream>> {
    if state.store.snapshot().upstream(&id).is_none() {
        return Err(ApiError::not_found("服务组不存在"));
    }
    validate_upstream(&input)?;
    let item = normalize_upstream(id, input);
    Ok(Json(state.store.upsert_upstream(item).map_err(store_err)?))
}

async fn delete_upstream(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    state.store.delete_upstream(&id).map_err(store_err)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HostInput {
    hostname: String,
    #[serde(default = "default_http")]
    protocol: String,
    #[serde(default)]
    https_enabled: bool,
    cert_id: Option<String>,
    #[serde(default)]
    force_https: bool,
    default_upstream_id: String,
    #[serde(default)]
    routes: Vec<RouteInput>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RouteInput {
    id: Option<String>,
    #[serde(alias = "prefix")]
    pattern: String,
    match_type: MatchType,
    rewrite_uri: Option<String>,
    #[serde(default)]
    methods: Vec<String>,
    #[serde(default)]
    request_headers: Vec<HeaderRewrite>,
    #[serde(default)]
    response_headers: Vec<HeaderRewrite>,
    upstream_id: String,
}

fn default_http() -> String {
    "http".into()
}

fn validate_host(input: &HostInput, store: &Store) -> ApiResult<()> {
    require_name(&input.hostname)?;
    if input.default_upstream_id.trim().is_empty() {
        return Err(ApiError::bad("请选择默认后端服务组"));
    }
    if store
        .snapshot()
        .upstream(&input.default_upstream_id)
        .is_none()
    {
        return Err(ApiError::bad("默认后端服务组不存在"));
    }
    if input.https_enabled && input.cert_id.as_deref().unwrap_or("").is_empty() {
        return Err(ApiError::bad("开启 HTTPS 时需要选择证书"));
    }
    for route in &input.routes {
        if route.pattern.trim().is_empty() {
            return Err(ApiError::bad("路由匹配内容不能为空"));
        }
        if route.upstream_id.trim().is_empty()
            || store.snapshot().upstream(&route.upstream_id).is_none()
        {
            return Err(ApiError::bad("路由目标服务组无效"));
        }
        if route.match_type == MatchType::Regex {
            regex::Regex::new(&route.pattern)
                .map_err(|e| ApiError::bad(format!("正则无效: {e}")))?;
        }
    }
    Ok(())
}

fn into_routes(routes: Vec<RouteInput>) -> Vec<Route> {
    routes
        .into_iter()
        .map(|r| Route {
            id: r.id.filter(|s| !s.is_empty()).unwrap_or_else(new_id),
            pattern: r.pattern,
            match_type: r.match_type,
            rewrite_uri: empty_to_none(r.rewrite_uri),
            methods: r.methods,
            request_headers: r.request_headers,
            response_headers: r.response_headers,
            upstream_id: r.upstream_id,
        })
        .collect()
}

async fn list_hosts(State(state): State<AppState>) -> Json<Vec<Host>> {
    Json(state.store.snapshot().hosts)
}

async fn get_host(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Host>> {
    state
        .store
        .snapshot()
        .host(&id)
        .cloned()
        .map(Json)
        .ok_or_else(|| ApiError::not_found("主机不存在"))
}

async fn create_host(
    State(state): State<AppState>,
    Json(input): Json<HostInput>,
) -> ApiResult<Json<Host>> {
    validate_host(&input, &state.store)?;
    let item = Host {
        id: new_id(),
        hostname: input.hostname.trim().to_ascii_lowercase(),
        protocol: input.protocol,
        https_enabled: input.https_enabled,
        cert_id: empty_to_none(input.cert_id),
        force_https: input.force_https,
        default_upstream_id: input.default_upstream_id,
        routes: into_routes(input.routes),
    };
    Ok(Json(state.store.upsert_host(item).map_err(store_err)?))
}

async fn update_host(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<HostInput>,
) -> ApiResult<Json<Host>> {
    if state.store.snapshot().host(&id).is_none() {
        return Err(ApiError::not_found("主机不存在"));
    }
    validate_host(&input, &state.store)?;
    let item = Host {
        id,
        hostname: input.hostname.trim().to_ascii_lowercase(),
        protocol: input.protocol,
        https_enabled: input.https_enabled,
        cert_id: empty_to_none(input.cert_id),
        force_https: input.force_https,
        default_upstream_id: input.default_upstream_id,
        routes: into_routes(input.routes),
    };
    Ok(Json(state.store.upsert_host(item).map_err(store_err)?))
}

async fn delete_host(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    state.store.delete_host(&id).map_err(store_err)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StreamInput {
    name: String,
    listen_port: u16,
    protocol: StreamProtocol,
    target_ip: String,
    target_port: u16,
}

fn validate_stream(input: &StreamInput) -> ApiResult<()> {
    require_name(&input.name)?;
    if input.listen_port == 0 || input.target_port == 0 {
        return Err(ApiError::bad("端口无效"));
    }
    if input.target_ip.trim().is_empty() {
        return Err(ApiError::bad("目标 IP 不能为空"));
    }
    Ok(())
}

async fn list_streams(State(state): State<AppState>) -> Json<Vec<StreamConfig>> {
    Json(state.store.snapshot().streams)
}

async fn get_stream(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<StreamConfig>> {
    state
        .store
        .snapshot()
        .stream(&id)
        .cloned()
        .map(Json)
        .ok_or_else(|| ApiError::not_found("数据流不存在"))
}

async fn create_stream(
    State(state): State<AppState>,
    Json(input): Json<StreamInput>,
) -> ApiResult<Json<StreamConfig>> {
    validate_stream(&input)?;
    let item = StreamConfig {
        id: new_id(),
        name: input.name,
        listen_port: input.listen_port,
        protocol: input.protocol,
        target_ip: input.target_ip.trim().to_string(),
        target_port: input.target_port,
    };
    Ok(Json(state.store.upsert_stream(item).map_err(store_err)?))
}

async fn update_stream(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<StreamInput>,
) -> ApiResult<Json<StreamConfig>> {
    if state.store.snapshot().stream(&id).is_none() {
        return Err(ApiError::not_found("数据流不存在"));
    }
    validate_stream(&input)?;
    let item = StreamConfig {
        id,
        name: input.name,
        listen_port: input.listen_port,
        protocol: input.protocol,
        target_ip: input.target_ip.trim().to_string(),
        target_port: input.target_port,
    };
    Ok(Json(state.store.upsert_stream(item).map_err(store_err)?))
}

async fn delete_stream(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    state.store.delete_stream(&id).map_err(store_err)?;
    Ok(StatusCode::NO_CONTENT)
}

fn require_name(value: &str) -> ApiResult<String> {
    let v = value.trim();
    if v.is_empty() {
        return Err(ApiError::bad("名称不能为空"));
    }
    Ok(v.to_string())
}

fn empty_to_none(value: Option<String>) -> Option<String> {
    value.and_then(|v| {
        let t = v.trim();
        if t.is_empty() {
            None
        } else {
            Some(t.to_string())
        }
    })
}

fn store_err(err: flb_store::StoreError) -> ApiError {
    match err {
        flb_store::StoreError::Message(msg) => ApiError::bad(msg),
        other => ApiError::internal(other.to_string()),
    }
}

struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn bad(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }
    fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
        }
    }
    fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = serde_json::json!({ "message": self.message });
        (self.status, Json(body)).into_response()
    }
}

type ApiResult<T> = Result<T, ApiError>;
