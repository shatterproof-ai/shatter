//! Symbolic expression types for representing constraints on function inputs.

use std::collections::HashSet;

use crate::types::ComplexKind;
use serde::{Deserialize, Serialize};

/// A symbolic expression representing a constraint on function inputs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SymExpr {
    /// Reference to a function parameter, with optional field path.
    /// e.g., param "config", path ["timeout"] represents config.timeout
    Param {
        name: String,
        #[serde(default)]
        path: Vec<String>,
    },

    /// A literal constant value.
    Const(ConstValue),

    /// Binary operation: left op right.
    BinOp {
        op: BinOpKind,
        left: Box<SymExpr>,
        right: Box<SymExpr>,
    },

    /// Unary operation: op operand.
    UnOp {
        op: UnOpKind,
        operand: Box<SymExpr>,
    },

    /// Method/function call with symbolic arguments.
    Call {
        name: String,
        receiver: Option<Box<SymExpr>>,
        args: Vec<SymExpr>,
    },

    /// If-then-else: condition ? then_expr : else_expr.
    /// Used by loop state merging to build ITE chains from per-iteration constraints.
    Ite {
        condition: Box<SymExpr>,
        then_expr: Box<SymExpr>,
        else_expr: Box<SymExpr>,
    },

    /// Could not be tracked symbolically — fall back to fuzzing.
    Unknown,
}

/// A concrete constant value in a symbolic expression.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum ConstValue {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Null,
    Undefined,
    /// Complex constant (e.g., Date literal in a comparison).
    /// `repr` is the underlying value the solver reasons about
    /// (epoch millis for dates, digit string for bigint, codepoint for char).
    Complex {
        kind: ComplexKind,
        repr: Box<ConstValue>,
    },
}

/// Binary operators for symbolic expressions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BinOpKind {
    // Comparison
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    // Arithmetic
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    // Logical
    And,
    Or,
    // Bitwise
    BitwiseAnd,
    BitwiseOr,
    BitwiseXor,
    // Shifts and bit-clear (Go-specific)
    Shl,
    Shr,
    BitClear,
    // JS-specific
    In,
    InstanceOf,
}

/// Collect all parameter names referenced in a symbolic expression tree.
/// Used for branch-parameter attribution — identifying which parameters
/// influence a given branch condition.
pub fn extract_param_names(expr: &SymExpr) -> HashSet<String> {
    let mut names = HashSet::new();
    collect_param_names(expr, &mut names);
    names
}

fn collect_param_names(expr: &SymExpr, names: &mut HashSet<String>) {
    match expr {
        SymExpr::Param { name, .. } => {
            names.insert(name.clone());
        }
        SymExpr::Const(_) | SymExpr::Unknown => {}
        SymExpr::BinOp { left, right, .. } => {
            collect_param_names(left, names);
            collect_param_names(right, names);
        }
        SymExpr::UnOp { operand, .. } => {
            collect_param_names(operand, names);
        }
        SymExpr::Call {
            receiver, args, ..
        } => {
            if let Some(r) = receiver {
                collect_param_names(r, names);
            }
            for arg in args {
                collect_param_names(arg, names);
            }
        }
        SymExpr::Ite {
            condition,
            then_expr,
            else_expr,
        } => {
            collect_param_names(condition, names);
            collect_param_names(then_expr, names);
            collect_param_names(else_expr, names);
        }
    }
}

/// Unary operators for symbolic expressions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnOpKind {
    Not,
    Neg,
    BitwiseNot,
    #[serde(alias = "typeof")]
    TypeOf,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip<T: Serialize + for<'de> Deserialize<'de> + PartialEq + std::fmt::Debug>(
        value: &T,
    ) {
        let json = serde_json::to_string(value).expect("serialize");
        let deserialized: T = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(*value, deserialized, "round-trip failed for json: {json}");
    }

    #[test]
    fn sym_expr_param_round_trips() {
        round_trip(&SymExpr::Param {
            name: "config".into(),
            path: vec!["timeout".into()],
        });
    }

    #[test]
    fn sym_expr_const_round_trips() {
        round_trip(&SymExpr::Const(ConstValue::Int(42)));
        round_trip(&SymExpr::Const(ConstValue::Float(3.14)));
        round_trip(&SymExpr::Const(ConstValue::Str("hello".into())));
        round_trip(&SymExpr::Const(ConstValue::Bool(true)));
        round_trip(&SymExpr::Const(ConstValue::Null));
        round_trip(&SymExpr::Const(ConstValue::Undefined));
    }

    #[test]
    fn sym_expr_binop_round_trips() {
        round_trip(&SymExpr::BinOp {
            op: BinOpKind::Gt,
            left: Box::new(SymExpr::Param {
                name: "x".into(),
                path: vec![],
            }),
            right: Box::new(SymExpr::Const(ConstValue::Int(10))),
        });
    }

    #[test]
    fn sym_expr_unop_round_trips() {
        round_trip(&SymExpr::UnOp {
            op: UnOpKind::Not,
            operand: Box::new(SymExpr::Param {
                name: "flag".into(),
                path: vec![],
            }),
        });
    }

    #[test]
    fn sym_expr_call_round_trips() {
        round_trip(&SymExpr::Call {
            name: "includes".into(),
            receiver: Some(Box::new(SymExpr::Param {
                name: "arr".into(),
                path: vec![],
            })),
            args: vec![SymExpr::Const(ConstValue::Str("needle".into()))],
        });
    }

    #[test]
    fn sym_expr_call_without_receiver_round_trips() {
        round_trip(&SymExpr::Call {
            name: "isValid".into(),
            receiver: None,
            args: vec![SymExpr::Param {
                name: "input".into(),
                path: vec![],
            }],
        });
    }

    #[test]
    fn sym_expr_unknown_round_trips() {
        round_trip(&SymExpr::Unknown);
    }

    #[test]
    fn sym_expr_ite_round_trips() {
        let expr = SymExpr::Ite {
            condition: Box::new(SymExpr::BinOp {
                op: BinOpKind::Eq,
                left: Box::new(SymExpr::Param {
                    name: "iter".into(),
                    path: vec![],
                }),
                right: Box::new(SymExpr::Const(ConstValue::Int(0))),
            }),
            then_expr: Box::new(SymExpr::Param {
                name: "x".into(),
                path: vec![],
            }),
            else_expr: Box::new(SymExpr::Const(ConstValue::Int(42))),
        };
        round_trip(&expr);

        // Verify the serde tag is "ite"
        let json = serde_json::to_value(&expr).expect("serialize");
        assert_eq!(json["kind"], "ite");
    }

    #[test]
    fn extract_param_names_ite() {
        let expr = SymExpr::Ite {
            condition: Box::new(SymExpr::Param {
                name: "cond".into(),
                path: vec![],
            }),
            then_expr: Box::new(SymExpr::Param {
                name: "a".into(),
                path: vec![],
            }),
            else_expr: Box::new(SymExpr::Param {
                name: "b".into(),
                path: vec![],
            }),
        };
        assert_eq!(
            extract_param_names(&expr),
            HashSet::from(["cond".into(), "a".into(), "b".into()])
        );
    }

    #[test]
    fn extract_param_names_ite_deduplicates() {
        let expr = SymExpr::Ite {
            condition: Box::new(SymExpr::Param {
                name: "x".into(),
                path: vec![],
            }),
            then_expr: Box::new(SymExpr::Param {
                name: "x".into(),
                path: vec![],
            }),
            else_expr: Box::new(SymExpr::Param {
                name: "x".into(),
                path: vec![],
            }),
        };
        assert_eq!(
            extract_param_names(&expr),
            HashSet::from(["x".into()])
        );
    }

    #[test]
    fn nested_binop_round_trips() {
        let expr = SymExpr::BinOp {
            op: BinOpKind::And,
            left: Box::new(SymExpr::BinOp {
                op: BinOpKind::Gt,
                left: Box::new(SymExpr::Param {
                    name: "x".into(),
                    path: vec![],
                }),
                right: Box::new(SymExpr::Const(ConstValue::Int(0))),
            }),
            right: Box::new(SymExpr::BinOp {
                op: BinOpKind::Lt,
                left: Box::new(SymExpr::Param {
                    name: "x".into(),
                    path: vec![],
                }),
                right: Box::new(SymExpr::Const(ConstValue::Int(100))),
            }),
        };
        round_trip(&expr);
    }

    #[test]
    fn all_binop_kinds_round_trip() {
        let ops = [
            BinOpKind::Eq,
            BinOpKind::Ne,
            BinOpKind::Lt,
            BinOpKind::Le,
            BinOpKind::Gt,
            BinOpKind::Ge,
            BinOpKind::Add,
            BinOpKind::Sub,
            BinOpKind::Mul,
            BinOpKind::Div,
            BinOpKind::Mod,
            BinOpKind::And,
            BinOpKind::Or,
            BinOpKind::BitwiseAnd,
            BinOpKind::BitwiseOr,
            BinOpKind::BitwiseXor,
            BinOpKind::Shl,
            BinOpKind::Shr,
            BinOpKind::BitClear,
            BinOpKind::In,
            BinOpKind::InstanceOf,
        ];
        for op in ops {
            round_trip(&op);
        }
    }

    #[test]
    fn all_unop_kinds_round_trip() {
        let ops = [
            UnOpKind::Not,
            UnOpKind::Neg,
            UnOpKind::BitwiseNot,
            UnOpKind::TypeOf,
        ];
        for op in ops {
            round_trip(&op);
        }
    }

    #[test]
    fn const_value_complex_round_trips() {
        use crate::types::ComplexKind;

        // Date with Int repr
        round_trip(&ConstValue::Complex {
            kind: ComplexKind::Date,
            repr: Box::new(ConstValue::Int(1704067200000)),
        });

        // BigInt with Str repr
        round_trip(&ConstValue::Complex {
            kind: ComplexKind::BigInt,
            repr: Box::new(ConstValue::Str("99999999999999999999".into())),
        });

        // Char with Int repr (codepoint)
        round_trip(&ConstValue::Complex {
            kind: ComplexKind::Char,
            repr: Box::new(ConstValue::Int(8364)),
        });
    }

    #[test]
    fn sym_expr_with_complex_const_round_trips() {
        use crate::types::ComplexKind;

        round_trip(&SymExpr::BinOp {
            op: BinOpKind::Gt,
            left: Box::new(SymExpr::Param {
                name: "date".into(),
                path: vec![],
            }),
            right: Box::new(SymExpr::Const(ConstValue::Complex {
                kind: ComplexKind::Date,
                repr: Box::new(ConstValue::Int(1704067200000)),
            })),
        });
    }

    #[test]
    fn typeof_alias_deserializes_from_frontend_format() {
        // The TS frontend sends "typeof" but Rust serializes as "type_of".
        // Both must deserialize correctly.
        let from_frontend: UnOpKind =
            serde_json::from_str(r#""typeof""#).expect("typeof alias should deserialize");
        assert_eq!(from_frontend, UnOpKind::TypeOf);

        let from_rust: UnOpKind =
            serde_json::from_str(r#""type_of""#).expect("type_of should deserialize");
        assert_eq!(from_rust, UnOpKind::TypeOf);
    }

    /// Reproduction test for str-a4c: Go frontend sends "bit_clear", "shl", "shr"
    /// which must deserialize into BinOpKind variants.
    #[test]
    fn go_bitwise_ops_deserialize() {
        let bit_clear: BinOpKind =
            serde_json::from_str(r#""bit_clear""#).expect("bit_clear should deserialize");
        assert_eq!(bit_clear, BinOpKind::BitClear);

        let shl: BinOpKind =
            serde_json::from_str(r#""shl""#).expect("shl should deserialize");
        assert_eq!(shl, BinOpKind::Shl);

        let shr: BinOpKind =
            serde_json::from_str(r#""shr""#).expect("shr should deserialize");
        assert_eq!(shr, BinOpKind::Shr);

        // Full BinOp expression with bit_clear, as sent by the Go frontend
        let json = r#"{"kind":"bin_op","op":"bit_clear","left":{"kind":"param","name":"x","path":[]},"right":{"kind":"const","type":"int","value":255}}"#;
        let expr: SymExpr = serde_json::from_str(json).expect("bit_clear binop should deserialize");
        match expr {
            SymExpr::BinOp { op, .. } => assert_eq!(op, BinOpKind::BitClear),
            other => panic!("expected BinOp, got {other:?}"),
        }
    }

    #[test]
    fn extract_param_names_single_param() {
        let expr = SymExpr::Param {
            name: "x".into(),
            path: vec![],
        };
        let names = extract_param_names(&expr);
        assert_eq!(names, HashSet::from(["x".into()]));
    }

    #[test]
    fn extract_param_names_const_returns_empty() {
        let expr = SymExpr::Const(ConstValue::Int(42));
        assert!(extract_param_names(&expr).is_empty());
    }

    #[test]
    fn extract_param_names_unknown_returns_empty() {
        assert!(extract_param_names(&SymExpr::Unknown).is_empty());
    }

    #[test]
    fn extract_param_names_binop_two_params() {
        let expr = SymExpr::BinOp {
            op: BinOpKind::Add,
            left: Box::new(SymExpr::Param {
                name: "x".into(),
                path: vec![],
            }),
            right: Box::new(SymExpr::Param {
                name: "y".into(),
                path: vec![],
            }),
        };
        assert_eq!(
            extract_param_names(&expr),
            HashSet::from(["x".into(), "y".into()])
        );
    }

    #[test]
    fn extract_param_names_deduplicates() {
        // x > 0 && x < 100 — "x" appears twice but should be deduplicated
        let expr = SymExpr::BinOp {
            op: BinOpKind::And,
            left: Box::new(SymExpr::BinOp {
                op: BinOpKind::Gt,
                left: Box::new(SymExpr::Param {
                    name: "x".into(),
                    path: vec![],
                }),
                right: Box::new(SymExpr::Const(ConstValue::Int(0))),
            }),
            right: Box::new(SymExpr::BinOp {
                op: BinOpKind::Lt,
                left: Box::new(SymExpr::Param {
                    name: "x".into(),
                    path: vec![],
                }),
                right: Box::new(SymExpr::Const(ConstValue::Int(100))),
            }),
        };
        let names = extract_param_names(&expr);
        assert_eq!(names, HashSet::from(["x".into()]));
    }

    #[test]
    fn extract_param_names_unop() {
        let expr = SymExpr::UnOp {
            op: UnOpKind::Not,
            operand: Box::new(SymExpr::Param {
                name: "flag".into(),
                path: vec![],
            }),
        };
        assert_eq!(
            extract_param_names(&expr),
            HashSet::from(["flag".into()])
        );
    }

    #[test]
    fn extract_param_names_call_with_receiver_and_args() {
        // arr.includes(needle) — receiver is param "arr", arg is param "needle"
        let expr = SymExpr::Call {
            name: "includes".into(),
            receiver: Some(Box::new(SymExpr::Param {
                name: "arr".into(),
                path: vec![],
            })),
            args: vec![SymExpr::Param {
                name: "needle".into(),
                path: vec![],
            }],
        };
        assert_eq!(
            extract_param_names(&expr),
            HashSet::from(["arr".into(), "needle".into()])
        );
    }

    #[test]
    fn extract_param_names_call_no_receiver() {
        let expr = SymExpr::Call {
            name: "isValid".into(),
            receiver: None,
            args: vec![SymExpr::Param {
                name: "input".into(),
                path: vec![],
            }],
        };
        assert_eq!(
            extract_param_names(&expr),
            HashSet::from(["input".into()])
        );
    }

    #[test]
    fn extract_param_names_deeply_nested() {
        // (a > 0 && b < 10) || !c — three distinct params across nested ops
        let expr = SymExpr::BinOp {
            op: BinOpKind::Or,
            left: Box::new(SymExpr::BinOp {
                op: BinOpKind::And,
                left: Box::new(SymExpr::BinOp {
                    op: BinOpKind::Gt,
                    left: Box::new(SymExpr::Param {
                        name: "a".into(),
                        path: vec![],
                    }),
                    right: Box::new(SymExpr::Const(ConstValue::Int(0))),
                }),
                right: Box::new(SymExpr::BinOp {
                    op: BinOpKind::Lt,
                    left: Box::new(SymExpr::Param {
                        name: "b".into(),
                        path: vec![],
                    }),
                    right: Box::new(SymExpr::Const(ConstValue::Int(10))),
                }),
            }),
            right: Box::new(SymExpr::UnOp {
                op: UnOpKind::Not,
                operand: Box::new(SymExpr::Param {
                    name: "c".into(),
                    path: vec![],
                }),
            }),
        };
        assert_eq!(
            extract_param_names(&expr),
            HashSet::from(["a".into(), "b".into(), "c".into()])
        );
    }

    #[test]
    fn extract_param_names_with_field_path() {
        // config.timeout > 0 — param name is "config" regardless of path
        let expr = SymExpr::BinOp {
            op: BinOpKind::Gt,
            left: Box::new(SymExpr::Param {
                name: "config".into(),
                path: vec!["timeout".into()],
            }),
            right: Box::new(SymExpr::Const(ConstValue::Int(0))),
        };
        assert_eq!(
            extract_param_names(&expr),
            HashSet::from(["config".into()])
        );
    }
}
