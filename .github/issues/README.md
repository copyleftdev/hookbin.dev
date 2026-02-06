# Hookbin GitHub Issues

Machine-readable GitHub issues in JSON format for the Hookbin webhook inbox.

## Structure

```text
issues/
├── _schema.json           # Issue schema definition
├── _labels.json           # Label definitions
├── _milestones.json       # Milestone definitions
├── _index.json            # Complete issue index
├── epics/
│   ├── 00-foundation.json     # Core binary + storage
│   ├── 01-ingestion.json      # Webhook capture
│   ├── 02-api.json            # Management API
│   ├── 03-dashboard.json      # Embedded UI
│   └── 04-operations.json     # Retention, rate limiting, config
└── stories/
    ├── foundation/        # Foundation stories
    ├── ingestion/         # Ingestion stories
    ├── api/               # API stories
    ├── dashboard/         # Dashboard stories
    └── operations/        # Operations stories
```

## Issue Format

Each issue follows a consistent schema with:

- **Acceptance Criteria** (AC) as Given/When/Then statements
- **Technical Context** with crates, files, and function signatures
- **Dependencies** between issues
- **Performance Constraints** where applicable
- **Single-Binary Checklist** for architecture compliance

## Workflow

1. `claude /plan-issue HB-XXX` — Review and plan implementation
2. `claude /implement-issue HB-XXX` — Full branch workflow
3. `claude /review-issue HB-XXX` — Verify implementation
4. `claude /sync-issues` — Push to GitHub

## Key Constraints

- **Single Binary**: No external databases, no external services
- **TigerBeetle Philosophy**: Bounded resources, crash-safe, deterministic
- **No .unwrap()**: Handle every error outside tests
