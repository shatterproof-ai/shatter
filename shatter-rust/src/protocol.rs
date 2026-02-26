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
            code: None,
            message: None,
        }
    }
}
