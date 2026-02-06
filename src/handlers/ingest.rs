use std::net::SocketAddr;

use axum::body::Bytes;
use axum::extract::connect_info::ConnectInfo;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, Method, Uri};
use axum::response::IntoResponse;
use serde_json::json;

use crate::error::AppError;
use crate::models::CapturedRequest;
use crate::server::AppState;

/// Handle any HTTP method to /h/{hook_id} — capture the full request.
///
/// Extracts method, headers, raw body, query string, and source IP.
/// Stores the captured request in SQLite and returns 200 with the request_id.
pub async fn ingest_webhook(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Path(hook_id): Path<String>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<impl IntoResponse, AppError> {
    // Verify the hook exists
    state.db.get_hook(&hook_id)?;

    // Capture the full URI path including query string
    let path_and_query = uri
        .path_and_query()
        .map(|pq| pq.as_str().to_owned())
        .unwrap_or_default();

    let captured = CapturedRequest::new(
        &hook_id,
        method.as_str(),
        &path_and_query,
        &headers,
        body.to_vec(),
        addr.ip(),
    );

    let request_id = captured.request_id.clone();

    state.db.insert_request(&captured).map_err(|e| {
        tracing::error!(
            hook_id = %hook_id,
            request_id = %request_id,
            error = %e,
            "failed to store captured request"
        );
        e
    })?;

    tracing::info!(
        hook_id = %hook_id,
        request_id = %request_id,
        method = %method,
        content_length = body.len(),
        "webhook captured"
    );

    let response = json!({
        "request_id": request_id,
        "hook_id": hook_id,
    });

    Ok(axum::Json(response))
}
