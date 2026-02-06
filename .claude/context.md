# Context

## What

Single-binary webhook inbox. Developers point webhooks at it, inspect captured requests, replay them.

## Why

- Testing webhooks requires a public URL, a server, and logs
- Existing tools need Docker, databases, cloud accounts
- Hookbin is one binary — download, run, done

## Flow

```
POST /h/{hook_id} → capture headers + body → SQLite insert → 200 OK
GET /api/hooks/{id}/requests → SQLite query → JSON response
GET / → serve embedded dashboard
```

## Philosophy

**TigerBeetle-inspired:** single binary, zero dependencies, deterministic resources, crash-safe.

- No Postgres, no Redis, no S3
- SQLite in WAL mode — crash-safe, fast, embedded
- Everything bounded — max hooks, max payload, max requests, retention
- Runs on a $5 VPS or a Raspberry Pi

## Constraints

| Resource | Default |
|----------|---------|
| Max hooks | 100 |
| Max payload | 1 MB |
| Retention | 24 hours |
| Rate limit | 60 req/min/hook |
| Max requests/hook | 1000 |

## Files

| File | What |
|------|------|
| `CLAUDE.md` | Read this first |
| `src/main.rs` | Entry point, CLI |
| `src/server.rs` | Axum router |
| `src/db.rs` | SQLite layer |
| `src/models.rs` | Domain types |
| `src/handlers/` | HTTP handlers |

## Next

- Cargo.toml + dependency selection
- Core types and database schema
- Webhook ingestion handler
- Hook CRUD API
- Embedded dashboard UI
- Retention cleanup
- Rate limiting
