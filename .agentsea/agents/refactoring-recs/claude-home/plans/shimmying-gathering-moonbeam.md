# Plan: MCP Search Request Validation (kapow-kcw)

## Context

The GraphQL search resolver calls `search.ValidateRequest()` before building SQL queries, providing clear validation errors for invalid filters, auth-gated fields, pagination bounds, etc. The MCP `search_colleges` handler skips this validation entirely and goes straight to `BuildCountQuery`/`BuildQuery`, meaning invalid requests produce low-level SQL/build errors instead of actionable messages. This creates inconsistent behavior between the two entry points.

## Changes

### 1. Add validation to MCP search handler

**File**: `api/internal/mcp/tools.go`

In `makeSearchHandler`, add a `ValidateRequest` call between building the `SearchRequest` (line 62) and the `BuildCountQuery` call (line 65):

```go
// After building req (line 62), add:
import "github.com/ketang/kapow/api/internal/auth"

authenticated := auth.GetClaims(ctx) != nil
if errs := search.ValidateRequest(registry, req, authenticated); len(errs) > 0 {
    // Return MCP error result (not a Go error) with all validation errors
    data, _ := json.Marshal(map[string]any{
        "errors": errs,
    })
    return &mcp.CallToolResult{
        Content: []mcp.Content{
            &mcp.TextContent{Text: string(data)},
        },
        IsError: true,
    }, nil, nil
}
```

Key design decisions:
- Use `auth.GetClaims(ctx) != nil` (same as GraphQL resolver) rather than hardcoding `true` — consistent and future-proof if MCP ever supports optional auth
- Return all validation errors as structured JSON in an MCP error result (`IsError: true`), not just the first error — AI clients benefit from seeing all issues at once
- Use `IsError: true` with `nil` Go error (same pattern as `makeDetailHandler` for empty unit_id / not found) — this is the MCP-idiomatic way to signal tool-level errors vs transport errors

### 2. Add validation tests

**File**: `api/internal/mcp/tools_test.go`

Add unit tests (no DB required) that call `makeSearchHandler` with invalid inputs and verify:

1. **Unknown field** — filter with non-existent field name returns `IsError: true` with "unknown field" in the error
2. **Invalid enum value** — filter with bad enum value returns validation error
3. **Limit exceeds max** — `Limit: 200` returns validation error about limit
4. **Valid request still works** — existing mock tests already cover this (no regression)

These tests don't need pgxmock because validation runs before any DB call — the handler returns early with the MCP error result.

## Files to modify

| File | Change |
|---|---|
| `api/internal/mcp/tools.go` | Add `auth` import, add `ValidateRequest` call with MCP error response |
| `api/internal/mcp/tools_test.go` | Add 3 validation unit tests |

## Verification

```bash
cd <worktree>
make api-test-unit   # all unit tests pass including new ones
make api-lint        # zero lint issues
```
