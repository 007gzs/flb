use axum::body::Body;
use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;
use std::path::Path;

#[derive(RustEmbed)]
#[folder = "$OUT_DIR/www"]
struct Assets;

pub async fn embedded_ui(uri: Uri) -> Response {
    serve_embedded(uri.path())
}

fn serve_embedded(request_path: &str) -> Response {
    let path = normalize_path(request_path);
    if let Some(file) = Assets::get(&path) {
        return embedded_file(&path, file);
    }
    let looks_like_file = Path::new(&path).extension().is_some();
    if !looks_like_file && let Some(file) = Assets::get("index.html") {
        return embedded_file("index.html", file);
    }
    StatusCode::NOT_FOUND.into_response()
}

fn normalize_path(request_path: &str) -> String {
    let path = request_path.trim_start_matches('/');
    if path.is_empty() {
        "index.html".to_owned()
    } else {
        path.to_owned()
    }
}

fn embedded_file(path: &str, file: rust_embed::EmbeddedFile) -> Response {
    let mut builder = Response::builder().header(header::CONTENT_TYPE, file.metadata.mimetype());
    if path.starts_with("assets/") {
        builder = builder.header(header::CACHE_CONTROL, "public, max-age=31536000, immutable");
    }
    builder
        .body(Body::from(file.data))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

#[cfg(test)]
mod tests {
    use super::normalize_path;

    #[test]
    fn root_maps_to_index() {
        assert_eq!(normalize_path("/"), "index.html");
        assert_eq!(normalize_path(""), "index.html");
        assert_eq!(normalize_path("/assets/app.js"), "assets/app.js");
    }
}
