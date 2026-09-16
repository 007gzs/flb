use crate::AppState;
use axum::body::Body;
use axum::extract::connect_info::ConnectInfo;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, Method};
use axum::middleware::Next;
use axum::response::Response;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use std::net::SocketAddr;

pub async fn operation_log(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    if skip(&method, &path) {
        return next.run(req).await;
    }
    let client = client_addr(req.headers(), req.extensions().get());
    let (parts, body) = req.into_parts();
    let (request_body, body) = buffer_json(body).await;
    let req = Request::from_parts(parts, body);
    let res = next.run(req).await;
    let status = res.status().as_u16();
    let (res_parts, res_body) = res.into_parts();
    let (result_body, res_body) = buffer_json(res_body).await;
    let (action, resource, resource_id) = classify(&method, &path);
    let user = audit_user(&path, status, &request_body, &state.settings.admin_user);
    let mut event = json!({
        "ts": chrono::Local::now().to_rfc3339(),
        "user": user,
        "ip": client,
        "action": action,
        "resource": resource,
        "method": method.as_str(),
        "path": path,
        "status": status,
        "ok": (200..400).contains(&status),
    });
    if let Some(id) = resource_id {
        event["id"] = Value::String(id);
    }
    if !request_body.is_null() {
        event["request"] = request_body;
    }
    if status >= 400 {
        if let Some(message) = result_body.get("message").and_then(Value::as_str) {
            event["error"] = Value::String(message.to_string());
        }
    } else if !result_body.is_null() && path != "/api/logout" {
        event["result"] = slim_result(result_body);
    }
    if let Ok(line) = serde_json::to_string(&event) {
        state.audit_log.line(line);
    }
    Response::from_parts(res_parts, res_body)
}

fn skip(method: &Method, path: &str) -> bool {
    method == Method::GET
        || method == Method::HEAD
        || method == Method::OPTIONS
        || !path.starts_with("/api")
        || path == "/api/health"
}

fn audit_user(path: &str, status: u16, request: &Value, admin_user: &str) -> String {
    if path == "/api/login" {
        return request
            .get("username")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or("-")
            .to_string();
    }
    if status == 401 {
        return "-".into();
    }
    admin_user.to_string()
}

fn classify(method: &Method, path: &str) -> (&'static str, &'static str, Option<String>) {
    let rest = path.strip_prefix("/api/").unwrap_or(path);
    let segs: Vec<&str> = rest.split('/').filter(|s| !s.is_empty()).collect();
    let resource = resource_name(segs.first().copied().unwrap_or(""));
    if segs.first() == Some(&"login") {
        return ("登录", "会话", None);
    }
    if segs.first() == Some(&"logout") {
        return ("退出", "会话", None);
    }
    if segs.get(2) == Some(&"renew") {
        return ("续签", resource, segs.get(1).map(|s| (*s).to_string()));
    }
    let id = segs.get(1).map(|s| (*s).to_string());
    let action = match *method {
        Method::POST if id.is_none() => "创建",
        Method::POST => "操作",
        Method::PUT | Method::PATCH => "更新",
        Method::DELETE => "删除",
        _ => "操作",
    };
    (action, resource, id)
}

fn resource_name(seg: &str) -> &'static str {
    match seg {
        "login" | "logout" => "会话",
        "certs" => "证书",
        "dns-providers" => "DNS提供商",
        "domains" => "域名证书",
        "upstreams" => "服务组",
        "hosts" => "主机",
        "streams" => "数据流",
        _ => "管理接口",
    }
}

async fn buffer_json(body: Body) -> (Value, Body) {
    match body.collect().await {
        Ok(collected) => {
            let bytes = collected.to_bytes();
            let value = parse_and_redact(&bytes);
            (value, Body::from(bytes))
        }
        Err(_) => (Value::Null, Body::empty()),
    }
}

fn parse_and_redact(bytes: &[u8]) -> Value {
    if bytes.is_empty() {
        return Value::Null;
    }
    match serde_json::from_slice::<Value>(bytes) {
        Ok(mut value) => {
            redact(&mut value);
            value
        }
        Err(_) => Value::Null,
    }
}

fn redact(value: &mut Value) {
    match value {
        Value::Object(map) => {
            let keys: Vec<String> = map.keys().cloned().collect();
            for key in keys {
                if is_secret(&key) {
                    map.insert(key, Value::String("***".into()));
                } else if let Some(child) = map.get_mut(&key) {
                    redact(child);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                redact(item);
            }
        }
        _ => {}
    }
}

fn is_secret(key: &str) -> bool {
    matches!(
        key.to_ascii_lowercase().replace('_', "").as_str(),
        "password"
            | "token"
            | "certpem"
            | "keypem"
            | "accesssecret"
            | "jwtsecret"
            | "authorization"
    )
}

fn slim_result(value: Value) -> Value {
    match value {
        Value::Object(mut map) => {
            for key in ["certPem", "keyPem", "accessSecret", "token"] {
                map.remove(key);
            }
            Value::Object(map)
        }
        other => other,
    }
}

fn client_addr(headers: &HeaderMap, connect: Option<&ConnectInfo<SocketAddr>>) -> String {
    if let Some(xff) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok())
        && let Some(first) = xff.split(',').next()
    {
        let first = first.trim();
        if !first.is_empty() {
            return first.to_string();
        }
    }
    connect
        .map(|info| info.0.ip().to_string())
        .unwrap_or_else(|| "-".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skip_reads_and_health() {
        assert!(skip(&Method::GET, "/api/hosts"));
        assert!(skip(&Method::POST, "/api/health"));
        assert!(!skip(&Method::POST, "/api/login"));
        assert!(!skip(&Method::DELETE, "/api/certs/1"));
    }

    #[test]
    fn classify_admin_routes() {
        assert_eq!(
            classify(&Method::POST, "/api/login"),
            ("登录", "会话", None)
        );
        assert_eq!(
            classify(&Method::PUT, "/api/hosts/abc"),
            ("更新", "主机", Some("abc".into()))
        );
        assert_eq!(
            classify(&Method::POST, "/api/domains/d1/renew"),
            ("续签", "域名证书", Some("d1".into()))
        );
    }

    #[test]
    fn redact_secrets_keep_names() {
        let mut value = json!({
            "name": "web",
            "password": "secret",
            "keyPem": "-----BEGIN",
            "accessSecret": "ak",
            "servers": [{ "address": "10.0.0.1" }]
        });
        redact(&mut value);
        assert_eq!(value["name"], "web");
        assert_eq!(value["password"], "***");
        assert_eq!(value["keyPem"], "***");
        assert_eq!(value["accessSecret"], "***");
        assert_eq!(value["servers"][0]["address"], "10.0.0.1");
    }
}
