# Plan: Recording mode (`--record`) — str-3ky9.11

## Context

Shatter's external dependency mocking pipeline already captures `calls_to_external` during execution, but this data is never persisted. The `--record` flag adds a CLI mode where exploration runs with real external services (passthrough mocks), records all I/O, and saves the data as YAML fixtures. These fixtures seed `MockValueSpace::Seeded` for future autonomous runs — "run once against a real stack, then test autonomously forever."

## Approach

### 1. New module: `shatter-core/src/recorded_mocks.rs`

**Types:**
```rust
pub struct DepObservation {
    pub args: Vec<Value>,
    pub return_value: Value,
    pub error: Option<String>,       // error message if call failed
    pub latency_ms: f64,
}

pub struct ExternalDepBehavior {
    pub symbol: String,
    pub source_module: String,
    pub observations: Vec<DepObservation>,
}

pub struct RecordedMockFile {
    pub function_id: String,
    pub file: String,
    pub recorded_at: String,         // ISO8601
    pub dependencies: Vec<ExternalDepBehavior>,
}

pub enum RecordError {
    Io(std::io::Error),
    Yaml(serde_yaml::Error),
}
```

Constants: `RECORDED_MOCKS_DIR = "recorded-mocks"`.

**Functions:**

| Function | Purpose |
|----------|---------|
| `build_passthrough_mocks(deps: &[ExternalDependency]) -> Vec<MockConfig>` | All deps get Passthrough + should_track_calls=true |
| `aggregate_recordings(raw_results, deps) -> Vec<ExternalDepBehavior>` | Group calls_to_external by symbol |
| `save_recorded_mocks(mock_file, shatter_dir) -> Result<(), RecordError>` | Write to `.shatter/recorded-mocks/<file>/<func>.yaml` |
| `load_recorded_mocks(path) -> Result<RecordedMockFile, RecordError>` | Read back for seeded mode |
| `recorded_mocks_to_mock_configs(mock_file) -> Vec<MockConfig>` | Convert to MockConfig for future runs |

### 2. CLI flag: `shatter-cli/src/main.rs`

Add `--record` to the `Explore` command (around line 301):
```rust
/// Record external dependency calls (passthrough mode). Saves observed
/// I/O to .shatter/recorded-mocks/ for seeding future runs.
#[arg(long)]
record: bool,
```

Thread through: Explore match arm → `run_explore()` parameter.

### 3. Wiring in `run_explore()`

**Before exploration** (mock generation section, ~line 1311):
- If `record`: call `build_passthrough_mocks(deps)` instead of `generate_auto_mocks()`
- Set `mock_params = vec![]` (no dynamic generation in record mode)

**After exploration** (after behavior map creation, ~line 1508):
- If `record`: call `aggregate_recordings()` on `result.raw_results`
- Call `save_recorded_mocks()` to persist
- Log recorded dep count

**Both paths**: The random explorer and concolic orchestrator both produce `raw_results` with the same `(Vec<Value>, Vec<MockConfig>, ExecuteResult)` triple. The `From<ExploreResult> for ObservationOutput` impl preserves `raw_results` (pipeline.rs:138). Recording works identically on both paths.

### 4. Register module

Add `pub mod recorded_mocks;` to `shatter-core/src/lib.rs`.

## Files to modify

| File | Change |
|------|--------|
| `shatter-core/src/recorded_mocks.rs` | **NEW** — types, aggregation, save/load, conversion, tests |
| `shatter-core/src/lib.rs` | Add `pub mod recorded_mocks;` |
| `shatter-cli/src/main.rs` | Add `--record` flag, wire into `run_explore()` for both explorer paths |

## Key dependencies (read-only)

- `execution_record.rs` — `ExternalCall` type
- `protocol.rs` — `MockConfig`, `MockBehavior`, `ExternalDependency`, `ExecuteResult`
- `mock_value_space.rs` — `LiveCallOutcome` (not used in DepObservation to keep it simpler — error field suffices)
- `explorer.rs` — `ObservationOutput.raw_results`
- `auto_mock.rs` — pattern for build_passthrough_mocks

## Risks

1. **TS passthrough + track_calls**: The TS executor skips mock registry for `passthrough` behavior (executor.ts:644). The `__shatter_mock_call` callback only fires for mocked calls. In passthrough mode, calls go to real code but may not be recorded. **Mitigation**: Check if `should_track_calls` is respected for passthrough — if not, this is a known limitation documented in the output. The TS executor change would be str-3ky9.16 scope.

2. **Go frontend**: Same concern. Defer to str-3ky9.16 (Go frontend mock parity).

## Verification

1. `cargo test -p shatter-core` — unit + proptest for new module
2. `cargo clippy -p shatter-core -p shatter-cli -- -D warnings` — no warnings
3. Manual: `cargo run -- explore examples/typescript/src/12-external-deps.ts --record` — verify `.shatter/recorded-mocks/` created with YAML content
4. CLI parse test: add test that `--record` flag parses correctly

## Test plan

**Unit tests in recorded_mocks.rs:**
- Serde roundtrips for all types (DepObservation, ExternalDepBehavior, RecordedMockFile)
- `build_passthrough_mocks` — all deps get Passthrough + track_calls
- `aggregate_recordings` — groups by symbol, empty input → empty output, preserves data
- `save/load roundtrip` — write to tempdir, read back, verify equality
- `recorded_mocks_to_mock_configs` — correct conversion

**Proptest:**
- Arbitrary DepObservation/ExternalDepBehavior serde roundtrips
- Aggregate preserves total observation count

**CLI test:**
- `cli_parses_explore_record` — verify flag parsed correctly
