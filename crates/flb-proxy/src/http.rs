use crate::ProxyState;
use async_trait::async_trait;
use bytes::Bytes;
use flb_core::{HeaderRewrite, UpstreamProtocol};
use flb_router::{
    backend_socket, expand_headers, lookup_host, match_route, pick_backend, request_header_vars,
    resolve_sni, response_header_vars, strip_host_port,
};
use pingora::http::ResponseHeader;
use pingora::listeners::TcpSocketOptions;
use pingora::prelude::*;
use pingora::protocols::TcpKeepalive;
use pingora::proxy::{ProxyHttp, Session, http_proxy_service};
use pingora::upstreams::peer::HttpPeer;
use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::OwnedSemaphorePermit;
use tracing::warn;

pub struct FlbProxy {
    pub state: ProxyState,
}

pub struct ProxyCtx {
    request_headers: Vec<HeaderRewrite>,
    response_headers: Vec<HeaderRewrite>,
    peer_addr: Option<SocketAddr>,
    peer_tls: bool,
    peer_sni: String,
    peer_verify: bool,
    started: Instant,
    _permit: Option<OwnedSemaphorePermit>,
}

impl Default for ProxyCtx {
    fn default() -> Self {
        Self {
            request_headers: Vec::new(),
            response_headers: Vec::new(),
            peer_addr: None,
            peer_tls: false,
            peer_sni: String::new(),
            peer_verify: false,
            started: Instant::now(),
            _permit: None,
        }
    }
}

#[async_trait]
impl ProxyHttp for FlbProxy {
    type CTX = ProxyCtx;

    fn new_ctx(&self) -> Self::CTX {
        ProxyCtx::default()
    }

    async fn request_filter(&self, session: &mut Session, ctx: &mut Self::CTX) -> Result<bool> {
        if let Some(token) = session
            .req_header()
            .uri
            .path()
            .strip_prefix("/.well-known/acme-challenge/")
            && let Some(body) = self.state.acme_http01.get(token)
        {
            return write_plain(session, 200, body.value().clone())
                .await
                .map(|_| true);
        }

        let host_header = request_host(session.req_header());
        if session.req_header().headers.get("host").is_none() && !host_header.is_empty() {
            let _ = session
                .req_header_mut()
                .insert_header("Host", host_header.as_str());
        }

        let snapshot = self.state.store.snapshot();
        let index = self.state.host_index();
        let Some(host) = lookup_host(&snapshot.hosts, &index, &host_header) else {
            let msg = if host_header.is_empty() {
                "missing Host header"
            } else {
                "no host matched"
            };
            return write_plain(
                session,
                if host_header.is_empty() { 400 } else { 404 },
                msg.into(),
            )
            .await
            .map(|_| true);
        };

        if host.force_https
            && session
                .digest()
                .and_then(|d| d.ssl_digest.as_ref())
                .is_none()
        {
            let location = format!(
                "https://{}{}",
                strip_host_port(&host_header),
                session
                    .req_header()
                    .uri
                    .path_and_query()
                    .map(|p| p.as_str())
                    .unwrap_or("/")
            );
            return write_redirect(session, &location).await.map(|_| true);
        }

        let Ok(permit) = self.state.inflight.clone().try_acquire_owned() else {
            return write_plain(session, 503, "too many connections".into())
                .await
                .map(|_| true);
        };
        ctx._permit = Some(permit);

        let method = session.req_header().method.as_str();
        let uri = session
            .req_header()
            .uri
            .path_and_query()
            .map(|p| p.as_str())
            .unwrap_or("/");
        let selected = match_route(host, method, uri);

        if !selected.request_headers.is_empty() {
            let req_headers: Vec<(String, String)> = session
                .req_header()
                .headers
                .iter()
                .filter_map(|(k, v)| Some((k.as_str().to_string(), v.to_str().ok()?.to_string())))
                .collect();
            let vars = request_header_vars(&host_header, method, uri, &req_headers);
            ctx.request_headers =
                expand_headers(selected.request_headers, uri, &selected.captures, &vars);
        }
        if !selected.response_headers.is_empty() {
            ctx.response_headers = selected.response_headers.to_vec();
        }

        let rewritten = if selected.uri.as_ref() != uri {
            Some(
                selected
                    .uri
                    .parse::<http::Uri>()
                    .map_err(|e| Error::explain(ErrorType::InternalError, e.to_string()))?,
            )
        } else {
            None
        };
        let Some(upstream) = snapshot.upstream(selected.upstream_id) else {
            return write_plain(session, 502, "upstream not found".into())
                .await
                .map(|_| true);
        };
        let Some(server) = pick_backend(&upstream.servers) else {
            return write_plain(session, 502, "no upstream server".into())
                .await
                .map(|_| true);
        };

        ctx.peer_tls = matches!(
            server.protocol_or(upstream.protocol),
            UpstreamProtocol::Https
        );
        ctx.peer_addr = Some(resolve_peer_addr(
            &self.state.resolved,
            &backend_socket(server, upstream.protocol),
        )?);
        ctx.peer_sni = if ctx.peer_tls {
            resolve_sni(upstream, server)
        } else {
            String::new()
        };
        ctx.peer_verify = server.verify_tls_or(upstream.verify_tls);
        if let Some(rewritten) = rewritten {
            session.req_header_mut().set_uri(rewritten);
        }
        Ok(false)
    }

    async fn upstream_peer(
        &self,
        _session: &mut Session,
        ctx: &mut Self::CTX,
    ) -> Result<Box<HttpPeer>> {
        let addr = ctx
            .peer_addr
            .ok_or_else(|| Error::explain(ErrorType::InternalError, "no peer selected"))?;
        let mut peer = HttpPeer::new(addr, ctx.peer_tls, std::mem::take(&mut ctx.peer_sni));
        peer.options.verify_cert = ctx.peer_verify;
        peer.options.verify_hostname = ctx.peer_verify;
        peer.options.connection_timeout = Some(Duration::from_secs(2));
        peer.options.idle_timeout = Some(Duration::from_secs(75));
        peer.options.tcp_recv_buf = Some(256 * 1024);
        peer.options.tcp_fast_open = true;
        peer.options.tcp_keepalive = Some(TcpKeepalive {
            idle: Duration::from_secs(60),
            interval: Duration::from_secs(10),
            count: 3,
            #[cfg(target_os = "linux")]
            user_timeout: Duration::from_secs(75),
        });
        Ok(Box::new(peer))
    }

    async fn upstream_request_filter(
        &self,
        session: &mut Session,
        upstream_request: &mut pingora::http::RequestHeader,
        ctx: &mut Self::CTX,
    ) -> Result<()> {
        if let Some(addr) = session
            .client_addr()
            .and_then(|a| a.as_inet())
            .map(|sa| sa.ip().to_string())
        {
            let _ = upstream_request.insert_header("X-Real-IP", &addr);
            let forwarded = session
                .req_header()
                .headers
                .get("x-forwarded-for")
                .and_then(|v| v.to_str().ok())
                .map(|v| format!("{v}, {addr}"))
                .unwrap_or(addr);
            let _ = upstream_request.insert_header("X-Forwarded-For", forwarded);
        }
        let proto = if session
            .digest()
            .and_then(|d| d.ssl_digest.as_ref())
            .is_some()
        {
            "https"
        } else {
            "http"
        };
        let _ = upstream_request.insert_header("X-Forwarded-Proto", proto);

        for header in &ctx.request_headers {
            if header.value.is_empty() {
                upstream_request.remove_header(&header.name);
            } else if let Err(err) =
                upstream_request.insert_header(header.name.clone(), &header.value)
            {
                warn!(error = %err, header = %header.name, "failed to rewrite request header");
            }
        }
        Ok(())
    }

    async fn response_filter(
        &self,
        _session: &mut Session,
        upstream_response: &mut ResponseHeader,
        ctx: &mut Self::CTX,
    ) -> Result<()> {
        if ctx.response_headers.is_empty() {
            return Ok(());
        }
        let headers: Vec<(String, String)> = upstream_response
            .headers
            .iter()
            .filter_map(|(k, v)| Some((k.as_str().to_string(), v.to_str().ok()?.to_string())))
            .collect();
        let vars = response_header_vars(&headers);
        let uri = String::new();
        let expanded = expand_headers(&ctx.response_headers, &uri, &[], &vars);
        for header in expanded {
            if header.value.is_empty() {
                upstream_response.remove_header(&header.name);
            } else if let Err(err) =
                upstream_response.insert_header(header.name.clone(), &header.value)
            {
                warn!(error = %err, header = %header.name, "failed to rewrite response header");
            }
        }
        Ok(())
    }

    async fn logging(&self, session: &mut Session, e: Option<&Error>, ctx: &mut Self::CTX) {
        let req = session.req_header();
        let method = req.method.as_str();
        let uri = req.uri.path_and_query().map(|p| p.as_str()).unwrap_or("/");
        let host = request_host(req);
        if self.state.settings.access_log {
            let ua = req
                .headers
                .get("user-agent")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("-");
            let client = session
                .client_addr()
                .map(|a| a.to_string())
                .unwrap_or_else(|| "-".into());
            let status = session
                .response_written()
                .map(|h| h.status.as_u16())
                .unwrap_or(0);
            let bytes = session.body_bytes_sent();
            let ms = ctx.started.elapsed().as_millis();
            self.state.access_log.line(format!(
                "{client} {host} \"{method} {uri}\" {status} {bytes} {ms}ms \"{ua}\""
            ));
        }
        if let Some(err) = e {
            warn!(error = %err, host = %host, uri = %uri, "proxy error");
        }
    }
}

fn request_host(header: &pingora::http::RequestHeader) -> String {
    if let Some(host) = header
        .headers
        .get("host")
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty())
    {
        return host.to_string();
    }
    header
        .uri
        .host()
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_default()
}

async fn write_plain(session: &mut Session, status: u16, body: String) -> Result<()> {
    let mut header = ResponseHeader::build(status, None)?;
    header.insert_header("Content-Type", "text/plain; charset=utf-8")?;
    header.insert_header("Content-Length", body.len().to_string())?;
    session
        .write_response_header(Box::new(header), false)
        .await?;
    session
        .write_response_body(Some(Bytes::from(body)), true)
        .await?;
    Ok(())
}

async fn write_redirect(session: &mut Session, location: &str) -> Result<()> {
    let mut header = ResponseHeader::build(301, None)?;
    header.insert_header("Location", location)?;
    header.insert_header("Content-Length", "0")?;
    session
        .write_response_header(Box::new(header), true)
        .await?;
    Ok(())
}

pub fn run_http_proxy(state: ProxyState) -> pingora::Result<()> {
    let threads = worker_threads(state.settings.threads);
    let conf = pingora::server::configuration::ServerConf {
        threads,
        listener_tasks_per_fd: (threads.saturating_mul(8)).clamp(16, 256),
        work_stealing: true,
        upstream_keepalive_pool_size: 1024,
        upstream_connect_offload_threadpools: Some(2),
        upstream_connect_offload_thread_per_pool: Some(threads.max(2)),
        downstream_tls_offload_threadpools: Some(threads.max(2)),
        downstream_tls_offload_thread_per_pool: Some(2),
        max_retries: 1,
        ..Default::default()
    };
    let mut server = Server::new_with_opt_and_conf(
        Some(Opt {
            upgrade: false,
            daemon: false,
            nocapture: false,
            test: false,
            conf: None,
        }),
        conf,
    );
    server.bootstrap();

    let http_listen = state.settings.http_listen.clone();
    let https_listen = state.settings.https_listen.clone();
    tracing::info!(threads, "http proxy workers");
    let tls_resolver = crate::tls::DynamicCert::new(Arc::new(state.clone()));

    let mut service = http_proxy_service(&server.configuration, FlbProxy { state });
    let sock_opt = listener_socket_options();
    service.add_tcp_with_settings(&http_listen, sock_opt.clone());

    // rustls ignores OpenSSL session-cache APIs; SNI certs come from ResolvesServerCert.
    let mut tls_settings = pingora::listeners::tls::TlsSettings::intermediate("_", "_")?;
    tls_settings.set_cert_resolver(tls_resolver);
    tls_settings.enable_h2();
    tls_settings.set_offload_threadpool_from_server_conf(&server.configuration);
    service.add_tls_with_settings(&https_listen, Some(sock_opt), tls_settings);

    server.add_service(service);
    server.run_forever();
}

fn listener_socket_options() -> TcpSocketOptions {
    let mut opt = TcpSocketOptions::default();
    opt.tcp_fastopen = Some(256);
    opt.so_reuseport = Some(true);
    opt.tcp_keepalive = Some(TcpKeepalive {
        idle: Duration::from_secs(60),
        interval: Duration::from_secs(10),
        count: 3,
        #[cfg(target_os = "linux")]
        user_timeout: Duration::from_secs(75),
    });
    opt.tcp_snd_buf = Some(256 * 1024);
    opt.tcp_recv_buf = Some(256 * 1024);
    opt
}

fn worker_threads(configured: usize) -> usize {
    if configured > 0 {
        return configured;
    }
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .max(1)
}

fn resolve_peer_addr(
    cache: &dashmap::DashMap<String, SocketAddr>,
    addr: &str,
) -> Result<SocketAddr> {
    if let Ok(parsed) = addr.parse() {
        return Ok(parsed);
    }
    if let Some(cached) = cache.get(addr) {
        return Ok(*cached);
    }
    let resolved = addr
        .to_socket_addrs()
        .map_err(|e| Error::because(ErrorType::ConnectError, "resolve peer", e))?
        .next()
        .ok_or_else(|| Error::explain(ErrorType::ConnectError, "peer resolved to no addresses"))?;
    cache.insert(addr.to_string(), resolved);
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::resolve_peer_addr;
    use dashmap::DashMap;

    #[test]
    fn parse_literal_socket_addr() {
        let cache = DashMap::new();
        let addr = resolve_peer_addr(&cache, "127.0.0.1:8080").unwrap();
        assert_eq!(addr, "127.0.0.1:8080".parse().unwrap());
        assert!(cache.is_empty());
    }

    #[test]
    fn caches_hostname_lookup() {
        let cache = DashMap::new();
        let first = resolve_peer_addr(&cache, "localhost:80").unwrap();
        assert_eq!(cache.len(), 1);
        let second = resolve_peer_addr(&cache, "localhost:80").unwrap();
        assert_eq!(first, second);
    }
}
