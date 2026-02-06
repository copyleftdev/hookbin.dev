#![allow(dead_code)]

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection};

use crate::error::AppError;
use crate::models::{CapturedRequest, Hook};

/// SQLite database wrapper with WAL mode and embedded migrations.
///
/// Uses a `Mutex<Connection>` so the database can be shared across
/// async tasks via `Arc<Database>`. rusqlite's `Connection` is `!Sync`,
/// so the mutex provides the required thread safety.
pub struct Database {
    conn: Mutex<Connection>,
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

        Ok(Self {
            conn: Mutex::new(conn),
        })
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

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Lock the connection, mapping a poisoned mutex to AppError.
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>, AppError> {
        self.conn
            .lock()
            .map_err(|e| AppError::Internal(format!("database lock poisoned: {e}")))
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
        let conn = self.lock()?;
        conn.execute(
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
        let conn = self.lock()?;
        conn.query_row(
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

    /// List all hooks, ordered by created_at descending (newest first).
    pub fn list_hooks(&self) -> Result<Vec<Hook>, AppError> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare(
                "SELECT hook_id, name, created_at, request_count
                 FROM hooks
                 ORDER BY created_at DESC",
            )
            .map_err(|e| AppError::Internal(format!("failed to prepare hook list query: {e}")))?;

        let rows = stmt
            .query_map([], |row| {
                Ok(Hook {
                    hook_id: row.get(0)?,
                    name: row.get(1)?,
                    created_at: row.get(2)?,
                    request_count: row.get(3)?,
                })
            })
            .map_err(|e| AppError::Internal(format!("failed to list hooks: {e}")))?;

        let mut hooks = Vec::new();
        for row in rows {
            hooks.push(
                row.map_err(|e| AppError::Internal(format!("failed to read hook row: {e}")))?,
            );
        }

        Ok(hooks)
    }

    /// Delete a hook by ID. Captured requests are CASCADE-deleted by foreign key.
    /// Returns NotFound if the hook doesn't exist.
    pub fn delete_hook(&self, hook_id: &str) -> Result<(), AppError> {
        let conn = self.lock()?;
        let rows_affected = conn
            .execute("DELETE FROM hooks WHERE hook_id = ?1", params![hook_id])
            .map_err(|e| AppError::Internal(format!("failed to delete hook '{hook_id}': {e}")))?;

        if rows_affected == 0 {
            return Err(AppError::NotFound(format!("hook '{hook_id}' not found")));
        }

        Ok(())
    }

    /// Count total hooks in the database.
    pub fn count_hooks(&self) -> Result<u32, AppError> {
        let conn = self.lock()?;
        conn.query_row("SELECT COUNT(*) FROM hooks", [], |row| row.get(0))
            .map_err(|e| AppError::Internal(format!("failed to count hooks: {e}")))
    }

    // -- Request operations --

    /// Insert a captured webhook request.
    pub fn insert_request(&self, req: &CapturedRequest) -> Result<(), AppError> {
        let headers_json = serde_json::to_string(&req.headers)
            .map_err(|e| AppError::Internal(format!("failed to serialize headers: {e}")))?;

        let conn = self.lock()?;
        conn.execute(
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
        conn.execute(
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
        let conn = self.lock()?;
        let mut stmt = conn
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

    /// List captured requests for a hook with offset-based pagination, newest first.
    pub fn list_requests_paginated(
        &self,
        hook_id: &str,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<CapturedRequest>, AppError> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare(
                "SELECT request_id, hook_id, method, path, headers, body, content_length, source_ip, received_at
                 FROM requests
                 WHERE hook_id = ?1
                 ORDER BY received_at DESC
                 LIMIT ?2 OFFSET ?3",
            )
            .map_err(|e| AppError::Internal(format!("failed to prepare query: {e}")))?;

        let rows = stmt
            .query_map(params![hook_id, limit, offset], |row| {
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

    /// Get a single captured request by hook_id and request_id.
    pub fn get_request(
        &self,
        hook_id: &str,
        request_id: &str,
    ) -> Result<CapturedRequest, AppError> {
        let conn = self.lock()?;
        conn.query_row(
            "SELECT request_id, hook_id, method, path, headers, body, content_length, source_ip, received_at
             FROM requests
             WHERE hook_id = ?1 AND request_id = ?2",
            params![hook_id, request_id],
            |row| {
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
            },
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                AppError::NotFound(format!("request '{request_id}' not found in hook '{hook_id}'"))
            }
            other => AppError::Internal(format!(
                "failed to get request '{request_id}' for hook '{hook_id}': {other}"
            )),
        })
    }

    /// Count total requests for a hook.
    pub fn count_requests(&self, hook_id: &str) -> Result<u32, AppError> {
        let conn = self.lock()?;
        conn.query_row(
            "SELECT COUNT(*) FROM requests WHERE hook_id = ?1",
            params![hook_id],
            |row| row.get(0),
        )
        .map_err(|e| {
            AppError::Internal(format!(
                "failed to count requests for hook '{hook_id}': {e}"
            ))
        })
    }

    /// Delete oldest requests for a hook, keeping only the newest `keep` requests.
    /// Returns the number of deleted requests.
    /// Also updates the hook's request_count to reflect the deletion.
    pub fn delete_oldest_requests(&self, hook_id: &str, keep: u32) -> Result<u32, AppError> {
        let conn = self.lock()?;

        // Delete all but the newest `keep` requests for this hook.
        // The subquery selects request_ids to keep (newest by received_at).
        let deleted = conn
            .execute(
                "DELETE FROM requests
                 WHERE hook_id = ?1
                   AND request_id NOT IN (
                       SELECT request_id FROM requests
                       WHERE hook_id = ?1
                       ORDER BY received_at DESC
                       LIMIT ?2
                   )",
                params![hook_id, keep],
            )
            .map_err(|e| {
                AppError::Internal(format!(
                    "failed to delete oldest requests for hook '{hook_id}': {e}"
                ))
            })?;

        // Update hook's request_count to match actual row count
        if deleted > 0 {
            conn.execute(
                "UPDATE hooks SET request_count = (
                     SELECT COUNT(*) FROM requests WHERE hook_id = ?1
                 ) WHERE hook_id = ?1",
                params![hook_id],
            )
            .map_err(|e| {
                AppError::Internal(format!(
                    "failed to update request count for hook '{hook_id}': {e}"
                ))
            })?;
        }

        Ok(deleted as u32)
    }

    /// Get access to the underlying connection for pragma checks in tests.
    #[cfg(test)]
    pub fn with_conn<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&Connection) -> R,
    {
        let conn = self.conn.lock().expect("test lock");
        f(&conn)
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
        let count: i32 = db.with_conn(|conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('hooks', 'requests', 'tokens')",
                [],
                |row| row.get(0),
            )
            .unwrap()
        });
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
        let mode: String = db.with_conn(|conn| {
            conn.query_row("PRAGMA journal_mode", [], |row| row.get(0))
                .unwrap()
        });
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

        let mode: String = db.with_conn(|conn| {
            conn.query_row("PRAGMA journal_mode", [], |row| row.get(0))
                .unwrap()
        });
        assert_eq!(mode, "wal");

        std::fs::remove_dir_all(&tmp).ok();
    }

    // AC-4: Foreign keys enabled
    #[test]
    fn foreign_keys_are_enabled() {
        let db = test_db();
        let fk: i32 = db.with_conn(|conn| {
            conn.query_row("PRAGMA foreign_keys", [], |row| row.get(0))
                .unwrap()
        });
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

    // HB-014: list_hooks returns empty vec when no hooks
    #[test]
    fn list_hooks_empty() {
        let db = test_db();
        let hooks = db.list_hooks().unwrap();
        assert!(hooks.is_empty());
    }

    // HB-014: list_hooks returns all hooks ordered by created_at DESC
    #[test]
    fn list_hooks_ordered_by_created_at_desc() {
        let db = test_db();

        let mut h1 = make_hook("h1", "First");
        h1.created_at = 1000;
        let mut h2 = make_hook("h2", "Second");
        h2.created_at = 2000;
        let mut h3 = make_hook("h3", "Third");
        h3.created_at = 3000;

        db.insert_hook(&h1).unwrap();
        db.insert_hook(&h2).unwrap();
        db.insert_hook(&h3).unwrap();

        let hooks = db.list_hooks().unwrap();
        assert_eq!(hooks.len(), 3);
        assert_eq!(hooks[0].hook_id, "h3"); // newest
        assert_eq!(hooks[1].hook_id, "h2");
        assert_eq!(hooks[2].hook_id, "h1"); // oldest
    }

    // HB-015: delete_hook removes hook
    #[test]
    fn delete_hook_removes_hook() {
        let db = test_db();
        db.insert_hook(&make_hook("h1", "Doomed")).unwrap();
        assert!(db.get_hook("h1").is_ok());

        db.delete_hook("h1").unwrap();
        assert!(db.get_hook("h1").is_err());
    }

    // HB-015: delete_hook cascades to requests
    #[test]
    fn delete_hook_cascades_requests() {
        let db = test_db();
        db.insert_hook(&make_hook("h1", "Cascade")).unwrap();
        db.insert_request(&make_request("r1", "h1")).unwrap();
        db.insert_request(&make_request("r2", "h1")).unwrap();

        db.delete_hook("h1").unwrap();
        // Requests should be gone (CASCADE)
        // We can verify via a direct query since list_requests checks hook_id
        let count: i32 = db.with_conn(|conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM requests WHERE hook_id = 'h1'",
                [],
                |row| row.get(0),
            )
            .unwrap()
        });
        assert_eq!(count, 0);
    }

    // HB-015: delete_hook returns NotFound for missing hook
    #[test]
    fn delete_hook_not_found() {
        let db = test_db();
        let err = db.delete_hook("nonexistent").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("not found"), "got: {msg}");
    }

    // HB-015: delete_hook doesn't affect other hooks
    #[test]
    fn delete_hook_does_not_affect_others() {
        let db = test_db();
        db.insert_hook(&make_hook("h1", "Keep")).unwrap();
        db.insert_hook(&make_hook("h2", "Delete")).unwrap();
        db.insert_request(&make_request("r1", "h1")).unwrap();
        db.insert_request(&make_request("r2", "h2")).unwrap();

        db.delete_hook("h2").unwrap();

        assert!(db.get_hook("h1").is_ok());
        assert_eq!(db.list_requests("h1", 10).unwrap().len(), 1);
    }

    // HB-012 AC-5: count_hooks returns correct count
    #[test]
    fn count_hooks_empty() {
        let db = test_db();
        assert_eq!(db.count_hooks().unwrap(), 0);
    }

    #[test]
    fn count_hooks_after_inserts() {
        let db = test_db();
        db.insert_hook(&make_hook("h1", "Hook 1")).unwrap();
        db.insert_hook(&make_hook("h2", "Hook 2")).unwrap();
        db.insert_hook(&make_hook("h3", "Hook 3")).unwrap();
        assert_eq!(db.count_hooks().unwrap(), 3);
    }

    // HB-016: count_requests
    #[test]
    fn count_requests_empty_hook() {
        let db = test_db();
        db.insert_hook(&make_hook("h1", "Empty")).unwrap();
        assert_eq!(db.count_requests("h1").unwrap(), 0);
    }

    #[test]
    fn count_requests_after_inserts() {
        let db = test_db();
        db.insert_hook(&make_hook("h1", "Counter")).unwrap();
        for i in 0..3 {
            let mut req = make_request(&format!("r{i}"), "h1");
            req.received_at = 1000 + i as i64;
            db.insert_request(&req).unwrap();
        }
        assert_eq!(db.count_requests("h1").unwrap(), 3);
    }

    // HB-016: list_requests_paginated with offset
    #[test]
    fn list_requests_paginated_with_offset() {
        let db = test_db();
        db.insert_hook(&make_hook("h1", "Paginated")).unwrap();

        for i in 0..5 {
            let mut req = make_request(&format!("r{i}"), "h1");
            req.received_at = 1000 + i as i64;
            db.insert_request(&req).unwrap();
        }

        // Skip 2, take 2 (from newest-first ordering: r4, r3, r2, r1, r0)
        let results = db.list_requests_paginated("h1", 2, 2).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].request_id, "r2");
        assert_eq!(results[1].request_id, "r1");
    }

    #[test]
    fn list_requests_paginated_offset_beyond_end() {
        let db = test_db();
        db.insert_hook(&make_hook("h1", "Beyond")).unwrap();
        db.insert_request(&make_request("r1", "h1")).unwrap();

        let results = db.list_requests_paginated("h1", 10, 100).unwrap();
        assert!(results.is_empty());
    }

    // HB-012 AC-6: delete_oldest_requests evicts oldest, preserves newest
    #[test]
    fn delete_oldest_requests_evicts_oldest() {
        let db = test_db();
        db.insert_hook(&make_hook("h1", "Eviction Hook")).unwrap();

        // Insert 5 requests with sequential timestamps
        for i in 0..5 {
            let mut req = make_request(&format!("r{i}"), "h1");
            req.received_at = 1000 + i as i64;
            db.insert_request(&req).unwrap();
        }
        assert_eq!(db.get_hook("h1").unwrap().request_count, 5);

        // Keep only the 3 newest
        let deleted = db.delete_oldest_requests("h1", 3).unwrap();
        assert_eq!(deleted, 2);

        // Verify the 3 newest remain
        let remaining = db.list_requests("h1", 10).unwrap();
        assert_eq!(remaining.len(), 3);
        assert_eq!(remaining[0].request_id, "r4"); // newest
        assert_eq!(remaining[1].request_id, "r3");
        assert_eq!(remaining[2].request_id, "r2");

        // Verify request_count updated
        assert_eq!(db.get_hook("h1").unwrap().request_count, 3);
    }

    #[test]
    fn delete_oldest_requests_no_op_when_under_limit() {
        let db = test_db();
        db.insert_hook(&make_hook("h1", "Under Limit")).unwrap();

        let mut req = make_request("r1", "h1");
        req.received_at = 1000;
        db.insert_request(&req).unwrap();

        // Keep 5 but only 1 exists — no deletions
        let deleted = db.delete_oldest_requests("h1", 5).unwrap();
        assert_eq!(deleted, 0);

        let remaining = db.list_requests("h1", 10).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(db.get_hook("h1").unwrap().request_count, 1);
    }

    #[test]
    fn delete_oldest_requests_does_not_affect_other_hooks() {
        let db = test_db();
        db.insert_hook(&make_hook("h1", "Hook 1")).unwrap();
        db.insert_hook(&make_hook("h2", "Hook 2")).unwrap();

        for i in 0..3 {
            let mut req = make_request(&format!("h1-r{i}"), "h1");
            req.received_at = 1000 + i as i64;
            db.insert_request(&req).unwrap();
        }
        for i in 0..3 {
            let mut req = make_request(&format!("h2-r{i}"), "h2");
            req.received_at = 1000 + i as i64;
            db.insert_request(&req).unwrap();
        }

        // Evict from h1 only
        db.delete_oldest_requests("h1", 1).unwrap();

        assert_eq!(db.list_requests("h1", 10).unwrap().len(), 1);
        assert_eq!(db.list_requests("h2", 10).unwrap().len(), 3); // untouched
    }

    // HB-017: get_request returns full request detail
    #[test]
    fn get_request_round_trips() {
        let db = test_db();
        db.insert_hook(&make_hook("h1", "Detail")).unwrap();
        let req = make_request("r1", "h1");
        db.insert_request(&req).unwrap();

        let fetched = db.get_request("h1", "r1").unwrap();
        assert_eq!(fetched.request_id, "r1");
        assert_eq!(fetched.hook_id, "h1");
        assert_eq!(fetched.method, "POST");
        assert_eq!(fetched.path, "/webhook");
        assert_eq!(
            fetched.headers.get("content-type").unwrap(),
            "application/json"
        );
        assert_eq!(fetched.body, b"hello world");
        assert_eq!(fetched.content_length, 11);
        assert_eq!(fetched.source_ip, "192.168.1.1");
    }

    // HB-017: get_request returns NotFound for wrong request_id
    #[test]
    fn get_request_not_found() {
        let db = test_db();
        db.insert_hook(&make_hook("h1", "Missing Req")).unwrap();

        let err = db.get_request("h1", "nonexistent").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("not found"), "got: {msg}");
    }

    // HB-017: get_request returns NotFound when request exists on different hook
    #[test]
    fn get_request_wrong_hook() {
        let db = test_db();
        db.insert_hook(&make_hook("h1", "Hook 1")).unwrap();
        db.insert_hook(&make_hook("h2", "Hook 2")).unwrap();
        db.insert_request(&make_request("r1", "h1")).unwrap();

        let err = db.get_request("h2", "r1").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("not found"), "got: {msg}");
    }

    // Index existence check
    #[test]
    fn requests_index_exists() {
        let db = test_db();
        let count: i32 = db.with_conn(|conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='idx_requests_hook_id_received'",
                [],
                |row| row.get(0),
            )
            .unwrap()
        });
        assert_eq!(
            count, 1,
            "index on requests(hook_id, received_at) should exist"
        );
    }
}
