use axum::extract::State;
use axum::response::IntoResponse;
use serde_json::json;

use crate::server::AppState;

/// Health check endpoint.
///
/// Returns 200 with JSON body containing status, version, and uptime.
pub async fn health_check(State(state): State<AppState>) -> impl IntoResponse {
    let uptime = state.start_time.elapsed().as_secs();

    let body = json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
        "uptime_seconds": uptime,
    });

    axum::Json(body)
}
