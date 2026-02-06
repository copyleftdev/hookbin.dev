use axum::body::Bytes;
use axum::extract::State;
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

/// POST /api/hooks — create a new webhook inbox.
///
/// Accepts an optional JSON body with a `name` field.
/// If no name is provided (or the body is empty/`{}`), an auto-generated name is used.
/// Returns 201 with the created hook details and its ingestion URL.
pub async fn create_hook(
    State(state): State<AppState>,
    body: Bytes,
) -> Result<impl IntoResponse, AppError> {
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
