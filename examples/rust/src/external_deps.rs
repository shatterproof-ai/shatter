// Example: External crate dependencies
// Tests shatter's handling of functions that use real third-party crates.
// Unlike the other examples which use only stdlib, this file imports
// `regex` and `serde_json` — common crates requiring Cargo dependency resolution.

use regex::Regex;
use serde_json::Value;

/// ValidatePattern — 4 branches based on regex compilation and matching.
///
/// EXPECTED BRANCHES (4):
///   1. pattern is empty         → Err("empty pattern")
///   2. pattern is invalid regex → Err("invalid regex: ...")
///   3. input does not match     → Ok(false)
///   4. input matches            → Ok(true)
pub fn validate_pattern(pattern: &str, input: &str) -> Result<bool, String> {
    if pattern.is_empty() {
        return Err("empty pattern".to_string());
    }

    let re = Regex::new(pattern).map_err(|e| format!("invalid regex: {e}"))?;

    Ok(re.is_match(input))
}

/// ClassifyJson — 5 branches based on JSON value type.
/// Exercises serde_json's Value enum, a real external dependency.
///
/// EXPECTED BRANCHES (5):
///   1. Value::Null          → "null"
///   2. Value::Bool(_)       → "boolean"
///   3. Value::Number(_)     → "number"
///   4. Value::String(_)     → "string"
///   5. Value::Array/Object  → "composite"
pub fn classify_json(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) | Value::Object(_) => "composite",
    }
}

/// ExtractField — 4 branches for extracting a string field from JSON.
///
/// EXPECTED BRANCHES (4):
///   1. json is not an object        → Err("not an object")
///   2. field is missing             → Err("field not found")
///   3. field is not a string        → Err("field is not a string")
///   4. field is a string            → Ok(value)
pub fn extract_field(json: &Value, field: &str) -> Result<String, String> {
    let obj = json.as_object().ok_or_else(|| "not an object".to_string())?;

    let val = obj.get(field).ok_or_else(|| "field not found".to_string())?;

    val.as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "field is not a string".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_validate_pattern_empty() {
        assert_eq!(validate_pattern("", "abc"), Err("empty pattern".into()));
    }

    #[test]
    fn test_validate_pattern_invalid() {
        assert!(validate_pattern("[invalid", "abc").is_err());
    }

    #[test]
    fn test_validate_pattern_no_match() {
        assert_eq!(validate_pattern(r"^\d+$", "abc"), Ok(false));
    }

    #[test]
    fn test_validate_pattern_match() {
        assert_eq!(validate_pattern(r"^\d+$", "123"), Ok(true));
    }

    #[test]
    fn test_classify_json() {
        assert_eq!(classify_json(&json!(null)), "null");
        assert_eq!(classify_json(&json!(true)), "boolean");
        assert_eq!(classify_json(&json!(42)), "number");
        assert_eq!(classify_json(&json!("hi")), "string");
        assert_eq!(classify_json(&json!([1, 2])), "composite");
        assert_eq!(classify_json(&json!({"a": 1})), "composite");
    }

    #[test]
    fn test_extract_field_not_object() {
        assert_eq!(extract_field(&json!(42), "x"), Err("not an object".into()));
    }

    #[test]
    fn test_extract_field_missing() {
        assert_eq!(
            extract_field(&json!({"a": 1}), "b"),
            Err("field not found".into())
        );
    }

    #[test]
    fn test_extract_field_not_string() {
        assert_eq!(
            extract_field(&json!({"a": 1}), "a"),
            Err("field is not a string".into())
        );
    }

    #[test]
    fn test_extract_field_ok() {
        assert_eq!(
            extract_field(&json!({"name": "alice"}), "name"),
            Ok("alice".into())
        );
    }
}
