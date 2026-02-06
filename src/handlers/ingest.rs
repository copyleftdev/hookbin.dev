use std::net::SocketAddr;

use axum::body::{to_bytes, Body};
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
/// Enforces the configured max_payload limit before storing.
/// Stores the captured request in SQLite and returns 200 with the request_id.
pub async fn ingest_webhook(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Path(hook_id): Path<String>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Body,
) -> Result<impl IntoResponse, AppError> {
    // Verify the hook exists
    state.db.get_hook(&hook_id)?;

    let max_payload = state.config.max_payload;

    // Collect the body up to max_payload + 1 bytes.
    // DefaultBodyLimit on the route prevents truly huge bodies from reaching here.
    // We read up to the limit and check the size for a structured JSON 413.
    let body_bytes = to_bytes(body, max_payload.saturating_add(1))
        .await
        .map_err(|_| AppError::PayloadTooLarge {
            size: max_payload + 1,
            max: max_payload,
        })?;

    if body_bytes.len() > max_payload {
        return Err(AppError::PayloadTooLarge {
            size: body_bytes.len(),
            max: max_payload,
        });
    }

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
        body_bytes.to_vec(),
        addr.ip(),
    );

    let request_id = captured.request_id.clone();

    // Evict oldest requests if at capacity (circular buffer strategy).
    // We keep max_requests - 1 so that after inserting the new one, total = max_requests.
    let max_requests = state.config.max_requests;
    let hook = state.db.get_hook(&hook_id)?;
    if hook.request_count >= u64::from(max_requests) {
        let keep = max_requests.saturating_sub(1);
        let evicted = state.db.delete_oldest_requests(&hook_id, keep)?;
        if evicted > 0 {
            tracing::debug!(
                hook_id = %hook_id,
                evicted = evicted,
                max_requests = max_requests,
                "evicted oldest requests to maintain capacity"
            );
        }
    }

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
        content_length = body_bytes.len(),
        "webhook captured"
    );

    let response = json!({
        "request_id": request_id,
        "hook_id": hook_id,
    });

    Ok(axum::Json(response))
}
