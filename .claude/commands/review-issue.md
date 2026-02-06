Review implementation of Hookbin issue HB-$ARGUMENTS.

## Checklist

### 1. Load Issue Spec

```bash
cat .github/issues/stories/*/HB-$ARGUMENTS*.json 2>/dev/null || \
cat .github/issues/epics/*.json | jq 'select(.id == "HB-$ARGUMENTS")'
```

### 2. Verify Acceptance Criteria

For each AC in the spec:

- [ ] Implementation satisfies the criterion
- [ ] Test exists that validates the criterion
- [ ] Edge cases are handled

### 3. Single-Binary Compliance (CRITICAL)

- [ ] No external database dependencies added
- [ ] No external service dependencies added
- [ ] All state in SQLite (WAL mode) or in-process
- [ ] Resource bounds enforced and configurable

### 4. Code Quality (Matsakis + Turon Style)

- [ ] Clear is better than clever
- [ ] No `.unwrap()` outside tests
- [ ] No `unsafe` blocks (unless documented and justified)
- [ ] Ownership model is clean — minimal cloning
- [ ] Error types are exhaustive with suggestions

### 5. Crash Safety (TigerBeetle)

- [ ] SQLite WAL mode used
- [ ] No partial writes possible
- [ ] Graceful shutdown handles in-flight requests
- [ ] Data survives power failure

### 6. Resource Bounds

- [ ] Max hooks enforced
- [ ] Max payload size enforced
- [ ] Rate limiting active
- [ ] Retention cleanup working
- [ ] Max requests per hook enforced

### 7. Tests Pass

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build
```

### 8. Binary Size Check

```bash
cargo build --release
ls -lh target/release/hookbin
```

Target: <20MB

### 9. Report

Summarize findings and recommend: APPROVE or REQUEST CHANGES
