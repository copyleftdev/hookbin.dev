/// Application error type.
///
/// Every variant maps to an HTTP status code and includes a user-facing suggestion.
/// Full implementation in HB-003.
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

    #[error("internal error: {0}")]
    Internal(String),
}
