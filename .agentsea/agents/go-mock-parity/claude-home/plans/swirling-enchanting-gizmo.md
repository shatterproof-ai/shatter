# Plan: Mock Auth Provider for Local Development

## Context
Local development and E2E testing require either a full Supabase setup or manual JWT generation. A dev-only mock auth endpoint eliminates this friction by issuing valid JWTs using the configured `JWT_SECRET`.

## Implementation

### 1. Create handler: `api/internal/handler/devauth.go`

New handler function `DevLogin(secret string)` that:
- Accepts POST with JSON body: `{ "email": "...", "name": "...", "sub": "..." }`
  - `email` required, `name` optional, `sub` optional (generate UUID if absent)
- Creates JWT claims matching Supabase format:
  - `sub` → user ID (provided or generated deterministic UUID from email)
  - `email` → from request
  - `role` → `"authenticated"`
  - `aud` → `["authenticated"]`
  - `iss` → `"kapow-dev"`
  - `iat` → now, `exp` → now + 24h
  - `user_metadata.full_name` → name if provided
  - `app_metadata.provider` → `"dev"`
- Signs with HS256 using `JWT_SECRET`
- Returns JSON: `{ "access_token": "...", "token_type": "bearer", "expires_in": 86400 }`

### 2. Register route in `api/internal/router/router.go`

Inside `New()`, add a conditional route group:
```go
if deps.Config.Env == "development" {
    r.Post("/dev/auth/login", handler.DevLogin(deps.Config.JWTSecret))
}
```
No auth middleware needed — this is the login endpoint itself. Place it alongside the other route groups.

### 3. Write tests: `api/internal/handler/devauth_test.go`

Unit tests:
- **Happy path**: POST with email → returns valid JWT that `HMACValidator` accepts; verify claims match input
- **With all fields**: POST with email + name + sub → verify all claims populated correctly
- **Missing email**: POST without email → 400 Bad Request
- **Token validates**: Returned token passes through `auth.NewHMACValidator(secret).Validate()`

### 4. Route guard test: `api/internal/router/router_test.go`

- Verify `/dev/auth/login` returns 404 when `Config.Env != "development"`
- This can be a simple test that creates a router with `Env: "production"` and hits the endpoint

## Files to modify/create
- **Create**: `api/internal/handler/devauth.go`
- **Create**: `api/internal/handler/devauth_test.go`
- **Modify**: `api/internal/router/router.go` (add route registration)

## Dependencies to reuse
- `api/internal/auth.Claims` — JWT claims struct (`api/internal/auth/jwt.go`)
- `api/internal/auth.NewHMACValidator` — for test validation (`api/internal/auth/jwt.go`)
- `github.com/golang-jwt/jwt/v5` — already in use for token creation
- `api/internal/config.Config.Env` — environment check (`api/internal/config/config.go`)
- `api/internal/config.Config.JWTSecret` — signing secret

## Verification
```bash
cd /path/to/worktree
make api-test-unit && make api-lint
```
