# Plan: TDD Guard Claude Hooks (flt-it8.32.7)

## Context

Phase 3 of the governance hardening plan calls for TDD Guard integration. The
goal is to let Claude Code and the test toolchain invoke `tdd-guard` when it's
installed, without breaking anything when it's absent.

Some of these files already exist as **uncommitted changes on main's working
tree** (settings.json, wrapper script, Makefile targets, vitest config). This
branch will create them cleanly from scratch based on the same design.

## Deliverables

### 1. Claude hook wrapper — `scripts/hooks/claude-tdd-guard.sh`

```bash
#!/usr/bin/env bash
set -euo pipefail
if command -v tdd-guard >/dev/null 2>&1; then
  exec tdd-guard "$@"
fi
exit 0
```

Safe no-op when `tdd-guard` is not installed.

### 2. Claude Code settings — `.claude/settings.json`

Three hook events, all delegating to the wrapper:

- **PreToolUse** (matcher: `Write|Edit|MultiEdit|TodoWrite`) — guidance before file edits
- **UserPromptSubmit** — guidance on new prompts
- **SessionStart** (matcher: `startup|resume|clear`) — initial session guidance

### 3. Makefile targets

**Root Makefile** additions:
- `api-test-tdd` → delegates to `api/` Makefile
- `web-test-tdd` → delegates to `web/` Makefile

**api/Makefile** — `test-tdd` target:
- Pipes `go test -json ./...` through `tdd-guard-go` when installed
- Falls back to plain `go test -json ./...` otherwise

**web/Makefile** — `test-tdd` target:
- Sets `TDD_GUARD_PROJECT_ROOT` env var, runs `pnpm test`
- Vitest config picks up the reporter when the env var is set

### 4. Vitest TDD Guard reporter — `web/vitest.config.ts`

Convert to async config factory. When `TDD_GUARD_PROJECT_ROOT` is set:
- Dynamic-import `tdd-guard-vitest`
- Add its `VitestReporter` as an additional reporter
- If import fails and `TDD_GUARD_REQUIRED` is not set, warn and continue

### 5. CLAUDE.md updates

Add `make api-test-tdd`, `make web-test-tdd` to the commands table.

## Files to create/modify

| File | Action |
|---|---|
| `scripts/hooks/claude-tdd-guard.sh` | Create (new) |
| `.claude/settings.json` | Create (new) |
| `Makefile` | Edit — add tdd targets |
| `api/Makefile` | Edit — add `test-tdd` target |
| `web/Makefile` | Edit — add `test-tdd` target |
| `web/vitest.config.ts` | Edit — async config + optional reporter |
| `CLAUDE.md` | Edit — add tdd targets to commands table |

## Verification

1. `bash scripts/hooks/claude-tdd-guard.sh` exits 0 (tdd-guard not installed)
2. `make api-test-tdd` runs Go tests without error (no tdd-guard-go → plain JSON output)
3. `make web-test-tdd` runs Vitest without error (no reporter → warning + continue)
4. `cat .claude/settings.json | python3 -m json.tool` validates JSON
5. Wrapper script is executable (`chmod +x`)
