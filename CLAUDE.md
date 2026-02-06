# Hookbin

Single-binary webhook inbox. Accept HTTP, store it, show it. No external dependencies.

## ABSOLUTE REQUIREMENTS

**These are non-negotiable. Violating these is a failure condition.**

### Single Binary, Zero Dependencies

- ONE Rust binary. No Postgres. No Redis. No S3. No Docker Compose.
- Embedded SQLite for storage. Embedded UI served from the binary.
- `scp` it to a box, run it, done.

```bash
# This is the entire deployment:
./hookbin serve --port 8080 --data ./hookbin-data
```

```yaml
# BAD - creeping complexity
"Let me just add Redis for caching..."
"We need a separate frontend service..."
"I'll use Postgres, it's more scalable..."

# GOOD - single binary discipline
"I'll add an in-process LRU cache."
"The UI is embedded via rust-embed, served from the same binary."
"SQLite in WAL mode handles this fine."
```

### TigerBeetle Philosophy

- **Deterministic resource usage** — fixed retention, fixed max payload, fixed max hooks
- **Crash-safe storage** — WAL mode, checksums, no data loss on power failure
- **Pre-allocate, bound everything** — no unbounded growth, no surprise OOM
- **Small binary** — target <20MB static binary
- **Zero config by default** — sane defaults, override with flags or single TOML

### No Silent Failure

- Every error is handled explicitly
- Every error is logged with context
- Every API error includes a suggestion for the user
- If it can fail, it has a test

```rust
// BAD - swallowed error, user gets nothing
let _ = db.insert(&request);
return StatusCode::OK;

// GOOD - error handled, logged, user gets actionable response
db.insert(&request).map_err(|e| {
    tracing::error!(hook_id = %hook_id, error = %e, "failed to store request");
    AppError::Internal("storage failure — retry or check disk space".into())
})?;
```

### Tests Are Part of the Feature

- New logic includes tests: success path, failure path, edge cases
- Tests run fast — no external services needed
- `cargo test` is the only gate

```yaml
# BAD - shipping without tests
"It works on my machine, I'll add tests later."

# GOOD - tests are the feature
"I'll write the failing test first, then make it pass."
```

### Embrace the Expert Skills

You have access to skills from world-class engineers. **USE THEM:**

- **bellard**: For ALL architecture decisions — single-binary philosophy, extreme minimalism, making complex things simple
- **bos**: For ALL concurrency work — Tokio tasks, shared state, rate limiter, atomics
- **matsakis**: For ALL ownership/lifetime questions — zero-copy parsing, borrow checker patterns, API ergonomics
- **turon**: For ALL API design — Axum handlers, error types, public interface, async patterns

Do not write generic Rust. Write Rust as these engineers would.

```yaml
# BAD - ignoring the skills
"I'll just write a basic handler..."

# GOOD - channeling the experts
"This handler touches shared state — invoking bos for concurrency guidance."
"This API surface feels wrong — invoking turon for design review."
"This adds a dependency — invoking bellard to find the zero-dep solution."
```

## Architecture

### Core Flow

```text
Webhook POST → Axum handler → validate → SQLite insert → respond 200
Dashboard GET → Axum handler → SQLite query → serve HTML/JSON
```

### Key Entities

- **Hook**: A named webhook inbox with a unique URL
- **Request**: A captured HTTP request (method, headers, body, metadata)
- **Token**: Optional auth token for hook management

### What's In the Binary

| Component | Implementation |
|-----------|---------------|
| HTTP server | Axum + Tokio |
| Storage | SQLite (rusqlite) |
| Dashboard UI | rust-embed (compiled into binary) |
| Rate limiting | In-process token bucket |
| Retention | Background task, SQLite DELETE |
| Config | clap (CLI flags) + optional TOML |

### What Does NOT Exist

- No Postgres
- No Redis
- No S3 / object storage
- No Docker Compose
- No separate frontend service
- No message queue
- No external cache

## Stack

| Layer | Technology |
|-------|------------|
| Language | **Rust (stable)** |
| Async runtime | Tokio |
| HTTP framework | Axum |
| Database | SQLite (rusqlite, WAL mode) |
| UI embedding | rust-embed |
| CLI | clap |
| Serialization | serde + serde_json |
| Testing | cargo test (built-in) |

## Commands

```bash
# Rust verification (run before commit)
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build

# Run locally
cargo run -- serve --port 8080 --data ./data

# Release build + size check
cargo build --release
ls -lh target/release/hookbin

# Full pre-commit
make check
```

## Code Style

**Rust is the only language. Write tight, correct, boring code.**

- Clear is better than clever
- Return early with `?` operator
- No `.unwrap()` in non-test code — handle every error
- No `unsafe` unless proven necessary and documented
- Prefer `&str` over `String` at API boundaries
- Use `thiserror` for error types
- Structured logging with `tracing`

```rust
// BAD - panics, unclear
fn get_hook(id: &str) -> Hook {
    DB.lock().unwrap().get(id).unwrap()
}

// GOOD - explicit errors, context
fn get_hook(db: &Database, hook_id: &str) -> Result<Hook, AppError> {
    if hook_id.is_empty() {
        return Err(AppError::InvalidInput("hook_id cannot be empty".into()));
    }
    db.get_hook(hook_id)
        .map_err(|e| AppError::NotFound(format!("hook {hook_id}: {e}")))
}
```

### Project Structure

```text
src/
  main.rs              # Entry point, CLI parsing
  server.rs            # Axum router + server setup
  db.rs                # SQLite connection, migrations, queries
  models.rs            # Hook, Request, Token structs
  handlers/
    ingest.rs          # POST /{hook_id} — capture webhook
    hooks.rs           # CRUD hooks
    requests.rs        # List/inspect/replay requests
    dashboard.rs       # Serve embedded UI
    health.rs          # Health check
  error.rs             # AppError type
  config.rs            # CLI args + TOML config
  retention.rs         # Background cleanup task
  rate_limit.rs        # In-process rate limiter
ui/
  index.html           # Dashboard SPA (embedded at build)
  ...
```

### Naming

```text
hook_id:    nanoid (e.g., "abc123xyz")
request_id: UUID v4
endpoints:  /h/{hook_id}         (webhook ingress)
            /api/hooks           (management)
            /api/hooks/{id}/requests
            /                    (dashboard)
```

### Error Handling

```rust
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

// Every API error includes a suggestion
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, suggestion) = match &self {
            AppError::NotFound(_) => (404, "Check the hook ID and try again"),
            AppError::RateLimited => (429, "Wait and retry, or increase your rate limit"),
            AppError::PayloadTooLarge { .. } => (413, "Reduce payload size or configure --max-payload"),
            _ => (500, "This is a bug — please report it"),
        };
        // ... structured JSON response with suggestion
    }
}
```

## Constraints (TigerBeetle-style bounds)

| Resource | Default | Flag |
|----------|---------|------|
| Max hooks | 100 | `--max-hooks` |
| Max payload size | 1 MB | `--max-payload` |
| Request retention | 24 hours | `--retention` |
| Rate limit per hook | 60 req/min | `--rate-limit` |
| Max stored requests per hook | 1000 | `--max-requests` |
| Database WAL mode | always on | — |

## Operational Model

```bash
# Install (one-liner)
curl -fsSL https://hookbin.dev/install | sh

# Or just download the binary
wget https://hookbin.dev/releases/hookbin-linux-amd64
chmod +x hookbin-linux-amd64
./hookbin-linux-amd64 serve

# Runs on anything
# - $5 VPS
# - Raspberry Pi
# - Bare metal
# - Any Linux box with a port open
```

## File Structure

```text
hookbin.dev/
├── CLAUDE.md                 # This file — read it
├── .claude/                  # AI config
│   ├── settings.json
│   ├── context.md
│   ├── commands.md
│   ├── skills.md
│   └── commands/             # Slash commands
│
├── .github/
│   └── issues/               # Machine-readable issues
│       ├── _schema.json
│       ├── _labels.json
│       ├── _milestones.json
│       ├── _index.json
│       ├── epics/
│       └── stories/
│
├── Cargo.toml
├── Makefile
├── src/                      # All Rust source
└── ui/                       # Dashboard assets (embedded)
```

## Issue Workflow

Issues are defined as JSON in `.github/issues/`. Each has:
- **Acceptance Criteria**: Given/When/Then format
- **Technical Context**: Crates, files, functions
- **Performance Constraints**: Where applicable

### Commands

```bash
claude /plan-issue HB-XXX       # Review and plan implementation
claude /implement-issue HB-XXX  # Full branch workflow
claude /review-issue HB-XXX     # Verify implementation
claude /sync-issues              # Push to GitHub
claude /figure-it-out            # Crash recovery
```

## DO NOT

- **Add external databases** — SQLite only, embedded, WAL mode
- **Add external services** — no Redis, no S3, no queues
- **Create multiple binaries** — one binary does everything
- **Use `.unwrap()` outside tests** — handle every error
- **Use `unsafe`** — unless proven necessary and documented
- **Swallow errors** — structured errors with suggestions
- **Over-engineer** — boring technology, obvious code
- **Break the single-binary promise** — if it needs Docker Compose, you've failed
- **Add unbounded resource usage** — everything has a limit
- **Skip tests** — if it's not tested, it doesn't work
- **Ignore the expert skills** — bellard for architecture, bos for concurrency, matsakis for ownership, turon for APIs
- **Output errors without fix suggestions** — every error tells the user how to resolve
- **Hardcode anything** — config via CLI flags or TOML, always

## Quick Reference

### Endpoint Map

```text
POST /h/{hook_id}                    # Ingest webhook (the money endpoint)
GET  /api/hooks                      # List hooks
POST /api/hooks                      # Create hook
GET  /api/hooks/{id}                 # Get hook details
DEL  /api/hooks/{id}                 # Delete hook
GET  /api/hooks/{id}/requests        # List captured requests
GET  /api/hooks/{id}/requests/{rid}  # Get single request
GET  /                               # Dashboard (embedded UI)
GET  /health                         # Health check
```

### Skill Activation Quick Lookup

```text
Any Rust code          → matsakis + turon
Concurrency / state    → bos + matsakis
HTTP handlers / API    → turon + matsakis
Architecture / deps    → bellard
SQLite / crash safety  → bellard (TigerBeetle mindset)
Testing                → write the test first, always
```

### The Single Binary Promise

```bash
# Build it
cargo build --release

# Ship it
scp target/release/hookbin server:~/

# Run it
ssh server './hookbin serve'

# That's it. If you need more steps, you've failed.
```
