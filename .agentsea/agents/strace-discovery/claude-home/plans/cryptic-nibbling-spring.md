# flt-it8.32.8: TDD Guard Go Integration

## Context
The `make api-test-tdd` target already exists in both root and api Makefiles with graceful fallback when `tdd-guard-go` is not installed. This task verifies it works correctly and adds documentation for the install path.

## Current State
- `api/Makefile` has `test-tdd` target that checks for `tdd-guard-go` via `command -v`, falls back to plain `go test -json`
- Root `Makefile` delegates via `$(MAKE) -C api test-tdd`
- `tdd-guard-go` is not currently installed on this system and appears to be a private tool
- CLAUDE.md already references `make api-test-tdd` in the commands table

## Changes

### 1. Verify fallback works
- Run `make api-test-tdd` from project root — confirm it falls back to `go test -json` cleanly

### 2. Add `-short` flag to test-tdd target
- The current target runs `go test -json ./...` (all tests including integration) — should use `-short` for TDD workflows since they run frequently and shouldn't require a DB
- Update: `go test -json -short ./...` in both the tdd-guard-go and fallback paths

### 3. Document `-race` omission
- `-race` is intentionally omitted from the TDD target for speed — TDD loops need fast feedback
- Add a comment in `api/Makefile` explaining this

### 4. Add install documentation
- Add a `docs/ci/tdd-guard.md` file documenting:
  - What tdd-guard-go does (formats Go test JSON output for TDD workflows)
  - Install path: `go install <module>@latest` (need to confirm actual module path with user)
  - Usage via `make api-test-tdd`

## Files to modify
- `api/Makefile` — add `-short` flag, add comment about `-race` omission
- `docs/ci/tdd-guard.md` — new file with install/usage docs

## Verification
- `make api-test-tdd` from project root — must exit 0 with JSON output
- `make api-test-unit && make api-lint` — quality gate
