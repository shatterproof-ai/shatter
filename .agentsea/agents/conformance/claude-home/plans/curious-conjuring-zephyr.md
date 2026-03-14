# Plan: Testcontainer DB + Realistic Seed Data (flt-it8.19)

## Context
Integration tests currently require a manually provisioned `DATABASE_URL`. This makes them non-portable and skipped by default. Adding testcontainers-go lets tests spin up a real pgvector-enabled PostgreSQL container automatically. Seed data enables rapid dev database population and provides realistic test fixtures.

## Files to Create

### 1. `api/internal/testutil/testdb.go`
- `TestDB(t *testing.T) *pgxpool.Pool` — spins up pgvector container, runs migrations, returns pool
- `TestDBWithSeed(t *testing.T) *pgxpool.Pool` — TestDB + SeedAll
- Uses `testcontainers-go` PostgreSQL module with `pgvector/pgvector:pg16` image
- `sync.Once` to cache container per test binary run (all tests share one container)
- `t.Cleanup()` for pool close; container cleaned up via `TestMain` or lazy (lives for test binary lifetime)
- Skip guard: `if testing.Short() { t.Skip("skipping: requires Docker") }`
- Reuse existing `db.MigrateUp()` for migrations

### 2. `api/internal/testutil/seed.go`
- `SeedAll(ctx, pool) error` — calls SeedUsers, then SeedItems for each user, then SeedMCPClients, SeedAuditLog
- `SeedUsers(ctx, pool) ([]uuid.UUID, error)` — 3 users with bcrypt hashed passwords
- `SeedItems(ctx, pool, ownerID uuid.UUID) error` — per-user: 4 bookmarks + 2 voice notes + 3 text notes
- `SeedMCPClients(ctx, pool, ownerID uuid.UUID) ([]uuid.UUID, error)` — 2 MCP clients
- `SeedAuditLog(ctx, pool, userID, clientID uuid.UUID) error` — 5-6 entries

All SQL uses `$N` placeholders. Realistic content, tags, metadata, timestamps spread over 30 days. Uses `golang.org/x/crypto/bcrypt` for password hashing.

### 3. `api/internal/testutil/testdb_test.go`
- `TestTestDB` — integration test: verifies pool connects, `SELECT 1` works
- `TestTestDBWithSeed` — integration test: seeds, verifies row counts per table
- All guarded with `testing.Short()` skip

### 4. `api/cmd/seed/main.go`
- Loads `.env` via godotenv, reads `DATABASE_URL`, connects via `db.Connect`, runs `SeedAll`
- Minimal — follows same pattern as other cmd/ binaries

### 5. Makefile updates
- `api/Makefile`: add `seed` target (`go run ./cmd/seed`)
- Root `Makefile`: add `api-seed` target (delegates to `$(MAKE) -C api seed`)

### 6. Update `api/internal/db/db_test.go`
- Add `TestIntegrationConnect_Testcontainer` and `TestIntegrationMigrations_Testcontainer` that use `testutil.TestDB()` when `DATABASE_URL` is unset but Docker is available
- Existing tests remain unchanged

## Dependencies to Add
- `github.com/testcontainers/testcontainers-go` + PostgreSQL module
- `golang.org/x/crypto/bcrypt` (for seed password hashing)

## Verification
```bash
# Unit tests pass (testcontainer tests skipped)
make api-test-unit

# Lint passes
make api-lint

# Integration tests work with Docker (no DATABASE_URL needed)
cd api && go test ./internal/testutil/ -v -count=1

# Seed CLI works against dev DB
make dev-db-up
make api-migrate-up
make api-seed
```
