# Commands

## Rust (RUN BEFORE COMMIT)

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build
```

## Run

```bash
# Dev server
cargo run -- serve --port 8080 --data ./data

# Release build
cargo build --release

# Binary size check
ls -lh target/release/hookbin
```

## Full Pre-Commit

```bash
make check
```

## Test Webhook

```bash
# Create a hook, then send a test payload
curl -X POST http://localhost:8080/h/test-hook \
  -H "Content-Type: application/json" \
  -d '{"event": "test", "data": {"key": "value"}}'

# List captured requests
curl http://localhost:8080/api/hooks/test-hook/requests
```

## Reference

```
hook_id:    nanoid (abc123xyz)
request_id: UUID v4
ingest:     POST /h/{hook_id}
api:        /api/hooks, /api/hooks/{id}/requests
dashboard:  /
```
