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
    // Set body limit slightly above max_payload so our handler's check
    // produces a structured JSON 413 for normal oversized payloads,
    // while DefaultBodyLimit acts as a safety net against truly huge bodies.
    let body_limit = state.config.max_payload.saturating_add(1);

    Router::new()
        .route(
            "/health",
            axum::routing::get(handlers::health::health_check),
        )
        .route(
            "/api/hooks",
            axum::routing::get(handlers::hooks::list_hooks).post(handlers::hooks::create_hook),
        )
        .route(
            "/api/hooks/{hook_id}",
            axum::routing::get(handlers::hooks::get_hook_detail)
                .delete(handlers::hooks::delete_hook),
        )
        .route(
            "/h/{hook_id}",
            axum::routing::any(handlers::ingest::ingest_webhook),
        )
        .layer(axum::extract::DefaultBodyLimit::max(body_limit))
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

    /// Build a test state with a custom config.
    fn test_state_with_config(config: Config) -> AppState {
        let db = Database::open_in_memory().expect("test db");
        AppState {
            db: Arc::new(db),
            config: Arc::new(config),
            start_time: Instant::now(),
        }
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

    // -- HB-011: Payload size enforcement tests --

    // AC-1: Body exceeding max_payload returns 413 with structured error
    #[tokio::test]
    async fn ingest_oversized_payload_returns_413() {
        let config = Config {
            max_payload: 1024,
            ..Config::default()
        };
        let state = test_state_with_config(config);
        let hook_id = create_test_hook(&state, "Payload Test").await;
        let app = build_test_router(state);

        let oversized_body = vec![b'x'; 2000];

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/h/{hook_id}"))
                    .body(axum::body::Body::from(oversized_body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);

        let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["status"], 413);
        assert!(json["error"]
            .as_str()
            .unwrap()
            .contains("payload too large"));
        assert!(json["suggestion"]
            .as_str()
            .unwrap()
            .contains("--max-payload"));
    }

    // AC-2: Body exactly at max_payload is accepted
    #[tokio::test]
    async fn ingest_payload_at_limit_accepted() {
        let config = Config {
            max_payload: 1024,
            ..Config::default()
        };
        let state = test_state_with_config(config);
        let hook_id = create_test_hook(&state, "Limit Test").await;
        let app = build_test_router(state.clone());

        let exact_body = vec![b'x'; 1024];

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/h/{hook_id}"))
                    .body(axum::body::Body::from(exact_body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        // Verify stored
        let requests = state.db.list_requests(&hook_id, 10).unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].content_length, 1024);
    }

    // AC-2: Body one byte over limit is rejected
    #[tokio::test]
    async fn ingest_payload_one_over_limit_rejected() {
        let config = Config {
            max_payload: 1024,
            ..Config::default()
        };
        let state = test_state_with_config(config);
        let hook_id = create_test_hook(&state, "Over Limit Test").await;
        let app = build_test_router(state);

        let over_body = vec![b'x'; 1025];

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/h/{hook_id}"))
                    .body(axum::body::Body::from(over_body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    // AC-3: Empty body with low max_payload still accepted
    #[tokio::test]
    async fn ingest_empty_body_with_low_limit_accepted() {
        let config = Config {
            max_payload: 1024,
            ..Config::default()
        };
        let state = test_state_with_config(config);
        let hook_id = create_test_hook(&state, "Empty With Limit").await;
        let app = build_test_router(state);

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
    }

    // AC-4: Default config (1MB) accepts small payloads
    #[tokio::test]
    async fn ingest_default_limit_accepts_small_payload() {
        let state = test_state();
        let hook_id = create_test_hook(&state, "Default Limit").await;
        let app = build_test_router(state);

        let small_body = vec![b'x'; 1000];

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/h/{hook_id}"))
                    .body(axum::body::Body::from(small_body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    // AC-5: Rejection is structured JSON, not plain text
    #[tokio::test]
    async fn ingest_payload_rejection_is_structured_json() {
        let config = Config {
            max_payload: 100,
            ..Config::default()
        };
        let state = test_state_with_config(config);
        let hook_id = create_test_hook(&state, "JSON Error Test").await;
        let app = build_test_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/h/{hook_id}"))
                    .body(axum::body::Body::from(vec![b'x'; 200]))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);

        let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();

        // Must have all three structured error fields
        assert!(json["error"].is_string(), "missing 'error' field");
        assert!(json["status"].is_number(), "missing 'status' field");
        assert!(json["suggestion"].is_string(), "missing 'suggestion' field");
    }

    // -- HB-012: Resource bounds enforcement tests --

    // AC-1: Creating a hook when at max_hooks returns 409
    #[tokio::test]
    async fn create_hook_at_max_hooks_returns_409() {
        let config = Config {
            max_hooks: 2,
            ..Config::default()
        };
        let state = test_state_with_config(config);

        // Create 2 hooks (at the limit)
        create_test_hook(&state, "Hook 1").await;
        create_test_hook(&state, "Hook 2").await;

        // Third hook should be rejected
        let app = build_router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/hooks")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(r#"{"name": "Hook 3"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CONFLICT);

        let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["status"], 409);
        assert!(json["error"]
            .as_str()
            .unwrap()
            .contains("hook limit reached"));
        assert!(json["error"].as_str().unwrap().contains("2/2"));
        assert!(json["suggestion"].is_string());
    }

    // AC-2: Creating a hook when under max_hooks succeeds
    #[tokio::test]
    async fn create_hook_under_max_hooks_succeeds() {
        let config = Config {
            max_hooks: 2,
            ..Config::default()
        };
        let state = test_state_with_config(config);

        // Create 1 hook (under the limit)
        create_test_hook(&state, "Hook 1").await;

        // Second hook should succeed
        let app = build_router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/hooks")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(r#"{"name": "Hook 2"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CREATED);
    }

    // AC-3: Ingesting at max_requests evicts oldest, keeps total at max_requests
    #[tokio::test]
    async fn ingest_at_max_requests_evicts_oldest() {
        let config = Config {
            max_requests: 3,
            ..Config::default()
        };
        let state = test_state_with_config(config);
        let hook_id = create_test_hook(&state, "Eviction Test").await;

        // Ingest 3 requests (at capacity)
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

        // Verify 3 stored
        assert_eq!(state.db.list_requests(&hook_id, 10).unwrap().len(), 3);

        // Collect all request_ids before the 4th insert
        let ids_before: Vec<String> = state
            .db
            .list_requests(&hook_id, 10)
            .unwrap()
            .iter()
            .map(|r| r.request_id.clone())
            .collect();

        // Ingest a 4th request — should evict one of the oldest
        let app = build_test_router(state.clone());
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/h/{hook_id}"))
                    .body(axum::body::Body::from("request-new"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Verify total is still 3 (one was evicted)
        let requests_after = state.db.list_requests(&hook_id, 10).unwrap();
        assert_eq!(requests_after.len(), 3);

        // Verify the new request is present
        let has_new = requests_after.iter().any(|r| r.body == b"request-new");
        assert!(has_new, "new request should be stored");

        // Verify one of the old requests was evicted
        let remaining_old: Vec<_> = requests_after
            .iter()
            .filter(|r| ids_before.contains(&r.request_id))
            .collect();
        assert_eq!(
            remaining_old.len(),
            2,
            "one old request should have been evicted"
        );
    }

    // AC-4: Ingesting under max_requests stores normally (no eviction)
    #[tokio::test]
    async fn ingest_under_max_requests_no_eviction() {
        let config = Config {
            max_requests: 3,
            ..Config::default()
        };
        let state = test_state_with_config(config);
        let hook_id = create_test_hook(&state, "Under Limit").await;

        // Ingest 1 request
        let app = build_test_router(state.clone());
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/h/{hook_id}"))
                    .body(axum::body::Body::from("first"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Ingest a 2nd request — no eviction expected
        let app = build_test_router(state.clone());
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/h/{hook_id}"))
                    .body(axum::body::Body::from("second"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let requests = state.db.list_requests(&hook_id, 10).unwrap();
        assert_eq!(requests.len(), 2);
    }

    // AC-1 edge: max_hooks = 0 means no hooks can be created
    #[tokio::test]
    async fn create_hook_max_hooks_zero_rejects() {
        let config = Config {
            max_hooks: 0,
            ..Config::default()
        };
        let state = test_state_with_config(config);
        let app = build_router(state);

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

        assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    // AC-3 edge: max_requests = 1 means only the latest request is kept
    #[tokio::test]
    async fn ingest_max_requests_one_keeps_only_latest() {
        let config = Config {
            max_requests: 1,
            ..Config::default()
        };
        let state = test_state_with_config(config);
        let hook_id = create_test_hook(&state, "Single Slot").await;

        // Ingest first
        let app = build_test_router(state.clone());
        app.oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/h/{hook_id}"))
                .body(axum::body::Body::from("first"))
                .unwrap(),
        )
        .await
        .unwrap();

        // Ingest second — should evict first
        let app = build_test_router(state.clone());
        app.oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/h/{hook_id}"))
                .body(axum::body::Body::from("second"))
                .unwrap(),
        )
        .await
        .unwrap();

        let requests = state.db.list_requests(&hook_id, 10).unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].body, b"second");
    }

    // AC-1: 409 error includes structured JSON with suggestion
    #[tokio::test]
    async fn create_hook_limit_error_is_structured_json() {
        let config = Config {
            max_hooks: 1,
            ..Config::default()
        };
        let state = test_state_with_config(config);
        create_test_hook(&state, "Only Hook").await;

        let app = build_router(state);
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

        assert_eq!(response.status(), StatusCode::CONFLICT);

        let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();

        assert!(json["error"].is_string(), "missing 'error' field");
        assert!(json["status"].is_number(), "missing 'status' field");
        assert!(json["suggestion"].is_string(), "missing 'suggestion' field");
        assert!(json["error"].as_str().unwrap().contains("--max-hooks"));
    }

    // -- HB-014: List hooks tests --

    // AC-1: GET /api/hooks returns all hooks with count
    #[tokio::test]
    async fn list_hooks_returns_all_hooks() {
        let state = test_state();
        create_test_hook(&state, "Hook A").await;
        create_test_hook(&state, "Hook B").await;
        create_test_hook(&state, "Hook C").await;

        let app = build_router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/hooks")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["count"], 3);
        assert_eq!(json["hooks"].as_array().unwrap().len(), 3);
    }

    // AC-2: GET /api/hooks with no hooks returns empty array
    #[tokio::test]
    async fn list_hooks_empty_returns_zero() {
        let app = build_router(test_state());

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/hooks")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["count"], 0);
        assert!(json["hooks"].as_array().unwrap().is_empty());
    }

    // AC-3: Each hook has all required fields
    #[tokio::test]
    async fn list_hooks_contains_all_fields() {
        let state = test_state();
        create_test_hook(&state, "Field Check").await;

        let app = build_router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/hooks")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();

        let hook = &json["hooks"][0];
        assert!(hook["hook_id"].is_string(), "missing hook_id");
        assert_eq!(hook["name"], "Field Check");
        assert!(hook["created_at"].is_number(), "missing created_at");
        assert_eq!(hook["request_count"], 0);
        assert!(
            hook["url"].as_str().unwrap().starts_with("/h/"),
            "url should start with /h/"
        );
    }

    // AC-4: Hooks are ordered by created_at descending
    #[tokio::test]
    async fn list_hooks_ordered_newest_first() {
        let state = test_state();

        // Insert hooks with distinct created_at by using the DB directly
        use crate::models::Hook;
        let mut h1 = Hook::new("Alpha");
        h1.created_at = 1000;
        let mut h2 = Hook::new("Beta");
        h2.created_at = 2000;
        let mut h3 = Hook::new("Gamma");
        h3.created_at = 3000;

        state.db.insert_hook(&h1).unwrap();
        state.db.insert_hook(&h2).unwrap();
        state.db.insert_hook(&h3).unwrap();

        let app = build_router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/hooks")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();

        let hooks = json["hooks"].as_array().unwrap();
        assert_eq!(hooks.len(), 3);
        assert_eq!(hooks[0]["name"], "Gamma"); // newest (3000)
        assert_eq!(hooks[1]["name"], "Beta"); // (2000)
        assert_eq!(hooks[2]["name"], "Alpha"); // oldest (1000)
    }

    // AC-5: Single hook listing
    #[tokio::test]
    async fn list_hooks_single_hook() {
        let state = test_state();
        create_test_hook(&state, "Only Hook").await;

        let app = build_router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/hooks")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["count"], 1);
        assert_eq!(json["hooks"][0]["name"], "Only Hook");
    }

    // -- HB-015: Hook detail and deletion tests --

    // AC-1: GET /api/hooks/{id} returns hook details
    #[tokio::test]
    async fn get_hook_detail_returns_200() {
        let state = test_state();
        let hook_id = create_test_hook(&state, "Detail Test").await;
        let app = build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri(format!("/api/hooks/{hook_id}"))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["hook_id"], hook_id);
        assert_eq!(json["name"], "Detail Test");
        assert!(json["created_at"].is_number());
        assert_eq!(json["request_count"], 0);
        assert_eq!(json["url"], format!("/h/{hook_id}"));
    }

    // AC-2: GET /api/hooks/{id} with nonexistent hook returns 404
    #[tokio::test]
    async fn get_hook_detail_not_found() {
        let app = build_router(test_state());

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/hooks/nonexistent")
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

    // AC-3: DELETE /api/hooks/{id} removes hook and CASCADE-deletes requests
    #[tokio::test]
    async fn delete_hook_removes_hook_and_requests() {
        let state = test_state();
        let hook_id = create_test_hook(&state, "Delete Me").await;

        // Ingest some requests
        for i in 0..5 {
            let app = build_test_router(state.clone());
            app.oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/h/{hook_id}"))
                    .body(axum::body::Body::from(format!("req-{i}")))
                    .unwrap(),
            )
            .await
            .unwrap();
        }

        // Verify 5 requests stored
        assert_eq!(state.db.list_requests(&hook_id, 10).unwrap().len(), 5);

        // Delete the hook
        let app = build_router(state.clone());
        let response = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/api/hooks/{hook_id}"))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["deleted"], true);
        assert_eq!(json["hook_id"], hook_id);

        // Verify hook is gone
        assert!(state.db.get_hook(&hook_id).is_err());

        // Verify requests are CASCADE-deleted
        assert!(state.db.list_requests(&hook_id, 10).unwrap().is_empty());
    }

    // AC-4: DELETE /api/hooks/{id} with nonexistent hook returns 404
    #[tokio::test]
    async fn delete_hook_not_found() {
        let app = build_router(test_state());

        let response = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/api/hooks/nonexistent")
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

    // AC-5: Deleted hook no longer appears in GET /api/hooks list
    #[tokio::test]
    async fn deleted_hook_not_in_list() {
        let state = test_state();
        let hook_a = create_test_hook(&state, "Keep").await;
        let hook_b = create_test_hook(&state, "Remove").await;

        // Delete hook_b
        let app = build_router(state.clone());
        let response = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/api/hooks/{hook_b}"))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // List hooks — only hook_a should remain
        let app = build_router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/hooks")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["count"], 1);
        assert_eq!(json["hooks"][0]["hook_id"], hook_a);
    }

    // AC-3 edge: GET detail shows updated request_count after ingestion
    #[tokio::test]
    async fn get_hook_detail_reflects_request_count() {
        let state = test_state();
        let hook_id = create_test_hook(&state, "Counting").await;

        // Ingest 3 requests
        for _ in 0..3 {
            let app = build_test_router(state.clone());
            app.oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/h/{hook_id}"))
                    .body(axum::body::Body::from("data"))
                    .unwrap(),
            )
            .await
            .unwrap();
        }

        let app = build_router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .uri(format!("/api/hooks/{hook_id}"))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["request_count"], 3);
    }
}
