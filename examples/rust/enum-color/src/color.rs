//! str-2nfoe end-to-end fixture for Rust enum value-domain extraction.
//!
//! `Color` is a fieldless enum with a three-member value domain; `classify_color`
//! matches over all three variants. Without a value domain the core generator
//! only produces generic strings that all fail to deserialize into `Color`, so
//! no arm is ever reached. With `enum_values` carried on the param's union
//! `TypeInfo` (variant names "Red"/"Green"/"Blue"), the generator draws valid
//! members and reaches every arm.

use serde::{Deserialize, Serialize};

/// A fieldless enum whose default-serde value domain is its variant names.
#[derive(Serialize, Deserialize)]
pub enum Color {
    Red,
    Green,
    Blue,
}

/// Match over the `Color` enum, one distinct return per variant.
///
/// The match is exhaustive (no default arm): a fieldless enum with all variants
/// covered leaves no residual case, and an off-domain string simply fails to
/// deserialize into `Color` before the body runs.
pub fn classify_color(c: Color) -> &'static str {
    match c {
        Color::Red => "warm",
        Color::Green => "cool-green",
        Color::Blue => "cool-blue",
    }
}
