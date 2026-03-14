# str-8fcu: Frontend Teardown Side-Effect Parity

## Context

Teardown behavior diverges across frontends. The TypeScript frontend clears instrumented sources, module cache, and errors when no setup context exists. The Go frontend silently succeeds even without prior setup and doesn't clear any session state. This creates behavioral drift in multi-step sessions — the core engine gets different teardown semantics depending on which frontend it talks to.

## Intended Teardown Contract

Teardown resets session state so the next setup/execute cycle starts clean:
1. **Error if no context** — teardown without prior setup returns `internal_error` (not silent success)
2. **Clear setup context** — remove the stored context for the given scope/level
3. **Clear stale session state** — reset cached analysis/instrumentation/compilation state
4. **Clear live generator handles** — prevent handle leaks across teardown boundaries
5. **Don't clear expensive long-lived caches** — WASM plugin cache persists (expensive to reload; TS doesn't clear it either)

## Changes

### 1. Write failing Go tests (test-first)

**File: `shatter-go/protocol/handler_test.go`**

- **`TestTeardownWithoutSetupReturnsError`** — sends teardown with no prior setup, expects `status: "error"`, `code: ErrInternalError`, message containing "No setup context". Will FAIL against current code.
- **`TestTeardownClearsSessionState`** — sends analyze (sets `lastAnalyzedFile`), then setup+teardown, then verifies `lastAnalyzedFile` was cleared. Will FAIL against current code.
- **Update `TestNewCommandsInConversationSequence`** (line ~770) — the teardown after a failed setup should now expect error, not `teardown_ack`.

### 2. Fix Go `handleTeardown`

**File: `shatter-go/protocol/handler.go`** (lines 466-486)

```go
func (h *Handler) handleTeardown(resp Response, req Request) Response {
    // ... existing validation ...

    found := h.setupLoader.Teardown(req.Scope, string(req.Level))
    if !found {
        resp.Status = "error"
        resp.Code = ErrInternalError
        resp.Message = fmt.Sprintf("No setup context found for %s:%s. Call setup first.", req.Level, req.Scope)
        return resp
    }

    // Clear stale session state
    h.lastAnalyzedFile = ""
    h.registry.Handles.Clear()

    resp.Status = "teardown_ack"
    return resp
}
```

### 3. Add TS parity comment (documentation only)

**File: `shatter-ts/src/handlers.ts`** — TS behavior is already correct. Add a brief comment documenting the teardown contract for cross-reference.

### 4. Document the contract

**File: `PARITY.md`** or inline — document the intended teardown side-effect contract so future frontends implement it consistently.

## Files to Modify

| File | Change |
|------|--------|
| `shatter-go/protocol/handler.go` | Check `Teardown()` bool return; error if no context; clear `lastAnalyzedFile` and `registry.Handles` |
| `shatter-go/protocol/handler_test.go` | Add failing tests first; update conversation sequence test |
| `shatter-ts/src/handlers.ts` | Optional: teardown contract comment |

## What NOT to Change

- **Rust frontend** — teardown is a no-op stub, out of scope
- **WasmCache** — not cleared on teardown (expensive; TS doesn't clear either)
- **`setup/loader.go`** — already returns `bool`, no API change needed
- **TS handler logic** — already correct

## Verification

```bash
# 1. Confirm failing tests before fix
cd shatter-go && go test ./protocol/ -run TestTeardown -v  # should fail

# 2. Apply fix, confirm tests pass
cd shatter-go && go test ./... -v

# 3. Full tier (touches protocol behavior)
cargo test && cargo clippy -- -D warnings
cd shatter-ts && npm test
cd shatter-go && go test ./...
```
