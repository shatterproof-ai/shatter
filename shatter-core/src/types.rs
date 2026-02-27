//! Shared types for type information and parameter metadata.

use serde::{Deserialize, Serialize};

/// Well-known complex types that go beyond primitives and structural types.
///
/// Every supported complex type is an explicit variant. Adding a new type
/// requires adding a variant here, a generator in input_gen.rs, and
/// declaring support in the relevant frontend's handshake capabilities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComplexKind {
    // ── Temporal ──
    Date,
    DateTime,
    Time,
    Duration,

    // ── Text / Pattern ──
    RegExp,
    Char,
    /// JS/TS Symbol.
    Symbol,

    // ── Extended Numeric ──
    BigInt,
    BigDecimal,
    /// Complex numbers (real + imaginary).
    Complex,
    Rational,
    Range,

    // ── Binary ──
    Buffer,
    BitSet,

    // ── Error / Result ──
    Error,
    /// Option/Maybe wrapper.
    Option,
    /// Result/Either wrapper.
    Result,

    // ── Functional ──
    Closure,
    Iterator,

    // ── Network / Web ──
    Url,
    IpAddress,

    // ── Serialization / Interchange ──
    Uuid,

    // ── I/O ──
    Path,

    // ── Domain-Specific ──
    Money,
    SemVer,
    Email,
    MimeType,
    Color,
    GeoPoint,
    Locale,

    // ── Go-specific ──
    /// Go rune (int32 alias for Unicode codepoint).
    Rune,
    /// Go byte (uint8 alias).
    GoByte,
}

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
    /// Complex (non-primitive, non-collection) type.
    Complex {
        #[serde(rename = "complex_kind")]
        kind: ComplexKind,
        /// Language-specific metadata (e.g., `{"class":"TypeError"}`, `{"flags":"gi"}`).
        #[serde(default)]
        metadata: serde_json::Map<String, serde_json::Value>,
        /// Inner type for wrapper types (Option<T>, Result<T,E>).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        inner: Option<Box<TypeInfo>>,
    },
    /// Runtime resource type that cannot be meaningfully constructed for testing
    /// (sockets, file descriptors, database connections, streams, channels).
    ///
    /// Distinct from `Unknown` (type couldn't be resolved) and `Complex` (type is
    /// known and constructible).
    Opaque {
        label: String,
    },
    /// Type could not be determined statically.
    Unknown,
}

impl TypeInfo {
    /// Returns `true` if this type tree contains an `Opaque` variant anywhere
    /// (directly or nested inside Array elements, Object fields, Union variants,
    /// Nullable inner, or Complex inner).
    pub fn has_opaque(&self) -> bool {
        match self {
            TypeInfo::Opaque { .. } => true,
            TypeInfo::Array { element } => element.has_opaque(),
            TypeInfo::Object { fields } => fields.iter().any(|(_, t)| t.has_opaque()),
            TypeInfo::Union { variants } => variants.iter().any(|t| t.has_opaque()),
            TypeInfo::Nullable { inner } => inner.has_opaque(),
            TypeInfo::Complex { inner, .. } => {
                inner.as_deref().is_some_and(|t| t.has_opaque())
            }
            TypeInfo::Int
            | TypeInfo::Float
            | TypeInfo::Str
            | TypeInfo::Bool
            | TypeInfo::Unknown => false,
        }
    }

    /// Returns the label of the first `Opaque` variant found in this type tree,
    /// or `None` if no opaque type exists.
    pub fn find_opaque_label(&self) -> Option<String> {
        match self {
            TypeInfo::Opaque { label } => Some(label.clone()),
            TypeInfo::Array { element } => element.find_opaque_label(),
            TypeInfo::Object { fields } => {
                fields.iter().find_map(|(_, t)| t.find_opaque_label())
            }
            TypeInfo::Union { variants } => {
                variants.iter().find_map(|t| t.find_opaque_label())
            }
            TypeInfo::Nullable { inner } => inner.find_opaque_label(),
            TypeInfo::Complex { inner, .. } => {
                inner.as_deref().and_then(|t| t.find_opaque_label())
            }
            TypeInfo::Int
            | TypeInfo::Float
            | TypeInfo::Str
            | TypeInfo::Bool
            | TypeInfo::Unknown => None,
        }
    }
}

/// Metadata about a function parameter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParamInfo {
    pub name: String,
    #[serde(rename = "type")]
    pub typ: TypeInfo,
    /// The original type name as written in source code (e.g. `"User"`, `"Date"`).
    /// Used to match against type-level generators in config.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub type_name: Option<String>,
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
            type_name: None,
        });
    }

    #[test]
    fn param_info_with_unknown_type_round_trips() {
        round_trip(&ParamInfo {
            name: "x".into(),
            typ: TypeInfo::Unknown,
            type_name: None,
        });
    }

    #[test]
    fn complex_kind_all_variants_round_trip() {
        let variants = [
            ComplexKind::Date,
            ComplexKind::DateTime,
            ComplexKind::Time,
            ComplexKind::Duration,
            ComplexKind::RegExp,
            ComplexKind::Char,
            ComplexKind::Symbol,
            ComplexKind::BigInt,
            ComplexKind::BigDecimal,
            ComplexKind::Complex,
            ComplexKind::Rational,
            ComplexKind::Range,
            ComplexKind::Buffer,
            ComplexKind::BitSet,
            ComplexKind::Error,
            ComplexKind::Option,
            ComplexKind::Result,
            ComplexKind::Closure,
            ComplexKind::Iterator,
            ComplexKind::Url,
            ComplexKind::IpAddress,
            ComplexKind::Uuid,
            ComplexKind::Path,
            ComplexKind::Money,
            ComplexKind::SemVer,
            ComplexKind::Email,
            ComplexKind::MimeType,
            ComplexKind::Color,
            ComplexKind::GeoPoint,
            ComplexKind::Locale,
            ComplexKind::Rune,
            ComplexKind::GoByte,
        ];
        for kind in &variants {
            round_trip(kind);
        }
    }

    #[test]
    fn complex_kind_serializes_to_snake_case() {
        let json = serde_json::to_string(&ComplexKind::BigInt).unwrap();
        assert_eq!(json, "\"big_int\"");
        let json = serde_json::to_string(&ComplexKind::IpAddress).unwrap();
        assert_eq!(json, "\"ip_address\"");
        let json = serde_json::to_string(&ComplexKind::GoByte).unwrap();
        assert_eq!(json, "\"go_byte\"");
    }

    #[test]
    fn complex_type_info_with_metadata_round_trips() {
        let mut metadata = serde_json::Map::new();
        metadata.insert("class".into(), serde_json::Value::String("TypeError".into()));
        round_trip(&TypeInfo::Complex {
            kind: ComplexKind::Error,
            metadata,
            inner: None,
        });
    }

    #[test]
    fn complex_type_info_with_inner_round_trips() {
        round_trip(&TypeInfo::Complex {
            kind: ComplexKind::Option,
            metadata: serde_json::Map::new(),
            inner: Some(Box::new(TypeInfo::Int)),
        });
    }

    #[test]
    fn complex_type_info_empty_metadata_no_inner_round_trips() {
        round_trip(&TypeInfo::Complex {
            kind: ComplexKind::Date,
            metadata: serde_json::Map::new(),
            inner: None,
        });
    }

    #[test]
    fn complex_type_info_nested_in_array_round_trips() {
        round_trip(&TypeInfo::Array {
            element: Box::new(TypeInfo::Complex {
                kind: ComplexKind::Date,
                metadata: serde_json::Map::new(),
                inner: None,
            }),
        });
    }

    #[test]
    fn complex_type_info_nested_in_object_round_trips() {
        round_trip(&TypeInfo::Object {
            fields: vec![
                ("created_at".into(), TypeInfo::Complex {
                    kind: ComplexKind::Date,
                    metadata: serde_json::Map::new(),
                    inner: None,
                }),
                ("name".into(), TypeInfo::Str),
            ],
        });
    }

    #[test]
    fn complex_type_info_result_with_inner_round_trips() {
        round_trip(&TypeInfo::Complex {
            kind: ComplexKind::Result,
            metadata: serde_json::Map::new(),
            inner: Some(Box::new(TypeInfo::Complex {
                kind: ComplexKind::BigInt,
                metadata: serde_json::Map::new(),
                inner: None,
            })),
        });
    }

    // ── Opaque variant tests ──

    #[test]
    fn opaque_round_trips() {
        round_trip(&TypeInfo::Opaque {
            label: "net.Socket".to_string(),
        });
    }

    #[test]
    fn opaque_inside_array_round_trips() {
        round_trip(&TypeInfo::Array {
            element: Box::new(TypeInfo::Opaque {
                label: "fs.FileHandle".to_string(),
            }),
        });
    }

    #[test]
    fn opaque_inside_object_round_trips() {
        round_trip(&TypeInfo::Object {
            fields: vec![
                ("conn".into(), TypeInfo::Opaque {
                    label: "pg.Client".to_string(),
                }),
                ("name".into(), TypeInfo::Str),
            ],
        });
    }

    #[test]
    fn opaque_inside_nullable_round_trips() {
        round_trip(&TypeInfo::Nullable {
            inner: Box::new(TypeInfo::Opaque {
                label: "stream.Readable".to_string(),
            }),
        });
    }

    #[test]
    fn has_opaque_direct() {
        assert!(TypeInfo::Opaque { label: "net.Socket".into() }.has_opaque());
    }

    #[test]
    fn has_opaque_nested_in_array() {
        let typ = TypeInfo::Array {
            element: Box::new(TypeInfo::Opaque { label: "net.Socket".into() }),
        };
        assert!(typ.has_opaque());
    }

    #[test]
    fn has_opaque_nested_in_object() {
        let typ = TypeInfo::Object {
            fields: vec![
                ("conn".into(), TypeInfo::Opaque { label: "pg.Client".into() }),
                ("name".into(), TypeInfo::Str),
            ],
        };
        assert!(typ.has_opaque());
    }

    #[test]
    fn has_opaque_nested_in_nullable() {
        let typ = TypeInfo::Nullable {
            inner: Box::new(TypeInfo::Opaque { label: "channel".into() }),
        };
        assert!(typ.has_opaque());
    }

    #[test]
    fn has_opaque_false_for_all_primitive_tree() {
        let typ = TypeInfo::Object {
            fields: vec![
                ("items".into(), TypeInfo::Array {
                    element: Box::new(TypeInfo::Int),
                }),
                ("label".into(), TypeInfo::Nullable {
                    inner: Box::new(TypeInfo::Str),
                }),
                ("flag".into(), TypeInfo::Bool),
            ],
        };
        assert!(!typ.has_opaque());
    }

    #[test]
    fn has_opaque_false_for_primitives() {
        assert!(!TypeInfo::Int.has_opaque());
        assert!(!TypeInfo::Float.has_opaque());
        assert!(!TypeInfo::Str.has_opaque());
        assert!(!TypeInfo::Bool.has_opaque());
        assert!(!TypeInfo::Unknown.has_opaque());
    }

    #[test]
    fn find_opaque_label_direct() {
        let typ = TypeInfo::Opaque { label: "net.Socket".into() };
        assert_eq!(typ.find_opaque_label(), Some("net.Socket".to_string()));
    }

    #[test]
    fn find_opaque_label_nested() {
        let typ = TypeInfo::Object {
            fields: vec![
                ("name".into(), TypeInfo::Str),
                ("conn".into(), TypeInfo::Nullable {
                    inner: Box::new(TypeInfo::Opaque { label: "pg.Client".into() }),
                }),
            ],
        };
        assert_eq!(typ.find_opaque_label(), Some("pg.Client".to_string()));
    }

    #[test]
    fn find_opaque_label_returns_first() {
        let typ = TypeInfo::Union {
            variants: vec![
                TypeInfo::Opaque { label: "first".into() },
                TypeInfo::Opaque { label: "second".into() },
            ],
        };
        assert_eq!(typ.find_opaque_label(), Some("first".to_string()));
    }

    #[test]
    fn find_opaque_label_none_for_primitive_tree() {
        let typ = TypeInfo::Array {
            element: Box::new(TypeInfo::Int),
        };
        assert_eq!(typ.find_opaque_label(), None);
    }
}
