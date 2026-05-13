# Design: Stdin-Based Harness Loop (str-9u7n + str-lefp)

**Date:** 2026-03-23
**Issues:** str-9u7n (Stdin-based input passing), str-lefp (Persistent warm subprocess)
**Status:** Approved

## Overview

Change the Rust and Go harness executors so that compiled harness binaries receive inputs at runtime via stdin (NDJSON) rather than having inputs baked into generated source code. The harness runs as a persistent subprocess, processing multiple execute requests in a loop until EOF. The frontend caches compiled binaries keyed by source hash, skipping recompilation when the source hasn't changed.

This closes both str-9u7n (stdin inputs) and str-lefp (persistent subprocess) together, since the loop protocol is a natural extension of stdin-based inputs.

Crash recovery (dead subprocess restart, timeout enforcement) is handled by the orchestrator in shatter-core, not the frontends.

**Frontends in scope:** `shatter-go` and `shatter-rust` (the Rust language frontend). `shatter-ts` is out of scope (different execution model). The Shatter CLI binary is out of scope.

---

## Wire Protocol

The harness subprocess communicates with the frontend via newline-delimited JSON on stdin/stdout.

### Request (one line on stdin)
```json
{"inputs": [<json_value>, ...], "mocks": [{"symbol": "...", "return_values": [...]}]}
```

### Response (one line on stdout)
```json
{
  "return_value": <json_value>,
  "branch_path": [...],
  "lines_executed": [...],
  "side_effects": [...],
  "scope_events": [...],
  "globals_before": {...},
  "globals_after": {...},
  "external_calls": [...],
  "performance": {"wall_time_ms": 12.3, "cpu_time_us": 0, "heap_used_bytes": 0, "heap_allocated_bytes": 0}
}
```

### Error response
```json
{"error": "<message>"}
```

### Shutdown
The frontend closes the subprocess's stdin (EOF). The harness exits cleanly after the current request completes (or immediately if idle).

**Constraints:**
- The harness MUST NOT write to stdout outside of response lines.
- Stderr is available for diagnostic output.
- One response line per request line, in order.
- Maximum request line size is 4 MB (Go scanner limit). A request exceeding this limit causes `bufio.Scanner` to return `bufio.ErrTooLong`, which exits the scan loop and terminates the harness with exit code 1. The frontend detects this via a write/read error on the next request and returns an error to the caller (same as any other subprocess death).

---

## Go Harness: I/O Model Migration

### Current model (file-based output)
The current Go harness writes results to on-disk temp files (`shatter_results.json`, `shatter_return.json`, `shatter_perf.json`, `shatter_globals.json`, `shatter_external_calls.json`). The frontend assembles `ExecuteResult` by reading those files after the subprocess exits. Mock call records are dumped via `defer shatterDumpMockCalls()` which fires on process exit.

### New model (stdout-based output)
All output moves to a single JSON response line on stdout per request. The temp files are eliminated. The deferred `shatterDumpMockCalls()` is replaced by inline serialization inside the loop body, appending mock call records to the response struct before encoding. Global state snapshots (before/after) are included as fields in the response rather than in a separate file.

**Migration summary:**
- Remove `resultsPath`, `returnPath`, `perfPath`, `globalsPath` parameters from `generateHarness`
- Inline all result collection into the loop body, culminating in `json.NewEncoder(os.Stdout).Encode(response)`
- Replace `defer shatterDumpMockCalls()` with direct serialization into the response struct
- `defer os.RemoveAll(outputDir)` applies to the source/binary build dir; the binary is moved to the cache before this defer fires (see Binary Caching section)

---

## Harness Generation

### Signature changes
- Rust: `fn generate_harness(..., inputs_json: &str, mocks_json: &str, ...)` → remove `inputs_json` and `mocks_json` parameters
- Go: `func generateHarness(..., inputs []json.RawMessage, resultsPath, returnPath, perfPath, globalsPath string, ...)` → remove `inputs` and all path parameters

### Rust generated main() structure
```rust
fn main() {
    use std::io::BufRead;
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) if l.is_empty() => continue,
            Ok(l) => l,
            Err(_) => break,
        };
        let req: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                // unwrap() in generated harness code is intentionally exempt from
                // the no-unwrap() policy — harness binaries are throwaway subprocesses
                println!("{}", serde_json::json!({"error": e.to_string()}));
                continue;
            }
        };
        let inputs = req["inputs"].as_array().cloned().unwrap_or_default();
        let mocks = req["mocks"].as_array().cloned().unwrap_or_default();

        // register mocks, reset runtime state
        // deserialize inputs by position using existing type-mapping logic
        // call function inside catch_unwind, measure wall time
        // collect runtime results (branch_path, side_effects, etc.)
        // serialize all results into response object
        println!("{}", serde_json::to_string(&response).unwrap()); // exempt: generated code
    }
}
```

### Go generated main() structure
```go
func main() {
    scanner := bufio.NewScanner(os.Stdin)
    scanner.Buffer(make([]byte, 4*1024*1024), 4*1024*1024) // 4 MB limit per request line
    for scanner.Scan() {
        var req struct {
            Inputs []json.RawMessage `json:"inputs"`
            Mocks  []json.RawMessage `json:"mocks"`
        }
        if err := json.Unmarshal(scanner.Bytes(), &req); err != nil {
            json.NewEncoder(os.Stdout).Encode(map[string]string{"error": err.Error()})
            continue
        }
        // snapshot globals before
        // deserialize inputs by position using existing type-mapping logic
        // call function, capture panic/error
        // snapshot globals after, compute diff
        // collect mock calls inline (no defer)
        // serialize all results into response struct
        json.NewEncoder(os.Stdout).Encode(response)
    }
    // scanner.Err() != nil means oversized line or I/O error — exit, frontend detects via write failure
    os.Exit(1)
}
```

---

## Binary Caching

### Cache key
`sha256(original_source_content) + "_" + sha256(mocks_json) + "_" + sanitized_function_name`

Mocks are included because different mock configurations produce different harness binaries (mock return values are baked in at compile time). The original (pre-instrumentation) source file is used for the source hash. `sanitized_function_name` replaces non-alphanumeric/non-underscore characters to keep the path safe. The tradeoff: a shatter version bump that changes instrumentation output will reuse a stale binary until the source file changes. This is acceptable for now.

### Cache location
Both frontends resolve the cache root as:
1. `$SHATTER_HARNESS_CACHE` env var if set
2. Otherwise `os.TempDir()/shatter-harness-cache`

Binary path: `{cache_root}/{hash}_{funcname}/harness[.exe]`

### Cache hit path
If the binary exists at the cache path → skip harness generation, skip write, skip compile → go straight to subprocess launch.

### Cache miss path
1. Generate harness source
2. Write to a fresh temp build dir
3. Compile binary into the build dir
4. `os.MkdirAll` the cache entry dir
5. Move (rename) binary from build dir to cache path
6. `os.RemoveAll` the build dir (the binary is now safely in the cache)

**Note for Go:** The existing `defer os.RemoveAll(outputDir)` applies to the source/build directory only. Move the binary to the cache before the defer fires. After the move, removing the build dir is safe.

### Discovered dependencies
`discoverDependencies(sourcePath, activeMocks)` is a frontend-side analysis (not harness-side) — it scans source imports to identify external dependencies. In the new model it is computed once per function on first execute and memoized in `HarnessExecutor` alongside the subprocess entry:

```go
type runningHarness struct {
    cmd                    *exec.Cmd
    stdin                  io.WriteCloser
    stdout                 *bufio.Scanner
    discoveredDependencies []DiscoveredDependency // computed once, reused
}
```

On first launch, compute `discoverDependencies(sourcePath, activeMocks)` and store on the entry. On subsequent calls, return the cached value. This is correct since discovered dependencies are a property of the source file, not the inputs.

### Invalidation
No explicit invalidation. Source change → new hash → new binary path. Old entries are left in the cache dir (OS cleans on reboot or the user can clear `$SHATTER_HARNESS_CACHE` manually). No LRU or size limit.

---

## Subprocess Lifecycle

### State ownership
A subprocess map must be owned by persistent state, not by a stateless function. Each frontend introduces an executor struct to hold this state:

**Rust frontend (`shatter-rust`):** Add `HarnessExecutor` struct with `subprocesses: HashMap<String, RunningHarness>`. The `Handler` struct holds a `HarnessExecutor`.

**Go frontend (`shatter-go`):** Add `HarnessExecutor` struct in the `instrument` package with `subprocesses map[string]*runningHarness`. The protocol `Handler` struct holds a `*instrument.HarnessExecutor`. `ExecuteFunctionWithTiming` becomes a method on `HarnessExecutor` rather than a package-level function.

```go
type HarnessExecutor struct {
    subprocesses map[string]*runningHarness
}

type runningHarness struct {
    cmd                    *exec.Cmd
    stdin                  io.WriteCloser
    stdout                 *bufio.Scanner
    discoveredDependencies []DiscoveredDependency // computed once on first launch, reused
}

func NewHarnessExecutor() *HarnessExecutor { ... }

func (e *HarnessExecutor) ExecuteFunctionWithTiming(sourcePath, funcName string, inputs []json.RawMessage, ...) (*ExecuteResult, error) { ... }
```

### Public signature
`ExecuteFunctionWithTiming` still accepts `inputs []json.RawMessage` — the orchestrator provides inputs per call. Inputs are sent over stdin after subprocess launch, not at compile time.

### Execute flow
1. Compute cache key from original source hash + function name
2. Look up key in subprocess map
3. If found: write request JSON line to subprocess stdin, read response JSON line from stdout
4. If not found (or step 3 fails with write/read error): check cache, compile if miss, launch subprocess, add to map, write request, read response
5. On write/read error in step 3: remove entry from map, return error to caller. No transparent restart — the orchestrator handles retry.

**Liveness check:** No proactive liveness check (e.g., no `try_wait`). Death is detected only via write/read failure. This is intentional — proactive checks add complexity and the orchestrator handles the error anyway.

**Dead subprocess + cached binary:** If the subprocess died and is relaunched from cache, the same binary runs again. If it crashes on the same input, the error propagates to the orchestrator again. This is correct: the orchestrator decides whether to retry or skip.

### Teardown / shutdown
On `shutdown` protocol message: close stdin on all running subprocesses, wait for exit with a 1s timeout, force-kill if still running, clear the subprocess map.

On `teardown` protocol message: no change to harness subprocesses. The `teardown` command manages setup scopes (loaded modules), not harness processes. Harness processes persist until `shutdown`.

---

## Testing

### Unit tests to update
- `generate_harness_*` tests (Rust, Go): remove inputs/mocks parameters from call sites; assert generated code contains stdin read loop, not hardcoded JSON string literals
- `ExecuteFunctionWithTiming` / `execute_function_with_timing` tests: update to call through `HarnessExecutor`; verify subprocess receives inputs via stdin pipe

### New unit tests
| Test | What it verifies |
|------|-----------------|
| Cache hit | Two executes with same source, different inputs → compilation happens once (check binary mtime or spy on build invocation) |
| Loop reuse | N executes on same function → same subprocess PID, each result correct for its inputs |
| Subprocess exit on error | Kill subprocess between calls → error returned to caller, no frontend panic or hang |
| Shutdown | After `shutdown`, subprocess map is empty; next execute compiles and launches fresh |
| Request too large (Go) | Request exceeding 4 MB → scanner error handled, harness exits, caller gets error |
| Cache env var | `SHATTER_HARNESS_CACHE` set → binaries stored at that path, not temp dir |

### Property-based tests
Per project policy (CLAUDE.md), non-trivial stateful components require proptest/rapid coverage:
- **Go (rapid):** `HarnessExecutor` state machine — sequences of execute/teardown/shutdown messages must leave the subprocess map in a consistent state; no double-free of subprocesses
- **Rust (proptest):** same state machine properties for `HarnessExecutor`
- **Both:** roundtrip property — arbitrary inputs serialized to request JSON, deserialized by harness, produce results with correct structural shape

### E2E
Existing `e2e_concolic` tests exercise the full pipeline through both frontends. These should pass without modification — the stdin protocol change is transparent to the orchestrator.

### Acceptance criteria (from issue)
- [ ] Rust harness reads inputs from stdin, not baked into source
- [ ] Go harness reads inputs from stdin, not baked into source
- [ ] Second execute of same function with different inputs does NOT trigger recompilation
- [ ] All existing executor tests pass (updated for stdin protocol)
- [ ] E2E concolic tests pass

---

## Out of Scope
- Orchestrator-level crash recovery and timeout enforcement (handled in shatter-core)
- TypeScript frontend `shatter-ts` (already subprocess-based, different execution model)
- Shatter CLI binary (this design changes frontend harness executors only)
- Cache size limits or LRU eviction
- Per-function teardown of harness subprocesses (subprocesses persist until shutdown)
