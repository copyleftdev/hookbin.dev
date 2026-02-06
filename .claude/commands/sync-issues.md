Sync Hookbin issues to GitHub.

## Steps

### 1. Validate Issue Files

```bash
find .github/issues -name "*.json" -not -name "_*" | while read f; do
  jq empty "$f" || echo "Invalid JSON: $f"
done
```

### 2. Dry Run

```bash
cd scripts && GITHUB_TOKEN=$(gh auth token) python -m sync_issues --dry-run
```

### 3. Execute Sync (if dry run looks good)

```bash
cd scripts && GITHUB_TOKEN=$(gh auth token) python -m sync_issues
```

### 4. Report Results

List created/updated issues with their GitHub URLs.
