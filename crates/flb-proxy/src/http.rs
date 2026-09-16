use crate::ProxyState;
use async_trait::async_trait;
use bytes::Bytes;
use flb_core::{HeaderRewrite, UpstreamProtocol};
use flb_router::{
    backend_socket, expand_headers, find_host, match_route, pick_backend, request_header_vars,
    resolve_sni, response_header_vars, strip_host_port,
};
use pingora::http::ResponseHeader;
use pingora::prelude::*;
use pingora::proxy::{ProxyHttp, Session, http_proxy_service};
use pingora::upstreams::peer::HttpPeer;
use std::sync::Arc;
use tracing::warn;

pub struct FlbProxy {
    pub state: ProxyState,
}

#[derive(Default)]
pub struct ProxyCtx {
    request_headers: Vec<HeaderRewrite>,
    response_headers: Vec<HeaderRewrite>,
    peer_addr: Option<String>,
    peer_tls: bool,
    peer_sni: String,
    peer_verify: bool,
}

#[async_trait]
impl ProxyHttp for FlbProxy {
    type CTX = ProxyCtx;

    fn new_ctx(&self) -> Self::CTX {
        ProxyCtx::default()
    }

    async fn request_filter(&self, session: &mut Session, ctx: &mut Self::CTX) -> Result<bool> {
        let path = session.req_header().uri.path().to_string();
        if let Some(token) = path.strip_prefix("/.well-known/acme-challenge/")
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
        let Some(host) = find_host(&snapshot, &host_header) else {
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

        let is_tls = session
            .digest()
            .and_then(|d| d.ssl_digest.as_ref())
            .is_some();
        if host.force_https && !is_tls {
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

        let method = session.req_header().method.as_str().to_string();
        let uri = session
            .req_header()
            .uri
            .path_and_query()
            .map(|p| p.as_str().to_string())
            .unwrap_or_else(|| path.clone());
        let selected = match_route(host, &method, &uri);

        let req_headers: Vec<(String, String)> = session
            .req_header()
            .headers
            .iter()
            .filter_map(|(k, v)| Some((k.as_str().to_string(), v.to_str().ok()?.to_string())))
            .collect();
        let vars = request_header_vars(&host_header, &method, &uri, &req_headers);
        ctx.request_headers =
            expand_headers(&selected.request_headers, &uri, &selected.captures, &vars);
        ctx.response_headers = selected.response_headers.clone();

        if selected.uri != uri {
            session.req_header_mut().set_uri(
                selected
                    .uri
                    .parse::<http::Uri>()
                    .map_err(|e| Error::explain(ErrorType::InternalError, e.to_string()))?,
            );
        }

        let Some(upstream) = snapshot.upstream(&selected.upstream_id) else {
            return write_plain(session, 502, "upstream not found".into())
                .await
                .map(|_| true);
        };
        let Some(server) = pick_backend(&upstream.servers) else {
            return write_plain(session, 502, "no upstream server".into())
                .await
                .map(|_| true);
        };

        ctx.peer_addr = Some(backend_socket(server, upstream.protocol));
        ctx.peer_tls = matches!(
            server.protocol_or(upstream.protocol),
            UpstreamProtocol::Https
        );
        ctx.peer_sni = resolve_sni(upstream, server);
        ctx.peer_verify = server.verify_tls_or(upstream.verify_tls);

        for plugin in self.state.plugins.iter() {
            tracing::trace!(name = plugin.name(), "plugin loaded");
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
            .clone()
            .ok_or_else(|| Error::explain(ErrorType::InternalError, "no peer selected"))?;
        let mut peer = HttpPeer::new(addr, ctx.peer_tls, ctx.peer_sni.clone());
        peer.options.verify_cert = ctx.peer_verify;
        peer.options.verify_hostname = ctx.peer_verify;
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
    let mut server = Server::new(Some(Opt {
        upgrade: false,
        daemon: false,
        nocapture: false,
        test: false,
        conf: None,
    }))?;
    server.bootstrap();

    let http_listen = state.settings.http_listen.clone();
    let https_listen = state.settings.https_listen.clone();
    let tls_cb = crate::tls::DynamicCert::new(Arc::new(state.clone()));

    let mut service = http_proxy_service(&server.configuration, FlbProxy { state });
    service.add_tcp(&http_listen);

    let mut tls_settings = pingora::listeners::tls::TlsSettings::with_callbacks(tls_cb)?;
    tls_settings.enable_h2();
    service.add_tls_with_settings(&https_listen, None, tls_settings);

    server.add_service(service);
    server.run_forever();
}
