# Plan: verify-ci command (kapow-n92.8)

## Context

The `verify-agent` command was just merged, providing diff-aware local verification via a three-stage pipeline: `classify-changes.sh` → `resolve-checks.sh` → execute. We need a CI-friendly entrypoint (`verify-ci.sh`) that reuses this pipeline but adapts it for CI environments with explicit scopes.

## Design

`scripts/verify-ci.sh` — a thin wrapper that reuses the existing pipeline with two key differences from `verify-agent.sh`:

1. **Scope parameter** (`--scope merge|release`) instead of purely diff-derived scope
2. **CI-friendly output** (no ANSI colors when not a TTY, structured exit codes, timing info)

### Scope behavior

| Scope | Base ref | Check resolution | Description |
|-------|----------|-----------------|-------------|
| `merge` (default) | `--base` flag or auto-detect merge base | Same as verify-agent: classify changes → resolve checks | PR/merge validation |
| `release` | N/A (ignores diff) | Hardcoded: `make test-full`, `make lint`, `make web-build` | Full pre-release validation |

### Key differences from verify-agent.sh

- `--scope merge`: Uses merge-base detection (`git merge-base`) for smarter base ref
- `--scope release`: Bypasses classify/resolve entirely, runs the full suite
- No ANSI colors when stdout is not a TTY (CI-friendly)
- Prints timing for each command
- `--format` flag: `human` (default) or `summary` (one-line-per-check for CI log parsing)

## Files to create/modify

1. **`scripts/verify-ci.sh`** (new) — Main CI entrypoint
2. **`Makefile`** — Add `verify-ci` target
3. **`AGENTS.md`** — Document verify-ci alongside verify-agent

## Implementation: `scripts/verify-ci.sh`

```
Usage: ./scripts/verify-ci.sh [OPTIONS]

Options:
  --scope merge|release   Verification scope (default: merge)
  --base REF              Git ref for merge scope (default: auto-detect merge base)
  --dry-run               Show what would run without executing
  -h, --help              Show help

Exit codes:
  0  All checks passed
  1  One or more checks failed
```

### Logic flow

**merge scope:**
1. Auto-detect base: `git merge-base HEAD main` (or use `--base`)
2. Run `classify-changes.sh --base $BASE` → `resolve-checks.sh` (same pipeline as verify-agent)
3. Execute resolved commands sequentially, fail-fast
4. Print timing per command

**release scope:**
1. Skip classification entirely
2. Run hardcoded full suite: `make test-full`, `make lint`, `make web-build`
3. Execute sequentially, fail-fast

### Color handling
- Auto-detect: colors on if `[ -t 1 ]` (TTY), off otherwise
- Ensures clean CI logs

### Reuse strategy
- Calls `classify-changes.sh` and `resolve-checks.sh` directly (same as verify-agent)
- Shares the same JSON parsing approach (sed/grep, no jq dependency)
- The execution loop is similar but adds timing

## Makefile target

```makefile
verify-ci:
	bash scripts/verify-ci.sh
```

## Verification

1. `bash scripts/verify-ci.sh --dry-run` — merge scope dry run
2. `bash scripts/verify-ci.sh --scope release --dry-run` — release scope dry run
3. `bash scripts/verify-ci.sh --help` — help text
4. `make test-quick` — quality gate
