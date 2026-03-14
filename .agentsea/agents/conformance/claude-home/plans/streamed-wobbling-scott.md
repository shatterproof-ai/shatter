# Plan: flt-it8.32.6 — Git hooks rollout

## Context

The git hooks framework (`.githooks/pre-commit`, `.githooks/pre-push`, `scripts/install-hooks.sh`) already exists with basic implementations that delegate to `scripts/ci/run-changed.sh` and `scripts/ci/run-full.sh`. The CI scripts also exist on main. This task is about polish: adding skip support, better output, and documentation.

The worktree branched before the CI scripts were committed, so **step 0 is rebasing onto main**.

## Changes

### 1. Rebase worktree onto main
```bash
cd /home/ketan/project/flotsam/.claude/worktrees/worktree/git-hooks
git rebase main
```

### 2. Enhance `.githooks/pre-commit`
- Add `FLOTSAM_SKIP_HOOKS=1` bypass with a warning message
- Add timing (seconds elapsed)
- Set `FLOTSAM_STRICT=1` so pre-commit failures block the commit

### 3. Enhance `.githooks/pre-push`
- Add `FLOTSAM_SKIP_HOOKS=1` bypass with a warning message
- Add timing (seconds elapsed)
- Set `FLOTSAM_STRICT=1` so pre-push failures block the push

### 4. Enhance `scripts/install-hooks.sh`
- Verify the hooks are executable after configuring
- Print what checks each hook runs

### 5. Add `docs/ci/git-hooks.md`
- What hooks exist and what they run
- How to install (`make hooks-install`)
- How to skip (`FLOTSAM_SKIP_HOOKS=1`)
- How strict mode works (`FLOTSAM_STRICT`)

### 6. Update README.md
- Add `make hooks-install` to the Development section

### 7. Update CLAUDE.md
- Add `make hooks-install` to the Commands table

## Files to modify
- `.githooks/pre-commit`
- `.githooks/pre-push`
- `scripts/install-hooks.sh`
- `docs/ci/git-hooks.md` (new)
- `README.md`
- `CLAUDE.md`

## Verification
- Run `bash scripts/install-hooks.sh` — verify hooks are configured
- Run `FLOTSAM_SKIP_HOOKS=1 .githooks/pre-commit` — verify skip works
- Run `.githooks/pre-commit` on clean repo — verify it runs checks
- Shellcheck on all modified scripts (if available)
