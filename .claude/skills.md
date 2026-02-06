# Expert Skills

**USE THESE. Do not write generic code.**

Skills are installed in `.claude/skills/` — each has a `SKILL.md` with full guidance.

## The Dream Team

| Skill | Role | Use For |
|-------|------|---------|
| **matsakis** | Rust ownership | Every ownership decision, lifetime, borrow checker interaction |
| **bos** | Rust concurrency | Tokio tasks, shared state, rate limiter, atomics |
| **turon** | Rust API design | Axum handlers, error types, public API surface |
| **bellard** | Minimalist systems | Single-binary philosophy, extreme code density |
| **gray** | Transaction systems | SQLite WAL, crash safety, data integrity |
| **deterministic-simulation** | TigerBeetle testing | Deterministic simulation, fault injection |
| **torvalds** | Systems pragmatism | Performance, no-nonsense review, subsystem design |
| **beck-tdd** | TDD discipline | Red-green-refactor, tests as design |

## Activation

| Task | Primary | Support |
|-------|---------|---------|
| Any Rust code | matsakis | turon |
| Concurrency / shared state | bos | matsakis |
| HTTP handlers / API surface | turon | matsakis |
| Architecture / binary design | bellard | torvalds |
| SQLite / storage / crash safety | gray | bellard |
| Testing strategy | beck-tdd | deterministic-simulation |
| Performance / systems review | torvalds | muratori |
| Fault injection / chaos | deterministic-simulation | gray |

## Stop If

- Complex lifetime annotations → matsakis says simplify the design
- Shared mutable state → bos says use message passing or atomics
- Confusing API surface → turon says redesign for clarity
- Binary bloat or extra deps → bellard says cut it
- Partial writes / data loss risk → gray says WAL + fsync
- No tests for new logic → beck-tdd says red-green-refactor
- Unbounded growth → deterministic-simulation says add limits
- `.unwrap()` in non-test code → all skills object
