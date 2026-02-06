#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::net::IpAddr;
use std::time::{SystemTime, UNIX_EPOCH};

/// Returns current Unix timestamp in seconds.
fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// A named webhook inbox with a unique URL.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Hook {
    pub hook_id: String,
    pub name: String,
    pub created_at: i64,
    pub request_count: u64,
}

impl Hook {
    /// Create a new hook with a generated nanoid.
    pub fn new(name: &str) -> Self {
        Self {
            hook_id: nanoid::nanoid!(),
            name: name.to_owned(),
            created_at: now_unix(),
            request_count: 0,
        }
    }
}

impl fmt::Display for Hook {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Hook({}, \"{}\")", self.hook_id, self.name)
    }
}

/// A captured HTTP request stored for inspection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapturedRequest {
    pub request_id: String,
    pub hook_id: String,
    pub method: String,
    pub path: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
    pub content_length: usize,
    pub source_ip: String,
    pub received_at: i64,
}

impl CapturedRequest {
    /// Create a new captured request with a generated UUID v4.
    ///
    /// Headers are flattened from an Axum `HeaderMap` into a `HashMap<String, String>`.
    /// Multiple values for the same header key are joined with `, `.
    pub fn new(
        hook_id: &str,
        method: &str,
        path: &str,
        headers: &axum::http::HeaderMap,
        body: Vec<u8>,
        source_ip: IpAddr,
    ) -> Self {
        let content_length = body.len();
        let mut header_map = HashMap::new();
        for (key, value) in headers {
            let key_str = key.as_str().to_owned();
            let val_str = value.to_str().unwrap_or("<non-utf8>").to_owned();
            header_map
                .entry(key_str)
                .and_modify(|existing: &mut String| {
                    existing.push_str(", ");
                    existing.push_str(&val_str);
                })
                .or_insert(val_str);
        }

        Self {
            request_id: uuid::Uuid::new_v4().to_string(),
            hook_id: hook_id.to_owned(),
            method: method.to_owned(),
            path: path.to_owned(),
            headers: header_map,
            body,
            content_length,
            source_ip: source_ip.to_string(),
            received_at: now_unix(),
        }
    }
}

impl fmt::Display for CapturedRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Request({}, {} {})",
            self.request_id, self.method, self.path
        )
    }
}

/// Optional auth token for hook management.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Token {
    pub token_id: String,
    /// If `None`, the token is global (applies to all hooks).
    pub hook_id: Option<String>,
    pub created_at: i64,
    pub expires_at: i64,
}

impl Token {
    /// Create a new token with a generated nanoid.
    ///
    /// `hook_id`: `Some(id)` for hook-scoped, `None` for global.
    /// `ttl_secs`: time-to-live in seconds from now.
    pub fn new(hook_id: Option<&str>, ttl_secs: u64) -> Self {
        let now = now_unix();
        Self {
            token_id: nanoid::nanoid!(),
            hook_id: hook_id.map(|s| s.to_owned()),
            created_at: now,
            expires_at: now + ttl_secs as i64,
        }
    }

    /// Check whether this token has expired.
    pub fn is_expired(&self) -> bool {
        now_unix() >= self.expires_at
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.hook_id {
            Some(id) => write!(f, "Token({}, hook={})", self.token_id, id),
            None => write!(f, "Token({}, global)", self.token_id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;

    // -- Hook tests --

    #[test]
    fn hook_new_generates_nanoid_and_sets_fields() {
        let hook = Hook::new("my-webhook");
        assert!(!hook.hook_id.is_empty(), "hook_id should be non-empty");
        assert_eq!(hook.name, "my-webhook");
        assert!(
            hook.created_at > 0,
            "created_at should be a positive timestamp"
        );
        assert_eq!(hook.request_count, 0);
    }

    #[test]
    fn hook_ids_are_unique() {
        let a = Hook::new("a");
        let b = Hook::new("b");
        assert_ne!(a.hook_id, b.hook_id);
    }

    #[test]
    fn hook_serialization_round_trip() {
        let hook = Hook::new("test-hook");
        let json = serde_json::to_string(&hook).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        // AC-4: snake_case field names
        assert!(parsed["hook_id"].is_string());
        assert!(parsed["name"].is_string());
        assert!(parsed["created_at"].is_number());
        assert!(parsed["request_count"].is_number());

        // Round-trip
        let deserialized: Hook = serde_json::from_str(&json).unwrap();
        assert_eq!(hook, deserialized);
    }

    #[test]
    fn hook_display() {
        let hook = Hook {
            hook_id: "abc123".into(),
            name: "My Hook".into(),
            created_at: 1000,
            request_count: 5,
        };
        assert_eq!(hook.to_string(), "Hook(abc123, \"My Hook\")");
    }

    // -- CapturedRequest tests --

    #[test]
    fn captured_request_new_sets_all_fields() {
        let mut headers = HeaderMap::new();
        headers.insert("content-type", "application/json".parse().unwrap());
        headers.insert("x-custom", "value".parse().unwrap());

        let body = b"hello world".to_vec();
        let ip: IpAddr = "192.168.1.1".parse().unwrap();

        let req = CapturedRequest::new("hook123", "POST", "/webhook", &headers, body.clone(), ip);

        assert!(!req.request_id.is_empty());
        // Verify it's a valid UUID
        uuid::Uuid::parse_str(&req.request_id).unwrap();

        assert_eq!(req.hook_id, "hook123");
        assert_eq!(req.method, "POST");
        assert_eq!(req.path, "/webhook");
        assert_eq!(req.headers.get("content-type").unwrap(), "application/json");
        assert_eq!(req.headers.get("x-custom").unwrap(), "value");
        assert_eq!(req.body, body);
        assert_eq!(req.content_length, 11);
        assert_eq!(req.source_ip, "192.168.1.1");
        assert!(req.received_at > 0);
    }

    #[test]
    fn captured_request_ids_are_unique() {
        let headers = HeaderMap::new();
        let ip: IpAddr = "127.0.0.1".parse().unwrap();

        let a = CapturedRequest::new("h1", "GET", "/", &headers, vec![], ip);
        let b = CapturedRequest::new("h1", "GET", "/", &headers, vec![], ip);
        assert_ne!(a.request_id, b.request_id);
    }

    #[test]
    fn captured_request_serialization_round_trip() {
        let mut headers = HeaderMap::new();
        headers.insert("host", "example.com".parse().unwrap());

        let ip: IpAddr = "10.0.0.1".parse().unwrap();
        let req = CapturedRequest::new("h1", "PUT", "/data", &headers, vec![1, 2, 3], ip);

        let json = serde_json::to_string(&req).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        // AC-4: snake_case field names
        assert!(parsed["request_id"].is_string());
        assert!(parsed["hook_id"].is_string());
        assert!(parsed["method"].is_string());
        assert!(parsed["path"].is_string());
        assert!(parsed["headers"].is_object());
        assert!(parsed["body"].is_array());
        assert!(parsed["content_length"].is_number());
        assert!(parsed["source_ip"].is_string());
        assert!(parsed["received_at"].is_number());

        // Round-trip
        let deserialized: CapturedRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, deserialized);
    }

    #[test]
    fn captured_request_duplicate_headers_joined() {
        let mut headers = HeaderMap::new();
        headers.append("x-multi", "val1".parse().unwrap());
        headers.append("x-multi", "val2".parse().unwrap());

        let ip: IpAddr = "127.0.0.1".parse().unwrap();
        let req = CapturedRequest::new("h1", "GET", "/", &headers, vec![], ip);

        let multi = req.headers.get("x-multi").unwrap();
        assert!(multi.contains("val1"), "got: {multi}");
        assert!(multi.contains("val2"), "got: {multi}");
    }

    #[test]
    fn captured_request_empty_body() {
        let headers = HeaderMap::new();
        let ip: IpAddr = "::1".parse().unwrap();
        let req = CapturedRequest::new("h1", "DELETE", "/item", &headers, vec![], ip);

        assert_eq!(req.content_length, 0);
        assert!(req.body.is_empty());
        assert_eq!(req.source_ip, "::1");
    }

    #[test]
    fn captured_request_display() {
        let req = CapturedRequest {
            request_id: "550e8400-e29b-41d4-a716-446655440000".into(),
            hook_id: "h1".into(),
            method: "POST".into(),
            path: "/webhook".into(),
            headers: HashMap::new(),
            body: vec![],
            content_length: 0,
            source_ip: "1.2.3.4".into(),
            received_at: 1000,
        };
        assert_eq!(
            req.to_string(),
            "Request(550e8400-e29b-41d4-a716-446655440000, POST /webhook)"
        );
    }

    // -- Token tests --

    #[test]
    fn token_new_scoped_to_hook() {
        let token = Token::new(Some("hook123"), 3600);

        assert!(!token.token_id.is_empty());
        assert_eq!(token.hook_id, Some("hook123".to_owned()));
        assert!(token.created_at > 0);
        assert_eq!(token.expires_at, token.created_at + 3600);
    }

    #[test]
    fn token_new_global() {
        let token = Token::new(None, 7200);

        assert!(token.hook_id.is_none());
        assert_eq!(token.expires_at, token.created_at + 7200);
    }

    #[test]
    fn token_is_not_expired_when_fresh() {
        let token = Token::new(None, 3600);
        assert!(!token.is_expired());
    }

    #[test]
    fn token_is_expired_when_ttl_zero() {
        let token = Token {
            token_id: "t1".into(),
            hook_id: None,
            created_at: 1000,
            expires_at: 1000,
        };
        // expires_at is in the past
        assert!(token.is_expired());
    }

    #[test]
    fn token_serialization_round_trip() {
        let token = Token::new(Some("h1"), 3600);
        let json = serde_json::to_string(&token).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        // AC-4: snake_case field names
        assert!(parsed["token_id"].is_string());
        assert!(parsed["hook_id"].is_string());
        assert!(parsed["created_at"].is_number());
        assert!(parsed["expires_at"].is_number());

        // Round-trip
        let deserialized: Token = serde_json::from_str(&json).unwrap();
        assert_eq!(token, deserialized);
    }

    #[test]
    fn token_global_serializes_hook_id_as_null() {
        let token = Token::new(None, 3600);
        let json = serde_json::to_string(&token).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed["hook_id"].is_null());
    }

    #[test]
    fn token_display_scoped() {
        let token = Token {
            token_id: "tok_abc".into(),
            hook_id: Some("h1".into()),
            created_at: 1000,
            expires_at: 2000,
        };
        assert_eq!(token.to_string(), "Token(tok_abc, hook=h1)");
    }

    #[test]
    fn token_display_global() {
        let token = Token {
            token_id: "tok_xyz".into(),
            hook_id: None,
            created_at: 1000,
            expires_at: 2000,
        };
        assert_eq!(token.to_string(), "Token(tok_xyz, global)");
    }
}
