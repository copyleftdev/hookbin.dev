Implement Hookbin issue HB-$ARGUMENTS with full branch workflow.

**ABSOLUTE REQUIREMENTS:**

- Single binary: no external databases, no external services
- TigerBeetle philosophy: bounded resources, crash-safe, deterministic
- No `.unwrap()` outside tests
- Apply relevant skills: matsakis (ownership), bos (concurrency), turon (API), tigerbeetle (architecture)

## Workflow

### 1. Setup

```bash
git checkout main && git pull origin main
git checkout -b feat/HB-$ARGUMENTS-short-description
```

### 2. Understand

- Read spec: `cat .github/issues/stories/*/HB-$ARGUMENTS*.json 2>/dev/null || cat .github/issues/epics/*.json | jq 'select(.id == "HB-$ARGUMENTS")'`
- Check deps: `cat .github/issues/_index.json | jq '.dependency_graph["HB-$ARGUMENTS"]'`
- If deps incomplete, STOP and report which issues must be done first

### 3. Implement

- Create files from `technical_context.files`
- Implement EVERY acceptance criterion
- Write tests for each AC (Given/When/Then → Arrange/Act/Assert)
- Ensure all resource usage is bounded
- No `.unwrap()` outside tests — handle every error

### 4. Verify

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build
```

All must pass before proceeding.

### 5. Single-Binary Audit

- [ ] No new external service dependencies
- [ ] All state in SQLite or in-process memory
- [ ] Resource bounds enforced (max hooks, max payload, retention)
- [ ] Crash-safe (WAL mode, no partial writes)
- [ ] Binary size not regressed significantly

### 6. Commit & PR

```bash
git add -A
git commit -m "feat(component): HB-$ARGUMENTS - [title from spec]"
git push -u origin feat/HB-$ARGUMENTS-short-description
gh pr create --fill
```

### 7. Self-Review

- Read the PR diff
- Verify each AC is satisfied
- Confirm no `.unwrap()` outside tests
- Confirm error types include suggestions
- Verify tests cover edge cases

### 8. Merge & Cleanup

```bash
gh pr merge --squash --delete-branch
git checkout main
git pull origin main
```

### 9. Report completion and await next issue

**FAILURE CONDITIONS:**

- Adding external dependencies (Postgres, Redis, S3)
- Using `.unwrap()` in non-test code
- Unbounded resource usage
- Leaving PR unmerged
- Skipping any verification step
- Proceeding with failing tests
- API errors without fix suggestions
