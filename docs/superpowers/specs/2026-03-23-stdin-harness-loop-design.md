# Design: Stdin-Based Harness Loop (str-9u7n + str-lefp)

**Date:** 2026-03-23
**Issues:** str-9u7n (Stdin-based input passing), str-lefp (Persistent warm subprocess)
**Status:** Approved

## Overview

Change the Rust and Go harness executors so that compiled harness binaries receive inputs at runtime via stdin (NDJSON) rather than having inputs baked into generated source code. The harness runs as a persistent subprocess, processing multiple execute requests in a loop until EOF. The frontend caches compiled binaries keyed by source hash, skipping recompilation when the source hasn't changed.

This closes both str-9u7n (stdin inputs) and str-lefp (persistent subprocess) together, since the loop protocol is a natural extension of stdin-based inputs.

Crash recovery (dead subprocess restart, timeout enforcement) is handled by the orchestrator in shatter-core, not the frontends.

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
  "performance": {"wall_time_ms": 12.3, "cpu_time_us": 0, "heap_used_bytes": 0, "heap_allocated_bytes": 0}
}
```

### Error response
```json
{"error": "<message>"}
```

### Shutdown
The frontend closes the subprocess's stdin (EOF). The harness exits after the current request completes (or immediately if idle).

**Constraints:**
- The harness MUST NOT write to stdout outside of response lines
- Stderr remains available for diagnostic output
- One response line per request line, in order

---

## Harness Generation

### What changes
`generate_harness()` (Rust) and `generateHarness()` (Go) no longer accept inputs as a parameter. The generated `main()` becomes a read-execute-write loop.

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
                println!("{}", serde_json::json!({"error": e.to_string()}));
                continue;
            }
        };
        let inputs = req["inputs"].as_array().cloned().unwrap_or_default();
        let mocks = req["mocks"].as_array().cloned().unwrap_or_default();

        // register mocks, reset runtime state
        // deserialize inputs by position, call function, collect results
        // write response line
        println!("{}", serde_json::to_string(&response).unwrap());
    }
}
```

### Go generated main() structure
```go
func main() {
    scanner := bufio.NewScanner(os.Stdin)
    scanner.Buffer(make([]byte, 4*1024*1024), 4*1024*1024) // 4MB max line
    for scanner.Scan() {
        var req struct {
            Inputs []json.RawMessage `json:"inputs"`
            Mocks  []json.RawMessage `json:"mocks"`
        }
        if err := json.Unmarshal(scanner.Bytes(), &req); err != nil {
            json.NewEncoder(os.Stdout).Encode(map[string]string{"error": err.Error()})
            continue
        }
        // deserialize inputs by position, call function, collect results
        json.NewEncoder(os.Stdout).Encode(response)
    }
}
```

### Signature change
- Rust: `fn generate_harness(..., inputs_json: &str, ...)` → remove `inputs_json` parameter
- Go: `func generateHarness(..., inputs []json.RawMessage, ...)` → remove `inputs` parameter

The parameter deserialization logic (type-specific unmarshal, owned/ref conversions) moves inside the loop body, indexed against `req.inputs`.

---

## Binary Caching

### Cache key
`sha256(instrumented_source_content) + "_" + function_name`

The instrumented source is already produced before harness generation; hashing it is cheap and captures all meaningful variation.

### Cache location
Both frontends use: `{os_temp_dir}/shatter-harness-cache/{hash}_{funcname}/harness[.exe]`

### Cache hit path
If the binary exists at the cache path → skip harness generation, skip write, skip compile → go straight to subprocess launch.

### Cache miss path
Generate harness → write to temp dir → compile binary → move to cache location → launch.

### Invalidation
No explicit invalidation. Source change → new hash → new binary path. Old entries are left in temp dir (OS cleans on reboot). No LRU or size limit required.

---

## Subprocess Lifecycle

### Subprocess map
Each frontend maintains a map from cache key to running subprocess:
- Rust: `HashMap<String, std::process::Child>` + stdin/stdout handles
- Go: `map[string]*runningHarness` where `runningHarness` holds `*exec.Cmd`, `stdin io.WriteCloser`, `stdout *bufio.Scanner`

### Execute flow
1. Compute cache key from instrumented source hash + function name
2. If subprocess in map and alive → write request line to stdin, read response line from stdout
3. If subprocess not in map (or dead, detected by write/read error) → check cache, compile if miss, launch subprocess, add to map, then send request
4. On subprocess exit detected during step 2 → return error to caller (no transparent restart; orchestrator handles retry)

### Teardown / shutdown
On `teardown` or `shutdown` protocol message: close stdin on all running subprocesses, wait for exit with a short timeout (1s), force-kill if needed, clear the map.

---

## Testing

### Unit tests to update
- `generate_harness_*` tests (Rust, Go): remove inputs parameter; assert generated code contains stdin read loop, not hardcoded JSON string literals
- `execute_function_with_timing` tests: update call sites; verify subprocess receives inputs via stdin pipe

### New tests
| Test | What it verifies |
|------|-----------------|
| Cache hit | Two executes with same source, different inputs → compilation happens once (check binary mtime or spy on build) |
| Loop reuse | N executes on same function → same subprocess PID, each result correct |
| Subprocess exit | Kill subprocess between calls → error returned to caller, no frontend panic |
| Teardown | After teardown, subprocess map is empty; next execute compiles and launches fresh |

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
- Orchestrator-level crash recovery and timeout enforcement (separate concern, handled in shatter-core)
- TypeScript frontend (already subprocess-based, different execution model)
- Rust frontend `shatter-rust` (execute is unimplemented; no change needed)
