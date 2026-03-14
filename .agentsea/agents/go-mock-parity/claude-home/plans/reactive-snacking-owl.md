# Plan: API Startup Config & Router Assembly Tests

## Context

The API has strong unit coverage for individual middleware and auth validators, but no tests at the **assembled router** layer. This gap allowed configuration drift (missing auth startup validation, dead CORS config). Recent work (kapow-dxs) made `NewHMACValidator` reject empty secrets and startup fail fast — these behaviors need assembly-level tests.

## New File

**`api/internal/router/router_test.go`** — package `router_test` (external test package)

All tests are unit tests (no DB, run with `-short`).

## Test Helpers

- `testConfig(overrides ...func(*config.Config)) *config.Config` — minimal config with safe defaults
- `testDeps(cfg *config.Config, v auth.Validator) router.Deps` — builds Deps with nil DB/services (safe because router assembly doesn't query DB)
- `type stubValidator` — implements `auth.Validator`; returns configurable claims/error
- `selectValidator(jwtPublicKey, jwtSecret string) (auth.Validator, error)` — replicates the validator selection logic from `cmd/server/main.go` lines 80-95, making it testable without `os.Exit`

## Test Groups

### 1. Startup Auth Validation (`TestValidatorSelection`)

Table-driven test exercising `selectValidator()`:

| Case | JWTPublicKey | JWTSecret | Expected |
|------|-------------|-----------|----------|
| both empty | "" | "" | error (HMAC rejects empty) |
| whitespace secret | "" | "  \t" | error (HMAC rejects whitespace) |
| valid secret | "" | "my-secret" | success, HMACValidator |
| valid RSA PEM | valid PEM | "" | success, RSAValidator |
| invalid RSA PEM | "not-a-pem" | "" | error (RSA parse fails) |
| RSA takes precedence | valid PEM | "my-secret" | success, RSAValidator |

### 2. Router Construction (`TestNew_*`)

Smoke tests verifying `router.New()` doesn't panic with various configs:
- `TestNew_MinimalDeps` — nil DB, nil services, minimal config
- `TestNew_WithMCPEnabled` — `MCPEnabled: true`
- `TestNew_WithDevEndpoints` — `Env: "development"` registers `/dev/auth/login`

### 3. Route Protection (`TestRouteProtection`)

Table-driven test using `httptest` against the assembled router. Key cases:

| Route | No Auth Header | Invalid Token | Notes |
|-------|---------------|--------------|-------|
| `POST /graphql` | non-401 (anonymous OK) | 401 | OptionalAuth |
| `GET /api/export/csv` | non-401 (anonymous OK) | 401 | OptionalAuth |
| `POST /mcp/message` | 401 | 401 | Mandatory Auth (MCPEnabled=true) |
| `GET /.well-known/oauth-protected-resource` | 200 | N/A | No auth middleware |

MCP 401 response also checks `WWW-Authenticate` header contains `Bearer` and `resource_metadata`.

### 4. Conditional Routes (`TestConditionalRoutes`)

Table-driven: verify routes exist/absent based on config:

| Route | Config | Expect |
|-------|--------|--------|
| `POST /dev/auth/login` | `Env: "development"` | non-404 |
| `POST /dev/auth/login` | `Env: "production"` | 404 |
| `GET /playground` | `GraphQLPlayground: true` | 200 |
| `GET /playground` | `GraphQLPlayground: false` | 404 |
| `POST /mcp/message` | `MCPEnabled: false` | 404 |
| `POST /mcp/message` | `MCPEnabled: true` | non-404 (401 = route exists) |

### 5. CORS Absence (`TestNoCORSHeaders`)

Send `OPTIONS` to `/graphql` with `Origin: https://example.com`. Assert no `Access-Control-Allow-Origin` header in response — confirms CORS is handled externally, not by the router.

### 6. Private Route IP Restriction (`TestPrivateRouteIPRestriction`)

Build router with `PrivateCIDRs` set to `10.0.0.0/8`. Send request to `/private/healthcheck` from non-matching IP. Assert 403. (The healthcheck handler will have nil DB, but IP check happens first.)

## Key Design Decisions

- **Nil DB is safe**: `router.New()` wires handlers but doesn't query DB during construction. For auth-level tests, unauthenticated requests to OptionalAuth routes pass through without hitting DB. Invalid-token requests get 401 before reaching handlers. MCP mandatory-auth requests get 401 before reaching handlers.
- **No authenticated request tests**: Routes with `EnsureUser` would call `user.Service` on a nil receiver. That path is already covered by integration tests in `middleware/auth_integration_test.go`.
- **`selectValidator` is a test helper, not production code**: Extracting validator selection from `main.go` into a shared function would be cleaner, but the issue scope is "add tests", not "refactor main". The test helper replicates the logic.

## Files Modified

| File | Action |
|------|--------|
| `api/internal/router/router_test.go` | **Create** — all tests described above |

## Verification

```bash
cd api && go test ./internal/router/ -v -count=1
make api-test-unit
make api-lint
```
