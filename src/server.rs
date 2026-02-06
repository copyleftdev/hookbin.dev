use std::sync::Arc;
use std::time::Instant;

use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Router;
use serde_json::json;

use crate::config::Config;
use crate::db::Database;
use crate::handlers;

/// Shared application state passed to all handlers.
#[derive(Clone)]
#[allow(dead_code)]
pub struct AppState {
    pub db: Arc<Database>,
    pub config: Arc<Config>,
    pub start_time: Instant,
}

/// Build the Axum router with all routes and shared state.
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route(
            "/health",
            axum::routing::get(handlers::health::health_check),
        )
        .fallback(fallback_handler)
        .with_state(state)
}

/// Fallback handler for undefined routes.
///
/// Returns a structured JSON 404 instead of Axum's default plain text.
async fn fallback_handler() -> impl IntoResponse {
    let body = json!({
        "error": "not found",
        "status": 404,
        "suggestion": "Check the URL and try again",
    });

    (StatusCode::NOT_FOUND, axum::Json(body))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use axum::http::Request;
    use serde_json::Value;
    use tower::ServiceExt;

    fn test_state() -> AppState {
        let db = Database::open_in_memory().expect("test db");
        AppState {
            db: Arc::new(db),
            config: Arc::new(Config::default()),
            start_time: Instant::now(),
        }
    }

    // AC-2: Health check returns 200 with expected JSON structure
    #[tokio::test]
    async fn health_check_returns_200_with_json() {
        let app = build_router(test_state());

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["status"], "ok");
        assert!(json["version"].is_string());
        assert!(json["uptime_seconds"].is_number());
    }

    // AC-3: Undefined route returns structured JSON 404
    #[tokio::test]
    async fn undefined_route_returns_json_404() {
        let app = build_router(test_state());

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/nonexistent")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["error"], "not found");
        assert_eq!(json["status"], 404);
        assert!(json["suggestion"].is_string());
    }

    // AC-5: AppState holds db, config, and start_time
    #[test]
    fn app_state_holds_required_fields() {
        let state = test_state();

        // Verify we can access all fields
        let _db: &Database = &state.db;
        let _config: &Config = &state.config;
        let _start: Instant = state.start_time;
    }

    // AC-2: Uptime increases over time
    #[tokio::test]
    async fn health_check_uptime_is_non_negative() {
        let state = test_state();
        let app = build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();

        let uptime = json["uptime_seconds"].as_u64().unwrap();
        assert!(uptime < 5, "uptime should be small in test, got: {uptime}");
    }

    // AC-2: Version matches Cargo.toml
    #[tokio::test]
    async fn health_check_version_matches_cargo() {
        let app = build_router(test_state());

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["version"], env!("CARGO_PKG_VERSION"));
    }
}
