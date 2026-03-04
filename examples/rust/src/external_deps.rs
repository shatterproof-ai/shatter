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

/// ParseKeyValue — 5 branches using regex capture groups to parse "key=value" pairs.
/// Exercises Regex::captures() and capture group extraction — real regex engine work,
/// not just a type import.
///
/// EXPECTED BRANCHES (5):
///   1. input is empty                        → Err("empty input")
///   2. input doesn't match key=value format  → Err("no match")
///   3. key is empty (e.g. "=foo")            → Err("empty key")
///   4. value is empty (e.g. "foo=")          → Ok(("foo", None))
///   5. both key and value present            → Ok(("key", Some("value")))
pub fn parse_key_value(input: &str) -> Result<(String, Option<String>), String> {
    if input.is_empty() {
        return Err("empty input".to_string());
    }

    let re = Regex::new(r"^([^=]*)=(.*)$").expect("static regex");
    let caps = re.captures(input).ok_or_else(|| "no match".to_string())?;

    let key = caps.get(1).map(|m| m.as_str()).unwrap_or("");
    if key.is_empty() {
        return Err("empty key".to_string());
    }

    let value = caps.get(2).map(|m| m.as_str()).unwrap_or("");
    if value.is_empty() {
        Ok((key.to_string(), None))
    } else {
        Ok((key.to_string(), Some(value.to_string())))
    }
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

    #[test]
    fn test_parse_key_value_empty() {
        assert_eq!(parse_key_value(""), Err("empty input".into()));
    }

    #[test]
    fn test_parse_key_value_no_equals() {
        assert_eq!(parse_key_value("hello"), Err("no match".into()));
    }

    #[test]
    fn test_parse_key_value_empty_key() {
        assert_eq!(parse_key_value("=bar"), Err("empty key".into()));
    }

    #[test]
    fn test_parse_key_value_empty_value() {
        assert_eq!(parse_key_value("foo="), Ok(("foo".into(), None)));
    }

    #[test]
    fn test_parse_key_value_full() {
        assert_eq!(
            parse_key_value("host=localhost"),
            Ok(("host".into(), Some("localhost".into())))
        );
    }
}
