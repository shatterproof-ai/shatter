# flt-it8.20: Dev Token Generator CLI

## Context
Local development needs a quick way to mint a JWT for testing API endpoints without going through the registration/login flow. This CLI tool queries the DB for a user and prints a signed JWT to stdout.

## Files to Create
- `api/cmd/devtoken/main.go` — CLI entry point

## Files to Modify
- `api/Makefile` — add `dev-token` target
- `Makefile` (root) — add `api-dev-token` target

## Implementation

### 1. `api/cmd/devtoken/main.go`

Standalone CLI that:
1. Loads `.env` via `godotenv.Load()` (same pattern as `cmd/flotsamd`)
2. Reads `ENV` env var — **refuses to run unless `ENV=development`**
3. Parses flags: `--email` (string, default empty) and `--expiry` (duration, default `720h` = 30 days)
4. Connects to DB via `db.Connect(ctx, os.Getenv("DATABASE_URL"))`
5. Looks up user:
   - If `--email` provided: `SELECT id, email FROM users WHERE email = $1`
   - Otherwise: `SELECT id, email FROM users ORDER BY created_at LIMIT 1`
6. Creates `auth.NewIssuer(os.Getenv("JWT_SECRET"), expiry)` and calls `issuer.Issue(userID, email, "cli")`
7. Prints token to stdout, usage hint to stderr

Key reuse:
- `auth.NewIssuer` from `api/internal/auth/issuer.go`
- `db.Connect` from `api/internal/db/db.go`
- `godotenv` already a dependency

Keep it minimal — direct SQL for user lookup (no need to import user service), read env vars directly (no need for full config.Load since we only need 3 vars).

### 2. Makefile targets

**`api/Makefile`**: add `dev-token` target that runs `go run ./cmd/devtoken/`
**Root `Makefile`**: add `api-dev-token` that delegates to `$(MAKE) -C api dev-token`

### 3. Tests — `api/cmd/devtoken/main_test.go`

- Extract ENV gate logic into a testable function: `checkEnvGate(env string) error`
- Test: returns error when ENV is "production", "staging", or empty
- Test: returns nil when ENV is "development"
- Test flag parsing (expiry default, email default)

## Verification
```bash
make api-test-unit && make api-lint
```
