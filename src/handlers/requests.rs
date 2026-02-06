use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use base64::Engine;
use serde::Deserialize;
use serde_json::json;

use crate::error::AppError;
use crate::server::AppState;

/// Maximum number of requests per page.
const MAX_PAGE_LIMIT: u32 = 200;

/// Default number of requests per page.
const DEFAULT_PAGE_LIMIT: u32 = 50;

/// Query parameters for paginated request listing.
#[derive(Debug, Deserialize, Default)]
pub struct PaginationParams {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

/// GET /api/hooks/{id}/requests — list captured requests with pagination.
///
/// Returns summary fields only (no body or headers).
/// Supports ?limit=N (default 50, max 200) and ?offset=N (default 0).
pub async fn list_requests(
    State(state): State<AppState>,
    Path(hook_id): Path<String>,
    Query(params): Query<PaginationParams>,
) -> Result<impl IntoResponse, AppError> {
    // Verify the hook exists (returns 404 if not)
    state.db.get_hook(&hook_id)?;

    let limit = params
        .limit
        .unwrap_or(DEFAULT_PAGE_LIMIT)
        .min(MAX_PAGE_LIMIT);
    let offset = params.offset.unwrap_or(0);

    let total = state.db.count_requests(&hook_id)?;
    let requests = state.db.list_requests_paginated(&hook_id, limit, offset)?;
    let count = requests.len();

    let requests_json: Vec<serde_json::Value> = requests
        .iter()
        .map(|r| {
            json!({
                "request_id": r.request_id,
                "method": r.method,
                "path": r.path,
                "content_length": r.content_length,
                "source_ip": r.source_ip,
                "received_at": r.received_at,
            })
        })
        .collect();

    Ok(axum::Json(json!({
        "requests": requests_json,
        "count": count,
        "total": total,
    })))
}

/// GET /api/hooks/{id}/requests/{rid} — full request detail.
///
/// Returns all fields including headers (JSON object) and body (base64-encoded).
pub async fn get_request(
    State(state): State<AppState>,
    Path((hook_id, request_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    // Verify the hook exists (returns 404 if not)
    state.db.get_hook(&hook_id)?;

    let req = state.db.get_request(&hook_id, &request_id)?;

    let body_b64 = base64::engine::general_purpose::STANDARD.encode(&req.body);

    Ok(axum::Json(json!({
        "request_id": req.request_id,
        "hook_id": req.hook_id,
        "method": req.method,
        "path": req.path,
        "headers": req.headers,
        "body": body_b64,
        "content_length": req.content_length,
        "source_ip": req.source_ip,
        "received_at": req.received_at,
    })))
}
