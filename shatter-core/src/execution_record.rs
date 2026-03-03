//! Execution record types capturing the results of a single function execution.

use serde::{Deserialize, Serialize};

use crate::sym_expr::SymExpr;

/// A symbolic constraint captured at a branch point.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SymConstraint {
    /// A fully symbolic expression that can be sent to Z3.
    Expr { expr: SymExpr },
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

/// Information about an error thrown during execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrorInfo {
    /// The error type or class name.
    pub error_type: String,
    /// The error message.
    pub message: String,
    /// Optional stack trace.
    pub stack: Option<String>,
    /// Structured error category: "validation", "runtime", "infrastructure", or "unknown".
    /// Classified by the frontend using language-level signals.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_category: Option<String>,
}

/// A side effect observed during execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SideEffect {
    ConsoleOutput { level: String, message: String },
    FileWrite {
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        content: Option<String>,
    },
    NetworkRequest {
        method: String,
        url: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        body: Option<serde_json::Value>,
    },
    EnvironmentRead {
        variable: String,
        value: Option<String>,
    },
    GlobalMutation { name: String },
    ThrownError {
        error_type: String,
        message: String,
        stack: Option<String>,
    },
    GlobalStateChange {
        variable: String,
        before: serde_json::Value,
        after: serde_json::Value,
    },
}

/// A call to an external (mocked or observed) dependency during execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExternalCall {
    /// Fully qualified name of the called symbol.
    pub symbol: String,
    /// Arguments passed to the call.
    pub args: Vec<serde_json::Value>,
    /// Return value from the call.
    pub return_value: serde_json::Value,
}

/// Complete record of a single function execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionRecord {
    // Identity
    pub function_id: String,
    pub input_hash: u64,

    // Inputs
    pub parameters: Vec<serde_json::Value>,

    // Control flow
    pub branch_path: Vec<BranchDecision>,
    pub lines_executed: Vec<u32>,
    pub calls_to_external: Vec<ExternalCall>,
    pub path_constraints: Vec<SymConstraint>,

    // Outputs
    pub return_value: Option<serde_json::Value>,
    pub thrown_error: Option<ErrorInfo>,
    pub side_effects: Vec<SideEffect>,

    // Performance
    pub wall_time_ms: f64,
    pub cpu_time_us: u64,
    pub heap_used_bytes: u64,
    pub heap_allocated_bytes: u64,

    // Metadata
    pub timestamp: String,
    pub engine_version: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sym_expr::{BinOpKind, ConstValue, SymExpr};

    fn round_trip<T: Serialize + for<'de> Deserialize<'de> + PartialEq + std::fmt::Debug>(
        value: &T,
    ) {
        let json = serde_json::to_string(value).expect("serialize");
        let deserialized: T = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(*value, deserialized, "round-trip failed for json: {json}");
    }

    #[test]
    fn sym_constraint_expr_round_trips() {
        round_trip(&SymConstraint::Expr {
            expr: SymExpr::BinOp {
                op: BinOpKind::Gt,
                left: Box::new(SymExpr::Param {
                    name: "x".into(),
                    path: vec![],
                }),
                right: Box::new(SymExpr::Const(ConstValue::Int(10))),
            },
        });
    }

    #[test]
    fn sym_constraint_unknown_round_trips() {
        round_trip(&SymConstraint::Unknown {
            hint: "complex regex match".into(),
        });
    }

    #[test]
    fn branch_decision_round_trips() {
        round_trip(&BranchDecision {
            branch_id: 1,
            line: 42,
            taken: true,
            constraint: SymConstraint::Expr {
                expr: SymExpr::BinOp {
                    op: BinOpKind::Eq,
                    left: Box::new(SymExpr::Param {
                        name: "status".into(),
                        path: vec![],
                    }),
                    right: Box::new(SymExpr::Const(ConstValue::Str("active".into()))),
                },
            },
        });
    }

    #[test]
    fn error_info_round_trips() {
        round_trip(&ErrorInfo {
            error_type: "TypeError".into(),
            message: "Cannot read property of null".into(),
            stack: Some("at foo (main.ts:10)".into()),
            error_category: None,
        });
    }

    #[test]
    fn error_info_without_stack_round_trips() {
        round_trip(&ErrorInfo {
            error_type: "ValidationError".into(),
            message: "Invalid input".into(),
            stack: None,
            error_category: None,
        });
    }

    #[test]
    fn error_info_with_category_round_trips() {
        round_trip(&ErrorInfo {
            error_type: "TypeError".into(),
            message: "null is not an object".into(),
            stack: None,
            error_category: Some("runtime".into()),
        });
    }

    #[test]
    fn error_info_category_none_omitted_in_json() {
        let info = ErrorInfo {
            error_type: "Error".into(),
            message: "oops".into(),
            stack: None,
            error_category: None,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(!json.contains("error_category"));
    }

    #[test]
    fn side_effect_variants_round_trip() {
        round_trip(&SideEffect::ConsoleOutput {
            level: "warn".into(),
            message: "deprecated API".into(),
        });
        round_trip(&SideEffect::FileWrite {
            path: "/tmp/output.txt".into(),
            content: None,
        });
        round_trip(&SideEffect::FileWrite {
            path: "/tmp/output.txt".into(),
            content: Some("hello".into()),
        });
        round_trip(&SideEffect::NetworkRequest {
            method: "POST".into(),
            url: "https://api.example.com/data".into(),
            body: None,
        });
        round_trip(&SideEffect::NetworkRequest {
            method: "POST".into(),
            url: "https://api.example.com/data".into(),
            body: Some(serde_json::json!({"key": "value"})),
        });
        round_trip(&SideEffect::EnvironmentRead {
            variable: "HOME".into(),
            value: Some("/home/user".into()),
        });
        round_trip(&SideEffect::EnvironmentRead {
            variable: "MISSING".into(),
            value: None,
        });
        round_trip(&SideEffect::GlobalMutation {
            name: "window.count".into(),
        });
        round_trip(&SideEffect::ThrownError {
            error_type: "TypeError".into(),
            message: "cannot read property of null".into(),
            stack: Some("at foo (main.ts:10)".into()),
        });
        round_trip(&SideEffect::ThrownError {
            error_type: "Error".into(),
            message: "generic error".into(),
            stack: None,
        });
        round_trip(&SideEffect::GlobalStateChange {
            variable: "counter".into(),
            before: serde_json::json!(0),
            after: serde_json::json!(1),
        });
    }

    #[test]
    fn external_call_round_trips() {
        round_trip(&ExternalCall {
            symbol: "rateService.getExpressRate".into(),
            args: vec![serde_json::json!("90210")],
            return_value: serde_json::json!(12.99),
        });
    }

    #[test]
    fn full_execution_record_round_trips() {
        let record = ExecutionRecord {
            function_id: "calculateShipping".into(),
            input_hash: 0xdeadbeef,
            parameters: vec![serde_json::json!({"items": [1, 2, 3], "priority": "express"})],
            branch_path: vec![
                BranchDecision {
                    branch_id: 0,
                    line: 10,
                    taken: true,
                    constraint: SymConstraint::Expr {
                        expr: SymExpr::BinOp {
                            op: BinOpKind::Ge,
                            left: Box::new(SymExpr::Param {
                                name: "items".into(),
                                path: vec!["length".into()],
                            }),
                            right: Box::new(SymExpr::Const(ConstValue::Int(5))),
                        },
                    },
                },
                BranchDecision {
                    branch_id: 1,
                    line: 23,
                    taken: true,
                    constraint: SymConstraint::Unknown {
                        hint: "regex validation".into(),
                    },
                },
            ],
            lines_executed: vec![10, 11, 23, 24, 30],
            calls_to_external: vec![ExternalCall {
                symbol: "rateService.getExpressRate".into(),
                args: vec![serde_json::json!("90210")],
                return_value: serde_json::json!(12.99),
            }],
            path_constraints: vec![
                SymConstraint::Expr {
                    expr: SymExpr::BinOp {
                        op: BinOpKind::Ge,
                        left: Box::new(SymExpr::Param {
                            name: "items".into(),
                            path: vec!["length".into()],
                        }),
                        right: Box::new(SymExpr::Const(ConstValue::Int(5))),
                    },
                },
                SymConstraint::Unknown {
                    hint: "regex validation".into(),
                },
            ],
            return_value: Some(serde_json::json!({"cost": 12.99, "method": "express"})),
            thrown_error: None,
            side_effects: vec![SideEffect::ConsoleOutput {
                level: "info".into(),
                message: "Processing express order".into(),
            }],
            wall_time_ms: 0.3,
            cpu_time_us: 250,
            heap_used_bytes: 1024,
            heap_allocated_bytes: 2048,
            timestamp: "2026-02-23T10:30:00Z".into(),
            engine_version: "0.1.0".into(),
        };
        round_trip(&record);
    }

    #[test]
    fn execution_record_with_error_round_trips() {
        let record = ExecutionRecord {
            function_id: "validateInput".into(),
            input_hash: 0xcafebabe,
            parameters: vec![serde_json::json!(null)],
            branch_path: vec![],
            lines_executed: vec![1, 2],
            calls_to_external: vec![],
            path_constraints: vec![],
            return_value: None,
            thrown_error: Some(ErrorInfo {
                error_type: "TypeError".into(),
                message: "input is null".into(),
                stack: None, error_category: None }),
            side_effects: vec![],
            wall_time_ms: 0.01,
            cpu_time_us: 8,
            heap_used_bytes: 128,
            heap_allocated_bytes: 128,
            timestamp: "2026-02-23T10:31:00Z".into(),
            engine_version: "0.1.0".into(),
        };
        round_trip(&record);
    }
}
