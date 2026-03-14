# Plan: flt-it8.1 — DB Schema + Connection Package

## Context
Flotsam needs its initial database schema and Go connection package. The API module (`api/go.mod`) exists but has zero dependencies. No `internal/db/` or `migrations/` files exist yet. We're adapting proven patterns from the kapow project.

## Files to Create

### 1. `api/migrations/00001_initial_schema.sql`
Goose migration with `-- +goose Up` / `-- +goose Down` markers containing:
- Extensions: `vector`, `pg_trgm`
- Enums: `item_type`, `item_status`, `sensitivity_level`
- Tables: `users`, `items` (pgvector + generated tsvector), `mcp_clients`, `audit_log`
- All indexes from the spec (GIN, ivfflat, btree)
- `updated_at` trigger function + triggers on `users` and `items`
- Down migration drops everything in reverse order

### 2. `api/migrations/migrations.go`
Embed wrapper:
```go
package migrations

import "embed"

//go:embed *.sql
var FS embed.FS
```

### 3. `api/internal/db/db.go`
Adapted from kapow's `internal/db/db.go` + `migrate.go`:
- `Connect(ctx, databaseURL) (*pgxpool.Pool, error)` — 3 attempts, exponential backoff (1s, 2s, 3s), ping verification
- `MigrateUp(ctx, databaseURL) error` — goose up via `sql.Open("pgx", ...)` + embedded FS
- `MigrateDown(ctx, databaseURL) error` — goose down (rollback last)
- Error wrapping: `fmt.Errorf("db: ...: %w", err)`
- Structured slog logging

### 4. `api/internal/db/db_test.go`
- **Unit tests** (run with `-short`):
  - `TestConnect_InvalidURL` — Connect fails with garbage URL
  - `TestConnect_Unreachable` — Connect fails after retries with unreachable host (short timeout via context)
- **Integration tests** (skipped when `-short` or no `DATABASE_URL`):
  - `TestIntegrationConnect` — real connection to dev DB
  - `TestIntegrationMigrations` — MigrateUp + MigrateDown on real DB

### 5. `api/go.mod` / `api/go.sum`
Add dependencies via `go get`:
- `github.com/jackc/pgx/v5`
- `github.com/pressly/goose/v3`

## Implementation Order
1. Create `api/migrations/migrations.go`
2. Create `api/migrations/00001_initial_schema.sql`
3. Create `api/internal/db/db.go`
4. Run `go get` to add dependencies
5. Create `api/internal/db/db_test.go`
6. Run `make api-test-unit` and `make api-lint` (or `go vet`)
7. Commit on worktree branch

## Verification
```bash
cd /home/ketan/project/flotsam/.claude/worktrees/db-schema
make api-test-unit   # go test -race -short ./...
make api-lint        # go vet + golangci-lint (or just go vet)
```

## Reference Files
- Kapow Connect pattern: `/home/ketan/project/kapow/api/internal/db/db.go`
- Kapow Migrate pattern: `/home/ketan/project/kapow/api/internal/db/migrate.go`
- Schema spec: task description (from product-overview.md)
