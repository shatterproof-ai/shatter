# flt-it8.28: Dev Bootstrap Repair

## Context
`make dev` is the documented one-command bootstrap path but it calls `bash scripts/dev.sh` which doesn't exist. Individual targets (`make dev-db-up`, `make api-run`, `make web-dev`) work fine — the orchestration script is the missing piece. Additionally, README.md's project structure section has stale directory names.

## Plan

### 1. Create `scripts/dev.sh`
Orchestration script that:
- Starts Docker services (`docker compose -f compose/db.yml up -d --wait`)
- Waits for DB health, runs migrations (`make -C api migrate-up`)
- Ensures web deps are installed (`cd web && pnpm install --frozen-lockfile`)
- Starts API server with hot reload if `air` is available, else `go run` (`make -C api dev` or `make -C api run`)
- Starts Vite dev server (`make -C web dev`)
- Both run as background processes with proper cleanup on SIGINT/SIGTERM (trap)
- Prints service URLs on startup

### 2. Fix README.md project structure
Current (wrong) → Correct:
- `cmd/server/` → `cmd/flotsamd/` (plus `cmd/flotsam-worker/`, `cmd/flotsam/`)
- `extension/` → `chrome-extension/`
- `mobile/` → `android/`
- `scripts/` description should mention dev.sh

### 3. Verify
- `make dev` no longer errors (at minimum: script exists, is executable, starts correctly)
- `make test-standard` passes (quality gate)

## Files to modify
- `scripts/dev.sh` — **create** (orchestration script)
- `Makefile` — no change needed (already calls `bash scripts/dev.sh`)
- `README.md` — fix project structure section

## Verification
```bash
# Script exists and is executable
test -x scripts/dev.sh

# Quality gate
make test-standard
```
