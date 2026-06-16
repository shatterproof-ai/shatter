//! Shared types for type information and parameter metadata.

use serde::{Deserialize, Serialize};

use crate::executability::PathSegment;

/// Reason a type was detected as opaque via static analysis.
///
/// Complements the runtime opaque-type lookup tables with structural evidence
/// that a type can never be constructed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StaticOpacityReason {
    /// No public constructor and no exported create*/new*/open* factory function.
    NoConstructor,
    /// All constructors require an already-opaque argument.
    TransitivelyOpaque,
    /// Abstract class or private/protected constructor.
    AbstractType,
    /// Interface or abstract class with no concrete implementors in scope.
    NoImplementors,
}

/// Reason a type was detected as potentially opaque via medium-confidence static analysis.
///
/// Unlike [`StaticOpacityReason`], these signals are suggestive but not definitive.
/// They serve as supporting evidence in learning mode and should not alone trigger
/// high-confidence opaque suggestions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediumOpacityReason {
    /// Type comes from a known infrastructure package prefix (DB clients, cloud SDKs, etc.)
    InfrastructurePackage,
    /// Type implements a close/dispose/cleanup interface (io.Closer, Disposable, etc.)
    CloseableInterface,
    /// Type contains fields suggesting OS handles (fd, handle, FileDescriptor, unsafe.Pointer)
    NativeHandleField,
}

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
    /// Go unsigned integer (uint, uint16, uint32, uint64).
    ///
    /// Wire format is a plain non-negative JSON integer (no `__complex_type`
    /// wrapper) so that Go's `encoding/json.Unmarshal` into unsigned types
    /// succeeds. Analogous to `GoByte` for the [0, 255] range, but covers
    /// the full u64 domain.
    GoUint,
    /// Go `time.Duration` (int64 alias, value in nanoseconds).
    ///
    /// Wire format is a plain JSON integer (no `__complex_type` wrapper) so
    /// that Go's `encoding/json.Unmarshal` into `time.Duration` succeeds.
    /// The generic `Duration` complex kind emits `{"__complex_type":"duration","ms":…}`
    /// which Go rejects with "cannot unmarshal object into … time.Duration".
    GoDuration,
}

/// Describes the type of a value, as reported by a language frontend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TypeInfo {
    Int {
        /// Bit width of the integer type (8/16/32/64/128). `None` = unspecified.
        ///
        /// Carried from frontends that know the source integer width (e.g. the
        /// Rust analyzer maps `u8`→8/unsigned) so the input generator can keep
        /// generated values inside the type's range (str-ddxe). Defaults so the
        /// bare wire form `{"kind":"int"}` from TS/Go still deserializes.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        int_width: Option<u8>,
        /// Whether the integer type is signed. `None` = unspecified.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        int_signed: Option<bool>,
    },
    Float,
    Str,
    Bool,
    Array {
        element: Box<TypeInfo>,
    },
    Object {
        #[serde(default)]
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
        /// Reason the type was detected as opaque via static analysis, if available.
        /// Set by language frontends that perform structural inspection (e.g. abstract
        /// class detection). Absent for types detected via the runtime opaque-type tables.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        static_opacity: Option<StaticOpacityReason>,
        /// Medium-confidence opacity signal, if available.
        /// Set when a type shows suggestive (but not definitive) signals of being an
        /// opaque infrastructure resource (e.g. known infra package prefix, closeable
        /// interface, native handle field). Absent when not detected or when
        /// `static_opacity` is already set.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        medium_opacity: Option<MediumOpacityReason>,
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
            TypeInfo::Complex { inner, .. } => inner.as_deref().is_some_and(|t| t.has_opaque()),
            TypeInfo::Int { .. }
            | TypeInfo::Float
            | TypeInfo::Str
            | TypeInfo::Bool
            | TypeInfo::Unknown => false,
        }
    }

    /// Returns the first `Opaque` node found in this type tree as
    /// `(label, static_opacity, medium_opacity)`, appending nesting segments to
    /// `path` as it descends.
    ///
    /// On success the caller's `path` will end with the full nesting segments
    /// down to (but not including) the opaque node — the opaque node itself
    /// is represented by the returned label.  On failure `path` is unchanged.
    ///
    /// The caller should seed `path` with a `PathSegment::Param` entry before
    /// calling so that the resulting path starts from the parameter root.
    ///
    /// To get only the label (ignoring opacity reasons), use:
    /// `find_opaque_node(path).map(|(label, ..)| label)`
    pub fn find_opaque_node(
        &self,
        path: &mut Vec<PathSegment>,
    ) -> Option<(
        String,
        Option<StaticOpacityReason>,
        Option<MediumOpacityReason>,
    )> {
        match self {
            TypeInfo::Opaque {
                label,
                static_opacity,
                medium_opacity,
            } => Some((
                label.clone(),
                static_opacity.clone(),
                medium_opacity.clone(),
            )),
            TypeInfo::Array { element } => {
                path.push(PathSegment::ArrayElement);
                if let Some(result) = element.find_opaque_node(path) {
                    Some(result)
                } else {
                    path.pop();
                    None
                }
            }
            TypeInfo::Object { fields } => {
                if is_map_encoding(fields) {
                    return None;
                }
                for (name, t) in fields {
                    path.push(PathSegment::Field(name.clone()));
                    if let Some(result) = t.find_opaque_node(path) {
                        return Some(result);
                    }
                    path.pop();
                }
                None
            }
            TypeInfo::Union { variants } => {
                for t in variants {
                    path.push(PathSegment::UnionVariant);
                    if let Some(result) = t.find_opaque_node(path) {
                        return Some(result);
                    }
                    path.pop();
                }
                None
            }
            TypeInfo::Nullable { inner } => {
                path.push(PathSegment::NullableInner);
                if let Some(result) = inner.find_opaque_node(path) {
                    Some(result)
                } else {
                    path.pop();
                    None
                }
            }
            TypeInfo::Complex { inner, .. } => {
                if let Some(inner_type) = inner.as_deref() {
                    path.push(PathSegment::ComplexInner);
                    if let Some(result) = inner_type.find_opaque_node(path) {
                        return Some(result);
                    }
                    path.pop();
                }
                None
            }
            TypeInfo::Int { .. }
            | TypeInfo::Float
            | TypeInfo::Str
            | TypeInfo::Bool
            | TypeInfo::Unknown => None,
        }
    }

    /// If `self` is `Int { .. }`, return the inclusive `(min, max)` value range
    /// implied by its width/signedness — but only when that range fits in `i64`.
    /// Returns `None` for unsized ints and for widths whose bounds exceed `i64`
    /// (u64/i64/u128/i128/usize/isize), leaving them unconstrained.
    pub fn int_range(&self) -> Option<(i64, i64)> {
        match self {
            TypeInfo::Int {
                int_width,
                int_signed,
            } => int_range(*int_width, *int_signed),
            _ => None,
        }
    }
}

/// Inclusive `(min, max)` value range for an integer of the given width and
/// signedness, when that range fits in `i64`.
///
/// Returns `None` when width or signedness is unspecified, or when the type's
/// natural bounds exceed `i64` (u64, i64, u128, i128, usize, isize) — those stay
/// unconstrained so the solver/generator keep their existing full-range behavior.
pub fn int_range(width: Option<u8>, signed: Option<bool>) -> Option<(i64, i64)> {
    let width = width?;
    let signed = signed?;
    match (width, signed) {
        (8, false) => Some((0, u8::MAX as i64)),
        (8, true) => Some((i8::MIN as i64, i8::MAX as i64)),
        (16, false) => Some((0, u16::MAX as i64)),
        (16, true) => Some((i16::MIN as i64, i16::MAX as i64)),
        (32, false) => Some((0, u32::MAX as i64)),
        (32, true) => Some((i32::MIN as i64, i32::MAX as i64)),
        // 64-bit and 128-bit ranges exceed (or fill) i64; leave unconstrained.
        _ => None,
    }
}

fn is_map_encoding(fields: &[(String, TypeInfo)]) -> bool {
    matches!(
        fields,
        [key, value] if key.0 == "_key" && value.0 == "_value"
    )
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

    /// Bare, width-unspecified `TypeInfo::Int` for tests that don't care about
    /// integer range.
    fn int() -> TypeInfo {
        TypeInfo::Int {
            int_width: None,
            int_signed: None,
        }
    }

    #[test]
    fn primitive_types_round_trip() {
        round_trip(&int());
        round_trip(&TypeInfo::Float);
        round_trip(&TypeInfo::Str);
        round_trip(&TypeInfo::Bool);
        round_trip(&TypeInfo::Unknown);
    }

    #[test]
    fn array_type_round_trips() {
        round_trip(&TypeInfo::Array {
            element: Box::new(int()),
        });
    }

    #[test]
    fn object_type_round_trips() {
        round_trip(&TypeInfo::Object {
            fields: vec![("name".into(), TypeInfo::Str), ("age".into(), int())],
        });
    }

    #[test]
    fn union_type_round_trips() {
        round_trip(&TypeInfo::Union {
            variants: vec![TypeInfo::Str, int()],
        });
    }

    #[test]
    fn sized_int_round_trips() {
        round_trip(&TypeInfo::Int {
            int_width: Some(8),
            int_signed: Some(false),
        });
        round_trip(&TypeInfo::Int {
            int_width: Some(32),
            int_signed: Some(true),
        });
    }

    #[test]
    fn bare_int_deserializes_to_unspecified() {
        let parsed: TypeInfo = serde_json::from_str(r#"{"kind":"int"}"#).expect("deserialize");
        assert_eq!(parsed, int());
    }

    #[test]
    fn int_range_fits_in_i64_or_none() {
        assert_eq!(int_range(Some(8), Some(false)), Some((0, 255)));
        assert_eq!(int_range(Some(8), Some(true)), Some((-128, 127)));
        assert_eq!(int_range(Some(32), Some(false)), Some((0, u32::MAX as i64)));
        assert_eq!(
            int_range(Some(32), Some(true)),
            Some((i32::MIN as i64, i32::MAX as i64))
        );
        // 64/128-bit and unsized stay unconstrained.
        assert_eq!(int_range(Some(64), Some(false)), None);
        assert_eq!(int_range(Some(128), Some(true)), None);
        assert_eq!(int_range(None, None), None);
        // Method form mirrors the free function.
        assert_eq!(
            TypeInfo::Int {
                int_width: Some(8),
                int_signed: Some(false)
            }
            .int_range(),
            Some((0, 255))
        );
        assert_eq!(TypeInfo::Str.int_range(), None);
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
                (
                    "items".into(),
                    TypeInfo::Array {
                        element: Box::new(TypeInfo::Object {
                            fields: vec![
                                ("id".into(), int()),
                                (
                                    "label".into(),
                                    TypeInfo::Nullable {
                                        inner: Box::new(TypeInfo::Str),
                                    },
                                ),
                            ],
                        }),
                    },
                ),
                ("count".into(), int()),
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
                    (
                        "items".into(),
                        TypeInfo::Array {
                            element: Box::new(int()),
                        },
                    ),
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
        metadata.insert(
            "class".into(),
            serde_json::Value::String("TypeError".into()),
        );
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
            inner: Some(Box::new(int())),
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
            static_opacity: None,
            medium_opacity: None,
        });
    }

    #[test]
    fn opaque_with_static_opacity_round_trips() {
        round_trip(&TypeInfo::Opaque {
            label: "AbstractService".to_string(),
            static_opacity: Some(StaticOpacityReason::AbstractType),
            medium_opacity: None,
        });
        round_trip(&TypeInfo::Opaque {
            label: "DataSource".to_string(),
            static_opacity: Some(StaticOpacityReason::NoImplementors),
            medium_opacity: None,
        });
        round_trip(&TypeInfo::Opaque {
            label: "InternalHandle".to_string(),
            static_opacity: Some(StaticOpacityReason::NoConstructor),
            medium_opacity: None,
        });
        round_trip(&TypeInfo::Opaque {
            label: "SocketWrapper".to_string(),
            static_opacity: Some(StaticOpacityReason::TransitivelyOpaque),
            medium_opacity: None,
        });
    }

    #[test]
    fn opaque_with_medium_opacity_round_trips() {
        use super::MediumOpacityReason;
        round_trip(&TypeInfo::Opaque {
            label: "pg.Client".to_string(),
            static_opacity: None,
            medium_opacity: Some(MediumOpacityReason::InfrastructurePackage),
        });
        round_trip(&TypeInfo::Opaque {
            label: "MyResource".to_string(),
            static_opacity: None,
            medium_opacity: Some(MediumOpacityReason::CloseableInterface),
        });
        round_trip(&TypeInfo::Opaque {
            label: "FdHandle".to_string(),
            static_opacity: None,
            medium_opacity: Some(MediumOpacityReason::NativeHandleField),
        });
    }

    #[test]
    fn has_opaque_direct() {
        assert!(
            TypeInfo::Opaque {
                label: "net.Socket".into(),
                static_opacity: None,
                medium_opacity: None
            }
            .has_opaque()
        );
    }

    #[test]
    fn has_opaque_nested_in_array() {
        let typ = TypeInfo::Array {
            element: Box::new(TypeInfo::Opaque {
                label: "net.Socket".into(),
                static_opacity: None,
                medium_opacity: None,
            }),
        };
        assert!(typ.has_opaque());
    }

    #[test]
    fn has_opaque_nested_in_object() {
        let typ = TypeInfo::Object {
            fields: vec![
                (
                    "conn".into(),
                    TypeInfo::Opaque {
                        label: "pg.Client".into(),
                        static_opacity: None,
                        medium_opacity: None,
                    },
                ),
                ("name".into(), TypeInfo::Str),
            ],
        };
        assert!(typ.has_opaque());
    }

    #[test]
    fn has_opaque_nested_in_nullable() {
        let typ = TypeInfo::Nullable {
            inner: Box::new(TypeInfo::Opaque {
                label: "channel".into(),
                static_opacity: None,
                medium_opacity: None,
            }),
        };
        assert!(typ.has_opaque());
    }

    #[test]
    fn has_opaque_false_for_all_primitive_tree() {
        let typ = TypeInfo::Object {
            fields: vec![
                (
                    "items".into(),
                    TypeInfo::Array {
                        element: Box::new(int()),
                    },
                ),
                (
                    "label".into(),
                    TypeInfo::Nullable {
                        inner: Box::new(TypeInfo::Str),
                    },
                ),
                ("flag".into(), TypeInfo::Bool),
            ],
        };
        assert!(!typ.has_opaque());
    }

    #[test]
    fn has_opaque_false_for_primitives() {
        assert!(!int().has_opaque());
        assert!(!TypeInfo::Float.has_opaque());
        assert!(!TypeInfo::Str.has_opaque());
        assert!(!TypeInfo::Bool.has_opaque());
        assert!(!TypeInfo::Unknown.has_opaque());
    }

    // ── find_opaque_node tests ──

    #[test]
    fn find_opaque_node_returns_label_and_none_for_plain_opaque() {
        let typ = TypeInfo::Opaque {
            label: "net.Socket".into(),
            static_opacity: None,
            medium_opacity: None,
        };
        let mut path = vec![PathSegment::Param("sock".into())];
        let result = typ.find_opaque_node(&mut path);
        assert_eq!(result, Some(("net.Socket".to_string(), None, None)));
        assert_eq!(path, vec![PathSegment::Param("sock".into())]);
    }

    #[test]
    fn find_opaque_node_returns_static_reason_when_present() {
        let typ = TypeInfo::Opaque {
            label: "AbstractService".into(),
            static_opacity: Some(StaticOpacityReason::AbstractType),
            medium_opacity: None,
        };
        let mut path = vec![PathSegment::Param("svc".into())];
        let result = typ.find_opaque_node(&mut path);
        assert_eq!(
            result,
            Some((
                "AbstractService".to_string(),
                Some(StaticOpacityReason::AbstractType),
                None
            ))
        );
    }

    #[test]
    fn find_opaque_node_returns_medium_reason_when_present() {
        let typ = TypeInfo::Opaque {
            label: "redis.Client".into(),
            static_opacity: None,
            medium_opacity: Some(MediumOpacityReason::InfrastructurePackage),
        };
        let mut path = vec![PathSegment::Param("cache".into())];
        let result = typ.find_opaque_node(&mut path);
        assert_eq!(
            result,
            Some((
                "redis.Client".to_string(),
                None,
                Some(MediumOpacityReason::InfrastructurePackage)
            ))
        );
    }

    #[test]
    fn find_opaque_node_traverses_nested_object() {
        let typ = TypeInfo::Object {
            fields: vec![
                ("name".into(), TypeInfo::Str),
                (
                    "svc".into(),
                    TypeInfo::Opaque {
                        label: "DataSource".into(),
                        static_opacity: Some(StaticOpacityReason::NoImplementors),
                        medium_opacity: None,
                    },
                ),
            ],
        };
        let mut path = vec![PathSegment::Param("cfg".into())];
        let result = typ.find_opaque_node(&mut path);
        assert_eq!(
            result,
            Some((
                "DataSource".to_string(),
                Some(StaticOpacityReason::NoImplementors),
                None
            ))
        );
        assert_eq!(
            path,
            vec![
                PathSegment::Param("cfg".into()),
                PathSegment::Field("svc".into()),
            ]
        );
    }

    #[test]
    fn find_opaque_node_ignores_map_value_shape() {
        let typ = TypeInfo::Object {
            fields: vec![
                ("_key".into(), TypeInfo::Str),
                (
                    "_value".into(),
                    TypeInfo::Opaque {
                        label: "interface".into(),
                        static_opacity: None,
                        medium_opacity: None,
                    },
                ),
            ],
        };
        let mut path = vec![PathSegment::Param("m".into())];
        assert!(typ.find_opaque_node(&mut path).is_none());
        assert_eq!(path, vec![PathSegment::Param("m".into())]);
    }

    #[test]
    fn find_opaque_node_returns_none_for_primitives() {
        let mut path = vec![PathSegment::Param("x".into())];
        assert!(int().find_opaque_node(&mut path).is_none());
        assert!(TypeInfo::Str.find_opaque_node(&mut path).is_none());
        assert_eq!(path.len(), 1, "path should be unmodified on no-match");
    }
}
