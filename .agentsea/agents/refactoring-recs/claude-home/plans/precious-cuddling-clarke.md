# Plan: Router Assembly Policy Tests (kapow-r4q.2)

## Context

The existing `api/internal/router/router_test.go` (425 lines) already has good coverage across 6 test groups: validator selection, construction, route protection, conditional routes, CORS absence, and private route IP restriction. This task extends coverage to fill remaining policy gaps.

## Gaps to Fill

1. **CORS on actual requests** — existing test only checks OPTIONS preflight; should also verify no CORS headers on regular GET/POST with Origin header
2. **Successful authenticated access** — existing tests only use a fail-validator; need a test with valid claims showing OptionalAuth routes pass claims through and MCP routes succeed
3. **Method constraints** — no tests verify that routes reject wrong HTTP methods (e.g., PUT on /graphql, GET on /mcp/message)
4. **Private routes without IP restriction** — existing test requires PrivateCIDRs; should verify private routes work when no CIDRs configured (open access)
5. **GraphQL GET in route protection** — only POST tested for anonymous access in TestRouteProtection
6. **Export method constraint** — verify /api/export/csv only accepts GET

## Implementation

**File**: `api/internal/router/router_test.go`

### New test functions to add:

#### 1. `TestNoCORSOnActualRequests`
Extend CORS verification to non-preflight requests (GET /graphql with Origin header). Verify no `Access-Control-Allow-Origin` in response.

#### 2. `TestAuthenticatedAccess`
Use a stubValidator that returns valid claims. Table-driven tests:
- POST /graphql with valid Bearer → not 401
- GET /api/export/csv with valid Bearer → not 401
- POST /mcp/message with valid Bearer → not 401 (MCP enabled)

#### 3. `TestMethodConstraints`
Table-driven: verify routes reject incorrect HTTP methods with 405:
- PUT /graphql → 405
- DELETE /graphql → 405
- PUT /mcp/message → 405 (if applicable, depends on chi wildcard)
- POST /api/export/csv → 405
- POST /.well-known/oauth-protected-resource → 405
- POST /private/healthcheck → 405

#### 4. `TestPrivateRoutesOpenAccess`
Build router with no PrivateCIDRs. GET /private/healthcheck should not get 403 (may get 500 due to nil DB — that's fine, just not 403).

#### 5. Extend `TestRouteProtection` with GET /graphql case
Add a table entry for GET /graphql anonymous access.

## Verification

```bash
cd /home/ketan/project/kapow/.claude/worktrees/worktree/router-policy-tests
make api-test-unit && make api-lint
```
