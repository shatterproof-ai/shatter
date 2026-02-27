//! Checks whether a function's parameters contain opaque types,
//! making it unexecutable for automated testing.

use crate::types::ParamInfo;

/// Reason why a function parameter cannot be automatically tested.
#[derive(Debug, Clone, PartialEq)]
pub struct SkipReason {
    /// Name of the parameter containing the opaque type.
    pub param_name: String,
    /// Label of the opaque type found (e.g. "net.Socket", "pg.Client").
    pub opaque_label: String,
}

/// Checks each parameter for opaque types. Returns a `SkipReason` for every
/// parameter whose type tree contains an `Opaque` node.
pub fn check_executability(params: &[ParamInfo]) -> Vec<SkipReason> {
    params
        .iter()
        .filter_map(|p| {
            p.typ.find_opaque_label().map(|label| SkipReason {
                param_name: p.name.clone(),
                opaque_label: label,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ComplexKind, TypeInfo};

    fn param(name: &str, typ: TypeInfo) -> ParamInfo {
        ParamInfo {
            name: name.into(),
            typ,
            type_name: None,
        }
    }

    #[test]
    fn no_opaque_params_returns_empty() {
        let params = vec![param("a", TypeInfo::Int), param("b", TypeInfo::Str)];
        assert!(check_executability(&params).is_empty());
    }

    #[test]
    fn direct_opaque_param_returns_reason() {
        let params = vec![param(
            "conn",
            TypeInfo::Opaque {
                label: "pg.Client".into(),
            },
        )];
        let reasons = check_executability(&params);
        assert_eq!(reasons.len(), 1);
        assert_eq!(reasons[0].param_name, "conn");
        assert_eq!(reasons[0].opaque_label, "pg.Client");
    }

    #[test]
    fn array_of_opaque_returns_reason() {
        let params = vec![param(
            "sockets",
            TypeInfo::Array {
                element: Box::new(TypeInfo::Opaque {
                    label: "net.Socket".into(),
                }),
            },
        )];
        let reasons = check_executability(&params);
        assert_eq!(reasons.len(), 1);
        assert_eq!(reasons[0].param_name, "sockets");
        assert_eq!(reasons[0].opaque_label, "net.Socket");
    }

    #[test]
    fn all_primitive_params_returns_empty() {
        let params = vec![
            param("x", TypeInfo::Int),
            param("y", TypeInfo::Float),
            param("name", TypeInfo::Str),
            param("flag", TypeInfo::Bool),
        ];
        assert!(check_executability(&params).is_empty());
    }

    #[test]
    fn multiple_opaque_params_returns_multiple_reasons() {
        let params = vec![
            param(
                "db",
                TypeInfo::Opaque {
                    label: "pg.Client".into(),
                },
            ),
            param("name", TypeInfo::Str),
            param(
                "stream",
                TypeInfo::Opaque {
                    label: "fs.ReadStream".into(),
                },
            ),
        ];
        let reasons = check_executability(&params);
        assert_eq!(reasons.len(), 2);
        assert_eq!(reasons[0].param_name, "db");
        assert_eq!(reasons[0].opaque_label, "pg.Client");
        assert_eq!(reasons[1].param_name, "stream");
        assert_eq!(reasons[1].opaque_label, "fs.ReadStream");
    }

    #[test]
    fn deeply_nested_opaque_in_object_array_returns_reason() {
        let params = vec![param(
            "config",
            TypeInfo::Array {
                element: Box::new(TypeInfo::Object {
                    fields: vec![
                        ("name".into(), TypeInfo::Str),
                        (
                            "handler".into(),
                            TypeInfo::Opaque {
                                label: "http.Handler".into(),
                            },
                        ),
                    ],
                }),
            },
        )];
        let reasons = check_executability(&params);
        assert_eq!(reasons.len(), 1);
        assert_eq!(reasons[0].param_name, "config");
        assert_eq!(reasons[0].opaque_label, "http.Handler");
    }

    #[test]
    fn union_containing_opaque_returns_reason() {
        let params = vec![param(
            "input",
            TypeInfo::Union {
                variants: vec![
                    TypeInfo::Str,
                    TypeInfo::Opaque {
                        label: "stream.Readable".into(),
                    },
                ],
            },
        )];
        let reasons = check_executability(&params);
        assert_eq!(reasons.len(), 1);
        assert_eq!(reasons[0].param_name, "input");
        assert_eq!(reasons[0].opaque_label, "stream.Readable");
    }

    #[test]
    fn nullable_opaque_returns_reason() {
        let params = vec![param(
            "maybe_conn",
            TypeInfo::Nullable {
                inner: Box::new(TypeInfo::Opaque {
                    label: "pg.Pool".into(),
                }),
            },
        )];
        let reasons = check_executability(&params);
        assert_eq!(reasons.len(), 1);
        assert_eq!(reasons[0].param_name, "maybe_conn");
        assert_eq!(reasons[0].opaque_label, "pg.Pool");
    }

    #[test]
    fn complex_with_opaque_inner_returns_reason() {
        let params = vec![param(
            "wrapped",
            TypeInfo::Complex {
                kind: ComplexKind::Option,
                metadata: serde_json::Map::new(),
                inner: Some(Box::new(TypeInfo::Opaque {
                    label: "channel".into(),
                })),
            },
        )];
        let reasons = check_executability(&params);
        assert_eq!(reasons.len(), 1);
        assert_eq!(reasons[0].param_name, "wrapped");
        assert_eq!(reasons[0].opaque_label, "channel");
    }

    #[test]
    fn empty_params_returns_empty() {
        assert!(check_executability(&[]).is_empty());
    }
}
