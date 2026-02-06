use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

/// Application error type.
///
/// Every variant maps to an HTTP status code and includes a user-facing suggestion.
/// Errors are never swallowed — they always produce structured JSON responses.
#[allow(dead_code)]
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("not found: {0}")]
    NotFound(String),

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("rate limited")]
    RateLimited,

    #[error("payload too large: {size} bytes (max {max})")]
    PayloadTooLarge { size: usize, max: usize },

    #[error("limit reached: {0}")]
    LimitReached(String),

    #[error("internal error: {0}")]
    Internal(String),
}

impl AppError {
    fn status_code(&self) -> StatusCode {
        match self {
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::InvalidInput(_) => StatusCode::BAD_REQUEST,
            AppError::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            AppError::PayloadTooLarge { .. } => StatusCode::PAYLOAD_TOO_LARGE,
            AppError::LimitReached(_) => StatusCode::CONFLICT,
            AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn suggestion(&self) -> &'static str {
        match self {
            AppError::NotFound(_) => "Check the hook ID and try again",
            AppError::InvalidInput(_) => "Check your request parameters and try again",
            AppError::RateLimited => "Wait and retry, or increase your rate limit",
            AppError::PayloadTooLarge { .. } => "Reduce payload size or configure --max-payload",
            AppError::LimitReached(_) => {
                "Delete unused resources or increase the limit via CLI flags"
            }
            AppError::Internal(_) => "This is a bug \u{2014} please report it",
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let body = json!({
            "error": self.to_string(),
            "status": status.as_u16(),
            "suggestion": self.suggestion(),
        });

        (status, axum::Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use serde_json::Value;

    async fn error_to_json(err: AppError) -> (StatusCode, Value) {
        let response = err.into_response();
        let status = response.status();
        let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        (status, json)
    }

    #[tokio::test]
    async fn not_found_returns_404_with_suggestion() {
        let (status, json) = error_to_json(AppError::NotFound("hook abc123".into())).await;

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(json["status"], 404);
        assert_eq!(json["error"], "not found: hook abc123");
        assert_eq!(json["suggestion"], "Check the hook ID and try again");
    }

    #[tokio::test]
    async fn invalid_input_returns_400_with_suggestion() {
        let (status, json) =
            error_to_json(AppError::InvalidInput("name cannot be empty".into())).await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(json["status"], 400);
        assert_eq!(json["error"], "invalid input: name cannot be empty");
        assert_eq!(
            json["suggestion"],
            "Check your request parameters and try again"
        );
    }

    #[tokio::test]
    async fn rate_limited_returns_429_with_suggestion() {
        let (status, json) = error_to_json(AppError::RateLimited).await;

        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(json["status"], 429);
        assert_eq!(json["error"], "rate limited");
        assert_eq!(
            json["suggestion"],
            "Wait and retry, or increase your rate limit"
        );
    }

    #[tokio::test]
    async fn payload_too_large_returns_413_with_size_info() {
        let (status, json) = error_to_json(AppError::PayloadTooLarge {
            size: 2_000_000,
            max: 1_048_576,
        })
        .await;

        assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(json["status"], 413);
        assert_eq!(
            json["error"],
            "payload too large: 2000000 bytes (max 1048576)"
        );
        assert_eq!(
            json["suggestion"],
            "Reduce payload size or configure --max-payload"
        );
    }

    #[tokio::test]
    async fn limit_reached_returns_409_with_suggestion() {
        let (status, json) =
            error_to_json(AppError::LimitReached("hook limit reached (2/2)".into())).await;

        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(json["status"], 409);
        assert_eq!(json["error"], "limit reached: hook limit reached (2/2)");
        assert!(json["suggestion"]
            .as_str()
            .unwrap()
            .contains("Delete unused"));
    }

    #[tokio::test]
    async fn internal_returns_500_with_bug_report_suggestion() {
        let (status, json) = error_to_json(AppError::Internal("disk write failed".into())).await;

        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(json["status"], 500);
        assert_eq!(json["error"], "internal error: disk write failed");
        assert_eq!(
            json["suggestion"],
            "This is a bug \u{2014} please report it"
        );
    }

    #[tokio::test]
    async fn all_responses_have_required_fields() {
        let errors: Vec<AppError> = vec![
            AppError::NotFound("x".into()),
            AppError::InvalidInput("y".into()),
            AppError::RateLimited,
            AppError::PayloadTooLarge { size: 1, max: 0 },
            AppError::LimitReached("test".into()),
            AppError::Internal("z".into()),
        ];

        for err in errors {
            let (_, json) = error_to_json(err).await;
            assert!(json["error"].is_string(), "missing 'error' field");
            assert!(json["status"].is_number(), "missing 'status' field");
            assert!(json["suggestion"].is_string(), "missing 'suggestion' field");
        }
    }

    #[test]
    fn display_matches_thiserror_format() {
        assert_eq!(
            AppError::NotFound("hook abc".into()).to_string(),
            "not found: hook abc"
        );
        assert_eq!(
            AppError::InvalidInput("bad".into()).to_string(),
            "invalid input: bad"
        );
        assert_eq!(AppError::RateLimited.to_string(), "rate limited");
        assert_eq!(
            AppError::PayloadTooLarge { size: 100, max: 50 }.to_string(),
            "payload too large: 100 bytes (max 50)"
        );
        assert_eq!(
            AppError::LimitReached("max hooks".into()).to_string(),
            "limit reached: max hooks"
        );
        assert_eq!(
            AppError::Internal("oops".into()).to_string(),
            "internal error: oops"
        );
    }
}
