```
 _                 _    _     _
| |__   ___   ___ | | _| |__ (_)_ __
| '_ \ / _ \ / _ \| |/ / '_ \| | '_ \
| | | | (_) | (_) |   <| |_) | | | | |
|_| |_|\___/ \___/|_|\_\_.__/|_|_| |_|
```

**Single-binary webhook inbox. Accept HTTP, store it, show it. No external dependencies.**

---

- **One binary, zero dependencies** — no Postgres, no Redis, no Docker Compose. `scp` it to a box, run it, done.
- **Embedded SQLite** — WAL mode, crash-safe, deterministic resource bounds. Data lives in a single directory.
- **Live dashboard** — built-in SPA with live polling, syntax-highlighted JSON, request inspector with full headers and body.
- **134 tests** — every handler, every edge case, every error path. `cargo test` is the only gate.
- **Tiny footprint** — optimized release build with LTO, stripped symbols, and `panic = abort`.

## Quick Start

```bash
# 1. Build (or download a release binary)
cargo build --release

# 2. Run
./target/release/hookbin serve --port 8080

# 3. Create a hook and send a webhook
curl -s http://localhost:8080/api/hooks -X POST | jq .
# => { "id": "abc123xyz", "url": "/h/abc123xyz", ... }

curl -X POST http://localhost:8080/h/abc123xyz \
  -H "Content-Type: application/json" \
  -d '{"event": "deploy", "status": "success"}'
# => 200 OK

# 4. Open http://localhost:8080 to see it in the dashboard
```

## Dashboard

The embedded dashboard provides a complete webhook inspection UI — no separate frontend service needed.

- **Hook list** with request counts and creation timestamps
- **Request timeline** with method badges and relative timestamps
- **Request inspector** — headers, body, query params, and metadata
- **Syntax-highlighted JSON** with collapsible body viewer
- **Live polling** — new requests appear automatically
- **Responsive layout** — works on desktop and mobile

## API Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/h/{hook_id}` | Ingest a webhook (the money endpoint) |
| `GET` | `/api/hooks` | List all hooks |
| `POST` | `/api/hooks` | Create a new hook |
| `GET` | `/api/hooks/{id}` | Get hook details |
| `DELETE` | `/api/hooks/{id}` | Delete a hook |
| `GET` | `/api/hooks/{id}/requests` | List captured requests |
| `GET` | `/api/hooks/{id}/requests/{rid}` | Get a single request |
| `GET` | `/health` | Health check |
| `GET` | `/` | Dashboard (embedded SPA) |

All API endpoints return structured JSON with error suggestions:

```json
{
  "error": "not found: hook abc123xyz",
  "suggestion": "Check the hook ID and try again"
}
```

## Configuration

Three-layer config: **defaults** -> **TOML file** -> **CLI flags** (CLI always wins).

### CLI Flags

```bash
hookbin serve [OPTIONS]
```

| Flag | Default | Description |
|------|---------|-------------|
| `--port` | `3000` | Port to listen on |
| `--data` | `./hookbin-data` | Data directory for SQLite database |
| `--max-hooks` | `100` | Maximum number of hooks |
| `--max-payload` | `1048576` | Maximum payload size in bytes (1 MB) |
| `--retention` | `86400` | Request retention in seconds (24 hours) |
| `--rate-limit` | `60` | Rate limit per hook (requests/minute) |
| `--max-requests` | `1000` | Max stored requests per hook |
| `--config` | — | Path to TOML config file |

### TOML Config

```toml
# hookbin.toml
port = 8080
data = "/var/lib/hookbin"
max_hooks = 50
max_payload = 2097152
retention = 172800
rate_limit = 120
max_requests = 500
```

```bash
hookbin serve --config hookbin.toml
```

## Resource Limits

Hookbin follows the [TigerBeetle philosophy](https://tigerbeetle.com/): deterministic resource usage, pre-allocated bounds, no surprise OOM.

| Resource | Default | Configurable |
|----------|---------|--------------|
| Max hooks | 100 | `--max-hooks` |
| Max payload size | 1 MB | `--max-payload` |
| Request retention | 24 hours | `--retention` |
| Rate limit per hook | 60 req/min | `--rate-limit` |
| Max requests per hook | 1,000 | `--max-requests` |
| SQLite WAL mode | Always on | — |

Everything is bounded. Nothing grows without limit.

## Build from Source

```bash
# Prerequisites: Rust stable toolchain
git clone https://github.com/copyleftdev/hookbin.dev.git
cd hookbin.dev

# Development
cargo build             # Debug build
cargo test              # Run all tests
cargo run -- serve      # Run locally on :3000

# Release
cargo build --release   # Optimized binary
ls -lh target/release/hookbin
```

### Make Targets

| Target | Description |
|--------|-------------|
| `make build` | Debug build |
| `make release` | Optimized release build |
| `make test` | Run all tests |
| `make fmt` | Format code |
| `make clippy` | Run linter |
| `make check` | Full pre-commit (fmt + clippy + test + build) |
| `make run` | Run server on :8080 |
| `make dev` | Run with `./data` directory |
| `make size` | Show release binary size |
| `make clean` | Remove build artifacts |

## Project Structure

```
src/
  main.rs              # Entry point, CLI parsing
  server.rs            # Axum router + server setup
  db.rs                # SQLite connection, migrations, queries
  models.rs            # Hook, Request structs
  config.rs            # CLI args + TOML config (3-layer merge)
  error.rs             # AppError with structured suggestions
  retention.rs         # Background cleanup task
  rate_limit.rs        # In-process token bucket
  handlers/
    ingest.rs          # POST /h/{hook_id} — capture webhook
    hooks.rs           # CRUD hooks
    requests.rs        # List/inspect requests
    dashboard.rs       # Serve embedded UI
    health.rs          # Health check
ui/
  index.html           # Dashboard SPA
  assets/
    app.js             # Vanilla JS — hash routing, live polling
    style.css          # Dashboard styles
```

## Tech Stack

| Component | Implementation |
|-----------|---------------|
| Language | Rust (stable) |
| HTTP server | Axum 0.8 + Tokio |
| Storage | SQLite via rusqlite (bundled, WAL mode) |
| UI embedding | rust-embed (compiled into binary) |
| CLI | clap 4 |
| Serialization | serde + serde_json |
| Error handling | thiserror |
| Logging | tracing + tracing-subscriber |

## License

MIT
