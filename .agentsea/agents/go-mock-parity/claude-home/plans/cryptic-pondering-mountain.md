# Fix CSV Export Auth (kapow-ih72)

## Context
CSV export at `/api/export/csv` uses mandatory `Auth` middleware, which rejects unauthenticated requests with 401 ("missing or malformed authorization header"). The GraphQL route uses `OptionalAuth`, allowing anonymous access. The export route should behave the same way — work without auth when auth isn't configured, validate tokens when provided.

## Changes

### 1. Router: Switch Auth → OptionalAuth (`api/internal/router/router.go:110`)
- Change `kapowmiddleware.Auth(deps.Validator)` to `kapowmiddleware.OptionalAuth(deps.Validator)`
- Keep `EnsureUser` middleware (it already handles missing claims gracefully — passes through)
- Keep rate limiting unchanged

### 2. Add unit test (`api/internal/middleware/middleware_test.go`)
Add `TestOptionalAuth_NoToken_AllowsAccess` — a request to the export route without an Authorization header should pass through (200), not get 401. This directly reproduces the reported bug scenario. The existing test file already has `mockValidator` and test patterns for `Auth`; mirror those for `OptionalAuth`.

### 3. Update docs (`docs/specs/authentication.md`)
Update the route auth table to reflect that `/api/export/csv` now uses `OptionalAuth` instead of `Auth`.

## Verification
- `make api-test-unit` — all pass
- `make api-lint` — clean
