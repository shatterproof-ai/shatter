# flt-it8.14: MCP Authorization Model

## Context

The MCP endpoint currently uses JWT-only auth with no per-client restrictions. Any authenticated user has full access to all MCP tools and can read items at any sensitivity level (except data isolation by `owner_id`). This issue adds defense-in-depth: per-client API keys with tool permission allowlists, sensitivity-level filtering, and audit logging. Human-in-the-loop approval is deferred (requires push notification infrastructure).

The DB tables `mcp_clients` and `audit_log` already exist in migration `00001_initial_schema.sql`. No new migrations needed.

## Scope

**In scope:** API key auth, client registry, tool permissions, sensitivity filtering, audit logging, GraphQL management mutations, tests.
**Out of scope:** Human-in-the-loop approval, web UI for client management, audit log querying.

---

## Implementation Steps

### 1. New package: `api/internal/mcpclient/`

**Files:** `model.go`, `service.go`, `service_test.go`

Follow the `user/service.go` pattern (pgxpool, scanRow helper, sentinel errors).

- `Permissions` struct: `Tools []string`, `MaxSensitivity string` (maps to JSONB `permissions` column)
- `Client` struct: mirrors `mcp_clients` table
- `Service` with methods: `Create`, `GetByID`, `GetByAPIKey`, `ListByOwner`, `Update`, `Deactivate`, `TouchLastUsed`
- API key format: `flt_` + 32 random bytes base64url (~48 chars). Store **SHA-256 hash** (high-entropy key, need indexed lookup — matches GitHub/Stripe pattern)
- `Create` returns `(*Client, plainAPIKey, error)` — plaintext shown once

Unit tests mock a `Querier` interface wrapping the pgx methods used.

### 2. New package: `api/internal/audit/`

**Files:** `audit.go`, `audit_test.go`

- `Entry` struct: UserID, ClientID, Action, ResourceType, ResourceID, Request (map), ResponseSummary (map), Approved
- `Logger` struct with `New(pool) *Logger` and `Log(ctx, Entry) error`
- `Log` is best-effort: errors produce `slog.Error` but don't fail the MCP operation
- Append-only: no UPDATE/DELETE exposed

### 3. Modify `api/internal/auth/context.go` — MCP client context

Add parallel context key for MCP client info (avoids circular deps by inlining fields):

```go
type MCPClientInfo struct {
    ClientID       uuid.UUID
    OwnerID        uuid.UUID
    Name           string
    Tools          []string
    MaxSensitivity string
}
func WithMCPClient(ctx, *MCPClientInfo) context.Context
func GetMCPClient(ctx) *MCPClientInfo
```

### 4. Modify `api/internal/middleware/auth.go` — Dual auth

New function `AuthWithAPIKey(jwtValidator, ClientLookup)` for MCP route only:

1. Extract bearer token (reuse `bearerToken()`)
2. Try JWT first (fast, no DB)
3. If JWT fails and token has `flt_` prefix → SHA-256 hash → `ClientLookup.GetByAPIKey(hash)`
4. If found + active: set `Claims` (UserID=client.OwnerID, ClientType="mcp") + `MCPClientInfo` in context
5. Fire-and-forget `TouchLastUsed` in goroutine
6. Existing `Auth()` unchanged for non-MCP routes

`ClientLookup` interface defined in middleware package.

### 5. Modify `api/internal/mcp/` — Authorization enforcement

**New file:** `authz.go`, `authz_test.go`

Authorization helpers:
- `AllowedTool(ctx, toolName) bool` — JWT users always allowed; API key clients checked against `MCPClientInfo.Tools`
- `SensitivityFilter(ctx) []string` — returns allowed levels. Private NEVER included (hard rule). JWT users get `[public, normal, sensitive]`. API key clients get levels ≤ `MaxSensitivity`
- `FilterItemBySensitivity(ctx, *item.Item) bool` — single-item check for browse

**Modify `mcp.go`:** Add `AuditLogger` interface to `Deps`

**Modify `server.go`:** Propagate `MCPClientInfo` in `WithHTTPContextFunc`

**Modify `tools.go`:** Each handler gets:
1. `AllowedTool` check after `requireAuth`
2. Sensitivity filter merged into `item.Filter` (search) or post-check (browse)
3. Capture: prevent creating items above client's max sensitivity
4. Audit log call after each operation

**Update `tools_test.go`** with new test cases for denied tools, sensitivity filtering, audit calls.

### 6. Modify `api/internal/router/router.go` — Wiring

- Add `MCPClients *mcpclient.Service` and `AuditLogger *audit.Logger` to `Deps`
- MCP route: `AuthWithAPIKey(deps.Validator, deps.MCPClients)` instead of `Auth(deps.Validator)`
- Pass `deps.AuditLogger` into `mcp.Deps`

### 7. Modify `api/cmd/flotsamd/main.go` — Service init

```go
mcpClients := mcpclient.New(pool)
auditLogger := audit.New(pool)
```

Pass into `router.Deps`.

### 8. GraphQL: MCP client management

**New file:** `graph/schema/mcp_client.graphql` with types `MCPClient`, `MCPPermissions`, mutations `createMCPClient`, `updateMCPClient`, `deleteMCPClient`, query `mcpClients`.

Run `make api-generate`, implement resolvers in `graph/resolver/`.

---

## Key Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| API key hash | SHA-256 | 256-bit random keys immune to dictionary attacks; need indexed DB lookup |
| Private items via MCP | Never returned | Hard-coded rule regardless of client permissions |
| Audit failure handling | Best-effort (log + continue) | Audit DB issue shouldn't become denial of service |
| Human-in-the-loop | Deferred | Requires push notification infrastructure |
| `Auth()` unchanged | Yes | Only MCP route uses `AuthWithAPIKey`; other routes unaffected |

## Verification

1. `make api-test-unit` — all unit tests pass
2. `make api-lint` — zero warnings
3. Manual verification with `make api-test` (if DB available)
4. Coverage: ≥80% on `mcpclient`, `audit`, `mcp` packages

## Critical Files

- `api/internal/mcp/tools.go` — add permission + sensitivity + audit hooks
- `api/internal/middleware/auth.go` — dual auth
- `api/internal/auth/context.go` — MCPClientInfo context
- `api/internal/router/router.go` — wire deps
- `api/internal/user/service.go` — pattern reference for mcpclient
- `api/internal/mcp/tools_test.go` — existing test patterns to extend
