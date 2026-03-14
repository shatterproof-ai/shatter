# str-d3q: Fix TS frontend unbounded memory growth

## Context

The TS frontend has two module-level Maps that grow without bound:
1. `instrumentedSources` in `handlers.ts:55` — keyed by `file:function`, populated by every `instrument` command, never cleared in production
2. `compiledModuleCache` in `executor.ts:81` — keyed by absolute path, populated by every `loadModule()` call, never cleared in production

Both have `clear*()` functions (`clearInstrumentedSources()` at line 432, `clearModuleCache()` at line 789) but these are only called in test `beforeEach` blocks. On large codebases this causes OOM.

The `teardown` handler (handlers.ts:246) already cleans up `setupContexts` but does NOT clear these two caches. The `shutdown` handler (line 328) only clears the WASM cache.

## Fix

### 1. Reproduction test (handlers.test.ts)

Add a test that:
- Sends multiple `instrument` + `execute` cycles for different file:function keys
- Then sends a `teardown` command
- Asserts that the caches are cleared after teardown

Since the caches are module-private, we can't directly inspect their size. Instead, we'll verify the behavioral contract: after teardown, the caches should be cleared. We can test this indirectly by checking that `teardown` calls the clear functions, or we can export a `cacheSize()` helper for testing.

**Better approach**: Export `instrumentedSourcesSize()` and `compiledModuleCacheSize()` test-only helpers (following the existing pattern of exporting `clearInstrumentedSources()` and `clearModuleCache()` for tests). Then the test can assert sizes grow during instrument/execute and reset to 0 after teardown.

### 2. Production fix (handlers.ts)

In the `teardown` handler, after the existing teardown logic, call:
- `clearInstrumentedSources()` — but this also clears `loadedSetupModules` and `setupContexts`, which is too aggressive during teardown of a single function
- `clearModuleCache()` from executor.ts

Wait — `teardown` is per-function, not per-session. Clearing all caches on every single teardown would break multi-function workflows. The better place is:

**Option A: Clear on `shutdown`** — the shutdown handler already exists and runs once at end of session. Add cache clearing there alongside `clearWasmCache()`.

**Option B: Clear on `teardown`** — but this is per-function, so clearing all caches would be too aggressive.

**Decision: Option A** — clear both caches in the `shutdown` handler. This is the natural lifecycle boundary (session end), and the clear functions already exist.

Actually, re-reading the issue: it says "Option 1 (simplest): Call clearInstrumentedSources() and clearModuleCache() during teardown." But teardown is per-function. Let me reconsider...

In practice, teardown is called after exploring a function. Between functions, caches from the previous function are stale. Clearing on teardown is reasonable — the next function will re-instrument and re-compile anyway. And `clearInstrumentedSources()` already clears all three handler maps.

**Final decision**: Clear both caches in the `teardown` handler as the issue recommends. Also clear them in `shutdown` for completeness.

### Files to modify

1. **`shatter-ts/src/handlers.ts`**
   - Import `clearModuleCache` from `./executor.js`
   - In `teardown` case (line 260-261 area), after `setupContexts.delete()`, add calls to `instrumentedSources.clear()` and `clearModuleCache()`
   - In `shutdown` case (line 329), add `clearInstrumentedSources()` and `clearModuleCache()` alongside existing `clearWasmCache()`

2. **`shatter-ts/src/handlers.ts`** — Add `instrumentedSourcesSize()` export for test observability

3. **`shatter-ts/src/executor.ts`** — Add `compiledModuleCacheSize()` export for test observability

4. **`shatter-ts/src/handlers.test.ts`** — Add reproduction test

### Reproduction test outline

```typescript
describe("memory management", () => {
  it("teardown clears instrumented sources and module cache", async () => {
    // Setup: instrument a function (populates instrumentedSources)
    await handleRequest(makeRequest({ command: "instrument", file: testFile, function: "someFunc" }));
    expect(instrumentedSourcesSize()).toBeGreaterThan(0);

    // Setup: also need a setup call so teardown has context
    // ... or just test shutdown path

    // Teardown/shutdown clears caches
    await handleRequest(makeRequest({ command: "shutdown" }));
    expect(instrumentedSourcesSize()).toBe(0);
    expect(compiledModuleCacheSize()).toBe(0);
  });
});
```

### Verification

1. `cd shatter-ts && npm test` — all tests pass
2. New test fails before fix, passes after
