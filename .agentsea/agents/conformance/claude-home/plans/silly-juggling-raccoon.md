# Plan: Search Contract Test Matrix (kapow-4vy)

## Context

Search is exposed through 3 entry points (GraphQL, CSV export, MCP) that each
build a `search.SearchRequest` and call `search.ValidateRequest(reg, req, authenticated)`.
Tests currently validate each layer in isolation, which allowed contract drift
(e.g., export previously hardcoded `authenticated=true`, MCP skipped validation
entirely). Both issues are now fixed (kapow-lgd, kapow-kcw), but there's no
shared test proving the entry points agree on validation outcomes.

**Goal**: A shared contract test matrix where identical logical requests produce
identical validation outcomes across all 3 entry points.

## Approach: Shared test data, per-package runners

Define contract cases as exported structs in `search/contract.go` (non-test file,
importable by other packages — same pattern as existing `search/testing.go`).
Each entry point's test package runs the same cases through its own adapter.

## New Files

### 1. `api/internal/search/contract.go` — Shared case definitions

```go
type ContractCase struct {
    Name          string
    Request       SearchRequest
    Authenticated bool
    WantValid     bool       // true = 0 validation errors expected
    WantField     string     // expected error field (if invalid)
    WantContains  string     // substring in error message (if invalid)
    SkipExport    bool       // case doesn't apply to export
    SkipMCP       bool       // case doesn't apply to MCP
    Registry      *Registry  // nil = use NewRegistry()
}

func ContractCases() []ContractCase { ... }
```

~20 cases across 6 categories:

| Category | Cases | Skip |
|---|---|---|
| Valid filters | multi-enum (state=CA), numeric range | — |
| Invalid filters | unknown field, bad enum, empty multi-enum, range both nil | — |
| Auth: restricted field | anonymous → rejected, authenticated → accepted | — |
| Sort | distance w/o location, blend_score w/o blend, valid name, unknown field | skipMCP |
| Location | valid, invalid lat, invalid state code | — |
| Pagination | limit exceeds max, negative offset | offset: skipExport |
| Geo | valid radius, radius exceeds max, non-positive radius | — |

Auth cases use custom registry via `NewRegistryFromFields([]Field{NewTestSingleEnumField(...)})`.
All other cases use `NewRegistry()`.

### 2. `api/internal/search/contract_test.go` — Baseline runner

Iterates `ContractCases()`, calls `ValidateRequest(cc.Registry, cc.Request, cc.Authenticated)`,
asserts error count/field/message. This is the ground-truth reference.

### 3. `api/internal/handler/contract_test.go` — Export adapter runner

For each case where `!SkipExport`:
- Serialize `SearchRequest` → URL query params (filters JSON, sort, userLat/userLng/userState)
- Inject auth claims via `auth.SetClaims` if `cc.Authenticated`
- Call `handler.ExportCSV(nil, reg, 500)` via `httptest`
- Invalid: expect HTTP 400 with matching error field/message in JSON body
- Valid: expect panic from nil DB pool (validation passed) — use `recover()`

### 4. `api/internal/mcp/contract_test.go` — MCP adapter runner

For each case where `!SkipMCP`:
- Build `searchInput` from `cc.Request` (map Filters, Limit, Offset)
- Inject auth claims via `auth.SetClaims` if `cc.Authenticated`
- Call `makeSearchHandler(nil, reg)` directly (internal test package)
- Invalid: `result.IsError == true`, text contains expected error
- Valid: expect panic from nil `db.Querier` — use `recover()`

## Key Design Decisions

1. **Non-test file for cases**: `_test.go` files aren't importable cross-package.
   `search/testing.go` already sets this precedent. Overhead is trivial (~2KB of struct literals).

2. **No GraphQL resolver runner**: The resolver's adapter code maps `model.FilterInput` →
   `search.FilterInput` and calls `ValidateRequest`. Testing through the resolver requires
   either a full GraphQL execution engine or mock DB. The resolver is a thin adapter —
   the baseline + export + MCP runners provide sufficient coverage. Can add later if needed.

3. **`recover()` for valid cases**: All 3 entry points validate before touching DB.
   A nil DB that panics proves validation passed. This pattern already exists in
   `handler/export_test.go:TestExportCSV_RestrictedField_Authenticated`.

4. **Skip flags over separate lists**: Each case carries `SkipExport`/`SkipMCP` booleans
   that document which entry points it applies to and why, keeping the matrix self-documenting.

## Critical Files (read, not modified)

- `api/internal/search/validate.go` — ValidateRequest (the shared contract)
- `api/internal/search/testing.go` — NewTestSingleEnumField
- `api/internal/search/registry.go` — NewRegistryFromFields, NewRegistry
- `api/internal/handler/export.go` — ExportCSV handler
- `api/internal/mcp/tools.go` — makeSearchHandler
- `api/internal/handler/export_test.go` — existing patterns to follow
- `api/internal/mcp/tools_test.go` — existing patterns to follow

## Verification

```bash
# Create worktree, implement, then:
make api-test-unit    # all unit tests pass (including new contract tests)
make api-lint         # zero lint issues
```

No DB required — all contract tests are pure validation tests with nil DB pools.
