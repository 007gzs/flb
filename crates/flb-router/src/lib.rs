use flb_core::{
    ConfigData, HeaderRewrite, Host, MatchType, Route, Upstream, UpstreamProtocol, UpstreamServer,
    parse_hostnames,
};
use rand::Rng;
use regex::Regex;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct SelectedRoute {
    pub upstream_id: String,
    pub uri: String,
    pub request_headers: Vec<HeaderRewrite>,
    pub response_headers: Vec<HeaderRewrite>,
    pub captures: Vec<String>,
}

pub fn strip_host_port(host: &str) -> &str {
    if let Some(rest) = host.strip_prefix('[') {
        return rest.split(']').next().unwrap_or(host);
    }
    host.split(':').next().unwrap_or(host)
}

pub fn host_matches(pattern: &str, hostname: &str) -> bool {
    let hostname = strip_host_port(hostname).to_ascii_lowercase();
    let pattern = pattern.trim().to_ascii_lowercase();
    if pattern.is_empty() {
        return true;
    }
    if pattern == hostname {
        return true;
    }
    if let Some(suffix) = pattern.strip_prefix("*.") {
        if hostname == suffix {
            return false;
        }
        hostname
            .strip_suffix(suffix)
            .is_some_and(|prefix| prefix.ends_with('.'))
    } else {
        false
    }
}

pub fn host_specificity(pattern: &str) -> u32 {
    let pattern = pattern.trim();
    if pattern.is_empty() {
        1
    } else if pattern.contains('*') {
        100 + pattern.len() as u32
    } else {
        10_000 + pattern.len() as u32
    }
}

pub fn find_host<'a>(data: &'a ConfigData, hostname: &str) -> Option<&'a Host> {
    let hostname = strip_host_port(hostname);
    let mut best: Option<&Host> = None;
    let mut best_score = 0u32;
    for host in &data.hosts {
        let mut score = 0u32;
        for pattern in parse_hostnames(&host.hostname) {
            if host_matches(&pattern, hostname) {
                score = score.max(host_specificity(&pattern));
            }
        }
        if score > best_score {
            best_score = score;
            best = Some(host);
        }
    }
    best
}

pub fn match_route(host: &Host, method: &str, path: &str) -> SelectedRoute {
    for route in &host.routes {
        if !method_allowed(&route.methods, method) {
            continue;
        }
        if let Some(captures) = match_path(route, path) {
            let uri = rewrite_uri(path, route, &captures);
            return SelectedRoute {
                upstream_id: route.upstream_id.clone(),
                uri,
                request_headers: route.request_headers.clone(),
                response_headers: route.response_headers.clone(),
                captures,
            };
        }
    }
    SelectedRoute {
        upstream_id: host.default_upstream_id.clone(),
        uri: path.to_string(),
        request_headers: Vec::new(),
        response_headers: Vec::new(),
        captures: Vec::new(),
    }
}

fn method_allowed(methods: &[String], method: &str) -> bool {
    if methods.is_empty() {
        return true;
    }
    methods.iter().any(|m| m.eq_ignore_ascii_case(method))
}

fn match_path(route: &Route, path: &str) -> Option<Vec<String>> {
    match route.match_type {
        MatchType::Prefix => {
            if path.starts_with(&route.pattern) {
                Some(Vec::new())
            } else {
                None
            }
        }
        MatchType::Regex => {
            let re = Regex::new(&route.pattern).ok()?;
            let caps = re.captures(path)?;
            let captures = caps
                .iter()
                .skip(1)
                .map(|m| m.map(|v| v.as_str().to_string()).unwrap_or_default())
                .collect();
            Some(captures)
        }
    }
}

fn rewrite_uri(path: &str, route: &Route, captures: &[String]) -> String {
    let Some(template) = route.rewrite_uri.as_deref() else {
        return path.to_string();
    };
    if template.contains('$') {
        return expand_template(template, path, captures, &HashMap::new());
    }
    match route.match_type {
        MatchType::Prefix => {
            let rest = path.strip_prefix(&route.pattern).unwrap_or("");
            format!("{}{rest}", template.trim_end_matches('/'))
        }
        MatchType::Regex => template.to_string(),
    }
}

pub fn expand_template(
    template: &str,
    uri: &str,
    captures: &[String],
    vars: &HashMap<String, String>,
) -> String {
    let mut out = String::new();
    let bytes = template.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' {
            if i + 1 < bytes.len() && bytes[i + 1] == b'{' {
                if let Some(end) = template[i + 2..].find('}') {
                    let key = &template[i + 2..i + 2 + end];
                    out.push_str(&lookup_var(key, uri, captures, vars));
                    i += 3 + end;
                    continue;
                }
            } else {
                let rest = &template[i + 1..];
                let (key, consumed) = take_ident(rest);
                if !key.is_empty() {
                    out.push_str(&lookup_var(key, uri, captures, vars));
                    i += 1 + consumed;
                    continue;
                }
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn take_ident(s: &str) -> (&str, usize) {
    let mut len = 0;
    for (idx, ch) in s.char_indices() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            len = idx + ch.len_utf8();
        } else {
            break;
        }
    }
    (&s[..len], len)
}

fn lookup_var(key: &str, uri: &str, captures: &[String], vars: &HashMap<String, String>) -> String {
    if let Ok(idx) = key.parse::<usize>()
        && idx >= 1
        && let Some(value) = captures.get(idx - 1)
    {
        return value.clone();
    }
    if key.eq_ignore_ascii_case("uri") || key.eq_ignore_ascii_case("request_uri") {
        return uri.to_string();
    }
    if let Some(value) = vars.get(key) {
        return value.clone();
    }
    if let Some(value) = vars.get(&key.to_ascii_lowercase()) {
        return value.clone();
    }
    String::new()
}

pub fn expand_headers(
    headers: &[HeaderRewrite],
    uri: &str,
    captures: &[String],
    vars: &HashMap<String, String>,
) -> Vec<HeaderRewrite> {
    headers
        .iter()
        .map(|h| HeaderRewrite {
            name: h.name.clone(),
            value: expand_template(&h.value, uri, captures, vars),
        })
        .collect()
}

pub fn pick_backend(servers: &[UpstreamServer]) -> Option<&UpstreamServer> {
    if servers.is_empty() {
        return None;
    }
    let total: u32 = servers.iter().map(|s| s.weight.max(1)).sum();
    let mut ticket = rand::thread_rng().gen_range(0..total);
    for server in servers {
        let weight = server.weight.max(1);
        if ticket < weight {
            return Some(server);
        }
        ticket -= weight;
    }
    servers.last()
}

pub fn backend_socket(server: &UpstreamServer, fallback: UpstreamProtocol) -> String {
    let port = server.effective_port(fallback);
    if server.address.contains(':') && !server.address.starts_with('[') {
        format!("[{}]:{port}", server.address)
    } else {
        format!("{}:{port}", server.address)
    }
}

pub fn resolve_sni(upstream: &Upstream, server: &UpstreamServer) -> String {
    if let Some(sni) = upstream.sni.as_ref().filter(|s| !s.is_empty()) {
        return sni.clone();
    }
    server.address.clone()
}

pub fn request_header_vars(
    host: &str,
    method: &str,
    uri: &str,
    headers: &[(String, String)],
) -> HashMap<String, String> {
    let mut vars = HashMap::new();
    vars.insert("host".into(), strip_host_port(host).to_string());
    vars.insert("request_method".into(), method.to_string());
    vars.insert("uri".into(), uri.to_string());
    vars.insert("request_uri".into(), uri.to_string());
    for (name, value) in headers {
        let key = format!("http_{}", name.to_ascii_lowercase().replace('-', "_"));
        vars.insert(key, value.clone());
    }
    vars
}

pub fn response_header_vars(headers: &[(String, String)]) -> HashMap<String, String> {
    let mut vars = HashMap::new();
    for (name, value) in headers {
        let key = format!(
            "upstream_http_{}",
            name.to_ascii_lowercase().replace('-', "_")
        );
        vars.insert(key, value.clone());
    }
    vars
}

#[cfg(test)]
mod tests {
    use super::*;
    use flb_core::new_id;

    fn sample_host() -> Host {
        Host {
            id: new_id(),
            hostname: "*.example.com".into(),
            protocol: "http".into(),
            https_enabled: false,
            cert_id: None,
            force_https: false,
            default_upstream_id: "up-default".into(),
            routes: vec![
                Route {
                    id: new_id(),
                    pattern: r"^/user/(\d+)$".into(),
                    match_type: MatchType::Regex,
                    rewrite_uri: Some("/v2/user/$1".into()),
                    methods: vec!["GET".into()],
                    request_headers: vec![HeaderRewrite {
                        name: "X-Orig-Host".into(),
                        value: "$http_host".into(),
                    }],
                    response_headers: vec![],
                    upstream_id: "up-regex".into(),
                },
                Route {
                    id: new_id(),
                    pattern: "/api".into(),
                    match_type: MatchType::Prefix,
                    rewrite_uri: Some("/svc".into()),
                    methods: vec![],
                    request_headers: vec![],
                    response_headers: vec![],
                    upstream_id: "up-api".into(),
                },
            ],
        }
    }

    #[test]
    fn wildcard_host_match() {
        assert!(host_matches("*.example.com", "a.example.com"));
        assert!(!host_matches("*.example.com", "example.com"));
        assert!(host_matches("app.example.com", "app.example.com:8443"));
        assert!(host_matches("", "anything.example.com"));
        assert!(host_matches("  ", "missing-host"));
    }

    #[test]
    fn empty_hostname_is_default_fallback() {
        let specific = sample_host();
        let default_host = Host {
            id: new_id(),
            hostname: "".into(),
            protocol: "http".into(),
            https_enabled: false,
            cert_id: None,
            force_https: false,
            default_upstream_id: "up-catch-all".into(),
            routes: vec![],
        };
        let data = ConfigData {
            hosts: vec![default_host.clone(), specific.clone()],
            ..ConfigData::default()
        };
        assert_eq!(
            find_host(&data, "a.example.com").map(|h| h.default_upstream_id.as_str()),
            Some("up-default")
        );
        assert_eq!(
            find_host(&data, "other.com").map(|h| h.default_upstream_id.as_str()),
            Some("up-catch-all")
        );
    }

    #[test]
    fn multiple_hostnames_on_one_host() {
        let host = Host {
            id: new_id(),
            hostname: "a.example.com, *.app.test".into(),
            protocol: "http".into(),
            https_enabled: false,
            cert_id: None,
            force_https: false,
            default_upstream_id: "up-multi".into(),
            routes: vec![],
        };
        let data = ConfigData {
            hosts: vec![host],
            ..ConfigData::default()
        };
        assert_eq!(
            find_host(&data, "a.example.com").map(|h| h.default_upstream_id.as_str()),
            Some("up-multi")
        );
        assert_eq!(
            find_host(&data, "x.app.test").map(|h| h.default_upstream_id.as_str()),
            Some("up-multi")
        );
        assert!(find_host(&data, "other.com").is_none());
    }

    #[test]
    fn prefix_and_regex_routes() {
        let host = sample_host();
        let regex = match_route(&host, "GET", "/user/42");
        assert_eq!(regex.upstream_id, "up-regex");
        assert_eq!(regex.uri, "/v2/user/42");

        let prefix = match_route(&host, "POST", "/api/v1");
        assert_eq!(prefix.upstream_id, "up-api");
        assert_eq!(prefix.uri, "/svc/v1");

        let fallback = match_route(&host, "GET", "/other");
        assert_eq!(fallback.upstream_id, "up-default");
    }

    #[test]
    fn header_var_expand() {
        let vars = request_header_vars(
            "app.example.com",
            "GET",
            "/x",
            &[("Host".into(), "app.example.com".into())],
        );
        let out = expand_template("$http_host|$uri", "/x", &[], &vars);
        assert_eq!(out, "app.example.com|/x");
    }

    #[test]
    fn weighted_backend() {
        let servers = vec![UpstreamServer {
            address: "10.0.0.1".into(),
            port: Some(80),
            weight: 1,
            protocol: None,
            verify_tls: None,
        }];
        let picked = pick_backend(&servers).unwrap();
        assert_eq!(picked.address, "10.0.0.1");
    }

    #[test]
    fn empty_sni_uses_backend_address() {
        let upstream = Upstream {
            id: new_id(),
            name: "u".into(),
            protocol: UpstreamProtocol::Https,
            verify_tls: false,
            sni: None,
            servers: vec![UpstreamServer {
                address: "10.0.0.8".into(),
                port: Some(443),
                weight: 1,
                protocol: Some(UpstreamProtocol::Https),
                verify_tls: Some(false),
            }],
        };
        let sni = resolve_sni(&upstream, &upstream.servers[0]);
        assert_eq!(sni, "10.0.0.8");
    }

    #[test]
    fn explicit_sni_wins() {
        let upstream = Upstream {
            id: new_id(),
            name: "u".into(),
            protocol: UpstreamProtocol::Https,
            verify_tls: false,
            sni: Some("app.example.com".into()),
            servers: vec![UpstreamServer {
                address: "10.0.0.8".into(),
                port: Some(443),
                weight: 1,
                protocol: Some(UpstreamProtocol::Https),
                verify_tls: Some(false),
            }],
        };
        let sni = resolve_sni(&upstream, &upstream.servers[0]);
        assert_eq!(sni, "app.example.com");
    }

    #[test]
    fn empty_port_defaults_by_protocol() {
        let http = UpstreamServer {
            address: "10.0.0.1".into(),
            port: None,
            weight: 1,
            protocol: Some(UpstreamProtocol::Http),
            verify_tls: None,
        };
        let https = UpstreamServer {
            address: "10.0.0.1".into(),
            port: None,
            weight: 1,
            protocol: Some(UpstreamProtocol::Https),
            verify_tls: None,
        };
        assert_eq!(backend_socket(&http, UpstreamProtocol::Http), "10.0.0.1:80");
        assert_eq!(
            backend_socket(&https, UpstreamProtocol::Http),
            "10.0.0.1:443"
        );
    }
}
