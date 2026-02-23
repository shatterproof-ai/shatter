//! Shared types for type information and parameter metadata.

use serde::{Deserialize, Serialize};

/// Describes the type of a value, as reported by a language frontend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TypeInfo {
    Int,
    Float,
    Str,
    Bool,
    Array {
        element: Box<TypeInfo>,
    },
    Object {
        fields: Vec<(String, TypeInfo)>,
    },
    Union {
        variants: Vec<TypeInfo>,
    },
    Nullable {
        inner: Box<TypeInfo>,
    },
    /// Type could not be determined statically.
    Unknown,
}

/// Metadata about a function parameter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParamInfo {
    pub name: String,
    #[serde(rename = "type")]
    pub typ: TypeInfo,
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
    fn primitive_types_round_trip() {
        round_trip(&TypeInfo::Int);
        round_trip(&TypeInfo::Float);
        round_trip(&TypeInfo::Str);
        round_trip(&TypeInfo::Bool);
        round_trip(&TypeInfo::Unknown);
    }

    #[test]
    fn array_type_round_trips() {
        round_trip(&TypeInfo::Array {
            element: Box::new(TypeInfo::Int),
        });
    }

    #[test]
    fn object_type_round_trips() {
        round_trip(&TypeInfo::Object {
            fields: vec![
                ("name".into(), TypeInfo::Str),
                ("age".into(), TypeInfo::Int),
            ],
        });
    }

    #[test]
    fn union_type_round_trips() {
        round_trip(&TypeInfo::Union {
            variants: vec![TypeInfo::Str, TypeInfo::Int],
        });
    }

    #[test]
    fn nullable_type_round_trips() {
        round_trip(&TypeInfo::Nullable {
            inner: Box::new(TypeInfo::Str),
        });
    }

    #[test]
    fn nested_type_round_trips() {
        let typ = TypeInfo::Object {
            fields: vec![
                ("items".into(), TypeInfo::Array {
                    element: Box::new(TypeInfo::Object {
                        fields: vec![
                            ("id".into(), TypeInfo::Int),
                            ("label".into(), TypeInfo::Nullable {
                                inner: Box::new(TypeInfo::Str),
                            }),
                        ],
                    }),
                }),
                ("count".into(), TypeInfo::Int),
            ],
        };
        round_trip(&typ);
    }

    #[test]
    fn param_info_round_trips() {
        round_trip(&ParamInfo {
            name: "order".into(),
            typ: TypeInfo::Object {
                fields: vec![
                    ("items".into(), TypeInfo::Array {
                        element: Box::new(TypeInfo::Int),
                    }),
                    ("priority".into(), TypeInfo::Str),
                ],
            },
        });
    }

    #[test]
    fn param_info_with_unknown_type_round_trips() {
        round_trip(&ParamInfo {
            name: "x".into(),
            typ: TypeInfo::Unknown,
        });
    }
}
