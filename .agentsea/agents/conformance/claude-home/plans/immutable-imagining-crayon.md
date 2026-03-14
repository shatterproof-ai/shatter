# Plan: flt-it8.21 — Headless GraphQL CRUD Walkthrough

## Context
The existing demo infrastructure (`scripts/demo.sh` + Playwright tests) exercises Flotsam through the browser. What's missing is a **headless API-only walkthrough** that exercises GraphQL CRUD directly via curl, using a dev token. This complements the browser demo for CI, quick verification, and environments without a browser.

## Approach
Create a single bash script `scripts/demo-api.sh` and a `make demo-api` target.

### Script: `scripts/demo-api.sh`

**Prerequisites** (checked at startup):
- `curl` and `jq` available
- Database running (assumes `make dev-db-up` already done or DB available)
- API server running on `API_URL` (default `http://localhost:8081`)

**Flow** (each step prints `[PASS]`/`[FAIL]` with label):

1. **Health check** — `GET /health`, verify 200
2. **Generate dev token** — run `make -C api dev-token` (requires ENV=development, DATABASE_URL, JWT_SECRET), capture token from stdout
3. **Capture bookmark** — `mutation captureBookmark` with URL, title, tags; verify item returned with ID and status
4. **Capture note** — `mutation captureNote` with content, tags; verify item returned
5. **List items** — `query items`; verify total >= 2
6. **Search FTS** — `query search(query: "...")` using a term from the note; verify results
7. **Get single item** — `query item(id: ...)` using bookmark ID; verify fields
8. **Update tags** — `mutation tagItem(id, tags: ["demo", "updated"])`; verify tags returned
9. **Update sensitivity** — `mutation updateSensitivity(id, sensitivity: SENSITIVE)`; verify
10. **Run worker** — `go run ./cmd/flotsam-worker --once` from `api/`; verify exit 0 (processes any pending jobs)
11. **Delete item** — `mutation deleteItem(id: ...)`; verify returns true
12. **Verify deletion** — `query item(id: ...)` returns null/error

**Error handling**: Each step uses a helper function. On first failure, print `[FAIL]`, show response, exit 1. Summary at end shows pass/fail counts.

**Environment variables**:
- `API_URL` — default `http://localhost:8081`
- `DATABASE_URL` — required (for devtoken)
- `JWT_SECRET` — required (for devtoken)
- `ENV` — must be `development`

### Makefile change
Add to root `Makefile`:
```makefile
demo-api:
	bash scripts/demo-api.sh
```

### Files to create/modify
| File | Action |
|---|---|
| `scripts/demo-api.sh` | **Create** — the headless walkthrough script |
| `Makefile` | **Edit** — add `demo-api` target |

### Patterns to follow
- Same step-label format as `scripts/demo.sh` (colored output, step numbers)
- Use `jq` to parse GraphQL responses and extract fields
- Use `set -euo pipefail` for strict bash
- Helper function `gql()` that sends a GraphQL query with auth header and checks for errors

## Verification
1. `bash -n scripts/demo-api.sh` — syntax check
2. `make api-test-unit && make api-lint` — quality gates (no Go changes, but confirm nothing broken)
3. Manual run: `make dev-db-up && make api-migrate-up && make api-seed && make api-run &` then `make demo-api`
