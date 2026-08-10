//! Instrumentation runtime for Shatter's Rust frontend.
//!
//! This crate is added as a dependency of instrumented Rust code during compilation.
//! It provides functions that instrumented code calls to record branch decisions,
//! mock external dependencies, and capture execution traces.
//!
//! All state is thread-local so concurrent tests don't interfere.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---------------------------------------------------------------------------
// Wire-compatible types (match shatter-core protocol)
// ---------------------------------------------------------------------------

/// A symbolic constraint captured at a branch point.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SymConstraint {
    /// A fully symbolic expression that can be sent to Z3.
    Expr { expr: Value },
    /// Could not be tracked symbolically; the hint describes the original source.
    Unknown { hint: String },
}

/// A single branch decision recorded during execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BranchDecision {
    /// Unique identifier for this branch point within the function.
    pub branch_id: u32,
    /// Source line number of the branch.
    pub line: u32,
    /// Whether the true branch was taken.
    pub taken: bool,
    /// The symbolic constraint governing this branch.
    pub constraint: SymConstraint,
}

/// A call to an external (mocked or observed) dependency during execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExternalCall {
    /// Fully qualified name of the called symbol.
    pub symbol: String,
    /// Arguments passed to the call.
    pub args: Vec<Value>,
    /// Return value from the call.
    pub return_value: Value,
}

/// Performance metrics from a single execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    /// Wall clock time in milliseconds.
    pub wall_time_ms: f64,
    /// CPU time in microseconds.
    pub cpu_time_us: u64,
    /// Heap memory used in bytes.
    pub heap_used_bytes: u64,
    /// Heap memory allocated in bytes.
    pub heap_allocated_bytes: u64,
}

impl Default for PerformanceMetrics {
    fn default() -> Self {
        Self {
            wall_time_ms: 0.0,
            cpu_time_us: 0,
            heap_used_bytes: 0,
            heap_allocated_bytes: 0,
        }
    }
}

/// Information about an error thrown during execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrorInfo {
    /// The error type or class name.
    pub error_type: String,
    /// The error message.
    pub message: String,
    /// Optional stack trace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stack: Option<String>,
}

/// Result of executing an instrumented function.
/// Wire-compatible with `shatter-core`'s `ExecuteResult`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecuteResult {
    /// Return value from the function, if it returned normally.
    pub return_value: Option<Value>,
    /// Error thrown during execution, if any.
    pub thrown_error: Option<ErrorInfo>,
    /// Branch decisions recorded during execution.
    #[serde(default)]
    pub branch_path: Vec<BranchDecision>,
    /// Source lines executed.
    #[serde(default)]
    pub lines_executed: Vec<u32>,
    /// Calls to external dependencies observed.
    #[serde(default)]
    pub calls_to_external: Vec<ExternalCall>,
    /// Symbolic path constraints collected.
    #[serde(default)]
    pub path_constraints: Vec<SymConstraint>,
    /// Per-iteration loop body snapshots.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub loop_body_states: Vec<LoopBodyState>,
    /// Performance metrics.
    pub performance: PerformanceMetrics,
}

/// Per-iteration snapshot for one loop body.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoopBodyState {
    pub loop_id: u32,
    pub iteration: u32,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub locals: BTreeMap<String, Value>,
}

// ---------------------------------------------------------------------------
// Thread-local recording state
// ---------------------------------------------------------------------------

/// Entry in the mock registry for a single symbol.
struct MockEntry {
    return_values: Vec<Value>,
    call_index: usize,
}

/// All state recorded during a single execution.
struct RecordingState {
    branch_path: Vec<BranchDecision>,
    lines_executed: Vec<u32>,
    external_calls: Vec<ExternalCall>,
    mock_registry: HashMap<String, MockEntry>,
    loop_iterations: HashMap<u32, u32>,
    loop_body_states: Vec<LoopBodyState>,
}

impl RecordingState {
    fn new() -> Self {
        Self {
            branch_path: Vec::new(),
            lines_executed: Vec::new(),
            external_calls: Vec::new(),
            mock_registry: HashMap::new(),
            loop_iterations: HashMap::new(),
            loop_body_states: Vec::new(),
        }
    }

    fn clear(&mut self) {
        self.branch_path.clear();
        self.lines_executed.clear();
        self.external_calls.clear();
        self.mock_registry.clear();
        self.loop_iterations.clear();
        self.loop_body_states.clear();
    }
}

thread_local! {
    static STATE: RefCell<RecordingState> = RefCell::new(RecordingState::new());
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Record a branch decision during instrumented execution.
///
/// `constraint_json` should be a JSON-encoded symbolic constraint expression.
/// If it cannot be parsed, it is recorded as an `Unknown` constraint with the
/// raw string as a hint.
pub fn branch_hit(id: u32, line: u32, taken: bool, constraint_json: &str) {
    let constraint = serde_json::from_str::<Value>(constraint_json)
        .map(|expr| SymConstraint::Expr { expr })
        .unwrap_or_else(|_| SymConstraint::Unknown {
            hint: constraint_json.to_string(),
        });

    STATE.with(|state| {
        state.borrow_mut().branch_path.push(BranchDecision {
            branch_id: id,
            line,
            taken,
            constraint,
        });
    });
}

/// Record that a source line was executed.
pub fn line_hit(line: u32) {
    if line == 0 {
        return;
    }

    STATE.with(|state| {
        let mut state = state.borrow_mut();
        if !state.lines_executed.contains(&line) {
            state.lines_executed.push(line);
        }
    });
}

/// Record entry into a loop body and allocate a zero-based iteration index.
pub fn loop_enter(loop_id: u32) {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let iteration = *state.loop_iterations.get(&loop_id).unwrap_or(&0);
        state.loop_iterations.insert(loop_id, iteration + 1);
        state.loop_body_states.push(LoopBodyState {
            loop_id,
            iteration,
            locals: BTreeMap::new(),
        });
    });
}

/// Check the mock registry for `symbol`. If a mock is registered, return the
/// next mock value. Returns `None` if no mock is registered (caller should
/// fall through to the real implementation).
pub fn mock_call(symbol: &str, args_json: &str) -> Option<Value> {
    let args: Vec<Value> = serde_json::from_str(args_json).unwrap_or_default();

    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let entry = state.mock_registry.get_mut(symbol)?;

        if entry.return_values.is_empty() {
            return None;
        }

        let idx = entry.call_index.min(entry.return_values.len() - 1);
        let return_value = entry.return_values[idx].clone();
        entry.call_index += 1;

        // Also record the external call
        state.external_calls.push(ExternalCall {
            symbol: symbol.to_string(),
            args,
            return_value: return_value.clone(),
        });

        Some(return_value)
    })
}

/// Record a call to an external dependency (when not mocked).
pub fn record_external_call(symbol: &str, args_json: &str, return_value_json: &str) {
    let args: Vec<Value> = serde_json::from_str(args_json).unwrap_or_default();
    let return_value: Value =
        serde_json::from_str(return_value_json).unwrap_or(Value::Null);

    STATE.with(|state| {
        state.borrow_mut().external_calls.push(ExternalCall {
            symbol: symbol.to_string(),
            args,
            return_value,
        });
    });
}

/// Register mock return values for a symbol. When `mock_call` is invoked for
/// this symbol, values are returned in order. Once exhausted, the last value
/// is repeated.
pub fn register_mock(symbol: &str, return_values: Vec<Value>) {
    STATE.with(|state| {
        state.borrow_mut().mock_registry.insert(
            symbol.to_string(),
            MockEntry {
                return_values,
                call_index: 0,
            },
        );
    });
}

/// Clear all recorded state. Call before each execution to start fresh.
pub fn reset() {
    STATE.with(|state| {
        state.borrow_mut().clear();
    });
}

fn lines_executed_from_branch_path(branch_path: &[BranchDecision]) -> Vec<u32> {
    let mut lines = Vec::new();
    for decision in branch_path {
        if decision.line > 0 && !lines.contains(&decision.line) {
            lines.push(decision.line);
        }
    }
    lines
}

fn merge_lines_executed(recorded_lines: &[u32], branch_path: &[BranchDecision]) -> Vec<u32> {
    let mut lines = recorded_lines.to_vec();
    for line in lines_executed_from_branch_path(branch_path) {
        if !lines.contains(&line) {
            lines.push(line);
        }
    }
    lines
}

/// Serialize all recorded data into an `ExecuteResult` JSON string.
///
/// The caller is responsible for filling in `return_value`, `thrown_error`,
/// and `performance` — this function captures the branch path, external calls,
/// and path constraints from the recording state.
///
/// Returns the JSON string. On serialization failure, returns an error JSON.
pub fn flush_results() -> String {
    STATE.with(|state| {
        let state = state.borrow();

        // Extract path constraints from branch decisions
        let path_constraints: Vec<SymConstraint> = state
            .branch_path
            .iter()
            .map(|bd| bd.constraint.clone())
            .collect();

        let result = ExecuteResult {
            return_value: None,
            thrown_error: None,
            branch_path: state.branch_path.clone(),
            lines_executed: merge_lines_executed(&state.lines_executed, &state.branch_path),
            calls_to_external: state.external_calls.clone(),
            path_constraints,
            loop_body_states: state.loop_body_states.clone(),
            performance: PerformanceMetrics::default(),
        };

        serde_json::to_string(&result).unwrap_or_else(|e| {
            format!(
                r#"{{"return_value":null,"thrown_error":{{"error_type":"SerializationError","message":"{}"}},"branch_path":[],"lines_executed":[],"calls_to_external":[],"path_constraints":[],"performance":{{"wall_time_ms":0.0,"cpu_time_us":0,"heap_used_bytes":0,"heap_allocated_bytes":0}}}}"#,
                e
            )
        })
    })
}

// ---------------------------------------------------------------------------
// Harness helpers — reduce per-function boilerplate
// ---------------------------------------------------------------------------

/// Parse a JSON array of mock definitions and register each one.
///
/// Each element must be an object with `"symbol"` (string) and
/// `"return_values"` (array). Malformed entries are silently skipped.
pub fn register_mocks_from_json(mocks_json: &str) {
    let mocks: Vec<Value> = serde_json::from_str(mocks_json).unwrap_or_default();
    for mock in &mocks {
        if let (Some(symbol), Some(return_values)) = (
            mock.get("symbol").and_then(|s| s.as_str()),
            mock.get("return_values").and_then(|v| v.as_array()),
        ) {
            register_mock(symbol, return_values.clone());
        }
    }
}

/// Execute a closure with `catch_unwind` and wall-time measurement.
///
/// Returns `(Ok(return_value), wall_time_ms)` on success, or
/// `(Err(panic_message), wall_time_ms)` if the closure panics.
pub fn execute_with_timing<F, R>(f: F) -> (Result<R, String>, f64)
where
    F: FnOnce() -> R + std::panic::UnwindSafe,
{
    let start = std::time::Instant::now();
    let result = std::panic::catch_unwind(f);
    let wall_time_ms = start.elapsed().as_secs_f64() * 1000.0;

    let mapped = result.map_err(|panic_info| {
        if let Some(s) = panic_info.downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = panic_info.downcast_ref::<String>() {
            s.clone()
        } else {
            format!("{:?}", panic_info)
        }
    });

    (mapped, wall_time_ms)
}

/// Build a complete result JSON object by merging `flush_results()` output
/// with caller-provided return value, error, performance, and side effects.
pub fn build_result_json(
    return_value: Option<Value>,
    thrown_error: Option<Value>,
    wall_time_ms: f64,
    side_effects: Vec<Value>,
) -> Value {
    let runtime_json = flush_results();
    let mut obj: Value =
        serde_json::from_str(&runtime_json).unwrap_or(Value::Object(Default::default()));

    if let Some(map) = obj.as_object_mut() {
        if let Some(rv) = return_value {
            map.insert("return_value".into(), rv);
        }
        if let Some(te) = thrown_error {
            map.insert("thrown_error".into(), te);
        }
        map.insert(
            "performance".into(),
            serde_json::json!({
                "wall_time_ms": wall_time_ms,
                "cpu_time_us": 0,
                "heap_used_bytes": 0,
                "heap_allocated_bytes": 0,
            }),
        );
        let existing = map
            .entry("side_effects")
            .or_insert(serde_json::json!([]));
        if let Some(arr) = existing.as_array_mut() {
            arr.extend(side_effects);
        }
    }

    obj
}

/// Run a persistent stdin-loop harness.
///
/// Handles mock registration, stdin reading, JSON parsing, `reset()` per
/// iteration, calling the handler, and writing the response to stdout.
///
/// The `handler` receives the `"inputs"` array from each request and must
/// return a complete result JSON `Value` (typically built via
/// [`build_result_json`]).
/// Convert a single `{"__complex_type": K, ...}` envelope (produced by the
/// input generator for a complex-typed value the Rust frontend declared support
/// for) into the serde-native JSON the target Rust type expects to deserialize
/// from. Returns `None` for tags this runtime does not materialize, leaving the
/// envelope untouched (so an unexpected envelope surfaces as a clear deser
/// error rather than silently wrong data). See str-8euf.
fn complex_envelope_to_native(tag: &str, map: &serde_json::Map<String, Value>) -> Option<Value> {
    match tag {
        // `uuid`/`url` carry their canonical string under `value`; the target
        // types (`uuid::Uuid`, `url::Url`) deserialize from that string.
        "uuid" | "url" => map.get("value").cloned(),
        // `date`/`date_time` carry an epoch-ms integer under `value`; chrono's
        // `NaiveDate` deserializes from "%Y-%m-%d" and `DateTime<Utc>` from
        // RFC3339, so emit the matching ISO string (str-8euf).
        "date" => map
            .get("value")
            .and_then(Value::as_i64)
            .map(|ms| Value::String(iso_date_from_epoch_ms(ms))),
        "date_time" => map
            .get("value")
            .and_then(Value::as_i64)
            .map(|ms| Value::String(iso_datetime_from_epoch_ms(ms))),
        _ => None,
    }
}

/// Floor division (rounds toward negative infinity) for epoch math on dates
/// before 1970, where `/` would round toward zero.
fn floor_div(a: i64, b: i64) -> i64 {
    let q = a / b;
    if (a % b != 0) && ((a < 0) != (b < 0)) {
        q - 1
    } else {
        q
    }
}

/// Convert days since the Unix epoch to a `(year, month, day)` civil date using
/// Howard Hinnant's algorithm. Valid for the full proleptic Gregorian range.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32; // [1, 12]
    (y + i64::from(m <= 2), m, d)
}

fn iso_date_from_epoch_ms(epoch_ms: i64) -> String {
    let days = floor_div(epoch_ms, 86_400_000);
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

fn iso_datetime_from_epoch_ms(epoch_ms: i64) -> String {
    let days = floor_div(epoch_ms, 86_400_000);
    let ms_of_day = epoch_ms - days * 86_400_000; // [0, 86_400_000)
    let secs = ms_of_day / 1000;
    let (hh, mm, ss) = (secs / 3600, (secs % 3600) / 60, secs % 60);
    let (y, mo, d) = civil_from_days(days);
    format!("{y:04}-{mo:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z")
}

/// Recursively rewrite `__complex_type` envelopes anywhere in `value` into their
/// serde-native form (see [`complex_envelope_to_native`]). Applied to every
/// generated input before it is deserialized into the target parameter types,
/// so complex-typed struct fields (e.g. a `uuid::Uuid` field of a synthesized
/// struct) deserialize correctly instead of receiving a raw envelope object.
pub fn materialize_complex(value: &mut Value) {
    match value {
        Value::Object(map) => {
            let tag = map
                .get("__complex_type")
                .and_then(|t| t.as_str())
                .map(str::to_owned);
            if let Some(tag) = tag {
                if let Some(native) = complex_envelope_to_native(&tag, map) {
                    *value = native;
                    return;
                }
            }
            for child in map.values_mut() {
                materialize_complex(child);
            }
        }
        Value::Array(items) => {
            for item in items.iter_mut() {
                materialize_complex(item);
            }
        }
        _ => {}
    }
}

pub fn run_harness_loop<F>(mocks_json: &str, mut handler: F)
where
    F: FnMut(&[Value]) -> Value,
{
    register_mocks_from_json(mocks_json);

    let stdin = std::io::stdin();
    let mut reader = std::io::BufReader::new(stdin.lock());

    loop {
        let mut line = String::new();
        match std::io::BufRead::read_line(&mut reader, &mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let req: Value = serde_json::from_str(line).unwrap_or_default();
        let mut inputs = req["inputs"].as_array().cloned().unwrap_or_default();
        for input in inputs.iter_mut() {
            materialize_complex(input);
        }

        reset();

        let result = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            handler(&inputs)
        })) {
            Ok(result) => result,
            Err(panic_info) => build_result_json(
                None,
                Some(serde_json::json!({
                    "error_type": "runtime_error",
                    "message": panic_message(&panic_info),
                    "stack": null,
                })),
                0.0,
                vec![],
            ),
        };

        let output = serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string());
        println!("{output}");
        let _ = std::io::Write::flush(&mut std::io::stdout());
    }
}

/// Run a persistent stdin-loop harness with multi-function dispatch.
///
/// Like [`run_harness_loop`], but the handler also receives the `"function"`
/// name from each request, enabling a single harness binary to serve
/// multiple functions via match dispatch.
pub fn run_dispatch_loop<F>(mocks_json: &str, mut handler: F)
where
    F: FnMut(&str, &[Value]) -> Value,
{
    register_mocks_from_json(mocks_json);

    let stdin = std::io::stdin();
    let mut reader = std::io::BufReader::new(stdin.lock());

    loop {
        let mut line = String::new();
        match std::io::BufRead::read_line(&mut reader, &mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let req: Value = serde_json::from_str(line).unwrap_or_default();
        let function_name = req["function"].as_str().unwrap_or("");
        let mut inputs = req["inputs"].as_array().cloned().unwrap_or_default();
        for input in inputs.iter_mut() {
            materialize_complex(input);
        }

        reset();

        let result = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            handler(function_name, &inputs)
        })) {
            Ok(result) => result,
            Err(panic_info) => build_result_json(
                None,
                Some(serde_json::json!({
                    "error_type": "runtime_error",
                    "message": panic_message(&panic_info),
                    "stack": null,
                })),
                0.0,
                vec![],
            ),
        };

        let output = serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string());
        println!("{output}");
        let _ = std::io::Write::flush(&mut std::io::stdout());
    }
}

fn panic_message(panic_info: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = panic_info.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = panic_info.downcast_ref::<String>() {
        s.clone()
    } else {
        format!("{panic_info:?}")
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: reset state before each test.
    fn setup() {
        reset();
    }

    #[test]
    fn branch_hit_records_entries() {
        setup();

        branch_hit(1, 10, true, r#"{"kind":"unknown","hint":"x > 0"}"#);
        branch_hit(2, 15, false, r#"{"kind":"unknown","hint":"y == 1"}"#);

        STATE.with(|state| {
            let state = state.borrow();
            assert_eq!(state.branch_path.len(), 2);
            assert_eq!(state.branch_path[0].branch_id, 1);
            assert!(state.branch_path[0].taken);
            assert_eq!(state.branch_path[1].branch_id, 2);
            assert!(!state.branch_path[1].taken);
        });
    }

    #[test]
    fn loop_enter_records_zero_based_iterations() {
        setup();

        loop_enter(4);
        loop_enter(4);
        loop_enter(7);

        let json = flush_results();
        let result: ExecuteResult = serde_json::from_str(&json).expect("valid result");
        assert_eq!(result.loop_body_states.len(), 3);
        assert_eq!(result.loop_body_states[0].loop_id, 4);
        assert_eq!(result.loop_body_states[0].iteration, 0);
        assert!(result.loop_body_states[0].locals.is_empty());
        assert_eq!(result.loop_body_states[1].loop_id, 4);
        assert_eq!(result.loop_body_states[1].iteration, 1);
        assert_eq!(result.loop_body_states[2].loop_id, 7);
        assert_eq!(result.loop_body_states[2].iteration, 0);
    }

    #[test]
    fn branch_hit_with_invalid_json_records_unknown() {
        setup();

        branch_hit(1, 10, true, "not valid json");

        STATE.with(|state| {
            let state = state.borrow();
            assert_eq!(state.branch_path.len(), 1);
            match &state.branch_path[0].constraint {
                SymConstraint::Unknown { hint } => {
                    assert_eq!(hint, "not valid json");
                }
                other => panic!("expected Unknown, got {:?}", other),
            }
        });
    }

    #[test]
    fn branch_hit_with_valid_expr_json() {
        setup();

        let expr_json = r#"{"op":"gt","left":{"param":"x"},"right":{"const":10}}"#;
        branch_hit(1, 10, true, expr_json);

        STATE.with(|state| {
            let state = state.borrow();
            match &state.branch_path[0].constraint {
                SymConstraint::Expr { expr } => {
                    assert_eq!(expr["op"], "gt");
                }
                other => panic!("expected Expr, got {:?}", other),
            }
        });
    }

    #[test]
    fn mock_call_returns_registered_values() {
        setup();

        register_mock(
            "db::query",
            vec![
                serde_json::json!({"id": 1}),
                serde_json::json!({"id": 2}),
            ],
        );

        let v1 = mock_call("db::query", "[]");
        assert_eq!(v1, Some(serde_json::json!({"id": 1})));

        let v2 = mock_call("db::query", "[]");
        assert_eq!(v2, Some(serde_json::json!({"id": 2})));

        // After exhaustion, repeats last value
        let v3 = mock_call("db::query", "[]");
        assert_eq!(v3, Some(serde_json::json!({"id": 2})));
    }

    #[test]
    fn mock_call_returns_none_for_unregistered() {
        setup();

        let result = mock_call("unknown::func", "[]");
        assert_eq!(result, None);
    }

    #[test]
    fn mock_call_records_external_call() {
        setup();

        register_mock("fs::read", vec![serde_json::json!("file contents")]);
        mock_call("fs::read", r#"["path.txt"]"#);

        STATE.with(|state| {
            let state = state.borrow();
            assert_eq!(state.external_calls.len(), 1);
            assert_eq!(state.external_calls[0].symbol, "fs::read");
            assert_eq!(state.external_calls[0].args, vec![serde_json::json!("path.txt")]);
        });
    }

    #[test]
    fn record_external_call_captures_data() {
        setup();

        record_external_call("http::get", r#"["https://example.com"]"#, r#"{"status": 200}"#);

        STATE.with(|state| {
            let state = state.borrow();
            assert_eq!(state.external_calls.len(), 1);
            assert_eq!(state.external_calls[0].symbol, "http::get");
            assert_eq!(
                state.external_calls[0].return_value,
                serde_json::json!({"status": 200})
            );
        });
    }

    #[test]
    fn record_external_call_with_invalid_json() {
        setup();

        record_external_call("func", "not json", "also not json");

        STATE.with(|state| {
            let state = state.borrow();
            assert_eq!(state.external_calls.len(), 1);
            assert!(state.external_calls[0].args.is_empty());
            assert_eq!(state.external_calls[0].return_value, Value::Null);
        });
    }

    #[test]
    fn reset_clears_all_state() {
        setup();

        branch_hit(1, 10, true, r#""x""#);
        record_external_call("f", "[]", "null");
        register_mock("g", vec![serde_json::json!(1)]);

        reset();

        STATE.with(|state| {
            let state = state.borrow();
            assert!(state.branch_path.is_empty());
            assert!(state.external_calls.is_empty());
            assert!(state.mock_registry.is_empty());
        });
    }

    #[test]
    fn flush_results_produces_valid_json() {
        setup();

        branch_hit(1, 10, true, r#"{"op":"eq"}"#);
        record_external_call("f", r#"[1, 2]"#, r#""ok""#);

        let json = flush_results();
        let result: ExecuteResult =
            serde_json::from_str(&json).expect("flush_results should produce valid JSON");

        assert_eq!(result.branch_path.len(), 1);
        assert_eq!(result.branch_path[0].branch_id, 1);
        assert_eq!(result.calls_to_external.len(), 1);
        assert_eq!(result.calls_to_external[0].symbol, "f");
        assert_eq!(result.path_constraints.len(), 1);
        assert!(result.return_value.is_none());
        assert!(result.thrown_error.is_none());
        assert_eq!(result.performance.wall_time_ms, 0.0);
    }

    #[test]
    fn flush_results_reports_branch_lines_as_executed() {
        setup();

        branch_hit(1, 10, true, r#"{"op":"eq"}"#);
        branch_hit(2, 10, false, r#"{"op":"ne"}"#);
        branch_hit(3, 15, true, r#"{"op":"gt"}"#);

        let json = flush_results();
        let result: ExecuteResult =
            serde_json::from_str(&json).expect("flush_results should produce valid JSON");

        assert_eq!(result.lines_executed, vec![10, 15]);
    }

    #[test]
    fn flush_results_reports_line_hits_as_executed() {
        setup();

        line_hit(7);
        line_hit(7);
        branch_hit(1, 9, true, r#"{"op":"eq"}"#);

        let json = flush_results();
        let result: ExecuteResult =
            serde_json::from_str(&json).expect("flush_results should produce valid JSON");

        assert_eq!(result.lines_executed, vec![7, 9]);
    }

    #[test]
    fn flush_results_round_trips_as_execute_result() {
        setup();

        branch_hit(1, 5, true, r#"{"hint":"a > 0"}"#);
        branch_hit(2, 8, false, r#"{"hint":"b < 10"}"#);
        register_mock("dep", vec![serde_json::json!(42)]);
        mock_call("dep", r#"["arg"]"#);

        let json = flush_results();

        // Should round-trip through serde
        let parsed: Value = serde_json::from_str(&json).expect("valid JSON");
        let branch_path = parsed["branch_path"].as_array().expect("branch_path array");
        assert_eq!(branch_path.len(), 2);

        let calls = parsed["calls_to_external"]
            .as_array()
            .expect("calls_to_external array");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0]["symbol"], "dep");
    }

    #[test]
    fn register_mocks_from_json_registers_all() {
        setup();

        let mocks = r#"[
            {"symbol": "db::query", "return_values": [1, 2]},
            {"symbol": "fs::read", "return_values": ["hello"]}
        ]"#;
        register_mocks_from_json(mocks);

        assert_eq!(mock_call("db::query", "[]"), Some(serde_json::json!(1)));
        assert_eq!(
            mock_call("fs::read", "[]"),
            Some(serde_json::json!("hello"))
        );
    }

    #[test]
    fn register_mocks_from_json_skips_malformed() {
        setup();

        let mocks = r#"[{"bad": true}, {"symbol": "ok", "return_values": [42]}]"#;
        register_mocks_from_json(mocks);

        assert_eq!(mock_call("ok", "[]"), Some(serde_json::json!(42)));
    }

    #[test]
    fn register_mocks_from_json_handles_invalid_json() {
        setup();

        register_mocks_from_json("not json at all");
        // Should not panic, just register nothing
        assert_eq!(mock_call("anything", "[]"), None);
    }

    #[test]
    fn execute_with_timing_captures_success() {
        let (result, wall_ms) = execute_with_timing(|| 42);
        assert_eq!(result, Ok(42));
        assert!(wall_ms >= 0.0);
    }

    #[test]
    fn execute_with_timing_captures_panic() {
        let (result, wall_ms) = execute_with_timing(|| -> i32 { panic!("boom") });
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("boom"));
        assert!(wall_ms >= 0.0);
    }

    #[test]
    fn build_result_json_merges_all_fields() {
        setup();

        branch_hit(1, 10, true, r#""x""#);

        let result = build_result_json(
            Some(serde_json::json!(99)),
            None,
            1.5,
            vec![serde_json::json!({"kind": "console_output", "level": "log", "message": "hi"})],
        );

        let obj = result.as_object().expect("should be object");
        assert_eq!(obj["return_value"], serde_json::json!(99));
        assert!(obj.get("thrown_error").is_none() || obj["thrown_error"].is_null());
        assert_eq!(obj["performance"]["wall_time_ms"], serde_json::json!(1.5));
        let se = obj["side_effects"].as_array().expect("side_effects array");
        assert_eq!(se.len(), 1);
        assert_eq!(se[0]["kind"], "console_output");
        let bp = obj["branch_path"].as_array().expect("branch_path array");
        assert_eq!(bp.len(), 1);
    }

    #[test]
    fn build_result_json_with_error() {
        setup();

        let result = build_result_json(
            None,
            Some(serde_json::json!({"error_type": "runtime_error", "message": "oops"})),
            0.5,
            vec![],
        );

        let obj = result.as_object().expect("should be object");
        assert_eq!(obj["thrown_error"]["error_type"], "runtime_error");
        assert_eq!(obj["thrown_error"]["message"], "oops");
    }

    #[test]
    fn thread_local_isolation() {
        setup();

        // Record in main thread
        branch_hit(1, 10, true, r#""main""#);

        // Spawn a thread and record different data
        let handle = std::thread::spawn(|| {
            reset(); // ensure clean state in new thread
            branch_hit(99, 50, false, r#""other""#);
            STATE.with(|state| {
                let state = state.borrow();
                assert_eq!(state.branch_path.len(), 1);
                assert_eq!(state.branch_path[0].branch_id, 99);
            });
        });

        handle.join().expect("thread should not panic");

        // Main thread state should be unaffected
        STATE.with(|state| {
            let state = state.borrow();
            assert_eq!(state.branch_path.len(), 1);
            assert_eq!(state.branch_path[0].branch_id, 1);
        });
    }

    #[test]
    fn materialize_complex_unwraps_uuid_and_url_envelopes() {
        // A struct-shaped input with a nested uuid envelope field becomes a
        // plain string the target `uuid::Uuid` field can deserialize.
        let mut v = serde_json::json!({
            "id": {"__complex_type": "uuid", "value": "550e8400-e29b-41d4-a716-446655440000"},
            "site": {"__complex_type": "url", "value": "https://example.test/x"},
            "weight": 42,
            "nested": [{"__complex_type": "uuid", "value": "00000000-0000-0000-0000-000000000001"}]
        });
        materialize_complex(&mut v);
        assert_eq!(v["id"], serde_json::json!("550e8400-e29b-41d4-a716-446655440000"));
        assert_eq!(v["site"], serde_json::json!("https://example.test/x"));
        assert_eq!(v["weight"], serde_json::json!(42));
        assert_eq!(
            v["nested"][0],
            serde_json::json!("00000000-0000-0000-0000-000000000001")
        );
    }

    #[test]
    fn materialize_complex_converts_date_envelopes_to_iso() {
        // epoch 1704067200000 ms == 2024-01-01T00:00:00Z.
        let mut date = serde_json::json!({"__complex_type": "date", "value": 1_704_067_200_000_i64});
        materialize_complex(&mut date);
        assert_eq!(date, serde_json::json!("2024-01-01"));

        let mut dt = serde_json::json!({"__complex_type": "date_time", "value": 1_704_067_200_000_i64});
        materialize_complex(&mut dt);
        assert_eq!(dt, serde_json::json!("2024-01-01T00:00:00Z"));

        // epoch 0 and a pre-1970 value (floor division correctness).
        assert_eq!(iso_date_from_epoch_ms(0), "1970-01-01");
        assert_eq!(iso_datetime_from_epoch_ms(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso_date_from_epoch_ms(-86_400_000), "1969-12-31");
        // 2038-01-19T03:14:07Z (Y2K38).
        assert_eq!(iso_datetime_from_epoch_ms(2_147_483_647_000), "2038-01-19T03:14:07Z");
    }

    #[test]
    fn materialize_complex_leaves_unknown_envelopes_untouched() {
        // A tag this runtime does not materialize is left as-is (surfaces as a
        // clear deser error rather than silently wrong data).
        let mut v = serde_json::json!({"__complex_type": "big_int", "value": "12345"});
        let original = v.clone();
        materialize_complex(&mut v);
        assert_eq!(v, original);
    }
}
