# Stdin-Based Harness Inputs Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate per-iteration recompilation in Rust and Go frontends by passing inputs via stdin instead of baking them into generated source code, and switch to debug builds for faster compilation.

**Architecture:** The harness `main()` reads a JSON request from stdin instead of embedding input literals. The frontend pipes serialized inputs to the subprocess stdin and reads the JSON result from stdout. The binary is reusable across execute calls with different inputs -- only source changes trigger recompilation. Debug builds replace release builds for ~2-5x faster compilation.

**Tech Stack:** Rust (shatter-rust), Go (shatter-go), serde_json, encoding/json

**Context document:** `docs/plans/2026-03-21-harness-compilation-unified.md` (Phases 1-2)

**Issues:** str-9u7n (stdin inputs, P1), str-tc2h (debug builds, P2)

**Prerequisite knowledge:**
- @rust-conventions for Rust code style
- @go-conventions for Go code style
- The Rust executor is at `shatter-rust/src/executor.rs`. The function `generate_harness()` (line 313) builds a harness string with inputs baked in via `inputs_json` and `mocks_json` parameters. `execute_function_with_timing()` (line 512) writes the harness to a temp dir, runs `cargo build --release`, executes the binary, and deletes everything.
- The Go executor is at `shatter-go/instrument/executor.go`. The function `generateHarness()` (line 625) builds a harness string with inputs baked in via the `inputs` parameter. `ExecuteFunctionWithTiming()` (line 223) writes the harness, runs `go build`, executes, and deletes via `defer os.RemoveAll(outputDir)`.
- Both frontends currently delete the entire temp directory after every execute. This plan preserves the temp dir for binary reuse.

---

## File Map

| File | Action | Responsibility |
|---|---|---|
| `shatter-rust/src/executor.rs` | Modify | Remove `inputs_json`/`mocks_json` from `generate_harness()`, add stdin reading to harness template, pipe inputs in `execute_function_with_timing()`, switch to debug build, preserve temp dir |
| `shatter-go/instrument/executor.go` | Modify | Remove `inputs` from `generateHarness()`, add stdin reading to harness template, pipe inputs in `ExecuteFunctionWithTiming()`, preserve temp dir |
| `shatter-rust/src/executor.rs` (tests) | Modify | Update all `generate_harness` tests for new signature, add stdin-piping test |
| `shatter-go/instrument/executor_test.go` | Modify | Update harness tests, verify stdin-based execution |

No new files are created. This is a surgical change to two existing files per frontend.

---

## Task 1: Rust -- Remove Baked Inputs from Harness Generation

**Files:**
- Modify: `shatter-rust/src/executor.rs:313-496` (`generate_harness()`)
- Test: `shatter-rust/src/executor.rs:869+` (existing harness tests)

- [ ] **Step 1: Update `generate_harness` tests to expect stdin-based input reading**

Change the existing tests to assert the harness reads from stdin instead of containing embedded JSON. The tests call `generate_harness()` and check the output string.

All seven existing tests need their assertions updated:
- `generate_harness_contains_function_call` (line 869)
- `generate_harness_void_function` (line 889)
- `generate_harness_no_duplicate_main` (line 898)
- `generate_harness_str_ref_param_deserializes_to_owned` (line 962)
- `generate_harness_slice_ref_param_deserializes_to_vec` (line 1071)
- `generate_harness_with_static_mut_includes_snapshot_code` (line 1166)
- `generate_harness_no_static_mut_emits_no_snapshot_code` (line 1205)

For each test:
- Remove `inputs_json` and `mocks_json` arguments from the `generate_harness()` call
- Assert the output contains `serde_json::from_reader(std::io::stdin())` (or equivalent stdin reading)
- Assert the output does NOT contain any `r#"[` or `inputs_json` literals

Add one new test:

```rust
#[test]
fn generate_harness_reads_inputs_from_stdin() {
    let harness = generate_harness(
        "fn add(a: i32, b: i32) -> i32 { a + b }",
        "add",
        &["a".into(), "b".into()],
        &["i32".into(), "i32".into()],
        Some("i32"),
        &[],  // static_mut_names
    )
    .unwrap();
    assert!(harness.contains("stdin"), "harness should read from stdin");
    assert!(
        harness.contains("\"inputs\""),
        "harness should parse inputs field from request"
    );
    assert!(
        harness.contains("\"mocks\""),
        "harness should parse mocks field from request"
    );
    // No embedded JSON literals
    assert!(
        !harness.contains("inputs_json"),
        "harness should not contain embedded inputs"
    );
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd shatter-rust && cargo test generate_harness -- --nocapture 2>&1 | head -40`

Expected: FAIL -- `generate_harness()` still takes `inputs_json` and `mocks_json` parameters, so the updated call sites won't compile.

- [ ] **Step 3: Modify `generate_harness()` to read from stdin**

In `shatter-rust/src/executor.rs`, change the function signature at line 313:

Remove parameters `inputs_json: &str` and `mocks_json: &str`.

Replace the body section that embeds inputs (lines 330-348) with stdin reading:

```rust
// Read request from stdin
h.push_str("    let req: serde_json::Value = serde_json::from_reader(std::io::stdin()).expect(\"failed to read JSON from stdin\");\n");
h.push_str("    let inputs = req[\"inputs\"].as_array().cloned().unwrap_or_default();\n");
h.push_str("    let mocks = req[\"mocks\"].as_array().cloned().unwrap_or_default();\n\n");
```

Keep the mock registration loop (lines 341-348) as-is -- it already iterates over `mocks`.

Remove the `inputs_json` and `mocks_json` serialization from `execute_function_with_timing()` (lines 582-601) -- they're no longer needed.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd shatter-rust && cargo test generate_harness -- --nocapture`

Expected: All eight tests PASS (seven updated + one new).

- [ ] **Step 5: Commit**

```
git add shatter-rust/src/executor.rs
git commit -m "refactor(rust): generate_harness reads inputs from stdin instead of embedding"
```

---

## Task 2: Rust -- Pipe Inputs via Stdin in Execute

**Files:**
- Modify: `shatter-rust/src/executor.rs:512-811` (`execute_function_with_timing()`)

- [ ] **Step 1: Write a test that verifies inputs are piped to the subprocess**

Add a test that calls `execute_function_with_timing` for a simple function and checks the result. This tests the full pipeline: harness generation, compilation, stdin piping, and result parsing. Use the existing test pattern from the file.

```rust
#[test]
fn execute_function_pipes_inputs_via_stdin() {
    // This test requires shatter-rust-runtime to be findable.
    // Skip if SHATTER_RUNTIME_PATH is not set and auto-discovery fails.
    if find_runtime_crate_path().is_err() {
        eprintln!("skipping: runtime crate not found");
        return;
    }

    let dir = std::env::temp_dir().join("shatter-test-stdin-pipe");
    let _ = std::fs::create_dir_all(&dir);
    let src = dir.join("add.rs");
    std::fs::write(&src, "pub fn add(a: i32, b: i32) -> i32 { a + b }").unwrap();

    let result = execute_function(
        src.to_str().unwrap(),
        "add",
        &[serde_json::json!(3), serde_json::json!(4)],
        &[],
        5000,
    );

    let _ = std::fs::remove_dir_all(&dir);

    let result = result.expect("execute should succeed");
    assert_eq!(result.return_value, Some(serde_json::json!(7)));
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd shatter-rust && cargo test execute_function_pipes_inputs_via_stdin -- --nocapture 2>&1 | tail -20`

Expected: FAIL -- the harness now reads from stdin, but `execute_function_with_timing` doesn't pipe anything to stdin yet (it still uses `Command::new(&binary_path).output()` with no stdin).

- [ ] **Step 3: Modify `execute_function_with_timing` to pipe inputs via stdin**

In the subprocess execution section (around line 717), replace `Command::new(&binary_path).output()` with a pipeline that:

1. Serialize the request as JSON: `{"inputs": [...], "mocks": [...]}`
2. Spawn the process with `stdin(Stdio::piped())`
3. Write the JSON to the child's stdin
4. Close stdin (drop the writer)
5. Wait for the child and collect stdout/stderr

```rust
use std::process::Stdio;

let request = serde_json::json!({
    "inputs": inputs,
    "mocks": mocks,
});
let request_json = serde_json::to_string(&request).map_err(|e| {
    ExecuteError::InstrumentError(format!("cannot serialize request: {e}"))
})?;

let mut child = Command::new(&binary_path)
    .current_dir(&temp_dir)
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .map_err(|e| ExecuteError::OutputParseError(format!("failed to run binary: {e}")))?;

// Write inputs to stdin and close it
if let Some(mut stdin) = child.stdin.take() {
    use std::io::Write;
    let _ = stdin.write_all(request_json.as_bytes());
    // stdin is dropped here, closing the pipe
}

let run_output = child.wait_with_output().map_err(|e| {
    ExecuteError::OutputParseError(format!("failed to wait for binary: {e}"))
})?;
```

Also remove the `inputs_json`/`mocks_json` serialization that was feeding `generate_harness` -- it's now done as a request object piped to stdin.

Remove the two `generate_harness` call sites (lines 604-628) and replace with the new signature (no `inputs_json`, no `mocks_json`).

- [ ] **Step 4: Run the new test and existing tests**

Run: `cd shatter-rust && cargo test execute_function -- --nocapture 2>&1 | tail -20`

Expected: PASS for the new stdin-piping test. Existing `execute_function` tests should also pass (they exercise the full pipeline).

- [ ] **Step 5: Commit**

```
git add shatter-rust/src/executor.rs
git commit -m "feat(rust): pipe inputs via stdin to harness subprocess"
```

---

## Task 3: Rust -- Switch to Debug Builds and Preserve Temp Dir

**Files:**
- Modify: `shatter-rust/src/executor.rs:660-739`

- [ ] **Step 1: Write a test that verifies debug build is used by default**

```rust
#[test]
fn generate_cargo_toml_uses_debug_profile() {
    let runtime_path = match find_runtime_crate_path() {
        Ok(p) => p,
        Err(_) => {
            eprintln!("skipping: runtime crate not found");
            return;
        }
    };
    let toml = generate_cargo_toml(&runtime_path);
    // Should NOT contain [profile.release] or --release indicators
    // The binary should be built in debug mode by default
    assert!(
        !toml.contains("[profile.release]"),
        "Cargo.toml should not force release profile"
    );
}
```

- [ ] **Step 2: Run to verify it passes (or fails if there's a release profile in the toml)**

Run: `cd shatter-rust && cargo test generate_cargo_toml_uses_debug_profile -- --nocapture`

Check: if `generate_cargo_toml` adds a `[profile.release]` section, the test will fail, indicating something to fix.

- [ ] **Step 3: Change `cargo build --release` to `cargo build`**

In `execute_function_with_timing`, change the build command (around line 670):

```rust
// Before:
.args(["build", "--release"])
// After:
.args(["build"])
```

Update the binary path (around line 705):

```rust
// Before:
let binary_path = temp_dir.join("target/release").join(binary_name);
// After:
let binary_path = temp_dir.join("target/debug").join(binary_name);
```

Support opt-in release builds via env var:

```rust
let use_release = std::env::var("SHATTER_HARNESS_RELEASE").map_or(false, |v| v == "1");
let build_args = if use_release {
    vec!["build", "--release"]
} else {
    vec!["build"]
};
let profile_dir = if use_release { "release" } else { "debug" };

// Use build_args and profile_dir in the command and binary path
```

- [ ] **Step 4: Remove `std::fs::remove_dir_all(&temp_dir)` calls**

Remove the temp dir cleanup at line 739 (`let _ = std::fs::remove_dir_all(&temp_dir);`) and all other `remove_dir_all(&temp_dir)` calls in error paths (lines 687, 696, 708, 723).

Instead, reuse the temp dir across calls. Change the temp dir naming from timestamp-based to deterministic based on the source file:

```rust
let source_hash = {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    file_path.hash(&mut h);
    h.finish()
};
let temp_dir = std::env::temp_dir().join(format!("shatter-exec-{:016x}", source_hash));
```

This means the same source file always maps to the same temp dir, enabling Cargo's incremental compilation across calls.

- [ ] **Step 5: Run the full test suite**

Run: `cd shatter-rust && cargo test -- --nocapture 2>&1 | tail -30`

Expected: All tests PASS. Compilation should be noticeably faster in tests that invoke `execute_function` (debug mode).

- [ ] **Step 6: Commit**

```
git add shatter-rust/src/executor.rs
git commit -m "perf(rust): debug builds by default, preserve temp dir for incremental reuse"
```

---

## Task 4: Go -- Remove Baked Inputs from Harness Generation

**Files:**
- Modify: `shatter-go/instrument/executor.go:625-780` (`generateHarness()`)
- Test: `shatter-go/instrument/executor_test.go`

- [ ] **Step 1: Update `generateHarness` to read from stdin**

The current signature at line 625:

```go
func generateHarness(funcName string, params []paramInfo, retInfo returnTypeInfo,
    inputs []json.RawMessage, resultsPath, returnPath, perfPath, globalsPath string,
    globalVars []globalVarInfo, hasMocks bool) (string, error)
```

Remove the `inputs []json.RawMessage` parameter. The harness will read inputs from stdin instead.

Replace the input deserialization section (lines 648-660) with stdin reading:

```go
b.WriteString("\t// Read request from stdin\n")
b.WriteString("\tvar req struct {\n")
b.WriteString("\t\tInputs []json.RawMessage `json:\"inputs\"`\n")
b.WriteString("\t\tMocks  []json.RawMessage `json:\"mocks\"`\n")
b.WriteString("\t}\n")
b.WriteString("\tif err := json.NewDecoder(os.Stdin).Decode(&req); err != nil {\n")
b.WriteString("\t\tfmt.Fprintf(os.Stderr, \"failed to read request from stdin: %v\\n\", err)\n")
b.WriteString("\t\tos.Exit(1)\n")
b.WriteString("\t}\n\n")
```

Then change the per-parameter deserialization to use `req.Inputs[i]` instead of `inputs[i]`:

```go
for i, p := range params {
    b.WriteString(fmt.Sprintf("\tvar %s %s\n", p.Name, p.GoType))
    b.WriteString(fmt.Sprintf("\tif err := json.Unmarshal(req.Inputs[%d], &%s); err != nil {\n", i, p.Name))
    b.WriteString(fmt.Sprintf("\t\tfmt.Fprintf(os.Stderr, \"failed to unmarshal input %s: %%v\\n\", err)\n", p.Name))
    b.WriteString("\t\tos.Exit(1)\n")
    b.WriteString("\t}\n")
}
```

Note: the `inputs` parameter was also used to marshal per-input JSON at line 649 (`json.Marshal(string(inputs[i]))`). With stdin-based inputs, this indirection is no longer needed -- the harness reads raw JSON directly.

The output-file parameters (`resultsPath`, `returnPath`, `perfPath`, `globalsPath`) are unchanged -- the harness still writes results to files. Only input delivery changes from embedded literals to stdin.

- [ ] **Step 2: Update the call site in `ExecuteFunctionWithTiming`**

At line 279, remove the `inputs` argument from the `generateHarness()` call:

```go
// Before:
harness, err := generateHarness(funcName, params, returnInfo, inputs, resultsPath, ...)
// After:
harness, err := generateHarness(funcName, params, returnInfo, resultsPath, ...)
```

- [ ] **Step 3: Run tests to see what breaks**

Run: `cd shatter-go && go test ./instrument/ -run TestExecuteFunction -v -count=1 2>&1 | tail -30`

Expected: Compilation errors or test failures because the harness no longer embeds inputs, but the subprocess isn't receiving them via stdin.

- [ ] **Step 4: Pipe inputs via stdin to the subprocess**

In `ExecuteFunctionWithTiming`, around lines 318-325, change the subprocess execution to pipe a JSON request to stdin:

```go
// Serialize the request for stdin
reqJSON, err := json.Marshal(struct {
    Inputs []json.RawMessage `json:"inputs"`
    Mocks  []json.RawMessage `json:"mocks"`
}{
    Inputs: inputs,
    Mocks:  func() []json.RawMessage {
        // Convert active mocks to JSON for the harness
        var m []json.RawMessage
        for _, mc := range activeMocks {
            b, _ := json.Marshal(mc)
            m = append(m, b)
        }
        return m
    }(),
})
if err != nil {
    return nil, fmt.Errorf("serializing request: %w", err)
}

finishRun := timing.Start("execute.run")
runCmd := exec.CommandContext(runCtx, binaryPath)
runCmd.Dir = outputDir
runCmd.Stdin = bytes.NewReader(reqJSON)
var stdoutBuf, stderrBuf strings.Builder
runCmd.Stdout = &stdoutBuf
runCmd.Stderr = &stderrBuf
runErr := runCmd.Run()
finishRun()
```

Add `"bytes"` to the imports at the top of the file.

- [ ] **Step 5: Run the full executor test suite**

Run: `cd shatter-go && go test ./instrument/ -v -count=1 2>&1 | tail -40`

Expected: All `TestExecuteFunction*` tests PASS.

- [ ] **Step 6: Commit**

```
git add shatter-go/instrument/executor.go shatter-go/instrument/executor_test.go
git commit -m "feat(go): generateHarness reads inputs from stdin instead of embedding"
```

---

## Task 5: Go -- Preserve Build Directory and Use Debug Mode

**Files:**
- Modify: `shatter-go/instrument/executor.go:223-250`

- [ ] **Step 1: Remove `defer os.RemoveAll(outputDir)` at line 249**

This line deletes the entire temp directory after every execute, destroying the compiled binary and all Cargo/Go build cache.

- [ ] **Step 2: Make the output directory deterministic and reusable**

`InstrumentFileWithTiming` (called at line 244) creates a random temp dir internally and returns its path. To make it deterministic, modify `InstrumentFileWithTiming` to accept an optional output directory path. When provided, it writes instrumented files there instead of creating a new random dir.

Add a helper for deterministic directory names:

```go
import "crypto/sha256"

func stableOutputDir(sourcePath string) string {
    h := sha256.Sum256([]byte(sourcePath))
    return filepath.Join(os.TempDir(), fmt.Sprintf("shatter-exec-%x", h[:8]))
}
```

In `ExecuteFunctionWithTiming`, compute the stable dir before calling `InstrumentFileWithTiming` and pass it in:

```go
outputDir := stableOutputDir(sourcePath)
os.MkdirAll(outputDir, 0755)
outputDir, err := InstrumentFileWithTiming(sourcePath, &funcName, &outputDir, timing)
```

`InstrumentFileWithTiming` already takes a `projectRoot *string` parameter at position 3. The output dir parameter should be a new parameter (e.g., `outputDirOverride *string`) or threaded through differently -- check the actual signature and adapt. The key behavior: if the stable output dir already has a compiled binary and the source hasn't changed, skip re-instrumentation entirely.

- [ ] **Step 3: Verify Go already uses debug builds**

Check: the build command at line 304 is `go build -o binaryPath .` -- no optimization flags. Go defaults to debug-equivalent builds (with optimizations but also debug info). This is already the right behavior. No change needed for Go.

- [ ] **Step 4: Run the full test suite**

Run: `cd shatter-go && go test ./instrument/ -v -count=1 2>&1 | tail -40`

Expected: All tests PASS. Tests that call `ExecuteFunctionWithTiming` multiple times should be faster on subsequent calls (binary reuse).

- [ ] **Step 5: Run the full Go test suite**

Run: `cd shatter-go && go test ./... -v -count=1 2>&1 | tail -40`

Expected: All tests PASS including protocol handler tests.

- [ ] **Step 6: Commit**

```
git add shatter-go/instrument/executor.go
git commit -m "perf(go): preserve build directory for binary reuse across executions"
```

---

## Task 6: Integration Verification

**Files:** None modified -- verification only.

- [ ] **Step 1: Run Rust frontend tests**

Run: `cd shatter-rust && cargo test 2>&1 | tail -20`

Expected: All tests PASS.

- [ ] **Step 2: Run Go frontend tests**

Run: `cd shatter-go && go test ./... -count=1 2>&1 | tail -20`

Expected: All tests PASS.

- [ ] **Step 3: Run E2E concolic tests**

Run: `cd shatter-core && cargo test --test e2e_concolic 2>&1 | tail -20`

Expected: All E2E tests PASS. The Rust frontend is exercised end-to-end here.

- [ ] **Step 4: Run walkthrough**

Run: `task walkthrough 2>&1 | tail -40`

Expected: Walkthrough completes. Check the ERROR SUMMARY at the end. Step 10 (Rust explore) should complete within 120s.

- [ ] **Step 5: Measure warm iteration timing**

Manually verify: execute the same Rust function twice with different inputs. The second call should NOT trigger recompilation (no `cargo build` output in stderr). Wall time should be <1s.

- [ ] **Step 6: Commit any test fixture updates**

If proptest regressions or snapshot changes occurred:

```
git add -A shatter-rust/ shatter-go/
git commit -m "test: update fixtures after stdin-based harness changes"
```

---

## Stop Gate

After Task 6, measure walkthrough step 10 timing. If it completes within 120s, the immediate performance problem is solved.

Regardless of the timing result, proceed to the follow-on plans for:
- **Phase 3** (feature-gated in-crate harness + Go package-level builds): addresses semantic fidelity -- feature/macro/workspace correctness for Rust, Go package-private helpers and sibling files. Note: `pub(crate)` types in Rust function signatures remain inaccessible (inherent to Rust's crate model); functions with such signatures fall back to the legacy path
- **Phase 4** (persistent subprocess): addresses warm iteration speed (<5ms target)
- **Phase 5** (parallel compilation + precompiled template): addresses multi-function cold start

These will be separate plan documents because they are architecturally independent from Phases 1-2 and each produces working, testable software on its own.
