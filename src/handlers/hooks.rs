use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::Deserialize;
use serde_json::json;

use crate::error::AppError;
use crate::models::Hook;
use crate::server::AppState;

/// Maximum allowed hook name length.
const MAX_NAME_LENGTH: usize = 100;

/// Request body for creating a hook.
#[derive(Debug, Deserialize, Default)]
pub struct CreateHookRequest {
    pub name: Option<String>,
}

/// GET /api/hooks — list all webhook inboxes.
///
/// Returns a JSON object with a `hooks` array and a `count` field.
/// Hooks are ordered by created_at descending (newest first).
pub async fn list_hooks(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let hooks = state.db.list_hooks()?;
    let count = hooks.len();

    let hooks_json: Vec<serde_json::Value> = hooks
        .iter()
        .map(|h| {
            json!({
                "hook_id": h.hook_id,
                "name": h.name,
                "created_at": h.created_at,
                "request_count": h.request_count,
                "url": format!("/h/{}", h.hook_id),
            })
        })
        .collect();

    Ok(axum::Json(json!({
        "hooks": hooks_json,
        "count": count,
    })))
}

/// GET /api/hooks/{id} — return hook details.
///
/// Returns the full hook object with hook_id, name, created_at, request_count, and url.
/// Returns 404 if the hook doesn't exist.
pub async fn get_hook_detail(
    State(state): State<AppState>,
    Path(hook_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let hook = state.db.get_hook(&hook_id)?;

    Ok(axum::Json(json!({
        "hook_id": hook.hook_id,
        "name": hook.name,
        "created_at": hook.created_at,
        "request_count": hook.request_count,
        "url": format!("/h/{}", hook.hook_id),
    })))
}

/// DELETE /api/hooks/{id} — remove a hook and all its captured requests.
///
/// Deletes the hook by ID. All associated requests are CASCADE-deleted
/// via the foreign key constraint. Returns 404 if the hook doesn't exist.
pub async fn delete_hook(
    State(state): State<AppState>,
    Path(hook_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    // Verify the hook exists and get its name for the response
    let hook = state.db.get_hook(&hook_id)?;

    state.db.delete_hook(&hook_id).map_err(|e| {
        tracing::error!(hook_id = %hook_id, error = %e, "failed to delete hook");
        e
    })?;

    tracing::info!(hook_id = %hook_id, name = %hook.name, "hook deleted");

    Ok(axum::Json(json!({
        "deleted": true,
        "hook_id": hook_id,
    })))
}

/// POST /api/hooks — create a new webhook inbox.
///
/// Accepts an optional JSON body with a `name` field.
/// If no name is provided (or the body is empty/`{}`), an auto-generated name is used.
/// Returns 201 with the created hook details and its ingestion URL.
pub async fn create_hook(
    State(state): State<AppState>,
    body: Bytes,
) -> Result<impl IntoResponse, AppError> {
    // Enforce max_hooks limit
    let current_hooks = state.db.count_hooks()?;
    if current_hooks >= state.config.max_hooks {
        return Err(AppError::LimitReached(format!(
            "hook limit reached ({current_hooks}/{max_hooks}) — delete unused hooks or increase --max-hooks",
            max_hooks = state.config.max_hooks,
        )));
    }

    // Parse the body: empty body → default, otherwise parse as JSON
    let request: CreateHookRequest = if body.is_empty() {
        CreateHookRequest::default()
    } else {
        serde_json::from_slice(&body)
            .map_err(|e| AppError::InvalidInput(format!("invalid JSON: {e}")))?
    };

    // Determine the hook name
    let name = match request.name {
        Some(n) if n.trim().is_empty() => None,
        Some(n) => {
            if n.len() > MAX_NAME_LENGTH {
                return Err(AppError::InvalidInput(format!(
                    "name too long ({} chars, max {MAX_NAME_LENGTH})",
                    n.len(),
                )));
            }
            Some(n)
        }
        None => None,
    };

    let hook = Hook::new(name.as_deref().unwrap_or(""));

    // If no name was provided, generate one from the hook_id
    let hook = if name.is_none() {
        let short_id = &hook.hook_id[..hook.hook_id.len().min(8)];
        Hook {
            name: format!("hook-{short_id}"),
            ..hook
        }
    } else {
        hook
    };

    state.db.insert_hook(&hook).map_err(|e| {
        tracing::error!(hook_id = %hook.hook_id, error = %e, "failed to create hook");
        e
    })?;

    tracing::info!(hook_id = %hook.hook_id, name = %hook.name, "hook created");

    let response = json!({
        "hook_id": hook.hook_id,
        "name": hook.name,
        "created_at": hook.created_at,
        "request_count": hook.request_count,
        "url": format!("/h/{}", hook.hook_id),
    });

    Ok((StatusCode::CREATED, axum::Json(response)))
}
