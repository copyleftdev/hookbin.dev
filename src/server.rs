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
        .route(
            "/api/hooks",
            axum::routing::post(handlers::hooks::create_hook),
        )
        .route(
            "/h/{hook_id}",
            axum::routing::any(handlers::ingest::ingest_webhook),
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
    use axum::extract::connect_info::ConnectInfo;
    use axum::http::Request;
    use serde_json::Value;
    use std::net::SocketAddr;
    use tower::ServiceExt;

    fn test_state() -> AppState {
        let db = Database::open_in_memory().expect("test db");
        AppState {
            db: Arc::new(db),
            config: Arc::new(Config::default()),
            start_time: Instant::now(),
        }
    }

    /// Build a router with a fake ConnectInfo for testing ingestion.
    fn build_test_router(state: AppState) -> Router {
        let addr: SocketAddr = "192.168.1.42:12345".parse().unwrap();
        build_router(state).layer(axum::Extension(ConnectInfo(addr)))
    }

    /// Helper: create a hook via the API and return the hook_id.
    async fn create_test_hook(state: &AppState, name: &str) -> String {
        use crate::models::Hook;
        let hook = Hook::new(name);
        let hook_id = hook.hook_id.clone();
        state.db.insert_hook(&hook).unwrap();
        hook_id
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

    // -- HB-009: Hook creation tests --

    // AC-1: POST /api/hooks with name returns 201 with all fields
    #[tokio::test]
    async fn create_hook_with_name_returns_201() {
        let state = test_state();
        let app = build_router(state.clone());

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/hooks")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(r#"{"name": "My Webhook"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CREATED);

        let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();

        assert!(json["hook_id"].is_string());
        assert!(!json["hook_id"].as_str().unwrap().is_empty());
        assert_eq!(json["name"], "My Webhook");
        assert!(json["created_at"].is_number());
        assert_eq!(json["request_count"], 0);

        // URL must include the hook_id
        let url = json["url"].as_str().unwrap();
        let hook_id = json["hook_id"].as_str().unwrap();
        assert_eq!(url, format!("/h/{hook_id}"));
    }

    // AC-2: POST /api/hooks with empty body → auto-generated name
    #[tokio::test]
    async fn create_hook_empty_body_generates_name() {
        let app = build_router(test_state());

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/hooks")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CREATED);

        let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();

        let name = json["name"].as_str().unwrap();
        assert!(
            name.starts_with("hook-"),
            "auto name should start with 'hook-', got: {name}"
        );
    }

    // AC-2: POST /api/hooks with {} → auto-generated name
    #[tokio::test]
    async fn create_hook_empty_json_generates_name() {
        let app = build_router(test_state());

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/hooks")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CREATED);

        let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();

        let name = json["name"].as_str().unwrap();
        assert!(name.starts_with("hook-"), "got: {name}");
    }

    // AC-3: Name longer than 100 chars → 400
    #[tokio::test]
    async fn create_hook_name_too_long_returns_400() {
        let app = build_router(test_state());
        let long_name = "x".repeat(101);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/hooks")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        serde_json::to_string(&serde_json::json!({"name": long_name})).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();

        assert!(json["error"].as_str().unwrap().contains("name too long"));
        assert!(json["suggestion"].is_string());
    }

    // AC-3: Name exactly 100 chars → accepted
    #[tokio::test]
    async fn create_hook_name_at_limit_accepted() {
        let app = build_router(test_state());
        let name = "x".repeat(100);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/hooks")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        serde_json::to_string(&serde_json::json!({"name": name})).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CREATED);
    }

    // AC-4: Invalid JSON → 400 with structured error
    #[tokio::test]
    async fn create_hook_invalid_json_returns_400() {
        let app = build_router(test_state());

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/hooks")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from("{bad json"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();

        assert!(json["error"].as_str().unwrap().contains("invalid JSON"));
        assert!(json["suggestion"].is_string());
    }

    // AC-5: Hook persists in database after creation
    #[tokio::test]
    async fn create_hook_persists_in_database() {
        let state = test_state();
        let app = build_router(state.clone());

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/hooks")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(r#"{"name": "Persisted Hook"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CREATED);

        let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        let hook_id = json["hook_id"].as_str().unwrap();

        // Verify the hook exists in the database
        let hook = state.db.get_hook(hook_id).unwrap();
        assert_eq!(hook.name, "Persisted Hook");
        assert_eq!(hook.request_count, 0);
    }

    // Edge case: whitespace-only name → auto-generated
    #[tokio::test]
    async fn create_hook_whitespace_name_generates_name() {
        let app = build_router(test_state());

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/hooks")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(r#"{"name": "   "}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CREATED);

        let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();

        let name = json["name"].as_str().unwrap();
        assert!(
            name.starts_with("hook-"),
            "whitespace name should auto-generate, got: {name}"
        );
    }

    // -- HB-010: Webhook ingestion tests --

    // AC-1: POST /h/{hook_id} captures request and returns 200 with request_id
    #[tokio::test]
    async fn ingest_post_returns_200_with_request_id() {
        let state = test_state();
        let hook_id = create_test_hook(&state, "Ingest Test").await;
        let app = build_test_router(state.clone());

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/h/{hook_id}"))
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(r#"{"event": "push"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();

        assert!(json["request_id"].is_string());
        assert!(!json["request_id"].as_str().unwrap().is_empty());
        assert_eq!(json["hook_id"], hook_id);

        // Verify it's a valid UUID
        let rid = json["request_id"].as_str().unwrap();
        uuid::Uuid::parse_str(rid).unwrap();

        // AC-1: Verify stored in SQLite
        let requests = state.db.list_requests(&hook_id, 10).unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].request_id, rid);
        assert_eq!(requests[0].method, "POST");
        assert_eq!(requests[0].body, b"{\"event\": \"push\"}");
    }

    // AC-2: All HTTP methods are captured correctly
    #[tokio::test]
    async fn ingest_captures_all_http_methods() {
        let state = test_state();
        let hook_id = create_test_hook(&state, "Methods Test").await;

        let methods = ["POST", "PUT", "PATCH", "DELETE", "GET"];
        for method in methods {
            let app = build_test_router(state.clone());
            let response = app
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(format!("/h/{hook_id}"))
                        .body(axum::body::Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(
                response.status(),
                StatusCode::OK,
                "{method} should return 200"
            );
        }

        // Verify all 5 methods were stored
        let requests = state.db.list_requests(&hook_id, 10).unwrap();
        assert_eq!(requests.len(), 5);

        let stored_methods: Vec<&str> = requests.iter().map(|r| r.method.as_str()).collect();
        for method in methods {
            assert!(
                stored_methods.contains(&method),
                "method {method} should be stored"
            );
        }
    }

    // AC-3: Nonexistent hook returns 404
    #[tokio::test]
    async fn ingest_nonexistent_hook_returns_404() {
        let state = test_state();
        let app = build_test_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/h/nonexistent-hook-id")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();

        assert!(json["error"].as_str().unwrap().contains("not found"));
        assert!(json["suggestion"].is_string());
    }

    // AC-4: Headers are captured, duplicates joined with ", "
    #[tokio::test]
    async fn ingest_captures_headers() {
        let state = test_state();
        let hook_id = create_test_hook(&state, "Headers Test").await;
        let app = build_test_router(state.clone());

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/h/{hook_id}"))
                    .header("x-custom", "value1")
                    .header("x-another", "value2")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let requests = state.db.list_requests(&hook_id, 10).unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].headers.get("x-custom").unwrap(), "value1");
        assert_eq!(requests[0].headers.get("x-another").unwrap(), "value2");
    }

    // AC-5: Query string is captured in path field
    #[tokio::test]
    async fn ingest_captures_query_string() {
        let state = test_state();
        let hook_id = create_test_hook(&state, "Query Test").await;
        let app = build_test_router(state.clone());

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/h/{hook_id}?foo=bar&baz=1"))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let requests = state.db.list_requests(&hook_id, 10).unwrap();
        assert_eq!(requests.len(), 1);

        let path = &requests[0].path;
        assert!(
            path.contains("foo=bar"),
            "query string should be in path, got: {path}"
        );
        assert!(
            path.contains("baz=1"),
            "query string should be in path, got: {path}"
        );
    }

    // AC-6: Empty body captured correctly
    #[tokio::test]
    async fn ingest_empty_body() {
        let state = test_state();
        let hook_id = create_test_hook(&state, "Empty Body Test").await;
        let app = build_test_router(state.clone());

        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/h/{hook_id}"))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let requests = state.db.list_requests(&hook_id, 10).unwrap();
        assert_eq!(requests.len(), 1);
        assert!(requests[0].body.is_empty());
        assert_eq!(requests[0].content_length, 0);
    }

    // AC-7: Binary body stored without corruption
    #[tokio::test]
    async fn ingest_binary_body() {
        let state = test_state();
        let hook_id = create_test_hook(&state, "Binary Body Test").await;
        let app = build_test_router(state.clone());

        let binary_data: Vec<u8> = vec![0x00, 0xFF, 0xFE, 0x80, 0x01, 0x02, 0x03];

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/h/{hook_id}"))
                    .header("content-type", "application/octet-stream")
                    .body(axum::body::Body::from(binary_data.clone()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let requests = state.db.list_requests(&hook_id, 10).unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].body, binary_data);
        assert_eq!(requests[0].content_length, 7);
    }

    // AC-8: request_count is incremented on ingestion
    #[tokio::test]
    async fn ingest_increments_request_count() {
        let state = test_state();
        let hook_id = create_test_hook(&state, "Count Test").await;

        // Ingest 3 requests
        for i in 0..3 {
            let app = build_test_router(state.clone());
            let response = app
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(format!("/h/{hook_id}"))
                        .body(axum::body::Body::from(format!("request-{i}")))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }

        let hook = state.db.get_hook(&hook_id).unwrap();
        assert_eq!(hook.request_count, 3);
    }

    // AC-9: Source IP is captured from ConnectInfo
    #[tokio::test]
    async fn ingest_captures_source_ip() {
        let state = test_state();
        let hook_id = create_test_hook(&state, "IP Test").await;
        let app = build_test_router(state.clone());

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/h/{hook_id}"))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let requests = state.db.list_requests(&hook_id, 10).unwrap();
        assert_eq!(requests.len(), 1);
        // Our test helper sets ConnectInfo to 192.168.1.42:12345
        assert_eq!(requests[0].source_ip, "192.168.1.42");
    }
}
