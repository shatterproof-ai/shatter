# Stdin Harness Loop Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace per-input harness compilation in the Go and Rust frontends with a persistent subprocess that receives inputs via stdin in a loop, enabling binary caching and warm iteration speed.

**Architecture:** Harness binaries no longer embed inputs/mocks at compile time. Instead they run a `bufio.Scanner`/`BufRead` loop — one JSON request in, one JSON response out, per iteration — until stdin is closed. Each frontend maintains a `HarnessExecutor` with a subprocess map keyed by `sha256(source) + funcname + sha256(mocks)`. Cache hit = skip compile; subprocess alive = skip launch. Handler holds a persistent `HarnessExecutor`; shutdown closes all subprocesses.

**Tech Stack:** Go 1.24, Rust stable, `bufio.Scanner` (Go), `std::io::BufRead` (Rust), `sha2` crate (Rust), `crypto/sha256` (Go), existing `shatter_rust_runtime` crate.

**Spec:** `docs/superpowers/specs/2026-03-23-stdin-harness-loop-design.md`

**Closes:** str-9u7n, str-lefp

---

## File Map

### Go frontend (`shatter-go/`)

| File | Change |
|------|--------|
| `instrument/recorder.go` | Add `__shatter_reset()` to generated recorder source |
| `instrument/executor.go` | (1) Add reset/get funcs to mock file; (2) Refactor `generateHarness` to stdin loop; (3) Add binary cache helpers; (4) Add `HarnessExecutor` struct + methods; (5) Keep package-level wrappers |
| `instrument/executor_test.go` | Update existing tests for new API; add cache/loop/shutdown tests |
| `instrument/property_test.go` | Update `generateMockFile` call sites; add `HarnessExecutor` state machine property tests |
| `instrument/fuzz_test.go` | Update `generateMockFile` call sites; add fuzz target for harness response parsing |
| `protocol/handler.go` | Add `*instrument.HarnessExecutor` field; update execute call; add shutdown cleanup |

### Rust frontend (`shatter-rust/`)

| File | Change |
|------|--------|
| `src/executor.rs` | (1) Refactor `generate_harness` to stdin loop; (2) Add `HarnessExecutor` struct + methods with cache + subprocess map |
| `src/handler.rs` | Add `HarnessExecutor` field; update `handle_execute` → `&mut self`; add subprocess cleanup to `handle_shutdown` |

---

## Task 1: Go — Add `__shatter_reset()` to recorder generator

**Files:**
- Modify: `shatter-go/instrument/recorder.go`
- Test: `shatter-go/instrument/recorder_test.go`

The `generateRecorder()` function in `recorder.go` produces a Go source string. The generated code has module-level vars `__shatter_lines`, `__shatter_branches`, `__shatter_trace`. The stdin loop harness calls `__shatter_reset()` at the start of each iteration to clear these before the next execution.

- [ ] **Step 1: Write the failing test**

Add to `recorder_test.go`:
```go
func TestGeneratedRecorderHasResetFunction(t *testing.T) {
    src := generateRecorder("main")
    if !strings.Contains(src, "func __shatter_reset()") {
        t.Error("expected __shatter_reset() function in generated recorder")
    }
}
```

- [ ] **Step 2: Run to verify failure**

```bash
cd shatter-go && go test ./instrument/... -run TestGeneratedRecorderHasResetFunction -v
```
Expected: FAIL — `__shatter_reset()` not yet defined.

- [ ] **Step 3: Add `__shatter_reset()` to the generated source in `generateRecorder`**

In `recorder.go`, inside the `fmt.Sprintf(...)` template string, after the existing `__shatter_dump_results` function, add:

```go
func __shatter_reset() {
    __shatter_mu.Lock()
    __shatter_lines = nil
    __shatter_branches = nil
    __shatter_trace = nil
    __shatter_mu.Unlock()
}
```

Also add `__shatter_get_results()` in the same spot (needed later, add now so tests pass together):

```go
func __shatter_get_results() ([]byte, error) {
    __shatter_mu.Lock()
    results := __shatterResults{
        LinesExecuted: __shatter_lines,
        BranchPath:    __shatter_branches,
        ScopeEvents:   __shatter_trace,
    }
    if results.LinesExecuted == nil {
        results.LinesExecuted = []int{}
    }
    if results.BranchPath == nil {
        results.BranchPath = []__shatterBranchDecision{}
    }
    if results.ScopeEvents == nil {
        results.ScopeEvents = []__shatterTraceEvent{}
    }
    __shatter_mu.Unlock()
    return json.Marshal(results)
}
```

Also add `TestGeneratedRecorderHasGetResultsFunction` to `recorder_test.go` at the same time.

- [ ] **Step 4: Run to verify both tests pass**

```bash
cd shatter-go && go test ./instrument/... -run "TestGeneratedRecorder" -v
```
Expected: PASS for both new tests and all existing recorder tests.

- [ ] **Step 5: Commit**

```bash
git add shatter-go/instrument/recorder.go shatter-go/instrument/recorder_test.go
git commit -m "feat(shatter-go): add __shatter_reset and __shatter_get_results to generated recorder"
```

---

## Task 2: Go — Add reset/get helpers to mock file generator

**Files:**
- Modify: `shatter-go/instrument/executor.go` (the `generateMockFile` function at line ~798)
- Test: `shatter-go/instrument/executor_test.go`

The existing `shatterDumpMockCalls()` writes to a file. The stdin loop needs inline access to mock calls and a way to reset state between iterations.

Changes to `generateMockFile`:
1. Replace `shatterDumpMockCalls()` (file-based) with `shatterGetMockCalls()` returning `[]shatterMockCall`
2. Add `shatterResetMocks()` that zeros all call indices and clears `shatterMockCalls`

- [ ] **Step 1: Write the failing tests**

Add to `executor_test.go`:
```go
func TestGenerateMockFileHasResetMocks(t *testing.T) {
    src := generateMockFile([]MockConfig{
        {Symbol: "os.ReadFile", ReturnValues: []json.RawMessage{json.RawMessage(`null`)}},
    }, "/tmp/calls.json")
    if !strings.Contains(src, "func shatterResetMocks()") {
        t.Error("expected shatterResetMocks() in mock file")
    }
}

func TestGenerateMockFileHasGetMockCalls(t *testing.T) {
    src := generateMockFile([]MockConfig{
        {Symbol: "os.ReadFile", ReturnValues: []json.RawMessage{json.RawMessage(`null`)}},
    }, "/tmp/calls.json")
    if !strings.Contains(src, "func shatterGetMockCalls()") {
        t.Error("expected shatterGetMockCalls() in mock file")
    }
}

func TestGenerateMockFileDoesNotHaveDumpMockCalls(t *testing.T) {
    src := generateMockFile([]MockConfig{
        {Symbol: "os.ReadFile", ReturnValues: []json.RawMessage{json.RawMessage(`null`)}},
    }, "/tmp/calls.json")
    if strings.Contains(src, "func shatterDumpMockCalls()") {
        t.Error("shatterDumpMockCalls() should be removed in favor of shatterGetMockCalls()")
    }
}
```

- [ ] **Step 2: Run to verify failures**

```bash
cd shatter-go && go test ./instrument/... -run "TestGenerateMockFile" -v
```
Expected: New tests fail. Existing `TestGenerateMockFileContainsMockFunctions` still passes.

- [ ] **Step 3: Update `generateMockFile`**

In `executor.go`, replace the `shatterDumpMockCalls` function generation block with:

```go
// Reset helper — zeros all call indices and clears call records
b.WriteString("func shatterResetMocks() {\n")
b.WriteString("\tshatterMockCallsMu.Lock()\n")
b.WriteString("\tshatterMockCalls = shatterMockCalls[:0]\n")
b.WriteString("\tshatterMockCallsMu.Unlock()\n")
// reset each mock's call index
for i := range mocks {
    b.WriteString(fmt.Sprintf("\tshatterMock%d_callIdx = 0\n", i))
}
b.WriteString("}\n\n")

// Get helper — returns current call records without writing to a file
b.WriteString("func shatterGetMockCalls() []shatterMockCall {\n")
b.WriteString("\tshatterMockCallsMu.Lock()\n")
b.WriteString("\tdefer shatterMockCallsMu.Unlock()\n")
b.WriteString("\tresult := make([]shatterMockCall, len(shatterMockCalls))\n")
b.WriteString("\tcopy(result, shatterMockCalls)\n")
b.WriteString("\treturn result\n")
b.WriteString("}\n\n")
```

Note: The `shatterMockCallIdx` variables must now be non-`atomic` plain `int` or changed to a variable the reset can zero. Alternatively make them `var shatterMock%d_callIdx int64` and use `atomic.StoreInt64` in the reset. The simplest approach: change from `atomic.AddInt64` to `sync.Mutex`-protected int in each mock, controlled by the same `shatterMockCallsMu`.

Simpler implementation: keep `shatterMock%d_callIdx` as `int64` with `atomic`, and in `shatterResetMocks()` use:
```go
b.WriteString(fmt.Sprintf("\tatomic.StoreInt64(&shatterMock%d_callIdx, 0)\n", i))
```

Update mock counter variables to be exported via `shatterMock%d_callIdx` (currently they already are, just use `atomic.StoreInt64` in reset).

Remove the `externalCallsPath` parameter usage from `generateMockFile` (no longer needed since we no longer write to a file). Update callers accordingly.

- [ ] **Step 4: Update ALL callers of `generateMockFile`**

Remove `externalCallsPath string` from the `generateMockFile` signature and update all call sites. There are call sites in:
- `instrument/executor.go` (where `generateMockFile` is called in `ExecuteFunctionWithTiming`)
- `instrument/executor_test.go` (~9 call sites, e.g. lines 482, 615, 644, 771, 798, etc.)
- `instrument/property_test.go` (~5 call sites)
- `instrument/fuzz_test.go` (~1 call site)

Run `grep -n "generateMockFile" shatter-go/instrument/*.go` to find all call sites before editing.

- [ ] **Step 5: Run all tests to verify pass**

```bash
cd shatter-go && go test ./instrument/... -v 2>&1 | tail -20
```
Expected: All mock-related tests pass. If existing tests call `generateMockFile` with the old signature, update them.

- [ ] **Step 6: Commit**

```bash
git add shatter-go/instrument/executor.go shatter-go/instrument/executor_test.go
git commit -m "feat(shatter-go): replace shatterDumpMockCalls with reset/get helpers in mock file"
```

---

## Task 3: Go — Refactor `generateHarness` to stdin loop

**Files:**
- Modify: `shatter-go/instrument/executor.go` (the `generateHarness` function at line ~637)
- Test: `shatter-go/instrument/executor_test.go`

This is the core change. The generated harness goes from a one-shot `main()` to a loop.

**Current signature:** `func generateHarness(funcName string, params []paramInfo, retInfo returnTypeInfo, inputs []json.RawMessage, resultsPath, returnPath, perfPath, globalsPath string, globalVars []globalVarInfo, hasMocks bool) (string, error)`

**New signature:** `func generateHarness(funcName string, params []paramInfo, retInfo returnTypeInfo, globalVars []globalVarInfo, hasMocks bool) (string, error)`

(Remove: `inputs`, `resultsPath`, `returnPath`, `perfPath`, `globalsPath`)

- [ ] **Step 1: Write failing tests for new harness structure**

Add to `executor_test.go`:
```go
func TestGenerateHarnessHasStdinLoop(t *testing.T) {
    params := []paramInfo{{Name: "n", GoType: "int"}}
    retInfo := returnTypeInfo{Count: 1}
    src, err := generateHarness("add", params, retInfo, nil, false)
    if err != nil {
        t.Fatalf("generateHarness: %v", err)
    }
    if !strings.Contains(src, "bufio.NewScanner(os.Stdin)") {
        t.Error("expected bufio.NewScanner in generated harness")
    }
    if !strings.Contains(src, "__shatter_reset()") {
        t.Error("expected __shatter_reset() call in generated harness")
    }
}

func TestGenerateHarnessHasNoHardcodedInputs(t *testing.T) {
    params := []paramInfo{{Name: "n", GoType: "int"}}
    retInfo := returnTypeInfo{Count: 1}
    src, err := generateHarness("add", params, retInfo, nil, false)
    if err != nil {
        t.Fatalf("generateHarness: %v", err)
    }
    // Should not contain the old pattern of baked-in json.Unmarshal from string literal
    if strings.Contains(src, `json.Unmarshal([]byte(`) {
        t.Error("harness should not contain hardcoded JSON unmarshal from literal")
    }
}
```

- [ ] **Step 2: Run to verify failures**

```bash
cd shatter-go && go test ./instrument/... -run "TestGenerateHarness" -v
```
Expected: New tests fail (wrong signature or missing scanner).

- [ ] **Step 3: Rewrite `generateHarness`**

Replace the function body with a stdin-loop-based generator. The new `main()` body structure:

```go
// Imports needed in harness
b.WriteString("import (\n")
b.WriteString("\t\"bufio\"\n")
b.WriteString("\t\"encoding/json\"\n")
b.WriteString("\t\"fmt\"\n")
b.WriteString("\t\"os\"\n")
b.WriteString("\t\"runtime\"\n")
b.WriteString("\t\"time\"\n")
b.WriteString(")\n\n")

b.WriteString("func main() {\n")
b.WriteString("\tscanner := bufio.NewScanner(os.Stdin)\n")
b.WriteString("\tscanner.Buffer(make([]byte, 4*1024*1024), 4*1024*1024)\n")
b.WriteString("\tfor scanner.Scan() {\n")

// Parse request
b.WriteString("\t\tvar __req struct {\n")
b.WriteString("\t\t\tInputs []json.RawMessage `json:\"inputs\"`\n")
b.WriteString("\t\t}\n")
b.WriteString("\t\tif err := json.Unmarshal(scanner.Bytes(), &__req); err != nil {\n")
b.WriteString("\t\t\tjson.NewEncoder(os.Stdout).Encode(map[string]string{\"error\": err.Error()})\n")
b.WriteString("\t\t\tcontinue\n")
b.WriteString("\t\t}\n\n")

// Reset state
b.WriteString("\t\t__shatter_reset()\n")
if hasMocks {
    b.WriteString("\t\tshatterResetMocks()\n")
}

// Perf start
b.WriteString("\t\tvar __memBefore runtime.MemStats\n")
b.WriteString("\t\truntime.ReadMemStats(&__memBefore)\n")
b.WriteString("\t\t__cpuStart := time.Now()\n\n")

// Globals snapshot before
for each globalVar:
    b.WriteString(fmt.Sprintf("\t\t__before_%s, __ok_%s := ...\n", v.Name, v.Name))

// Deserialize params by position from __req.Inputs
for i, p := range params:
    b.WriteString(fmt.Sprintf("\t\tvar %s %s\n", p.Name, p.GoType))
    b.WriteString(fmt.Sprintf("\t\tif len(__req.Inputs) > %d {\n", i))
    b.WriteString(fmt.Sprintf("\t\t\tjson.Unmarshal(__req.Inputs[%d], &%s)\n", i, p.Name))
    b.WriteString("\t\t}\n")

// Call function
// ... same as before but inside the loop

// Collect results inline
b.WriteString("\t\t__recBytes, _ := __shatter_get_results()\n")
b.WriteString("\t\tvar __resp map[string]any\n")
b.WriteString("\t\tjson.Unmarshal(__recBytes, &__resp)\n")
// add return_value, thrown_error, perf, globals diff, mock calls

b.WriteString("\t\tjson.NewEncoder(os.Stdout).Encode(__resp)\n")
b.WriteString("\t}\n") // end for scanner.Scan()
// scanner.Err() exit
b.WriteString("\tif scanner.Err() != nil {\n")
b.WriteString("\t\tos.Exit(1)\n")
b.WriteString("\t}\n")
b.WriteString("}\n")
```

Key details:
- Globals diff: inline the before/after snapshot code, serialize as `global_state_change` side effects added to `__resp["side_effects"]`
- Mock calls: if `hasMocks`, call `shatterGetMockCalls()` and add to `__resp["external_calls"]`
- Perf: `runtime.ReadMemStats(&__memAfter)`, compute CPU elapsed, add to `__resp["performance"]`
- Return value: same serialization as before, stored in `__resp["return_value"]`
- Panic: use `recover()` inside a closure or `defer`-based panic handler

**Panic handling** in the loop: wrap the function call in a closure with recover:
```go
b.WriteString("\t\t__retVal, __panicErr := func() (any, any) {\n")
b.WriteString("\t\t\tdefer func() { if r := recover(); r != nil { panic(r) } }()\n")
```
Actually, use a simpler pattern: two-return closure:
```go
b.WriteString("\t\t__runErr := func() (retErr any) {\n")
b.WriteString("\t\t\tdefer func() { retErr = recover() }()\n")
b.WriteString(fmt.Sprintf("\t\t\t%s = %s(%s)\n", retVarAssign, funcName, argList))
b.WriteString("\t\t\treturn nil\n")
b.WriteString("\t\t}()\n")
```
If `__runErr != nil`, set `thrown_error` in `__resp`. Otherwise set `return_value`.

- [ ] **Step 4: Update `ExecuteFunctionWithTiming` to use new signature**

Find the call to `generateHarness` at line ~291 and remove the `inputs`, `resultsPath`, `returnPath`, `perfPath`, `globalsPath` arguments.

Also update: `mocksPath` no longer needed (mock calls are inline), `globalsPath` no longer needed. Remove related `os.ReadFile` parse blocks after the binary runs (since results come via stdout now).

**Transitional one-shot execution (before Task 5):** `ExecuteFunctionWithTiming` still compiles (or uses cached) binary and runs it once per call. The difference from before is that stdin/stdout are used instead of a subprocess that exits and files are read. Use this temporary approach in Task 3:

```go
// Run the binary with stdin/stdout pipes (one-shot)
cmd := exec.CommandContext(runCtx, binaryPath)
cmd.Dir = outputDir
stdinPipe, _ := cmd.StdinPipe()
var stdoutBuf, stderrBuf strings.Builder
cmd.Stdout = &stdoutBuf
cmd.Stderr = &stderrBuf
cmd.Start()

req, _ := json.Marshal(harnessRequest{Inputs: inputs})
fmt.Fprintf(stdinPipe, "%s\n", req)
stdinPipe.Close() // EOF → harness exits after one iteration

cmd.Wait()
// Parse first line of stdoutBuf as JSON result
```

Task 5 replaces this with a persistent subprocess (no `cmd.Wait()` per call).

- [ ] **Step 5: Run existing executor integration tests**

```bash
cd shatter-go && go test ./instrument/... -run "TestExecuteFunction" -v -count=1 2>&1 | tail -30
```
Expected: All `TestExecuteFunction*` tests pass. These compile and run the harness, verifying the loop produces correct output.

Note: Tests will still compile on every run — subprocess caching is Task 5.

- [ ] **Step 6: Commit**

```bash
git add shatter-go/instrument/executor.go shatter-go/instrument/executor_test.go
git commit -m "feat(shatter-go): refactor generateHarness to stdin loop, remove file-based output"
```

---

## Task 4: Go — Binary caching

**Files:**
- Modify: `shatter-go/instrument/executor.go`
- Test: `shatter-go/instrument/executor_test.go`

Add a cache key + binary cache hit/miss in `ExecuteFunctionWithTiming`. At this point the subprocess is still one-shot (launched, runs one request, exits); subprocess reuse comes in Task 5. This task adds the "don't recompile if binary exists" optimization.

- [ ] **Step 1: Write the failing cache hit test**

```go
func TestExecuteFunctionCacheHitSkipsRecompile(t *testing.T) {
    if testing.Short() {
        t.Skip("skipping compilation test in short mode")
    }
    srcDir := t.TempDir()
    src := writeExecTestSource(t, srcDir, "target.go", `package main
func add(a int, b int) int { return a + b }
`)
    cacheDir := t.TempDir()
    t.Setenv("SHATTER_HARNESS_CACHE", cacheDir)

    inputs := []json.RawMessage{json.RawMessage("3"), json.RawMessage("4")}

    // First call: compiles binary
    _, err := ExecuteFunction(src, "add", inputs, false)
    if err != nil {
        t.Fatalf("first call: %v", err)
    }
    // Find binary in cache
    entries, _ := os.ReadDir(cacheDir)
    if len(entries) == 0 {
        t.Fatal("expected a cache entry after first call")
    }
    // Get mtime
    binaryPath := filepath.Join(cacheDir, entries[0].Name(), "harness")
    info1, _ := os.Stat(binaryPath)

    // Second call with same source, different inputs: should NOT recompile
    _, err = ExecuteFunction(src, "add", []json.RawMessage{json.RawMessage("10"), json.RawMessage("20")}, false)
    if err != nil {
        t.Fatalf("second call: %v", err)
    }
    info2, _ := os.Stat(binaryPath)
    if !info1.ModTime().Equal(info2.ModTime()) {
        t.Error("binary was recompiled on second call — cache miss when hit expected")
    }
}
```

- [ ] **Step 2: Run to verify failure**

```bash
cd shatter-go && go test ./instrument/... -run TestExecuteFunctionCacheHitSkipsRecompile -v -count=1
```
Expected: FAIL — binary is recompiled (mtime changes) or cache entry not found.

- [ ] **Step 3: Add `harnessCacheKey()` and cache hit/miss to `ExecuteFunctionWithTiming`**

Add a helper function:
```go
// harnessCacheKey returns a filesystem-safe string identifying a compiled harness for
// a given source file + function + mocks combination.
func harnessCacheKey(sourcePath, funcName string, mocks []MockConfig) (string, error) {
    data, err := os.ReadFile(sourcePath)
    if err != nil {
        return "", fmt.Errorf("reading source for cache key: %w", err)
    }
    srcHash := sha256.Sum256(data)
    mocksJSON, _ := json.Marshal(mocks)
    mocksHash := sha256.Sum256(mocksJSON)
    return fmt.Sprintf("%x_%x_%s", srcHash, mocksHash, sanitizeMockName(funcName)), nil
}
```

Add a helper for the binary path:
```go
func harnessBinaryPath(cacheKey string) string {
    root := harnessCacheDir()
    if root == "" {
        root = filepath.Join(os.TempDir(), "shatter-harness-cache")
    }
    binaryName := "harness"
    if runtime.GOOS == "windows" {
        binaryName += ".exe"
    }
    return filepath.Join(root, cacheKey, binaryName)
}
```

In `ExecuteFunctionWithTiming`, after instrumentation and before harness generation:
```go
cacheKey, err := harnessCacheKey(sourcePath, funcName, activeMocks)
if err != nil {
    return nil, fmt.Errorf("computing cache key: %w", err)
}
binaryPath := harnessBinaryPath(cacheKey)

if _, err := os.Stat(binaryPath); os.IsNotExist(err) {
    // Cache miss: generate + compile
    harness, err := generateHarness(...)
    // write to build dir, compile, move to binaryPath
    os.MkdirAll(filepath.Dir(binaryPath), 0755)
    os.Rename(builtBinary, binaryPath)
    os.RemoveAll(buildDir)
} else if err != nil {
    return nil, fmt.Errorf("checking cache: %w", err)
}
// Cache hit: binaryPath already exists, skip compile
```

Note: Add `crypto/sha256` to Go imports.

- [ ] **Step 4: Run to verify test passes**

```bash
cd shatter-go && go test ./instrument/... -run TestExecuteFunctionCacheHitSkipsRecompile -v -count=1
```
Expected: PASS.

- [ ] **Step 5: Run all integration tests**

```bash
cd shatter-go && go test ./instrument/... -run "TestExecuteFunction" -v -count=1 2>&1 | tail -20
```
Expected: All pass.

- [ ] **Step 6: Commit**

```bash
git add shatter-go/instrument/executor.go shatter-go/instrument/executor_test.go
git commit -m "feat(shatter-go): add binary caching keyed by source+func+mocks hash"
```

---

## Task 5: Go — Add `HarnessExecutor` with subprocess loop

**Files:**
- Modify: `shatter-go/instrument/executor.go`
- Test: `shatter-go/instrument/executor_test.go`

This task introduces the persistent subprocess. `HarnessExecutor` holds a subprocess map. `ExecuteFunction` and `ExecuteFunctionWithTiming` become methods. Package-level wrappers keep backward compatibility.

- [ ] **Step 1: Write failing subprocess reuse test**

```go
func TestHarnessExecutorReusesSubprocess(t *testing.T) {
    if testing.Short() {
        t.Skip("skipping compilation test")
    }
    srcDir := t.TempDir()
    src := writeExecTestSource(t, srcDir, "target.go", `package main
func add(a int, b int) int { return a + b }
`)
    ex := NewHarnessExecutor()
    defer ex.Shutdown()

    // First call: launches subprocess
    r1, err := ex.ExecuteFunction(src, "add", []json.RawMessage{json.RawMessage("1"), json.RawMessage("2")}, false)
    if err != nil {
        t.Fatalf("first: %v", err)
    }
    // Second call: reuses subprocess
    r2, err := ex.ExecuteFunction(src, "add", []json.RawMessage{json.RawMessage("10"), json.RawMessage("20")}, false)
    if err != nil {
        t.Fatalf("second: %v", err)
    }

    var v1, v2 int
    json.Unmarshal(r1.ReturnValue, &v1)
    json.Unmarshal(r2.ReturnValue, &v2)
    if v1 != 3 {
        t.Errorf("first call: expected 3, got %d", v1)
    }
    if v2 != 30 {
        t.Errorf("second call: expected 30, got %d", v2)
    }
}

func TestHarnessExecutorShutdownCleansSubprocesses(t *testing.T) {
    if testing.Short() {
        t.Skip("skipping compilation test")
    }
    srcDir := t.TempDir()
    src := writeExecTestSource(t, srcDir, "target.go", `package main
func noop() {}
`)
    ex := NewHarnessExecutor()
    ex.ExecuteFunction(src, "noop", nil, false)
    ex.Shutdown()
    if len(ex.subprocesses) != 0 {
        t.Errorf("expected subprocess map empty after shutdown, got %d entries", len(ex.subprocesses))
    }
}
```

- [ ] **Step 2: Run to verify failures**

```bash
cd shatter-go && go test ./instrument/... -run "TestHarnessExecutor" -v -count=1
```
Expected: Compile error — `NewHarnessExecutor`, `HarnessExecutor`, `ex.subprocesses` not defined.

- [ ] **Step 3: Add `HarnessExecutor` struct and methods to `executor.go`**

```go
// HarnessExecutor manages compiled harness subprocesses for reuse across execute calls.
type HarnessExecutor struct {
    subprocesses map[string]*runningHarness
}

type runningHarness struct {
    cmd                    *exec.Cmd
    stdin                  io.WriteCloser
    stdout                 *bufio.Scanner
    discoveredDependencies []DiscoveredDependency // computed once on first launch
}

func NewHarnessExecutor() *HarnessExecutor {
    return &HarnessExecutor{subprocesses: make(map[string]*runningHarness)}
}
```

Add `ExecuteFunction` method:
```go
func (e *HarnessExecutor) ExecuteFunction(sourcePath, funcName string, inputs []json.RawMessage, capture bool, mocks ...[]MockConfig) (*ExecuteResult, error) {
    return e.ExecuteFunctionWithTiming(sourcePath, funcName, inputs, nil, capture, mocks...)
}
```

Add `ExecuteFunctionWithTiming` method (main logic):
1. Compute cache key + binary path
2. Look up subprocess in `e.subprocesses[cacheKey]`
3. If not found: check/build binary, launch subprocess, compute discovered deps, store in map
4. Send request line: `json.NewEncoder(harness.stdin).Encode(req)`
5. Read response line: `harness.stdout.Scan()` + `json.Unmarshal`
6. On write/read error: delete from map, return error
7. Populate `DiscoveredDependencies` from `harness.discoveredDependencies`

Request struct to send to harness:
```go
type harnessRequest struct {
    Inputs []json.RawMessage `json:"inputs"`
}
```

Response parsing: `json.Unmarshal(harness.stdout.Bytes(), &result)`. If `result` has an `error` field, return it as `ExecuteError`.

Launching subprocess:
```go
cmd := exec.Command(binaryPath)
cmd.Dir = outputDir // or just the cache entry dir
stdin, _ := cmd.StdinPipe()
stdout, _ := cmd.StdoutPipe()
cmd.Stderr = os.Stderr // pass through harness stderr for diagnostics
cmd.Start()
scanner := bufio.NewScanner(stdout)
scanner.Buffer(make([]byte, 4*1024*1024), 4*1024*1024)
```

Add `Shutdown` method. Note: do NOT delete from the map while ranging over it — collect and drain separately:
```go
func (e *HarnessExecutor) Shutdown() {
    for _, h := range e.subprocesses {
        h.stdin.Close()
        done := make(chan struct{})
        go func(h *runningHarness) { h.cmd.Wait(); close(done) }(h)
        select {
        case <-done:
        case <-time.After(1 * time.Second):
            h.cmd.Process.Kill()
            <-done
        }
    }
    e.subprocesses = make(map[string]*runningHarness)
}
```

- [ ] **Step 4: Update package-level wrappers to use single-use HarnessExecutor**

```go
func ExecuteFunction(sourcePath, funcName string, inputs []json.RawMessage, capture bool, mocks ...[]MockConfig) (*ExecuteResult, error) {
    ex := NewHarnessExecutor()
    defer ex.Shutdown()
    return ex.ExecuteFunction(sourcePath, funcName, inputs, capture, mocks...)
}

func ExecuteFunctionWithTiming(sourcePath, funcName string, inputs []json.RawMessage, timing *frontendtiming.Collector, capture bool, mocks ...[]MockConfig) (*ExecuteResult, error) {
    ex := NewHarnessExecutor()
    defer ex.Shutdown()
    return ex.ExecuteFunctionWithTiming(sourcePath, funcName, inputs, timing, capture, mocks...)
}
```

- [ ] **Step 5: Run all instrument tests**

```bash
cd shatter-go && go test ./instrument/... -v -count=1 2>&1 | tail -30
```
Expected: All pass, including new subprocess reuse and shutdown tests.

- [ ] **Step 6: Commit**

```bash
git add shatter-go/instrument/executor.go shatter-go/instrument/executor_test.go
git commit -m "feat(shatter-go): add HarnessExecutor with persistent subprocess and binary caching"
```

---

## Task 5b: Go — Property tests and fuzz target for `HarnessExecutor`

**Files:**
- Modify: `shatter-go/instrument/property_test.go`
- Modify: `shatter-go/instrument/fuzz_test.go`

Required by project policy (CLAUDE.md): non-trivial stateful components need rapid property tests; new deserialization boundaries need fuzz targets.

- [ ] **Step 1: Add `HarnessExecutor` state machine property tests to `property_test.go`**

Using `rapid`:
```go
func TestHarnessExecutorStateConsistency(t *testing.T) {
    rapid.Check(t, func(t *rapid.T) {
        // Generate a sequence of 1-5 execute calls on the same source
        n := rapid.IntRange(1, 5).Draw(t, "n")
        srcDir := t.TempDir()
        src := writeExecTestSource(t, srcDir, "target.go", `package main
func add(a int, b int) int { return a + b }
`)
        ex := NewHarnessExecutor()
        defer ex.Shutdown()
        for i := 0; i < n; i++ {
            a := rapid.Int().Draw(t, "a")
            b := rapid.Int().Draw(t, "b")
            r, err := ex.ExecuteFunction(src, "add", []json.RawMessage{
                json.RawMessage(fmt.Sprintf("%d", a)),
                json.RawMessage(fmt.Sprintf("%d", b)),
            }, false)
            if err != nil {
                t.Fatalf("execute %d: %v", i, err)
            }
            var got int
            json.Unmarshal(r.ReturnValue, &got)
            if got != a+b {
                t.Fatalf("expected %d, got %d", a+b, got)
            }
        }
        ex.Shutdown()
        if len(ex.subprocesses) != 0 {
            t.Errorf("subprocess map not empty after shutdown")
        }
    })
}
```

- [ ] **Step 2: Add fuzz target for harness response parsing**

The `HarnessExecutor` parses arbitrary JSON from subprocess stdout. Add a fuzz target in `fuzz_test.go`:
```go
func FuzzHarnessResponseParsing(f *testing.F) {
    // Seed corpus: valid responses
    f.Add([]byte(`{"return_value":42,"branch_path":[],"lines_executed":[],"side_effects":[],"scope_events":[],"performance":{"wall_time_ms":1.0,"cpu_time_us":0,"heap_used_bytes":0,"heap_allocated_bytes":0}}`))
    f.Add([]byte(`{"error":"panic: index out of range"}`))
    f.Add([]byte(`{}`))
    f.Fuzz(func(t *testing.T, data []byte) {
        var result ExecuteResult
        // Must not panic
        _ = json.Unmarshal(data, &result)
    })
}
```

- [ ] **Step 3: Run property and fuzz tests**

```bash
cd shatter-go && go test ./instrument/... -run TestHarnessExecutorStateConsistency -v -count=1
cd shatter-go && go test ./instrument/... -run FuzzHarnessResponseParsing -fuzz=FuzzHarnessResponseParsing -fuzztime=10s
```
Expected: Property test passes. Fuzz test runs 10s with no crashes.

- [ ] **Step 4: Commit**

```bash
git add shatter-go/instrument/property_test.go shatter-go/instrument/fuzz_test.go
git commit -m "test(shatter-go): add property tests and fuzz target for HarnessExecutor"
```

---

## Task 6: Go — Integrate `HarnessExecutor` into `Handler`

**Files:**
- Modify: `shatter-go/protocol/handler.go`
- Test: `shatter-go/protocol/handler_test.go` (if it exists, otherwise `protocol/` integration tests)

The protocol `Handler` should hold a persistent `*instrument.HarnessExecutor` so subprocesses are reused across multiple execute calls in the same session.

- [ ] **Step 1: Add `harnessExecutor` field to `Handler`**

In `handler.go`, update `Handler` struct. Note: update ALL constructors — `NewHandler`, `NewHandlerWithLogLevel`, and any test constructors in `handler_test.go` — to initialize `harnessExecutor: instrument.NewHarnessExecutor()`.
```go
type Handler struct {
    reader           *bufio.Scanner
    writer           io.Writer
    log              *slog.Logger
    lastAnalyzedFile string
    registry         *generators.Registry
    setupLoader      *setup.Loader
    timingEnabled    bool
    harnessExecutor  *instrument.HarnessExecutor // NEW
}
```

Update `NewHandler` and `NewHandlerWithLogLevel`:
```go
harnessExecutor: instrument.NewHarnessExecutor(),
```

- [ ] **Step 2: Update execute handler to use `h.harnessExecutor`**

In the `handleExecute` function (line ~300), replace:
```go
result, err := instrument.ExecuteFunctionWithTiming(file, *req.Function, req.Inputs, timing, capture, execMocks)
```
with:
```go
result, err := h.harnessExecutor.ExecuteFunctionWithTiming(file, *req.Function, req.Inputs, timing, capture, execMocks)
```

- [ ] **Step 3: Add subprocess cleanup to the shutdown handler**

Find the `handleShutdown` function or where the `shutdown` command is handled. Add:
```go
h.harnessExecutor.Shutdown()
```

- [ ] **Step 4: Run protocol-level tests**

```bash
cd shatter-go && go test ./protocol/... -v -count=1 2>&1 | tail -20
```
Expected: All existing protocol tests pass.

- [ ] **Step 5: Commit**

```bash
git add shatter-go/protocol/handler.go
git commit -m "feat(shatter-go): integrate HarnessExecutor into protocol Handler"
```

---

## Task 7: Rust — Refactor `generate_harness` to stdin loop

**Files:**
- Modify: `shatter-rust/src/executor.rs`
- Test: `shatter-rust/src/executor.rs` (tests module at line ~1036)

**Current signature:** `fn generate_harness(instrumented_source, function_name, param_names, param_types, return_type, inputs_json, mocks_json, static_mut_names) -> Result<String, ExecuteError>`

**New signature:** `fn generate_harness(instrumented_source, function_name, param_names, param_types, return_type, static_mut_names) -> Result<String, ExecuteError>`

(Remove: `inputs_json`, `mocks_json`)

In the Rust harness, `shatter_rust_runtime::reset()` already clears branches AND mocks (see `clear()` in the runtime). In the loop, for each iteration:
1. Read request line from stdin (`serde_json::from_str`)
2. Extract `inputs` and `mocks` from request
3. Register mocks: `shatter_rust_runtime::register_mock(symbol, return_values)`
4. Call `shatter_rust_runtime::reset()` — this clears state AND mocks, so register mocks AFTER reset

Wait: in the current harness, mocks are registered THEN reset() is called. Looking at the runtime: `reset()` calls `clear()` which calls `self.mock_registry.clear()`. So mocks are cleared by reset. But then the harness calls `reset()` AFTER registering mocks... that would clear the mocks!

Look at this sequence in the current harness:
```
1. register mocks
2. reset()  ← clears mock_registry!
3. execute
```

This seems like a bug. Let me re-check what `reset()` does vs `flush_results()`.

Actually looking at the harness code in `generate_harness`:
- Line 551: embed inputs_json
- Line 557: embed mocks_json
- Line 561-568: register mocks from mocks_json
- Line 571: `shatter_rust_runtime::reset()`  ← AFTER mocks are registered

But `reset()` clears `mock_registry`! So this would break mocks. Either:
a) The test suite doesn't test Rust mocks in practice
b) I'm misreading the order

Let me re-read:
```rust
h.push_str(&format!("    let mocks_json = r#\"{}\"#;\n", mocks_json));
h.push_str("    let mocks: Vec<Value> = serde_json::from_str(mocks_json).unwrap_or_default();\n");
h.push_str("    for mock in &mocks {\n");
// ... register mocks
h.push_str("    // Reset runtime state\n");
h.push_str("    shatter_rust_runtime::reset();\n\n");
```

Yes, mocks are registered THEN reset is called. This clears the mock registry. For the loop, we should call `reset()` FIRST, then register mocks. This is also the correct order for the current one-shot harness — it's a pre-existing bug that doesn't matter for tests that don't actually use mocks with Rust.

For the stdin loop in Rust: correct order per iteration:
1. Parse request (inputs + mocks)
2. `shatter_rust_runtime::reset()` — clear all state including mocks
3. Register mocks from request
4. Execute function
5. Collect results via `flush_results()`

- [ ] **Step 1: Write failing tests for new harness structure**

In the `mod tests` block in `executor.rs`, add:
```rust
#[test]
fn generate_harness_uses_stdin_loop() {
    let source = "fn add(a: i32, b: i32) -> i32 { a + b }";
    let harness = generate_harness(
        source,
        "add",
        &["a".to_string(), "b".to_string()],
        &["i32".to_string(), "i32".to_string()],
        Some("i32"),
        &[],
    ).unwrap();
    assert!(harness.contains("stdin().lock().lines()"),
        "expected stdin loop in harness, got:\n{}", harness);
    assert!(!harness.contains("let inputs_json = r#"),
        "harness should not contain hardcoded inputs_json");
}

#[test]
fn generate_harness_no_inputs_param() {
    // New signature: no inputs_json or mocks_json
    let harness = generate_harness(
        "fn f() {}",
        "f",
        &[],
        &[],
        None,
        &[],
    );
    assert!(harness.is_ok());
}
```

- [ ] **Step 2: Run to verify failures**

```bash
cd shatter-rust && cargo test generate_harness_uses_stdin_loop generate_harness_no_inputs_param -- --nocapture 2>&1 | tail -20
```
Expected: Compile error — wrong number of arguments to `generate_harness`.

- [ ] **Step 3: Rewrite `generate_harness`**

Remove `inputs_json: &str` and `mocks_json: &str` parameters. Replace the hardcoded input/mock embedding with a stdin loop body.

New generated `main()` structure:
```rust
fn main() {
    use std::io::BufRead;
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) if l.trim().is_empty() => continue,
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

        // Reset runtime state (clears branches, external_calls, and mock_registry)
        shatter_rust_runtime::reset();

        // Register mocks from request
        for mock in &mocks {
            if let (Some(symbol), Some(return_values)) = (
                mock.get("symbol").and_then(|s| s.as_str()),
                mock.get("return_values").and_then(|v| v.as_array()),
            ) {
                shatter_rust_runtime::register_mock(symbol, return_values.clone());
            }
        }

        // [snapshot mutable globals before — same as before]
        // [deserialize params from inputs[0], inputs[1], ...]
        // [call function in catch_unwind, measure wall_time_ms]
        // [collect runtime results via flush_results()]
        // [add return_value/thrown_error, performance, side_effects]
        println!("{}", serde_json::to_string(&exec_result).unwrap()); // exempt: generated harness
    }
}
```

Update `execute_function_with_timing` to remove the `serialize_inputs` timing block and stop passing `inputs_json`/`mocks_json` to `generate_harness`.

- [ ] **Step 4: Run existing harness structure tests**

```bash
cd shatter-rust && cargo test generate_harness -- --nocapture 2>&1 | tail -20
```
Expected: All pass, including old tests that check for function calls, void functions, etc. Update any that checked for `inputs_json` presence.

- [ ] **Step 5: Commit**

```bash
git add shatter-rust/src/executor.rs
git commit -m "feat(shatter-rust): refactor generate_harness to stdin loop, remove hardcoded inputs"
```

---

## Task 8: Rust — Binary caching

**Files:**
- Modify: `shatter-rust/src/executor.rs`
- Test: `shatter-rust/src/executor.rs` (tests module)

Note: Add `sha2` crate to `shatter-rust/Cargo.toml` as a dependency.

- [ ] **Step 1: Add `sha2` dependency**

In `shatter-rust/Cargo.toml`, under `[dependencies]`:
```toml
sha2 = "0.10"
```

- [ ] **Step 2: Write failing cache test**

```rust
#[test]
fn execute_function_caches_binary() {
    let cache_dir = std::env::temp_dir().join(format!("shatter-test-cache-{}", std::process::id()));
    std::env::set_var("SHATTER_HARNESS_CACHE", &cache_dir);
    defer_rm_dir(&cache_dir); // helper: remove on drop

    let source = r#"pub fn add(a: i32, b: i32) -> i32 { a + b }"#;
    let tmp = tempfile_with_content(source, "add_test.rs"); // helper

    // First call
    execute_function(&tmp, "add", &[json!(1), json!(2)], &[], 10000).unwrap();

    // Find binary in cache
    let entries: Vec<_> = std::fs::read_dir(&cache_dir).unwrap().collect();
    assert!(!entries.is_empty(), "expected cache entry");
    let bin = entries[0].as_ref().unwrap().path().join("harness");
    let mtime1 = std::fs::metadata(&bin).unwrap().modified().unwrap();

    // Second call with different inputs
    execute_function(&tmp, "add", &[json!(10), json!(20)], &[], 10000).unwrap();
    let mtime2 = std::fs::metadata(&bin).unwrap().modified().unwrap();

    assert_eq!(mtime1, mtime2, "binary should not be recompiled on cache hit");
    std::env::remove_var("SHATTER_HARNESS_CACHE");
}
```

- [ ] **Step 3: Add cache key + hit/miss logic**

Add helper functions:
```rust
fn harness_cache_key(source: &str, function_name: &str, mocks: &[Value]) -> String {
    use sha2::{Sha256, Digest};
    let mut src_hasher = Sha256::new();
    src_hasher.update(source.as_bytes());
    let src_hash = src_hasher.finalize();
    let mocks_json = serde_json::to_string(mocks).unwrap_or_default();
    let mut mock_hasher = Sha256::new();
    mock_hasher.update(mocks_json.as_bytes());
    let mock_hash = mock_hasher.finalize();
    // Sanitize function_name: keep only alphanumeric + underscore to prevent path issues
    let safe_name: String = function_name.chars()
        .map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' })
        .collect();
    format!("{:x}_{:x}_{}", src_hash, mock_hash, safe_name)
}

fn harness_binary_path(cache_key: &str) -> std::path::PathBuf {
    let root = std::env::var("SHATTER_HARNESS_CACHE")
        .ok()
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("shatter-harness-cache"));
    let binary_name = if cfg!(windows) { "harness.exe" } else { "harness" };
    root.join(cache_key).join(binary_name)
}
```

In `execute_function_with_timing`, after reading source and before harness generation:
```rust
let cache_key = harness_cache_key(&source, function_name, mocks);
let binary_path = harness_binary_path(&cache_key);

if !binary_path.exists() {
    // cache miss: generate, compile, move to cache
    let harness = generate_harness(...)?;
    // ... write to temp_dir, cargo build ...
    std::fs::create_dir_all(binary_path.parent().unwrap())?;
    std::fs::rename(&built_binary_path, &binary_path)?;
    std::fs::remove_dir_all(&temp_dir)?;
}
// binary_path now exists (hit or miss)
```

- [ ] **Step 4: Run tests**

```bash
cd shatter-rust && cargo test -- --nocapture 2>&1 | tail -20
```
Expected: Cache test passes, all existing tests still pass.

- [ ] **Step 5: Commit**

```bash
git add shatter-rust/src/executor.rs shatter-rust/Cargo.toml
git commit -m "feat(shatter-rust): add binary caching keyed by source+func+mocks hash"
```

---

## Task 9: Rust — Add `HarnessExecutor` with subprocess loop

**Files:**
- Modify: `shatter-rust/src/executor.rs`
- Test: `shatter-rust/src/executor.rs` (tests module)

- [ ] **Step 1: Write failing subprocess reuse test**

```rust
#[test]
fn harness_executor_reuses_subprocess() {
    let source = r#"pub fn add(a: i32, b: i32) -> i32 { a + b }"#;
    let tmp = tempfile_with_content(source, "add_test.rs");

    let mut ex = HarnessExecutor::new();
    let r1 = ex.execute_function(&tmp, "add", &[json!(1), json!(2)], &[], 10000).unwrap();
    let r2 = ex.execute_function(&tmp, "add", &[json!(10), json!(20)], &[], 10000).unwrap();
    ex.shutdown();

    assert_eq!(r1.return_value, Some(json!(3)));
    assert_eq!(r2.return_value, Some(json!(30)));
}

#[test]
fn harness_executor_shutdown_clears_map() {
    let source = r#"pub fn noop() {}"#;
    let tmp = tempfile_with_content(source, "noop_test.rs");
    let mut ex = HarnessExecutor::new();
    ex.execute_function(&tmp, "noop", &[], &[], 10000).unwrap();
    ex.shutdown();
    assert!(ex.subprocesses.is_empty());
}
```

- [ ] **Step 2: Run to verify failures**

```bash
cd shatter-rust && cargo test harness_executor -- --nocapture 2>&1 | tail -10
```
Expected: Compile error — `HarnessExecutor` not defined.

- [ ] **Step 3: Add `HarnessExecutor` to `executor.rs`**

```rust
use std::collections::HashMap;
use std::process::{Child, ChildStdin, ChildStdout};
use std::io::{BufRead, BufReader, Write};

struct RunningHarness {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

pub struct HarnessExecutor {
    subprocesses: HashMap<String, RunningHarness>,
}

impl HarnessExecutor {
    pub fn new() -> Self {
        Self { subprocesses: HashMap::new() }
    }

    pub fn execute_function(
        &mut self,
        file_path: &str,
        function_name: &str,
        inputs: &[Value],
        mocks: &[Value],
        timeout_ms: u64,
    ) -> Result<ExecuteResult, ExecuteError> {
        self.execute_function_with_timing(file_path, function_name, inputs, mocks, timeout_ms, None)
    }

    pub fn execute_function_with_timing(
        &mut self,
        file_path: &str,
        function_name: &str,
        inputs: &[Value],
        mocks: &[Value],
        timeout_ms: u64,
        timing: Option<&mut TimingCollector>,
    ) -> Result<ExecuteResult, ExecuteError> {
        // [same prep as execute_function_with_timing: read source, extract sig, etc.]
        // Compute cache key + binary path
        // Check binary exists (compile if not)
        // Look up in self.subprocesses
        // If not found: launch subprocess, store in map
        // Send request JSON line to stdin
        // Read response JSON line from stdout
        // Parse and return ExecuteResult
        // On error: remove from map, return Err
        todo!()
    }

    pub fn shutdown(&mut self) {
        for (_, mut h) in self.subprocesses.drain() {
            drop(h.stdin); // close stdin → EOF → harness exits
            let timeout = std::time::Duration::from_secs(1);
            let start = std::time::Instant::now();
            loop {
                if h.child.try_wait().map(|s| s.is_some()).unwrap_or(true) { break; }
                if start.elapsed() > timeout { let _ = h.child.kill(); break; }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
    }
}
```

Subprocess launch code inside `execute_function_with_timing`:
```rust
use std::process::{Command, Stdio};
let mut child = Command::new(&binary_path)
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::inherit())
    .spawn()
    .map_err(|e| ExecuteError::OutputParseError(format!("failed to spawn harness: {e}")))?;
let stdin = child.stdin.take().unwrap();
let stdout = BufReader::new(child.stdout.take().unwrap());
self.subprocesses.insert(cache_key.clone(), RunningHarness { child, stdin, stdout });
```

Send request + read response:
```rust
let req = serde_json::json!({"inputs": inputs, "mocks": mocks});
let h = self.subprocesses.get_mut(&cache_key).unwrap();
let req_line = serde_json::to_string(&req).unwrap() + "\n";
if h.stdin.write_all(req_line.as_bytes()).is_err() {
    self.subprocesses.remove(&cache_key);
    return Err(ExecuteError::OutputParseError("harness stdin write failed".to_string()));
}
let mut resp_line = String::new();
if h.stdout.read_line(&mut resp_line).is_err() || resp_line.is_empty() {
    self.subprocesses.remove(&cache_key);
    return Err(ExecuteError::OutputParseError("harness stdout read failed".to_string()));
}
serde_json::from_str::<ExecuteResult>(resp_line.trim())
    .map_err(|e| ExecuteError::OutputParseError(format!("parse error: {e}")))
```

- [ ] **Step 4: Update package-level `execute_function` / `execute_function_with_timing` to use single-use executor**

```rust
pub fn execute_function(
    file_path: &str, function_name: &str, inputs: &[Value], mocks: &[Value], timeout_ms: u64,
) -> Result<ExecuteResult, ExecuteError> {
    let mut ex = HarnessExecutor::new();
    let r = ex.execute_function(file_path, function_name, inputs, mocks, timeout_ms);
    ex.shutdown();
    r
}
```

- [ ] **Step 5: Run all Rust tests**

```bash
cd shatter-rust && cargo test -- --nocapture 2>&1 | tail -30
```
Expected: All pass.

- [ ] **Step 6: Run Clippy**

```bash
cd shatter-rust && cargo clippy 2>&1 | grep "^error"
```
Expected: No errors.

- [ ] **Step 7: Commit**

```bash
git add shatter-rust/src/executor.rs
git commit -m "feat(shatter-rust): add HarnessExecutor with persistent subprocess and binary caching"
```

---

## Task 9b: Rust — Property tests for `HarnessExecutor`

**Files:**
- Modify: `shatter-rust/src/executor.rs` (proptest module)

Required by CLAUDE.md: non-trivial stateful components need proptest coverage.

- [ ] **Step 1: Add state machine property test**

In the `#[cfg(test)]` module in `executor.rs`:
```rust
#[cfg(test)]
mod harness_executor_props {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn harness_executor_n_calls_same_function(
            a_vals in prop::collection::vec(any::<i32>(), 1..5),
            b_vals in prop::collection::vec(any::<i32>(), 1..5),
        ) {
            let n = a_vals.len().min(b_vals.len());
            // write a temp source file with an add function
            let dir = std::env::temp_dir().join(format!("shatter-prop-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let src_path = dir.join("add.rs");
            std::fs::write(&src_path, "pub fn add(a: i32, b: i32) -> i32 { a + b }").unwrap();

            let mut ex = HarnessExecutor::new();
            for i in 0..n {
                let r = ex.execute_function(
                    src_path.to_str().unwrap(),
                    "add",
                    &[json!(a_vals[i]), json!(b_vals[i])],
                    &[],
                    10000,
                ).unwrap();
                let got: i32 = serde_json::from_value(r.return_value.unwrap()).unwrap();
                prop_assert_eq!(got, a_vals[i].wrapping_add(b_vals[i]));
            }
            ex.shutdown();
            prop_assert!(ex.subprocesses.is_empty());
            std::fs::remove_dir_all(&dir).ok();
        }
    }
}
```

- [ ] **Step 2: Run property tests**

```bash
cd shatter-rust && cargo test harness_executor_n_calls_same_function -- --nocapture 2>&1 | tail -10
```
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add shatter-rust/src/executor.rs
git commit -m "test(shatter-rust): add property tests for HarnessExecutor state machine"
```

---

## Task 10: Rust — Integrate `HarnessExecutor` into `Handler`

**Files:**
- Modify: `shatter-rust/src/handler.rs`
- Test: `shatter-rust/src/handler.rs` (tests module)

- [ ] **Step 1: Add `harness_executor` field to `Handler`**

In `handler.rs`, import `HarnessExecutor` from the executor module:
```rust
use crate::executor::HarnessExecutor;
```

Add to `Handler<R, W, L>` struct:
```rust
harness_executor: HarnessExecutor,
```

Update ALL constructors — `Handler::new`, `Handler::with_log_level`, and the test-only `Handler::new_with_opts` (if present) — to include:
```rust
harness_executor: HarnessExecutor::new(),
```

Note: there is also a test constructor at line ~139 that creates `Handler` by value inside `#[cfg(test)]` — find it with `grep -n "Handler {" shatter-rust/src/handler.rs` and add the field there too.

Drop the `#[allow(dead_code)]` on `harness_cache_dir` and `harness_scratch_dir` — these may now be used.

- [ ] **Step 2: Update `handle_execute` to use `self.harness_executor`**

Change `fn handle_execute(&self, ...)` → `fn handle_execute(&mut self, ...)`.

Update `dispatch` to pass `mut` self through to `handle_execute`. `dispatch` already takes `&mut self`.

Replace:
```rust
crate::executor::execute_function_with_timing(...)
crate::executor::execute_function(...)
```
with:
```rust
self.harness_executor.execute_function_with_timing(...)
self.harness_executor.execute_function(...)
```

- [ ] **Step 3: Add subprocess cleanup to `handle_shutdown`**

Change `fn handle_shutdown(&self, ...)` → `fn handle_shutdown(&mut self, ...)`.

Add:
```rust
self.harness_executor.shutdown();
```

- [ ] **Step 4: Run handler tests**

```bash
cd shatter-rust && cargo test -- --nocapture 2>&1 | tail -30
```
Expected: All pass.

- [ ] **Step 5: Commit**

```bash
git add shatter-rust/src/handler.rs
git commit -m "feat(shatter-rust): integrate HarnessExecutor into Handler, add shutdown cleanup"
```

---

## Task 11: E2E Verification

**Files:** Read-only. Run tests only.

- [ ] **Step 1: Run Go full test suite**

```bash
cd shatter-go && go test ./... -count=1 2>&1 | tail -20
```
Expected: All pass.

- [ ] **Step 2: Run Rust full test suite including E2E**

```bash
cargo test --workspace 2>&1 | tail -20
```
Expected: All pass.

- [ ] **Step 3: Run Go frontend integration tests end-to-end**

These tests compile and run real harness binaries, exercising the full stdin/stdout loop:
```bash
cd shatter-go && go test ./instrument/... -run "TestExecuteFunction" -v -count=1 2>&1 | tail -30
```
Expected: All `TestExecuteFunction*` pass, including the new reuse and cache tests.

- [ ] **Step 4: Run E2E concolic tests for the Rust pipeline**

```bash
cargo test --test e2e_concolic -- --nocapture 2>&1 | tail -30
```
Expected: All E2E concolic tests pass. These exercise the full analyze → instrument → execute → solve pipeline through the TypeScript frontend. The Rust-language frontend changes are validated by the integration tests in Step 3.

- [ ] **Step 5: Run quick test tier**

```bash
npx task test-quick 2>&1 | tail -20
```
Expected: Pass.

- [ ] **Step 6: Run standard test tier**

```bash
npx task test-standard 2>&1 | tail -20
```
Expected: Pass.

- [ ] **Step 7: Run parity check**

```bash
npx task parity 2>&1 | tail -10
```
Expected: Pass (no protocol-visible behavior changes in this implementation).

- [ ] **Step 8: Mark issues closed**

```bash
bd close str-9u7n --reason "stdin harness loop implemented in both Go and Rust frontends"
bd close str-lefp --reason "persistent subprocess with binary caching implemented; closes with str-9u7n"
```

- [ ] **Step 9: Final commit**

```bash
git add .
git commit -m "chore: close str-9u7n and str-lefp — stdin harness loop complete"
```
