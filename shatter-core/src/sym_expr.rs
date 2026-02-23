//! Symbolic expression types for representing constraints on function inputs.

use serde::{Deserialize, Serialize};

/// A symbolic expression representing a constraint on function inputs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SymExpr {
    /// Reference to a function parameter, with optional field path.
    /// e.g., param "config", path ["timeout"] represents config.timeout
    Param { name: String, path: Vec<String> },

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
    // JS-specific
    In,
    InstanceOf,
}

/// Unary operators for symbolic expressions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnOpKind {
    Not,
    Neg,
    BitwiseNot,
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
    fn sym_expr_param_empty_path_round_trips() {
        round_trip(&SymExpr::Param {
            name: "x".into(),
            path: vec![],
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
}
