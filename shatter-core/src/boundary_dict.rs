//! Built-in dictionary of common boundary values organized by type.
//!
//! Provides a static set of known edge-case values (e.g., `i32::MAX`, empty
//! string, `NaN`) that improve branch coverage for common boundary checks.
//! These are used as low-priority seed inputs during exploration, after solver
//! and user-provided values.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::types::TypeInfo;

/// Why a boundary value is interesting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoundaryCategory {
    /// Classic boundary: zero, min, max, off-by-one.
    Boundary,
    /// Security-relevant: null bytes, injection patterns.
    Security,
    /// Unicode edge cases: emoji, RTL, BOM, combining characters.
    Unicode,
    /// Floating-point precision traps.
    Precision,
    /// Empty / absent values.
    Empty,
    /// Overflow / underflow triggers.
    Overflow,
}

/// A single boundary value with metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoundaryEntry {
    /// The JSON-encoded boundary value.
    pub value: Value,
    /// Why this value is interesting.
    pub category: BoundaryCategory,
    /// Human-readable description.
    pub description: String,
}

impl BoundaryEntry {
    fn new(value: Value, category: BoundaryCategory, description: &str) -> Self {
        Self {
            value,
            category,
            description: description.to_string(),
        }
    }
}

/// Returns boundary values applicable to the given type.
#[must_use]
pub fn get_boundary_values(type_info: &TypeInfo) -> Vec<BoundaryEntry> {
    match type_info {
        TypeInfo::Int => int_boundaries(),
        TypeInfo::Float => float_boundaries(),
        TypeInfo::Str => string_boundaries(),
        TypeInfo::Bool => bool_boundaries(),
        TypeInfo::Array { .. } => array_boundaries(),
        TypeInfo::Object { .. } => object_boundaries(),
        TypeInfo::Nullable { inner } => {
            let mut entries = vec![BoundaryEntry::new(
                Value::Null,
                BoundaryCategory::Empty,
                "null value",
            )];
            entries.extend(get_boundary_values(inner));
            entries
        }
        TypeInfo::Union { variants } => {
            let mut entries = Vec::new();
            for variant in variants {
                entries.extend(get_boundary_values(variant));
            }
            entries
        }
        TypeInfo::Complex { .. } | TypeInfo::Opaque { .. } | TypeInfo::Unknown => Vec::new(),
    }
}

/// Returns boundary values for a specific category only.
#[must_use]
pub fn get_boundary_values_for_category(
    type_info: &TypeInfo,
    category: BoundaryCategory,
) -> Vec<BoundaryEntry> {
    get_boundary_values(type_info)
        .into_iter()
        .filter(|e| e.category == category)
        .collect()
}

fn int_boundaries() -> Vec<BoundaryEntry> {
    vec![
        BoundaryEntry::new(json!(0), BoundaryCategory::Boundary, "zero"),
        BoundaryEntry::new(json!(-1), BoundaryCategory::Boundary, "negative one"),
        BoundaryEntry::new(json!(1), BoundaryCategory::Boundary, "positive one"),
        BoundaryEntry::new(
            json!(i32::MIN),
            BoundaryCategory::Overflow,
            "i32 minimum (-2147483648)",
        ),
        BoundaryEntry::new(
            json!(i32::MAX),
            BoundaryCategory::Overflow,
            "i32 maximum (2147483647)",
        ),
        BoundaryEntry::new(
            json!(i64::MIN),
            BoundaryCategory::Overflow,
            "i64 minimum",
        ),
        BoundaryEntry::new(
            json!(i64::MAX),
            BoundaryCategory::Overflow,
            "i64 maximum",
        ),
        BoundaryEntry::new(json!(255), BoundaryCategory::Boundary, "u8 max (byte boundary)"),
        BoundaryEntry::new(json!(256), BoundaryCategory::Boundary, "u8 max + 1"),
        BoundaryEntry::new(json!(65535), BoundaryCategory::Boundary, "u16 max"),
        BoundaryEntry::new(json!(65536), BoundaryCategory::Boundary, "u16 max + 1"),
    ]
}

fn float_boundaries() -> Vec<BoundaryEntry> {
    // Note: JSON cannot represent NaN, Infinity, or -Infinity as numbers.
    // We use string sentinels ("NaN", "Infinity", "-Infinity") that frontends
    // can parse into their language's native float type.
    vec![
        BoundaryEntry::new(json!(0.0), BoundaryCategory::Boundary, "zero"),
        BoundaryEntry::new(json!(-0.0), BoundaryCategory::Boundary, "negative zero"),
        BoundaryEntry::new(
            json!("Infinity"),
            BoundaryCategory::Overflow,
            "positive infinity (string sentinel)",
        ),
        BoundaryEntry::new(
            json!("-Infinity"),
            BoundaryCategory::Overflow,
            "negative infinity (string sentinel)",
        ),
        BoundaryEntry::new(
            json!("NaN"),
            BoundaryCategory::Precision,
            "NaN (string sentinel)",
        ),
        BoundaryEntry::new(json!(f64::EPSILON), BoundaryCategory::Precision, "machine epsilon"),
        BoundaryEntry::new(
            json!(0.1_f64 + 0.2_f64),
            BoundaryCategory::Precision,
            "0.1 + 0.2 (IEEE 754 representation artifact)",
        ),
        BoundaryEntry::new(
            json!(1.7976931348623157e308),
            BoundaryCategory::Overflow,
            "f64 near-max",
        ),
        BoundaryEntry::new(
            json!(-1.7976931348623157e308),
            BoundaryCategory::Overflow,
            "f64 near-min",
        ),
        BoundaryEntry::new(
            json!(5e-324),
            BoundaryCategory::Precision,
            "f64 smallest positive subnormal",
        ),
    ]
}

fn string_boundaries() -> Vec<BoundaryEntry> {
    vec![
        BoundaryEntry::new(json!(""), BoundaryCategory::Empty, "empty string"),
        BoundaryEntry::new(json!(" "), BoundaryCategory::Boundary, "single space"),
        BoundaryEntry::new(json!("0"), BoundaryCategory::Boundary, "string zero"),
        BoundaryEntry::new(json!("null"), BoundaryCategory::Security, "string 'null'"),
        BoundaryEntry::new(
            json!("undefined"),
            BoundaryCategory::Security,
            "string 'undefined'",
        ),
        BoundaryEntry::new(json!("NaN"), BoundaryCategory::Security, "string 'NaN'"),
        BoundaryEntry::new(json!("true"), BoundaryCategory::Security, "string 'true'"),
        BoundaryEntry::new(json!("false"), BoundaryCategory::Security, "string 'false'"),
        BoundaryEntry::new(
            json!("\0"),
            BoundaryCategory::Security,
            "null byte character",
        ),
        BoundaryEntry::new(
            json!("a".repeat(10_000)),
            BoundaryCategory::Overflow,
            "very long string (10k chars)",
        ),
        // Unicode edge cases
        BoundaryEntry::new(
            json!("\u{1F600}"),
            BoundaryCategory::Unicode,
            "emoji (grinning face)",
        ),
        BoundaryEntry::new(
            json!("\u{200F}"),
            BoundaryCategory::Unicode,
            "RTL mark (right-to-left override)",
        ),
        BoundaryEntry::new(
            json!("\u{FEFF}"),
            BoundaryCategory::Unicode,
            "BOM (byte order mark)",
        ),
        BoundaryEntry::new(
            json!("e\u{0301}"),
            BoundaryCategory::Unicode,
            "combining character (e + acute accent)",
        ),
    ]
}

fn bool_boundaries() -> Vec<BoundaryEntry> {
    vec![
        BoundaryEntry::new(json!(true), BoundaryCategory::Boundary, "true"),
        BoundaryEntry::new(json!(false), BoundaryCategory::Boundary, "false"),
    ]
}

fn array_boundaries() -> Vec<BoundaryEntry> {
    vec![
        BoundaryEntry::new(json!([]), BoundaryCategory::Empty, "empty array"),
        BoundaryEntry::new(
            json!([null]),
            BoundaryCategory::Boundary,
            "single-element array",
        ),
        BoundaryEntry::new(
            json!([1, 1]),
            BoundaryCategory::Boundary,
            "duplicate-element array",
        ),
    ]
}

fn object_boundaries() -> Vec<BoundaryEntry> {
    vec![
        BoundaryEntry::new(json!({}), BoundaryCategory::Empty, "empty object"),
        BoundaryEntry::new(Value::Null, BoundaryCategory::Empty, "null"),
    ]
}

/// Generate all boundary-value input vectors for a function's parameter list.
///
/// Returns one input vector per boundary value per parameter position.
/// Other parameters get their first boundary value (or a default).
#[must_use]
pub fn generate_boundary_inputs(params: &[crate::types::ParamInfo]) -> Vec<Vec<Value>> {
    if params.is_empty() {
        return Vec::new();
    }

    let param_boundaries: Vec<Vec<BoundaryEntry>> =
        params.iter().map(|p| get_boundary_values(&p.typ)).collect();

    // Defaults: first boundary value per param, or json null.
    let defaults: Vec<Value> = param_boundaries
        .iter()
        .map(|b| {
            b.first()
                .map(|e| e.value.clone())
                .unwrap_or(Value::Null)
        })
        .collect();

    let mut inputs = Vec::new();

    // For each parameter position, generate one input vector per boundary value,
    // holding other params at their default.
    for (i, boundaries) in param_boundaries.iter().enumerate() {
        for entry in boundaries {
            let mut row = defaults.clone();
            row[i] = entry.value.clone();
            inputs.push(row);
        }
    }

    inputs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ParamInfo;

    #[test]
    fn int_boundaries_include_expected_values() {
        let entries = get_boundary_values(&TypeInfo::Int);
        let values: Vec<&Value> = entries.iter().map(|e| &e.value).collect();
        assert!(values.contains(&&json!(0)), "should contain 0");
        assert!(values.contains(&&json!(-1)), "should contain -1");
        assert!(values.contains(&&json!(1)), "should contain 1");
        assert!(values.contains(&&json!(i32::MIN)), "should contain i32::MIN");
        assert!(values.contains(&&json!(i32::MAX)), "should contain i32::MAX");
        assert!(values.contains(&&json!(i64::MIN)), "should contain i64::MIN");
        assert!(values.contains(&&json!(i64::MAX)), "should contain i64::MAX");
        assert!(values.contains(&&json!(255)), "should contain 255");
        assert!(values.contains(&&json!(256)), "should contain 256");
    }

    #[test]
    fn float_boundaries_include_nan_infinity_epsilon() {
        let entries = get_boundary_values(&TypeInfo::Float);

        // NaN and Infinity are string sentinels since JSON can't represent them
        let has_nan = entries
            .iter()
            .any(|e| e.value.as_str() == Some("NaN"));
        assert!(has_nan, "should include NaN sentinel");

        let has_infinity = entries
            .iter()
            .any(|e| e.value.as_str() == Some("Infinity"));
        assert!(has_infinity, "should include positive infinity sentinel");

        let has_neg_infinity = entries
            .iter()
            .any(|e| e.value.as_str() == Some("-Infinity"));
        assert!(has_neg_infinity, "should include negative infinity sentinel");

        let has_epsilon = entries
            .iter()
            .any(|e| e.description.contains("epsilon"));
        assert!(has_epsilon, "should include epsilon");
    }

    #[test]
    fn string_boundaries_include_expected_values() {
        let entries = get_boundary_values(&TypeInfo::Str);
        let values: Vec<String> = entries
            .iter()
            .filter_map(|e| e.value.as_str().map(|s| s.to_string()))
            .collect();
        assert!(values.contains(&String::new()), "should contain empty string");
        assert!(values.contains(&" ".to_string()), "should contain single space");
        assert!(values.contains(&"null".to_string()), "should contain 'null'");
        assert!(values.contains(&"NaN".to_string()), "should contain 'NaN'");
        assert!(values.contains(&"\0".to_string()), "should contain null byte");
    }

    #[test]
    fn string_boundaries_include_unicode() {
        let entries = get_boundary_values(&TypeInfo::Str);
        let unicode_entries: Vec<_> = entries
            .iter()
            .filter(|e| e.category == BoundaryCategory::Unicode)
            .collect();
        assert!(
            unicode_entries.len() >= 3,
            "should have at least 3 unicode entries, got {}",
            unicode_entries.len()
        );
    }

    #[test]
    fn bool_boundaries_return_both_values() {
        let entries = get_boundary_values(&TypeInfo::Bool);
        assert_eq!(entries.len(), 2);
        let values: Vec<&Value> = entries.iter().map(|e| &e.value).collect();
        assert!(values.contains(&&json!(true)));
        assert!(values.contains(&&json!(false)));
    }

    #[test]
    fn unknown_type_returns_empty() {
        let entries = get_boundary_values(&TypeInfo::Unknown);
        assert!(entries.is_empty());
    }

    #[test]
    fn opaque_type_returns_empty() {
        let entries = get_boundary_values(&TypeInfo::Opaque {
            label: "net.Socket".to_string(),
        });
        assert!(entries.is_empty());
    }

    #[test]
    fn category_filtering_works() {
        let overflow = get_boundary_values_for_category(&TypeInfo::Int, BoundaryCategory::Overflow);
        assert!(
            !overflow.is_empty(),
            "int should have overflow boundary values"
        );
        for entry in &overflow {
            assert_eq!(entry.category, BoundaryCategory::Overflow);
        }

        let security =
            get_boundary_values_for_category(&TypeInfo::Str, BoundaryCategory::Security);
        assert!(
            !security.is_empty(),
            "str should have security boundary values"
        );
        for entry in &security {
            assert_eq!(entry.category, BoundaryCategory::Security);
        }
    }

    #[test]
    fn all_entries_have_descriptions() {
        for type_info in &[
            TypeInfo::Int,
            TypeInfo::Float,
            TypeInfo::Str,
            TypeInfo::Bool,
            TypeInfo::Array {
                element: Box::new(TypeInfo::Int),
            },
            TypeInfo::Object { fields: vec![] },
        ] {
            for entry in get_boundary_values(type_info) {
                assert!(
                    !entry.description.is_empty(),
                    "entry {:?} has empty description",
                    entry.value
                );
            }
        }
    }

    #[test]
    fn all_entries_are_valid_json() {
        for type_info in &[
            TypeInfo::Int,
            TypeInfo::Float,
            TypeInfo::Str,
            TypeInfo::Bool,
        ] {
            for entry in get_boundary_values(type_info) {
                let serialized = serde_json::to_string(&entry.value);
                assert!(
                    serialized.is_ok(),
                    "entry {:?} failed to serialize: {:?}",
                    entry.description,
                    serialized.err()
                );
            }
        }
    }

    #[test]
    fn nullable_includes_null_plus_inner_boundaries() {
        let entries = get_boundary_values(&TypeInfo::Nullable {
            inner: Box::new(TypeInfo::Int),
        });
        let has_null = entries.iter().any(|e| e.value.is_null());
        assert!(has_null, "nullable should include null");
        let has_zero = entries.iter().any(|e| e.value == json!(0));
        assert!(has_zero, "nullable<int> should include int boundaries");
    }

    #[test]
    fn generate_boundary_inputs_produces_correct_count() {
        let params = vec![
            ParamInfo {
                name: "x".to_string(),
                typ: TypeInfo::Int,
                type_name: None,
            },
            ParamInfo {
                name: "s".to_string(),
                typ: TypeInfo::Str,
                type_name: None,
            },
        ];
        let inputs = generate_boundary_inputs(&params);
        let expected_count =
            get_boundary_values(&TypeInfo::Int).len() + get_boundary_values(&TypeInfo::Str).len();
        assert_eq!(inputs.len(), expected_count);
        // Each input vector should have 2 elements (one per param)
        for input in &inputs {
            assert_eq!(input.len(), 2);
        }
    }

    #[test]
    fn generate_boundary_inputs_empty_params() {
        let inputs = generate_boundary_inputs(&[]);
        assert!(inputs.is_empty());
    }

    #[test]
    fn boundary_entry_round_trips() {
        let entry = BoundaryEntry::new(json!(42), BoundaryCategory::Boundary, "test value");
        let json_str = serde_json::to_string(&entry).expect("serialize");
        let deserialized: BoundaryEntry =
            serde_json::from_str(&json_str).expect("deserialize");
        assert_eq!(entry, deserialized);
    }

    #[test]
    fn boundary_category_round_trips() {
        let categories = [
            BoundaryCategory::Boundary,
            BoundaryCategory::Security,
            BoundaryCategory::Unicode,
            BoundaryCategory::Precision,
            BoundaryCategory::Empty,
            BoundaryCategory::Overflow,
        ];
        for cat in &categories {
            let json_str = serde_json::to_string(cat).expect("serialize");
            let deserialized: BoundaryCategory =
                serde_json::from_str(&json_str).expect("deserialize");
            assert_eq!(*cat, deserialized);
        }
    }
}
