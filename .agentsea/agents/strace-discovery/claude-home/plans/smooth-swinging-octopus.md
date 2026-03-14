# Fix: MCP browse nil guard (flt-9lf)

## Context
`browseHandler` in `api/internal/mcp/tools.go` panics when looking up a non-existent item. `item.Service.GetByID` returns `(nil, nil)` for not-found, but the handler only checks `err != nil`, then dereferences `it` on the sensitivity check → panic.

## Fix

### 1. Add nil guard in `api/internal/mcp/tools.go` (~line 127)
After the `err != nil` check, add:
```go
if it == nil {
    auditToolCall(ctx, deps.Audit, claims, "browse", req.GetArguments(), false)
    return mcpgo.NewToolResultError("item not found"), nil
}
```

### 2. Fix existing test + add regression test in `api/internal/mcp/tools_test.go`
- **Fix `TestBrowseHandler_NotFound`** (line 349): Change mock to return `(nil, nil)` instead of an error, matching real `GetByID` behavior. Verify result is an error result with "item not found".
- **Add `TestBrowseHandler_NotFound_WithError`**: Keep the error-path test (mock returns an error) to cover both branches.

## Verification
```bash
make api-test-unit && make api-lint
```
