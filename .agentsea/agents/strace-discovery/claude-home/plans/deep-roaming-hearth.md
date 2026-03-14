# flt-it8.32.9 — TDD Guard Vitest Integration

## Context
Wire the `tdd-guard-vitest` npm reporter into the web test pipeline so `make web-test-tdd` reports test results to TDD Guard when installed. The vitest config and Makefile targets already exist — this task completes the wiring and adds docs.

## Current State (already done)
- `web/vitest.config.ts` — conditional `tdd-guard-vitest` reporter loading via dynamic import (graceful fallback when not installed)
- `web/Makefile` — `test-tdd` target sets `TDD_GUARD_PROJECT_ROOT`
- Root `Makefile` — `web-test-tdd` delegates to `web/Makefile`
- `TDD_GUARD_REQUIRED=1` env var support for strict mode

## Remaining Work

### 1. Add `tdd-guard-vitest` as optional dev dependency
- `cd web && pnpm add -D tdd-guard-vitest`
- This makes it available in the project but doesn't break anything if removed
- Verify `pnpm test` still passes with it installed

### 2. Verify `make web-test-tdd` works from project root
- Run `make web-test-tdd` and confirm the reporter activates
- Run `make web-test` and confirm the reporter does NOT activate (no `TDD_GUARD_PROJECT_ROOT`)

### 3. Add TDD Guard docs
- Create `docs/ci/tdd-guard.md` with installation and usage instructions for both Go and Vitest reporters
- Cover: what TDD Guard is, how to install, how to use make targets, env vars

### 4. Quality gate
- `cd web && pnpm build && pnpm lint && pnpm test`

## Files to modify
- `web/package.json` — add `tdd-guard-vitest` dev dependency
- `docs/ci/tdd-guard.md` — new documentation file

## Verification
1. `cd web && pnpm test` — passes normally (no TDD Guard output)
2. `cd web && TDD_GUARD_PROJECT_ROOT=$(pwd) pnpm test` — reporter activates
3. `make web-test-tdd` from root — works
4. `cd web && pnpm build && pnpm lint` — zero errors/warnings
