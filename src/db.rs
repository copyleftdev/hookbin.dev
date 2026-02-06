#![allow(dead_code)]

use std::collections::HashMap;
use std::path::Path;

use rusqlite::{params, Connection};

use crate::error::AppError;
use crate::models::{CapturedRequest, Hook};

/// SQLite database wrapper with WAL mode and embedded migrations.
pub struct Database {
    conn: Connection,
}

impl Database {
    /// Open or create the SQLite database at `{data_dir}/hookbin.db`.
    ///
    /// Creates the data directory if it doesn't exist, enables WAL mode
    /// and foreign keys, then runs idempotent migrations.
    pub fn open(data_dir: &Path) -> Result<Self, AppError> {
        std::fs::create_dir_all(data_dir).map_err(|e| {
            AppError::Internal(format!(
                "cannot create data directory '{}': {}",
                data_dir.display(),
                e
            ))
        })?;

        let db_path = data_dir.join("hookbin.db");
        let conn = Connection::open(&db_path).map_err(|e| {
            AppError::Internal(format!(
                "cannot open database '{}': {}",
                db_path.display(),
                e
            ))
        })?;

        Self::configure(&conn)?;
        Self::run_migrations(&conn)?;

        Ok(Self { conn })
    }

    /// Open an in-memory database for testing.
    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self, AppError> {
        let conn = Connection::open_in_memory()
            .map_err(|e| AppError::Internal(format!("cannot open in-memory database: {e}")))?;

        // WAL mode is not supported for :memory:, but we still enable foreign keys
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(|e| AppError::Internal(format!("cannot enable foreign keys: {e}")))?;

        Self::run_migrations(&conn)?;

        Ok(Self { conn })
    }

    /// Configure SQLite pragmas: WAL mode, foreign keys.
    fn configure(conn: &Connection) -> Result<(), AppError> {
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;
             PRAGMA synchronous = NORMAL;",
        )
        .map_err(|e| AppError::Internal(format!("cannot configure database pragmas: {e}")))?;
        Ok(())
    }

    /// Run idempotent schema migrations (CREATE TABLE IF NOT EXISTS).
    fn run_migrations(conn: &Connection) -> Result<(), AppError> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS hooks (
                hook_id       TEXT PRIMARY KEY NOT NULL,
                name          TEXT NOT NULL,
                created_at    INTEGER NOT NULL,
                request_count INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS requests (
                request_id     TEXT PRIMARY KEY NOT NULL,
                hook_id        TEXT NOT NULL,
                method         TEXT NOT NULL,
                path           TEXT NOT NULL,
                headers        TEXT NOT NULL,
                body           BLOB NOT NULL,
                content_length INTEGER NOT NULL,
                source_ip      TEXT NOT NULL,
                received_at    INTEGER NOT NULL,
                FOREIGN KEY (hook_id) REFERENCES hooks(hook_id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_requests_hook_id_received
                ON requests(hook_id, received_at DESC);

            CREATE TABLE IF NOT EXISTS tokens (
                token_id   TEXT PRIMARY KEY NOT NULL,
                hook_id    TEXT,
                created_at INTEGER NOT NULL,
                expires_at INTEGER NOT NULL,
                FOREIGN KEY (hook_id) REFERENCES hooks(hook_id) ON DELETE CASCADE
            );",
        )
        .map_err(|e| AppError::Internal(format!("migration failed: {e}")))?;

        Ok(())
    }

    // -- Hook operations --

    /// Insert a new hook.
    pub fn insert_hook(&self, hook: &Hook) -> Result<(), AppError> {
        self.conn
            .execute(
                "INSERT INTO hooks (hook_id, name, created_at, request_count)
                 VALUES (?1, ?2, ?3, ?4)",
                params![hook.hook_id, hook.name, hook.created_at, hook.request_count],
            )
            .map_err(|e| {
                AppError::Internal(format!("failed to insert hook '{}': {}", hook.hook_id, e))
            })?;
        Ok(())
    }

    /// Fetch a hook by ID.
    pub fn get_hook(&self, hook_id: &str) -> Result<Hook, AppError> {
        self.conn
            .query_row(
                "SELECT hook_id, name, created_at, request_count FROM hooks WHERE hook_id = ?1",
                params![hook_id],
                |row| {
                    Ok(Hook {
                        hook_id: row.get(0)?,
                        name: row.get(1)?,
                        created_at: row.get(2)?,
                        request_count: row.get(3)?,
                    })
                },
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    AppError::NotFound(format!("hook '{hook_id}' not found"))
                }
                other => AppError::Internal(format!("failed to get hook '{hook_id}': {other}")),
            })
    }

    // -- Request operations --

    /// Insert a captured webhook request.
    pub fn insert_request(&self, req: &CapturedRequest) -> Result<(), AppError> {
        let headers_json = serde_json::to_string(&req.headers)
            .map_err(|e| AppError::Internal(format!("failed to serialize headers: {e}")))?;

        self.conn
            .execute(
                "INSERT INTO requests (request_id, hook_id, method, path, headers, body, content_length, source_ip, received_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    req.request_id,
                    req.hook_id,
                    req.method,
                    req.path,
                    headers_json,
                    req.body,
                    req.content_length,
                    req.source_ip,
                    req.received_at,
                ],
            )
            .map_err(|e| {
                AppError::Internal(format!(
                    "failed to insert request '{}': {}",
                    req.request_id, e
                ))
            })?;

        // Increment the hook's request_count
        self.conn
            .execute(
                "UPDATE hooks SET request_count = request_count + 1 WHERE hook_id = ?1",
                params![req.hook_id],
            )
            .map_err(|e| {
                AppError::Internal(format!(
                    "failed to update request count for hook '{}': {}",
                    req.hook_id, e
                ))
            })?;

        Ok(())
    }

    /// List captured requests for a hook, newest first.
    pub fn list_requests(
        &self,
        hook_id: &str,
        limit: u32,
    ) -> Result<Vec<CapturedRequest>, AppError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT request_id, hook_id, method, path, headers, body, content_length, source_ip, received_at
                 FROM requests
                 WHERE hook_id = ?1
                 ORDER BY received_at DESC
                 LIMIT ?2",
            )
            .map_err(|e| AppError::Internal(format!("failed to prepare query: {e}")))?;

        let rows = stmt
            .query_map(params![hook_id, limit], |row| {
                let headers_json: String = row.get(4)?;
                let headers: HashMap<String, String> =
                    serde_json::from_str(&headers_json).unwrap_or_default();

                Ok(CapturedRequest {
                    request_id: row.get(0)?,
                    hook_id: row.get(1)?,
                    method: row.get(2)?,
                    path: row.get(3)?,
                    headers,
                    body: row.get(5)?,
                    content_length: row.get(6)?,
                    source_ip: row.get(7)?,
                    received_at: row.get(8)?,
                })
            })
            .map_err(|e| {
                AppError::Internal(format!("failed to list requests for hook '{hook_id}': {e}"))
            })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(
                row.map_err(|e| AppError::Internal(format!("failed to read request row: {e}")))?,
            );
        }

        Ok(results)
    }

    /// Get the underlying connection (for pragma checks in tests).
    #[cfg(test)]
    pub fn conn(&self) -> &Connection {
        &self.conn
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Hook;
    use std::collections::HashMap;

    fn test_db() -> Database {
        Database::open_in_memory().expect("failed to open test database")
    }

    fn make_hook(id: &str, name: &str) -> Hook {
        Hook {
            hook_id: id.to_owned(),
            name: name.to_owned(),
            created_at: 1700000000,
            request_count: 0,
        }
    }

    fn make_request(req_id: &str, hook_id: &str) -> CapturedRequest {
        let mut headers = HashMap::new();
        headers.insert("content-type".to_owned(), "application/json".to_owned());
        headers.insert("x-custom".to_owned(), "test-value".to_owned());

        CapturedRequest {
            request_id: req_id.to_owned(),
            hook_id: hook_id.to_owned(),
            method: "POST".to_owned(),
            path: "/webhook".to_owned(),
            headers,
            body: b"hello world".to_vec(),
            content_length: 11,
            source_ip: "192.168.1.1".to_owned(),
            received_at: 1700000001,
        }
    }

    // AC-1: Database creation (in-memory variant — file-based tested via AC-3)
    #[test]
    fn open_in_memory_creates_schema() {
        let db = test_db();
        // Verify tables exist by querying sqlite_master
        let count: i32 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('hooks', 'requests', 'tokens')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 3, "expected 3 tables: hooks, requests, tokens");
    }

    // AC-1: File-based database creation
    #[test]
    fn open_creates_directory_and_database() {
        let tmp = std::env::temp_dir().join(format!("hookbin-test-{}", nanoid::nanoid!()));
        assert!(!tmp.exists());

        let db = Database::open(&tmp).expect("should create directory and db");

        assert!(tmp.exists(), "data directory should be created");
        assert!(tmp.join("hookbin.db").exists(), "db file should exist");

        // Verify WAL mode on file-based database
        let mode: String = db
            .conn()
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        assert_eq!(mode, "wal");

        // Clean up
        std::fs::remove_dir_all(&tmp).ok();
    }

    // AC-2: Idempotent migrations
    #[test]
    fn migrations_are_idempotent() {
        let tmp = std::env::temp_dir().join(format!("hookbin-test-{}", nanoid::nanoid!()));

        // Open twice — second open should not error
        {
            let _db1 = Database::open(&tmp).expect("first open");
        }
        {
            let _db2 = Database::open(&tmp).expect("second open should be idempotent");
        }

        std::fs::remove_dir_all(&tmp).ok();
    }

    // AC-3: WAL mode (file-based)
    #[test]
    fn file_database_uses_wal_mode() {
        let tmp = std::env::temp_dir().join(format!("hookbin-test-{}", nanoid::nanoid!()));
        let db = Database::open(&tmp).expect("should open");

        let mode: String = db
            .conn()
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        assert_eq!(mode, "wal");

        std::fs::remove_dir_all(&tmp).ok();
    }

    // AC-4: Foreign keys enabled
    #[test]
    fn foreign_keys_are_enabled() {
        let db = test_db();
        let fk: i32 = db
            .conn()
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .unwrap();
        assert_eq!(fk, 1, "foreign keys should be enabled");
    }

    // AC-5: Hook round-trip
    #[test]
    fn hook_insert_and_get_round_trips() {
        let db = test_db();
        let hook = make_hook("h1", "My Webhook");

        db.insert_hook(&hook).expect("insert should succeed");
        let fetched = db.get_hook("h1").expect("get should succeed");

        assert_eq!(fetched.hook_id, "h1");
        assert_eq!(fetched.name, "My Webhook");
        assert_eq!(fetched.created_at, 1700000000);
        assert_eq!(fetched.request_count, 0);
    }

    // AC-5: get_hook returns NotFound for missing hook
    #[test]
    fn get_hook_returns_not_found() {
        let db = test_db();
        let err = db.get_hook("nonexistent").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("not found"), "got: {msg}");
    }

    // AC-6: CapturedRequest round-trip
    #[test]
    fn request_insert_and_list_round_trips() {
        let db = test_db();
        let hook = make_hook("h1", "Test Hook");
        db.insert_hook(&hook).unwrap();

        let req = make_request("req-001", "h1");
        db.insert_request(&req).unwrap();

        let requests = db.list_requests("h1", 10).unwrap();
        assert_eq!(requests.len(), 1);

        let fetched = &requests[0];
        assert_eq!(fetched.request_id, "req-001");
        assert_eq!(fetched.hook_id, "h1");
        assert_eq!(fetched.method, "POST");
        assert_eq!(fetched.path, "/webhook");
        assert_eq!(
            fetched.headers.get("content-type").unwrap(),
            "application/json"
        );
        assert_eq!(fetched.headers.get("x-custom").unwrap(), "test-value");
        assert_eq!(fetched.body, b"hello world");
        assert_eq!(fetched.content_length, 11);
        assert_eq!(fetched.source_ip, "192.168.1.1");
        assert_eq!(fetched.received_at, 1700000001);
    }

    // AC-6: Request count is incremented
    #[test]
    fn insert_request_increments_hook_count() {
        let db = test_db();
        let hook = make_hook("h1", "Counter Hook");
        db.insert_hook(&hook).unwrap();

        db.insert_request(&make_request("r1", "h1")).unwrap();
        db.insert_request(&make_request("r2", "h1")).unwrap();

        let fetched = db.get_hook("h1").unwrap();
        assert_eq!(fetched.request_count, 2);
    }

    // AC-6: list_requests returns newest first
    #[test]
    fn list_requests_newest_first() {
        let db = test_db();
        db.insert_hook(&make_hook("h1", "Order Hook")).unwrap();

        let mut r1 = make_request("r1", "h1");
        r1.received_at = 1000;
        let mut r2 = make_request("r2", "h1");
        r2.received_at = 2000;
        let mut r3 = make_request("r3", "h1");
        r3.received_at = 3000;

        db.insert_request(&r1).unwrap();
        db.insert_request(&r2).unwrap();
        db.insert_request(&r3).unwrap();

        let results = db.list_requests("h1", 10).unwrap();
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].request_id, "r3"); // newest
        assert_eq!(results[1].request_id, "r2");
        assert_eq!(results[2].request_id, "r1"); // oldest
    }

    // AC-6: list_requests respects limit
    #[test]
    fn list_requests_respects_limit() {
        let db = test_db();
        db.insert_hook(&make_hook("h1", "Limit Hook")).unwrap();

        for i in 0..5 {
            let mut req = make_request(&format!("r{i}"), "h1");
            req.received_at = 1000 + i as i64;
            db.insert_request(&req).unwrap();
        }

        let results = db.list_requests("h1", 2).unwrap();
        assert_eq!(results.len(), 2);
    }

    // AC-6: list_requests for empty hook returns empty vec
    #[test]
    fn list_requests_empty_hook() {
        let db = test_db();
        db.insert_hook(&make_hook("h1", "Empty Hook")).unwrap();

        let results = db.list_requests("h1", 10).unwrap();
        assert!(results.is_empty());
    }

    // AC-7: Duplicate hook_id → error mapped to AppError
    #[test]
    fn duplicate_hook_id_returns_error() {
        let db = test_db();
        let hook = make_hook("h1", "First");
        db.insert_hook(&hook).unwrap();

        let err = db.insert_hook(&hook).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("internal error"),
            "should map to AppError::Internal, got: {msg}"
        );
    }

    // AC-7: Foreign key violation → error mapped to AppError
    #[test]
    fn request_without_hook_returns_error() {
        let db = test_db();
        let req = make_request("r1", "nonexistent-hook");

        let err = db.insert_request(&req).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("internal error"),
            "should map to AppError::Internal, got: {msg}"
        );
    }

    // AC-6: Empty body and empty headers round-trip
    #[test]
    fn request_with_empty_body_and_headers() {
        let db = test_db();
        db.insert_hook(&make_hook("h1", "Empty Body")).unwrap();

        let req = CapturedRequest {
            request_id: "r-empty".to_owned(),
            hook_id: "h1".to_owned(),
            method: "GET".to_owned(),
            path: "/".to_owned(),
            headers: HashMap::new(),
            body: vec![],
            content_length: 0,
            source_ip: "::1".to_owned(),
            received_at: 1700000000,
        };
        db.insert_request(&req).unwrap();

        let results = db.list_requests("h1", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].headers.is_empty());
        assert!(results[0].body.is_empty());
        assert_eq!(results[0].content_length, 0);
    }

    // Index existence check
    #[test]
    fn requests_index_exists() {
        let db = test_db();
        let count: i32 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='idx_requests_hook_id_received'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            count, 1,
            "index on requests(hook_id, received_at) should exist"
        );
    }
}
