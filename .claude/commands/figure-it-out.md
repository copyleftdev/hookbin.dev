---
description: Crash recovery - analyze state, close completed work, resume where left off
---

# Figure It Out - Crash Recovery

When invoked after a crash or context loss, systematically determine current state and resume work.

## Step 1: Git State Analysis

```bash
git status
git log --oneline -5
git branch -a
```

Determine:
- Current branch (feature branch vs main?)
- Uncommitted changes?
- Last commit message (what was being worked on?)

## Step 2: Issue State Check

Read `.github/issues/_index.json` and identify:
- Which issues are marked `in_progress`
- Which issues are marked `ready` but have implementation started
- Dependency order

## Step 3: Code State Analysis

Check for partial implementations:
- Look for `TODO` or `FIXME` comments in recent changes
- Check if tests exist but are failing
- Look for uncommitted work in `src/`

```bash
git diff --stat
cargo build 2>&1 | head -50
cargo test 2>&1 | head -50
```

## Step 4: Determine Current Task

Based on analysis, identify:
1. **Active Issue**: Which HB-XXX was being worked on?
2. **Progress**: What's done vs remaining?
3. **Blockers**: Any errors or test failures?

## Step 5: Report State

Output a structured summary:

```
## Recovery Analysis

**Branch**: `<branch-name>`
**Active Issue**: HB-XXX - <title>
**Last Commit**: <message>

### Completed
- [ ] <completed items>

### In Progress
- [ ] <current item with status>

### Remaining
- [ ] <remaining items>

### Blockers
- <any errors or failures>

### Recommended Action
<what to do next>
```

## Step 6: Resume or Clean Up

Based on state:

**If work is complete but uncommitted:**
```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test
git add -A
git commit -m "<appropriate message>"
```

**If work is incomplete:**
- Resume implementation from where it stopped
- Run `/plan-issue HB-XXX` to re-establish context if needed

**If branch is stale or confused:**
- Stash changes: `git stash`
- Return to main: `git checkout main`
- Start fresh with `/implement-issue HB-XXX`

## Step 7: Update Issue Status

If any issues are now complete:
1. Update status in `.github/issues/stories/<path>/<issue>.json`
2. Run sync: `GITHUB_TOKEN=$(gh auth token) python -m sync_issues`

## Auto-Recovery Checklist

- [ ] Identified current branch and its purpose
- [ ] Found active issue being worked on
- [ ] Assessed code completion state
- [ ] Verified tests pass (or identified failures)
- [ ] Determined next action
- [ ] Either committed completed work OR resumed incomplete work
