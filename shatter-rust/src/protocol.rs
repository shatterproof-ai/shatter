//! Protocol types for the Shatter Rust frontend.
//!
//! These types match the JSON wire format defined in `shatter-core/src/protocol.rs`.
//! The protocol uses newline-delimited JSON (NDJSON) over stdin/stdout between
//! the core engine and this frontend.
//!
//! Like the Go frontend, we use flat structs with optional fields rather than
//! tagged enums — simpler for a standalone frontend that only needs to parse
//! requests and emit responses.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Well-known complex types beyond primitives and structural types.
/// Matches `ComplexKind` in shatter-core/src/types.rs.
#[allow(dead_code)] // used once analyze/execute are implemented
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComplexKind {
    Date, DateTime, Time, Duration,
    RegExp, Char, Symbol,
    BigInt, BigDecimal, Complex, Rational, Range,
    Buffer, BitSet,
    Error, Option, Result,
    Closure, Iterator,
    Url, IpAddress,
    Uuid,
    Path,
    Money, SemVer, Email, MimeType, Color, GeoPoint, Locale,
    Rune, GoByte,
}

/// Describes the type of a value, as reported by a language frontend.
/// Matches `TypeInfo` in shatter-core/src/types.rs.
#[allow(dead_code)] // used once analyze/execute are implemented
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TypeInfo {
    Int,
    Float,
    Str,
    Bool,
    Array { element: Box<TypeInfo> },
    Object { fields: Vec<(String, TypeInfo)> },
    Union { variants: Vec<TypeInfo> },
    Nullable { inner: Box<TypeInfo> },
    Complex {
        #[serde(rename = "complex_kind")]
        kind: ComplexKind,
        #[serde(default)]
        metadata: HashMap<String, serde_json::Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        inner: Option<Box<TypeInfo>>,
    },
    Opaque {
        label: String,
    },
    Unknown,
}

/// Current protocol version.
pub const PROTOCOL_VERSION: &str = "0.1.0";

/// Frontend version.
pub const FRONTEND_VERSION: &str = "0.1.0";

/// Language identifier for this frontend.
pub const FRONTEND_LANGUAGE: &str = "rust";

/// A request message from the core engine to this frontend.
#[derive(Debug, Clone, Deserialize)]
pub struct Request {
    pub protocol_version: String,
    pub id: u64,
    pub command: String,

    // Handshake fields
    #[allow(dead_code)] // will be used when capability negotiation is implemented
    #[serde(default)]
    pub capabilities: Vec<String>,

    // Analyze/Instrument fields
    #[serde(default)]
    pub file: Option<String>,
    #[allow(dead_code)] // will be used when analyze is implemented
    #[serde(default)]
    pub function: Option<String>,

    // Execute fields
    #[allow(dead_code)] // will be used when execute is implemented
    #[serde(default)]
    pub inputs: Vec<serde_json::Value>,
    #[allow(dead_code)] // will be used when execute/instrument is implemented
    #[serde(default)]
    pub mocks: Vec<serde_json::Value>,
    /// Opaque context returned by a prior Setup command, if any.
    #[allow(dead_code)] // will be used when execute is implemented
    #[serde(default)]
    pub setup_context: Option<serde_json::Value>,

    // Setup fields
    /// When to run setup relative to executions ("per_function" or "per_execution").
    #[allow(dead_code)] // will be used when setup is implemented
    #[serde(default)]
    pub mode: Option<String>,

    // Generate fields
    /// Name of the type or parameter to generate a value for.
    #[allow(dead_code)] // will be used when generate is implemented
    #[serde(default)]
    pub name: Option<String>,
    /// Whether the generator targets a type name or a parameter name.
    #[allow(dead_code)] // will be used when generate is implemented
    #[serde(default)]
    pub kind: Option<String>,
}

/// A response message from this frontend to the core engine.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Response {
    pub protocol_version: String,
    pub id: u64,
    pub status: String,

    // Handshake fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frontend_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<Vec<String>>,

    // Setup fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub setup_context: Option<serde_json::Value>,

    // Generate fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,

    // Error fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl Response {
    /// Create a base response with protocol version and request ID.
    pub fn base(id: u64) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION.to_string(),
            id,
            status: String::new(),
            frontend_version: None,
            language: None,
            capabilities: None,
            setup_context: None,
            value: None,
            code: None,
            message: None,
        }
    }
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
    fn typeinfo_opaque_round_trips() {
        round_trip(&TypeInfo::Opaque {
            label: "net.Socket".to_string(),
        });
    }

    #[test]
    fn typeinfo_opaque_serializes_with_correct_kind() {
        let ti = TypeInfo::Opaque {
            label: "fs.FileHandle".to_string(),
        };
        let json: serde_json::Value = serde_json::to_value(&ti).expect("serialize");
        assert_eq!(json["kind"], "opaque");
        assert_eq!(json["label"], "fs.FileHandle");
    }

    #[test]
    fn typeinfo_opaque_deserializes_from_json() {
        let json = r#"{"kind":"opaque","label":"pg.Client"}"#;
        let ti: TypeInfo = serde_json::from_str(json).expect("deserialize");
        assert_eq!(
            ti,
            TypeInfo::Opaque {
                label: "pg.Client".to_string(),
            }
        );
    }

    #[test]
    fn typeinfo_opaque_inside_array_round_trips() {
        round_trip(&TypeInfo::Array {
            element: Box::new(TypeInfo::Opaque {
                label: "stream.Readable".to_string(),
            }),
        });
    }

    #[test]
    fn typeinfo_opaque_inside_nullable_round_trips() {
        round_trip(&TypeInfo::Nullable {
            inner: Box::new(TypeInfo::Opaque {
                label: "channel".to_string(),
            }),
        });
    }

    #[test]
    fn typeinfo_opaque_inside_object_round_trips() {
        round_trip(&TypeInfo::Object {
            fields: vec![
                (
                    "conn".into(),
                    TypeInfo::Opaque {
                        label: "pg.Client".to_string(),
                    },
                ),
                ("name".into(), TypeInfo::Str),
            ],
        });
    }

    #[test]
    fn existing_typeinfo_variants_still_round_trip() {
        round_trip(&TypeInfo::Int);
        round_trip(&TypeInfo::Float);
        round_trip(&TypeInfo::Str);
        round_trip(&TypeInfo::Bool);
        round_trip(&TypeInfo::Unknown);
        round_trip(&TypeInfo::Array {
            element: Box::new(TypeInfo::Int),
        });
        round_trip(&TypeInfo::Nullable {
            inner: Box::new(TypeInfo::Str),
        });
        round_trip(&TypeInfo::Object {
            fields: vec![("x".into(), TypeInfo::Int)],
        });
        round_trip(&TypeInfo::Union {
            variants: vec![TypeInfo::Str, TypeInfo::Int],
        });
    }

    #[test]
    fn typeinfo_complex_still_round_trips() {
        round_trip(&TypeInfo::Complex {
            kind: ComplexKind::Date,
            metadata: HashMap::new(),
            inner: None,
        });
    }

    #[test]
    fn opaque_in_function_analysis_json_deserializes() {
        // Verify TypeInfo::Opaque works when embedded in a FunctionAnalysis-shaped JSON,
        // parsed as a generic Value and then extracting the type field.
        let json = r#"{"kind": "opaque", "label": "stream.Readable"}"#;
        let param_type: TypeInfo = serde_json::from_str(json).expect("deserialize param type");
        assert_eq!(
            param_type,
            TypeInfo::Opaque {
                label: "stream.Readable".to_string(),
            }
        );

        // Nested inside an object field (simulating a return_type in analysis results)
        let nested_json = r#"{"kind": "object", "fields": [["conn", {"kind": "opaque", "label": "pg.Client"}], ["ready", {"kind": "bool"}]]}"#;
        let nested: TypeInfo = serde_json::from_str(nested_json).expect("deserialize nested");
        if let TypeInfo::Object { fields } = &nested {
            assert_eq!(fields.len(), 2);
            assert_eq!(
                fields[0].1,
                TypeInfo::Opaque {
                    label: "pg.Client".to_string(),
                }
            );
        } else {
            panic!("expected Object, got {:?}", nested);
        }
    }

    // -- Request deserialization tests for new commands --

    #[test]
    fn setup_request_deserializes() {
        let json = r#"{"protocol_version":"0.1.0","id":20,"command":"setup","file":"./setup.ts","function":"processOrder","mode":"per_function"}"#;
        let req: Request = serde_json::from_str(json).expect("deserialize setup");
        assert_eq!(req.id, 20);
        assert_eq!(req.command, "setup");
        assert_eq!(req.file.as_deref(), Some("./setup.ts"));
        assert_eq!(req.function.as_deref(), Some("processOrder"));
        assert_eq!(req.mode.as_deref(), Some("per_function"));
    }

    #[test]
    fn setup_request_per_execution_deserializes() {
        let json = r#"{"protocol_version":"0.1.0","id":21,"command":"setup","file":"./setup.ts","function":"auth","mode":"per_execution"}"#;
        let req: Request = serde_json::from_str(json).expect("deserialize setup per_execution");
        assert_eq!(req.mode.as_deref(), Some("per_execution"));
    }

    #[test]
    fn teardown_request_deserializes() {
        let json = r#"{"protocol_version":"0.1.0","id":22,"command":"teardown","function":"processOrder"}"#;
        let req: Request = serde_json::from_str(json).expect("deserialize teardown");
        assert_eq!(req.id, 22);
        assert_eq!(req.command, "teardown");
        assert_eq!(req.function.as_deref(), Some("processOrder"));
    }

    #[test]
    fn generate_request_type_name_deserializes() {
        let json = r#"{"protocol_version":"0.1.0","id":23,"command":"generate","file":"./gen.ts","name":"User","kind":"type_name"}"#;
        let req: Request = serde_json::from_str(json).expect("deserialize generate type_name");
        assert_eq!(req.id, 23);
        assert_eq!(req.command, "generate");
        assert_eq!(req.file.as_deref(), Some("./gen.ts"));
        assert_eq!(req.name.as_deref(), Some("User"));
        assert_eq!(req.kind.as_deref(), Some("type_name"));
    }

    #[test]
    fn generate_request_param_name_deserializes() {
        let json = r#"{"protocol_version":"0.1.0","id":24,"command":"generate","file":"./gen.ts","name":"authToken","kind":"param_name"}"#;
        let req: Request = serde_json::from_str(json).expect("deserialize generate param_name");
        assert_eq!(req.kind.as_deref(), Some("param_name"));
    }

    #[test]
    fn execute_request_with_setup_context_deserializes() {
        let json = r#"{"protocol_version":"0.1.0","id":25,"command":"execute","function":"fn1","inputs":[1],"mocks":[],"setup_context":{"db":"conn_42"}}"#;
        let req: Request = serde_json::from_str(json).expect("deserialize execute with setup_context");
        assert_eq!(req.setup_context, Some(serde_json::json!({"db": "conn_42"})));
    }

    #[test]
    fn execute_request_without_setup_context_defaults_to_none() {
        let json = r#"{"protocol_version":"0.1.0","id":26,"command":"execute","function":"fn1","inputs":[],"mocks":[]}"#;
        let req: Request = serde_json::from_str(json).expect("deserialize execute without setup_context");
        assert_eq!(req.setup_context, None);
    }

    // -- Response round-trip tests for new statuses --

    #[test]
    fn setup_response_round_trips() {
        let resp = Response {
            protocol_version: PROTOCOL_VERSION.to_string(),
            id: 20,
            status: "setup".to_string(),
            frontend_version: None,
            language: None,
            capabilities: None,
            setup_context: Some(serde_json::json!({"db_handle": "conn_42"})),
            value: None,
            code: None,
            message: None,
        };
        round_trip(&resp);
    }

    #[test]
    fn teardown_ack_response_round_trips() {
        let resp = Response {
            protocol_version: PROTOCOL_VERSION.to_string(),
            id: 21,
            status: "teardown_ack".to_string(),
            frontend_version: None,
            language: None,
            capabilities: None,
            setup_context: None,
            value: None,
            code: None,
            message: None,
        };
        round_trip(&resp);
    }

    #[test]
    fn generate_response_round_trips() {
        let resp = Response {
            protocol_version: PROTOCOL_VERSION.to_string(),
            id: 22,
            status: "generate".to_string(),
            frontend_version: None,
            language: None,
            capabilities: None,
            setup_context: None,
            value: Some(serde_json::json!({"id": 1, "name": "Alice"})),
            code: None,
            message: None,
        };
        round_trip(&resp);
    }

    #[test]
    fn generate_response_primitive_value_round_trips() {
        let resp = Response {
            protocol_version: PROTOCOL_VERSION.to_string(),
            id: 23,
            status: "generate".to_string(),
            frontend_version: None,
            language: None,
            capabilities: None,
            setup_context: None,
            value: Some(serde_json::json!("tok_abc123")),
            code: None,
            message: None,
        };
        round_trip(&resp);
    }

    #[test]
    fn error_response_still_round_trips() {
        let resp = Response {
            protocol_version: PROTOCOL_VERSION.to_string(),
            id: 99,
            status: "error".to_string(),
            frontend_version: None,
            language: None,
            capabilities: None,
            setup_context: None,
            value: None,
            code: Some("internal_error".to_string()),
            message: Some("something broke".to_string()),
        };
        round_trip(&resp);
    }

    #[test]
    fn handshake_response_still_round_trips() {
        let resp = Response {
            protocol_version: PROTOCOL_VERSION.to_string(),
            id: 1,
            status: "handshake".to_string(),
            frontend_version: Some(FRONTEND_VERSION.to_string()),
            language: Some(FRONTEND_LANGUAGE.to_string()),
            capabilities: Some(vec!["analyze".to_string()]),
            setup_context: None,
            value: None,
            code: None,
            message: None,
        };
        round_trip(&resp);
    }
}
