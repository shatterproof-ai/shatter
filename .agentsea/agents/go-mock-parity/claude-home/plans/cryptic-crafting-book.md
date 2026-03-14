# Plan: Router CORS config enforcement (kapow-36o)

## Context

The API exposes `CORS_ORIGINS` in config but never applies CORS middleware. Browser clients currently depend on proxy behavior instead of explicit server policy. This adds proper CORS enforcement at the HTTP layer.

## Approach

Write a simple CORS middleware in-house (no external library needed — the requirements are straightforward: origin allowlist, standard preflight handling). The `rs/cors` library would work but adds a dependency for ~50 lines of code.

## Files to modify

1. **`api/internal/middleware/cors.go`** (new) — CORS middleware
2. **`api/internal/middleware/cors_test.go`** (new) — unit tests
3. **`api/internal/router/router.go`** — wire CORS middleware into global stack
4. **`api/internal/router/router_test.go`** — update Groups 5/7 (currently assert no CORS headers) and add CORS-specific tests

## Implementation

### 1. `middleware/cors.go`

```go
func CORS(origins string) func(http.Handler) http.Handler
```

- Parse `origins` as comma-separated, trimmed list → `allowedOrigins map[string]bool`
- If empty/blank, return a no-op passthrough (no CORS headers ever set)
- On each request:
  - Read `Origin` header; if absent or not in allowlist, pass through (no headers)
  - If `OPTIONS` + `Access-Control-Request-Method` (preflight):
    - Set `Access-Control-Allow-Origin: <origin>`
    - Set `Access-Control-Allow-Methods: GET, POST, PUT, DELETE, OPTIONS`
    - Set `Access-Control-Allow-Headers: Authorization, Content-Type`
    - Set `Access-Control-Max-Age: 86400`
    - Return 204 (don't call next)
  - Otherwise (simple/actual request):
    - Set `Access-Control-Allow-Origin: <origin>`
    - Set `Vary: Origin`
    - Call next

### 2. Router wiring

Add `r.Use(kapowmiddleware.CORS(deps.Config.CORSOrigins))` early in the global middleware stack — after `RequestID` (so CORS responses get request IDs) and before `Logging`.

### 3. Tests

**`middleware/cors_test.go`** — table-driven unit tests:
- Empty origins → no CORS headers on any request
- Allowed origin → correct `Access-Control-Allow-Origin` on actual request
- Disallowed origin → no CORS headers
- Preflight (OPTIONS) with allowed origin → 204 + all `Access-Control-*` headers
- Preflight with disallowed origin → no CORS headers, passes through
- No `Origin` header → no CORS headers

**`router/router_test.go`** — update existing tests:
- Groups 5 and 7 (`TestNoCORSHeaders`, `TestNoCORSOnActualRequests`) currently assert no CORS headers with empty config — these should still pass since empty `CORSOrigins` = no-op
- Add new test group: router with `CORSOrigins` set, verify headers appear on allowed-origin requests to `/graphql`, `/api/export/csv`

## Verification

```bash
make api-test-unit && make api-lint
```
