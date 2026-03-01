//! Instrumentation runtime for Shatter's Rust frontend.
//!
//! This crate is added as a dependency of instrumented Rust code during compilation.
//! It provides functions that instrumented code calls to record branch decisions,
//! mock external dependencies, and capture execution traces.
//!
//! All state is thread-local so concurrent tests don't interfere.

use std::cell::RefCell;
use std::collections::HashMap;

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
    /// Performance metrics.
    pub performance: PerformanceMetrics,
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
    external_calls: Vec<ExternalCall>,
    mock_registry: HashMap<String, MockEntry>,
}

impl RecordingState {
    fn new() -> Self {
        Self {
            branch_path: Vec::new(),
            external_calls: Vec::new(),
            mock_registry: HashMap::new(),
        }
    }

    fn clear(&mut self) {
        self.branch_path.clear();
        self.external_calls.clear();
        self.mock_registry.clear();
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
            lines_executed: Vec::new(),
            calls_to_external: state.external_calls.clone(),
            path_constraints,
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
}
