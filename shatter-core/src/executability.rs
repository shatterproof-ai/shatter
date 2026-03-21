//! Checks whether a function's parameters contain opaque types,
//! making it unexecutable for automated testing.

use crate::config::CustomOpaqueType;
use crate::types::ParamInfo;
use serde::{Deserialize, Serialize};

/// Categorizes WHY a type is opaque — what kind of runtime resource it represents.
///
/// This lets CLI output explain not just *what* was detected but *why* the type
/// cannot be automatically constructed.
// TODO(str-asnl): HTML expandable paths, JSON structured skip_reasons, frontend category tags
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OpaqueCategory {
    /// Network socket or connection (e.g. net.Socket, net.Conn).
    NetworkHandle,
    /// OS file descriptor, pipe, or byte stream (e.g. stream.Readable, io.Reader, os.File).
    IoStream,
    /// Live database connection or connection pool (e.g. pg.Client, database/sql.DB).
    DatabaseConnection,
    /// Runtime concurrency primitive with scheduler state (e.g. chan T, Worker).
    ConcurrencyPrimitive,
    /// Wrapped OS process handle (e.g. child_process.ChildProcess).
    ProcessHandle,
    /// User-configured opaque type (from `.shatter/config.yaml` `opaque_types`).
    UserConfigured,
    /// Opaque type that doesn't match any known category.
    Unknown,
}

impl OpaqueCategory {
    /// Short human label used in CLI output (e.g. "network handle").
    pub fn label(&self) -> &'static str {
        match self {
            OpaqueCategory::NetworkHandle => "network handle",
            OpaqueCategory::IoStream => "I/O stream",
            OpaqueCategory::DatabaseConnection => "database connection",
            OpaqueCategory::ConcurrencyPrimitive => "concurrency primitive",
            OpaqueCategory::ProcessHandle => "process handle",
            OpaqueCategory::UserConfigured => "user-configured opaque type",
            OpaqueCategory::Unknown => "opaque type",
        }
    }

    /// One-sentence explanation of why this category cannot be constructed automatically.
    pub fn reason(&self) -> &'static str {
        match self {
            OpaqueCategory::NetworkHandle => "requires live network binding",
            OpaqueCategory::IoStream => "wraps OS file descriptor or pipe",
            OpaqueCategory::DatabaseConnection => "requires live database connection",
            OpaqueCategory::ConcurrencyPrimitive => "runtime scheduling state",
            OpaqueCategory::ProcessHandle => "wraps OS process",
            OpaqueCategory::UserConfigured => "marked as non-synthesizable",
            OpaqueCategory::Unknown => "type cannot be automatically synthesized",
        }
    }
}

/// Classifies an opaque label into an [`OpaqueCategory`] based on well-known
/// type names from Node.js and Go standard libraries.
pub fn category_for_label(label: &str) -> OpaqueCategory {
    // Network handles: sockets, listeners, TLS connections
    match label {
        "net.Socket"
        | "net.Server"
        | "net.Conn"
        | "net.Listener"
        | "net.PacketConn"
        | "tls.TLSSocket"
        | "tls.Server"
        | "dgram.Socket"
        | "http.Server" => return OpaqueCategory::NetworkHandle,
        _ => {}
    }
    // I/O streams and file handles
    match label {
        "stream.Readable"
        | "stream.Writable"
        | "stream.Transform"
        | "stream.Duplex"
        | "stream.PassThrough"
        | "fs.ReadStream"
        | "fs.WriteStream"
        | "os.File"
        | "io.Reader"
        | "io.Writer"
        | "io.ReadWriter"
        | "io.Closer"
        | "io.ReadCloser"
        | "io.WriteCloser"
        | "http.IncomingMessage"
        | "http.ResponseWriter"
        | "http.ServerResponse" => return OpaqueCategory::IoStream,
        _ => {}
    }
    // Database connections and pools
    match label {
        "pg.Client" | "pg.Pool" | "sql.DB" | "sql.Tx" | "sql.Rows" => {
            return OpaqueCategory::DatabaseConnection;
        }
        _ => {}
    }
    if label.starts_with("database/sql.") {
        return OpaqueCategory::DatabaseConnection;
    }
    // Concurrency primitives: Go channels and worker threads
    if label.starts_with("chan ") || label == "worker_threads.Worker" {
        return OpaqueCategory::ConcurrencyPrimitive;
    }
    // Process handles
    if label == "child_process.ChildProcess" {
        return OpaqueCategory::ProcessHandle;
    }
    OpaqueCategory::Unknown
}

/// One segment of the nesting path from a parameter root to the opaque type node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PathSegment {
    /// The parameter itself (always the first segment).
    Param(String),
    /// Object field access.
    Field(String),
    /// Array element.
    ArrayElement,
    /// Map/dictionary value.
    MapValue,
    /// Union variant (one of several possible types).
    UnionVariant,
    /// Nullable inner type.
    NullableInner,
    /// Complex wrapper inner type (e.g. Option<T>, Result<T,E>).
    ComplexInner,
}

impl PathSegment {
    /// Human-readable display for this segment.
    pub fn display(&self) -> String {
        match self {
            PathSegment::Param(name) => format!("param \"{name}\""),
            PathSegment::Field(name) => format!("field \"{name}\""),
            PathSegment::ArrayElement => "[]".to_string(),
            PathSegment::MapValue => "value".to_string(),
            PathSegment::UnionVariant => "variant".to_string(),
            PathSegment::NullableInner => "inner".to_string(),
            PathSegment::ComplexInner => "inner".to_string(),
        }
    }
}

/// Formats a nesting path for human display.
///
/// - Depth ≤ 3: shows all segments joined by ` → `
/// - Depth > 3: collapses middle with `...` (shows first 2 and last 1)
///
/// # Examples
/// ```
/// # use shatter_core::executability::{PathSegment, format_nesting_path};
/// let path = vec![PathSegment::Param("config".into()), PathSegment::Field("db".into())];
/// assert_eq!(format_nesting_path(&path), r#"param "config" → field "db""#);
/// ```
pub fn format_nesting_path(path: &[PathSegment]) -> String {
    const MAX_FULL: usize = 3;
    if path.len() <= MAX_FULL {
        path.iter().map(PathSegment::display).collect::<Vec<_>>().join(" → ")
    } else {
        // Show first 2 segments, ..., last 1 segment
        let mut parts: Vec<String> = path[..2].iter().map(PathSegment::display).collect();
        parts.push("...".to_string());
        parts.push(path[path.len() - 1].display());
        parts.join(" → ")
    }
}

/// Reason why a function parameter cannot be automatically tested.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkipReason {
    /// Name of the parameter containing the opaque type.
    pub param_name: String,
    /// Label of the opaque type found (e.g. "net.Socket", "pg.Client").
    pub opaque_label: String,
    /// Categorized explanation of why this type is opaque.
    pub category: OpaqueCategory,
    /// Full nesting path from the parameter root to the opaque node.
    ///
    /// For a direct opaque param: `[Param("sock")]`
    /// For a nested field: `[Param("config"), Field("db")]`
    pub nesting_path: Vec<PathSegment>,
    /// Custom reason text from user config (only set for [`OpaqueCategory::UserConfigured`]
    /// entries that have an explicit `reason` field).
    pub user_reason: Option<String>,
}

impl SkipReason {
    /// Formats a human-readable one-line description.
    ///
    /// Format: `<path> → <label> (<category label> — <reason>)`
    ///
    /// # Examples
    /// - `param "sock" → net.Socket (network handle — requires live network binding)`
    /// - `param "config" → field "db" → pg.Client (database connection — requires live database connection)`
    pub fn format_human(&self) -> String {
        let path = format_nesting_path(&self.nesting_path);
        let reason_text = self
            .user_reason
            .as_deref()
            .unwrap_or_else(|| self.category.reason());
        format!(
            "{path} → {} ({} — {reason_text})",
            self.opaque_label,
            self.category.label()
        )
    }
}

/// Checks each parameter for opaque types. Returns a `SkipReason` for every
/// parameter whose type tree contains an `Opaque` node or whose `type_name`
/// matches an entry in `custom_opaque_types`.
#[must_use]
pub fn check_executability(
    params: &[ParamInfo],
    custom_opaque_types: &[CustomOpaqueType],
) -> Vec<SkipReason> {
    params
        .iter()
        .filter_map(|p| {
            // Check built-in opaque detection first.
            let mut path = vec![PathSegment::Param(p.name.clone())];
            if let Some(label) = p.typ.find_opaque_info(&mut path) {
                let category = category_for_label(&label);
                return Some(SkipReason {
                    param_name: p.name.clone(),
                    opaque_label: label,
                    category,
                    nesting_path: path,
                    user_reason: None,
                });
            }
            // Check user-configured opaque types by type_name.
            if let Some(ref tn) = p.type_name
                && let Some(entry) = custom_opaque_types.iter().find(|o| o.name() == tn)
            {
                return Some(SkipReason {
                    param_name: p.name.clone(),
                    opaque_label: tn.clone(),
                    category: OpaqueCategory::UserConfigured,
                    nesting_path: vec![PathSegment::Param(p.name.clone())],
                    user_reason: entry.reason().map(ToOwned::to_owned),
                });
            }
            None
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

    fn param_with_type_name(name: &str, typ: TypeInfo, type_name: &str) -> ParamInfo {
        ParamInfo {
            name: name.into(),
            typ,
            type_name: Some(type_name.into()),
        }
    }

    fn custom(name: &str) -> CustomOpaqueType {
        CustomOpaqueType::Name(name.to_string())
    }

    fn custom_with_reason(name: &str, reason: &str) -> CustomOpaqueType {
        CustomOpaqueType::Named {
            name: name.to_string(),
            reason: Some(reason.to_string()),
        }
    }

    #[test]
    fn no_opaque_params_returns_empty() {
        let params = vec![param("a", TypeInfo::Int), param("b", TypeInfo::Str)];
        assert!(check_executability(&params, &[]).is_empty());
    }

    #[test]
    fn direct_opaque_param_returns_reason() {
        let params = vec![param(
            "conn",
            TypeInfo::Opaque {
                label: "pg.Client".into(),
            },
        )];
        let reasons = check_executability(&params, &[]);
        assert_eq!(reasons.len(), 1);
        assert_eq!(reasons[0].param_name, "conn");
        assert_eq!(reasons[0].opaque_label, "pg.Client");
        assert_eq!(reasons[0].category, OpaqueCategory::DatabaseConnection);
        assert_eq!(
            reasons[0].nesting_path,
            vec![PathSegment::Param("conn".into())]
        );
        assert_eq!(reasons[0].user_reason, None);
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
        let reasons = check_executability(&params, &[]);
        assert_eq!(reasons.len(), 1);
        assert_eq!(reasons[0].param_name, "sockets");
        assert_eq!(reasons[0].opaque_label, "net.Socket");
        assert_eq!(reasons[0].category, OpaqueCategory::NetworkHandle);
        assert_eq!(
            reasons[0].nesting_path,
            vec![
                PathSegment::Param("sockets".into()),
                PathSegment::ArrayElement
            ]
        );
    }

    #[test]
    fn all_primitive_params_returns_empty() {
        let params = vec![
            param("x", TypeInfo::Int),
            param("y", TypeInfo::Float),
            param("name", TypeInfo::Str),
            param("flag", TypeInfo::Bool),
        ];
        assert!(check_executability(&params, &[]).is_empty());
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
        let reasons = check_executability(&params, &[]);
        assert_eq!(reasons.len(), 2);
        assert_eq!(reasons[0].param_name, "db");
        assert_eq!(reasons[0].opaque_label, "pg.Client");
        assert_eq!(reasons[0].category, OpaqueCategory::DatabaseConnection);
        assert_eq!(reasons[1].param_name, "stream");
        assert_eq!(reasons[1].opaque_label, "fs.ReadStream");
        assert_eq!(reasons[1].category, OpaqueCategory::IoStream);
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
                                label: "http.Server".into(),
                            },
                        ),
                    ],
                }),
            },
        )];
        let reasons = check_executability(&params, &[]);
        assert_eq!(reasons.len(), 1);
        assert_eq!(reasons[0].param_name, "config");
        assert_eq!(reasons[0].opaque_label, "http.Server");
        assert_eq!(reasons[0].category, OpaqueCategory::NetworkHandle);
        assert_eq!(
            reasons[0].nesting_path,
            vec![
                PathSegment::Param("config".into()),
                PathSegment::ArrayElement,
                PathSegment::Field("handler".into()),
            ]
        );
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
        let reasons = check_executability(&params, &[]);
        assert_eq!(reasons.len(), 1);
        assert_eq!(reasons[0].param_name, "input");
        assert_eq!(reasons[0].opaque_label, "stream.Readable");
        assert_eq!(reasons[0].category, OpaqueCategory::IoStream);
        assert_eq!(
            reasons[0].nesting_path,
            vec![
                PathSegment::Param("input".into()),
                PathSegment::UnionVariant,
            ]
        );
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
        let reasons = check_executability(&params, &[]);
        assert_eq!(reasons.len(), 1);
        assert_eq!(reasons[0].param_name, "maybe_conn");
        assert_eq!(reasons[0].opaque_label, "pg.Pool");
        assert_eq!(reasons[0].category, OpaqueCategory::DatabaseConnection);
        assert_eq!(
            reasons[0].nesting_path,
            vec![
                PathSegment::Param("maybe_conn".into()),
                PathSegment::NullableInner,
            ]
        );
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
        let reasons = check_executability(&params, &[]);
        assert_eq!(reasons.len(), 1);
        assert_eq!(reasons[0].param_name, "wrapped");
        assert_eq!(reasons[0].opaque_label, "channel");
        assert_eq!(
            reasons[0].nesting_path,
            vec![
                PathSegment::Param("wrapped".into()),
                PathSegment::ComplexInner,
            ]
        );
    }

    #[test]
    fn empty_params_returns_empty() {
        assert!(check_executability(&[], &[]).is_empty());
    }

    #[test]
    fn custom_opaque_type_matched_by_type_name() {
        let params = vec![param_with_type_name("pool", TypeInfo::Unknown, "DatabasePool")];
        let custom_types = vec![custom("DatabasePool")];
        let reasons = check_executability(&params, &custom_types);
        assert_eq!(reasons.len(), 1);
        assert_eq!(reasons[0].param_name, "pool");
        assert_eq!(reasons[0].opaque_label, "DatabasePool");
        assert_eq!(reasons[0].category, OpaqueCategory::UserConfigured);
        assert_eq!(reasons[0].user_reason, None);
    }

    #[test]
    fn custom_opaque_type_with_reason() {
        let params = vec![param_with_type_name("client", TypeInfo::Unknown, "HttpClient")];
        let custom_types = vec![custom_with_reason("HttpClient", "requires live HTTP connection")];
        let reasons = check_executability(&params, &custom_types);
        assert_eq!(reasons.len(), 1);
        assert_eq!(reasons[0].opaque_label, "HttpClient");
        assert_eq!(reasons[0].category, OpaqueCategory::UserConfigured);
        assert_eq!(
            reasons[0].user_reason,
            Some("requires live HTTP connection".into())
        );
    }

    #[test]
    fn custom_opaque_type_no_match_returns_empty() {
        let params = vec![param_with_type_name("name", TypeInfo::Str, "String")];
        let custom_types = vec![custom("DatabasePool")];
        assert!(check_executability(&params, &custom_types).is_empty());
    }

    #[test]
    fn custom_opaque_types_empty_list_preserves_default_behavior() {
        let params = vec![param("x", TypeInfo::Int)];
        assert!(check_executability(&params, &[]).is_empty());
    }

    #[test]
    fn builtin_opaque_takes_precedence_over_custom() {
        let params = vec![ParamInfo {
            name: "conn".into(),
            typ: TypeInfo::Opaque {
                label: "pg.Client".into(),
            },
            type_name: Some("DatabasePool".into()),
        }];
        let custom_types = vec![custom("DatabasePool")];
        let reasons = check_executability(&params, &custom_types);
        assert_eq!(reasons.len(), 1);
        assert_eq!(reasons[0].opaque_label, "pg.Client");
        assert_eq!(reasons[0].category, OpaqueCategory::DatabaseConnection);
    }

    #[test]
    fn custom_opaque_with_no_type_name_not_matched() {
        let params = vec![param("x", TypeInfo::Unknown)];
        let custom_types = vec![custom("SomeType")];
        assert!(check_executability(&params, &custom_types).is_empty());
    }

    #[test]
    fn multiple_custom_opaque_types_matched() {
        let params = vec![
            param_with_type_name("pool", TypeInfo::Unknown, "DatabasePool"),
            param("name", TypeInfo::Str),
            param_with_type_name("cache", TypeInfo::Unknown, "RedisClient"),
        ];
        let custom_types = vec![custom("DatabasePool"), custom("RedisClient")];
        let reasons = check_executability(&params, &custom_types);
        assert_eq!(reasons.len(), 2);
        assert_eq!(reasons[0].param_name, "pool");
        assert_eq!(reasons[0].opaque_label, "DatabasePool");
        assert_eq!(reasons[1].param_name, "cache");
        assert_eq!(reasons[1].opaque_label, "RedisClient");
    }

    // ── OpaqueCategory tests ──

    #[test]
    fn category_for_label_network_handles() {
        assert_eq!(category_for_label("net.Socket"), OpaqueCategory::NetworkHandle);
        assert_eq!(category_for_label("net.Conn"), OpaqueCategory::NetworkHandle);
        assert_eq!(category_for_label("tls.TLSSocket"), OpaqueCategory::NetworkHandle);
        assert_eq!(category_for_label("dgram.Socket"), OpaqueCategory::NetworkHandle);
        assert_eq!(category_for_label("http.Server"), OpaqueCategory::NetworkHandle);
    }

    #[test]
    fn category_for_label_io_streams() {
        assert_eq!(category_for_label("stream.Readable"), OpaqueCategory::IoStream);
        assert_eq!(category_for_label("stream.Writable"), OpaqueCategory::IoStream);
        assert_eq!(category_for_label("fs.ReadStream"), OpaqueCategory::IoStream);
        assert_eq!(category_for_label("os.File"), OpaqueCategory::IoStream);
        assert_eq!(category_for_label("io.Reader"), OpaqueCategory::IoStream);
        assert_eq!(category_for_label("io.ReadCloser"), OpaqueCategory::IoStream);
    }

    #[test]
    fn category_for_label_database_connections() {
        assert_eq!(category_for_label("pg.Client"), OpaqueCategory::DatabaseConnection);
        assert_eq!(category_for_label("pg.Pool"), OpaqueCategory::DatabaseConnection);
        assert_eq!(category_for_label("sql.DB"), OpaqueCategory::DatabaseConnection);
        assert_eq!(
            category_for_label("database/sql.DB"),
            OpaqueCategory::DatabaseConnection
        );
    }

    #[test]
    fn category_for_label_concurrency_primitives() {
        assert_eq!(
            category_for_label("chan int"),
            OpaqueCategory::ConcurrencyPrimitive
        );
        assert_eq!(
            category_for_label("chan github.com/my/pkg.Msg"),
            OpaqueCategory::ConcurrencyPrimitive
        );
        assert_eq!(
            category_for_label("worker_threads.Worker"),
            OpaqueCategory::ConcurrencyPrimitive
        );
    }

    #[test]
    fn category_for_label_process_handles() {
        assert_eq!(
            category_for_label("child_process.ChildProcess"),
            OpaqueCategory::ProcessHandle
        );
    }

    #[test]
    fn category_for_label_unknown_falls_back_to_unknown() {
        assert_eq!(category_for_label("some.UnknownType"), OpaqueCategory::Unknown);
        assert_eq!(category_for_label("grpc.Client"), OpaqueCategory::Unknown);
        assert_eq!(category_for_label("channel"), OpaqueCategory::Unknown);
    }

    // ── format_nesting_path tests ──

    #[test]
    fn format_nesting_path_single_param() {
        let path = vec![PathSegment::Param("sock".into())];
        assert_eq!(format_nesting_path(&path), r#"param "sock""#);
    }

    #[test]
    fn format_nesting_path_two_segments() {
        let path = vec![
            PathSegment::Param("config".into()),
            PathSegment::Field("db".into()),
        ];
        assert_eq!(
            format_nesting_path(&path),
            r#"param "config" → field "db""#
        );
    }

    #[test]
    fn format_nesting_path_three_segments_no_collapse() {
        let path = vec![
            PathSegment::Param("config".into()),
            PathSegment::Field("db".into()),
            PathSegment::Field("conn".into()),
        ];
        assert_eq!(
            format_nesting_path(&path),
            r#"param "config" → field "db" → field "conn""#
        );
    }

    #[test]
    fn format_nesting_path_deep_collapses_middle() {
        let path = vec![
            PathSegment::Param("opts".into()),
            PathSegment::Field("middleware".into()),
            PathSegment::ArrayElement,
            PathSegment::Field("handler".into()),
        ];
        // 4 segments → collapse: first 2, ..., last 1
        assert_eq!(
            format_nesting_path(&path),
            r#"param "opts" → field "middleware" → ... → field "handler""#
        );
    }

    // ── SkipReason::format_human tests ──

    #[test]
    fn format_human_network_handle() {
        let reason = SkipReason {
            param_name: "sock".into(),
            opaque_label: "net.Socket".into(),
            category: OpaqueCategory::NetworkHandle,
            nesting_path: vec![PathSegment::Param("sock".into())],
            user_reason: None,
        };
        assert_eq!(
            reason.format_human(),
            r#"param "sock" → net.Socket (network handle — requires live network binding)"#
        );
    }

    #[test]
    fn format_human_nested_field() {
        let reason = SkipReason {
            param_name: "config".into(),
            opaque_label: "pg.Client".into(),
            category: OpaqueCategory::DatabaseConnection,
            nesting_path: vec![
                PathSegment::Param("config".into()),
                PathSegment::Field("db".into()),
            ],
            user_reason: None,
        };
        assert_eq!(
            reason.format_human(),
            r#"param "config" → field "db" → pg.Client (database connection — requires live database connection)"#
        );
    }

    #[test]
    fn format_human_user_configured_with_reason() {
        let reason = SkipReason {
            param_name: "client".into(),
            opaque_label: "HttpClient".into(),
            category: OpaqueCategory::UserConfigured,
            nesting_path: vec![PathSegment::Param("client".into())],
            user_reason: Some("requires live HTTP connection".into()),
        };
        assert_eq!(
            reason.format_human(),
            r#"param "client" → HttpClient (user-configured opaque type — requires live HTTP connection)"#
        );
    }

    #[test]
    fn format_human_user_configured_without_reason() {
        let reason = SkipReason {
            param_name: "pool".into(),
            opaque_label: "DatabasePool".into(),
            category: OpaqueCategory::UserConfigured,
            nesting_path: vec![PathSegment::Param("pool".into())],
            user_reason: None,
        };
        assert_eq!(
            reason.format_human(),
            r#"param "pool" → DatabasePool (user-configured opaque type — marked as non-synthesizable)"#
        );
    }
}
