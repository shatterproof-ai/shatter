# Plan: Tasks #9, #10, #11 — Mock Parity, Refactoring Recs, strace Discovery

## Context

Three P3 issues from the external dependency mocking epic (str-3ky9). Task #10 is worked first (clearest scope), then #9, then #11. Each gets its own branch.

---

## Task #10: [str-3ky9.16] Go Frontend Mock Parity

### What's Already Working
- `generateMockFile()` handles `repeat_last`, `passthrough`, `cycle` behaviors
- Per-execution mock variation works (core orchestrator sends different MockConfig per execute request)
- Custom generator integration exists (`generate` command in `handler.go:499-535`)

### Gaps to Fill

#### 1. `throw_error` Behavior in `generateMockFile()`

**File:** `shatter-go/instrument/executor.go:614-666`

Add a `throw_error` case after the `passthrough` skip (line 615-617). The generated mock function:
- Extracts message from `return_values[idx]` (if map with `"message"` key, use it; else `"Mock error: <symbol>"`)
- Records call if `ShouldTrackCalls` (before panic, since defer in harness preserves dump)
- Calls `panic(msg)` — Go's equivalent of throw. The harness already captures panics as non-zero exit + stderr → `ThrownError` in response

Structure: reuse the existing retvals/idx preamble, then diverge at the return:
```go
if mock.DefaultBehavior == "throw_error" {
    // ... extract message from retVal map ...
    // ... record call if tracking ...
    // panic instead of return
}
```

#### 2. `DiscoveredDependency` Reporting

**File:** `shatter-go/protocol/types.go` — Add types:
```go
type DiscoveredDependency struct {
    Symbol           string `json:"symbol"`
    SourceModule     string `json:"source_module"`
    Kind             string `json:"kind"`                // "unmocked_import" | "subprocess_spawn"
    IsSubprocessSpawn bool  `json:"is_subprocess_spawn"`
}
```

**File:** `shatter-go/protocol/types.go:89-118` — Add to `Response`:
```go
DiscoveredDependencies []DiscoveredDependency `json:"discovered_dependencies,omitempty"`
```

**File:** `shatter-go/instrument/executor.go` — Add discovery logic:
- After analyzing the source AST, extract import paths
- Compare against mocked module prefixes (from mock symbols `"module:export"` → `"module"`)
- Flag `os/exec` as `subprocess_spawn`
- Flag non-stdlib, non-mocked imports as `unmocked_import`
- Heuristic for stdlib: imports without `.` in path are stdlib (e.g., `fmt`, `os`); third-party has domain prefix (`github.com/...`)

**File:** `shatter-go/protocol/handler.go` — Wire through:
- Convert `instrument.DiscoveredDependency` → `protocol.DiscoveredDependency` in `handleExecute()`

### Tests

| Test | File | Type |
|------|------|------|
| `TestGenerateMockFileThrowError` | `instrument/executor_test.go` | Unit — verify generated code contains `panic(` |
| `TestDiscoverDepsUnmockedImports` | `instrument/executor_test.go` | Unit — parse source with imports, verify discovery |
| `TestDiscoverDepsSubprocessSpawn` | `instrument/executor_test.go` | Unit — `os/exec` → subprocess_spawn |
| Execute response roundtrip with discovered_deps | `protocol/handler_test.go` | Integration |
| `TestPropertyThrowErrorMockContainsPanic` | `instrument/property_test.go` | Rapid PBT |
| `TestPropertyDiscoverDepsExcludesMocked` | `instrument/property_test.go` | Rapid PBT |

### Verification
```bash
cd shatter-go && go test ./... && go vet ./...
```

---

## Task #9: [str-3ky9.15] Refactoring Recs for Hard-to-Mock Deps

### Design

New module `shatter-core/src/mockability.rs` with detection logic and recommendation types. Integrates into scan output.

### Types

```rust
pub enum MockabilityIssue {
    DynamicDispatch,
    ClosureWithHiddenDeps,
    SubprocessSpawning,
    MultiLayerIndirection,
}

pub struct RefactoringRecommendation {
    pub function_name: String,
    pub dependency_symbol: String,
    pub call_sites: Vec<u32>,
    pub issue: MockabilityIssue,
    pub explanation: String,
    pub suggestion: String,
}
```

### Detection Functions

| Detector | Signal | Recommendation |
|----------|--------|----------------|
| `detect_subprocess_spawning` | dep.source_module in SUBPROCESS_MODULES | "Wrap subprocess call behind interface for test substitution" |
| `detect_dynamic_dispatch` | MethodCall + Unknown/Opaque return type | "Extract dependency to constructor parameter" |
| `detect_closure_with_hidden_deps` | Function-typed param + I/O deps on same fn | "Extract closure body to named function with explicit dependencies" |
| `detect_multi_layer_indirection` | CallGraph: A→B→external I/O dep | "Pass dependency as parameter to inner function" |

### Integration Points

- **`shatter-core/src/lib.rs`** — register `pub mod mockability`
- **`shatter-core/src/scan_orchestrator.rs`** — add `refactoring_recommendations` to `FunctionResult`/`ScanResult`, call `analyze_mockability()` after building analysis + call graph, update `format_scan_report()` and `format_dry_run_plan()`
- **`shatter-core/src/report.rs`** — add field to `FunctionReport`/`CodebaseReport` for JSON output

### Constants (in `mockability.rs`)

```rust
const SUBPROCESS_MODULES: &[&str] = &[
    "child_process", "node:child_process", "os/exec", "std::process",
];
```

### Tests

- Unit tests per detector (positive + no false positives)
- Proptest: recommendations have non-empty fields, count ≤ deps + callees
- Integration: `format_scan_report` includes/omits section correctly

### Verification
```bash
cargo test -p shatter-core && cargo clippy -p shatter-core -- -D warnings
```

---

## Task #11: [str-3ky9.17] strace-Based Dep Discovery

### Design

New module `shatter-core/src/strace_deps.rs` + CLI subcommand `shatter discover-deps`.

### Types (in `strace_deps.rs`)

```rust
pub enum Transport { Tcp, Udp, Unix, Other(String) }

pub struct NetworkEndpoint {
    pub transport: Transport,
    pub address: String,
    pub port: Option<u16>,
    pub syscall: String,
    pub count: u32,
}

pub struct StraceReport {
    pub command: Vec<String>,
    pub endpoints: Vec<NetworkEndpoint>,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
    pub warnings: Vec<String>,
}
```

### CLI (`shatter-cli/src/main.rs`)

```rust
DiscoverDeps {
    #[arg(long, default_value = "strace")]
    method: String,
    #[arg(long, default_value = "text")]
    format: String,
    #[arg(long, short)]
    output: Option<PathBuf>,
    #[arg(trailing_var_arg = true, required = true)]
    command: Vec<String>,
}
```

Usage: `shatter discover-deps --strace -- node app.js`

### strace Runner

- Platform check: `#[cfg(not(target_os = "linux"))]` → error
- Check strace availability
- Spawn: `strace -f -e trace=network -e signal=none -- <command>`
- Parse stderr for `connect()`, `sendto()`, `bind()` with AF_INET/AF_INET6/AF_UNIX
- Deduplicate by (address, port, transport)

### Regex Patterns (compiled once with `OnceLock`)

- `connect(...{sa_family=AF_INET, sin_port=htons(PORT), sin_addr=inet_addr("IP")}...)`
- IPv6 variant with `AF_INET6`
- Unix socket variant with `AF_UNIX, sun_path="PATH"`
- `sendto` variant

### Output Formats

- **text**: table with Protocol | Address | Port | Syscall | Count
- **json**: serde_json serialize
- **yaml**: serde_yaml serialize

### Tests

- Unit: parse known strace lines, deduplication, malformed lines skipped
- CLI arg parsing tests
- Integration (Linux-gated): trace `curl` and verify endpoint detected
- Proptest: arbitrary IP/port → valid strace line → roundtrip parse

### Files Changed

| File | Change |
|------|--------|
| `shatter-core/src/strace_deps.rs` | New — types, runner, parser |
| `shatter-core/src/lib.rs` | Add module |
| `shatter-cli/src/main.rs` | Add subcommand + handler |
| `demo/walkthrough.sh` | Add Linux-guarded discover-deps step |

### Verification
```bash
cargo test -p shatter-core && cargo clippy -- -D warnings
# On Linux: cargo run -- discover-deps --strace -- curl -s http://example.com
```

---

## Execution Order

1. **Task #10** (Go mock parity) — branch `str-3ky9.16`
2. **Task #9** (refactoring recs) — branch `str-3ky9.15`
3. **Task #11** (strace discovery) — branch `str-3ky9.17`

Each: `bd start <key>` → implement → test → commit → message team-lead.
