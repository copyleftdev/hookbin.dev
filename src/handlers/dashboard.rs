use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "ui/"]
struct Assets;

/// Determine content-type from file extension.
fn content_type(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("html") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js") => "application/javascript; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        Some("json") => "application/json",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("png") => "image/png",
        _ => "application/octet-stream",
    }
}

/// Serve the index.html page.
pub async fn index() -> impl IntoResponse {
    match Assets::get("index.html") {
        Some(file) => Html(file.data.to_vec()).into_response(),
        None => (StatusCode::INTERNAL_SERVER_ERROR, "index.html not found").into_response(),
    }
}

/// Serve a static asset from the embedded files.
pub async fn static_asset(axum::extract::Path(path): axum::extract::Path<String>) -> Response {
    match Assets::get(&format!("assets/{path}")) {
        Some(file) => {
            let mime = content_type(&path);
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, mime)],
                file.data.to_vec(),
            )
                .into_response()
        }
        None => (StatusCode::NOT_FOUND, "asset not found").into_response(),
    }
}

/// SPA fallback: serve index.html for non-API routes so hash routing works.
pub async fn spa_fallback(uri: axum::http::Uri) -> Response {
    let path = uri.path();

    // API, health, and ingestion routes should get JSON 404, not the SPA
    if path.starts_with("/api/") || path.starts_with("/h/") || path == "/health" {
        let body = serde_json::json!({
            "error": "not found",
            "status": 404,
            "suggestion": "Check the URL and try again",
        });
        return (StatusCode::NOT_FOUND, axum::Json(body)).into_response();
    }

    // Everything else gets index.html for SPA client-side routing
    index().await.into_response()
}
