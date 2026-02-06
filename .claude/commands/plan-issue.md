Plan implementation for Hookbin issue HB-$ARGUMENTS.

## Steps

### 1. Load Issue Spec

```bash
cat .github/issues/stories/*/HB-$ARGUMENTS*.json 2>/dev/null || \
cat .github/issues/epics/*.json | jq 'select(.id == "HB-$ARGUMENTS")'
```

### 2. Check Dependencies

```bash
cat .github/issues/_index.json | jq '.dependency_graph["HB-$ARGUMENTS"]'
```

If any dependencies are not `done`, list them and STOP.

### 3. Analyze Technical Context

- List files to create/modify
- Check crates needed
- Review function signatures
- Note performance constraints
- Identify resource bounds (TigerBeetle philosophy)

### 4. Map Acceptance Criteria to Tests

For each AC, describe:

- Test function name
- Arrange (Given)
- Act (When)
- Assert (Then)

### 5. Identify Risks

- Storage design → apply tigerbeetle skill
- Ownership/lifetimes → apply matsakis skill
- Concurrency → apply bos skill
- API surface → apply turon skill

### 6. Single-Binary Checklist

- [ ] No external dependencies introduced
- [ ] Resource usage is bounded
- [ ] Crash-safe (WAL, no partial writes)
- [ ] Error handling complete (no .unwrap())
- [ ] Binary size impact assessed

### 7. Estimate

- Story points (1-13)
- Confidence (high/medium/low)
- Blockers or open questions

### 8. Output Implementation Plan

Structured checklist ready for `/implement-issue`
