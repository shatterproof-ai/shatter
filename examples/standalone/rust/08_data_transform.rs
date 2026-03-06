// Example 8: Data transformation and config merging.
// Tests reasoning about map types, recursive depth, and multi-step validation.

use std::collections::HashMap;

type ConfigValue = serde_free::Value;

/// Simplified config value enum (no external deps).
mod serde_free {
    use std::collections::HashMap;

    #[derive(Debug, Clone)]
    pub enum Value {
        Null,
        Bool(bool),
        Number(f64),
        Str(String),
        Array(Vec<Value>),
        Object(HashMap<String, Value>),
    }

    impl Value {
        pub fn as_object(&self) -> Option<&HashMap<String, Value>> {
            match self {
                Value::Object(m) => Some(m),
                _ => None,
            }
        }

        pub fn as_array(&self) -> Option<&Vec<Value>> {
            match self {
                Value::Array(a) => Some(a),
                _ => None,
            }
        }

        pub fn is_null(&self) -> bool {
            matches!(self, Value::Null)
        }
    }
}

use serde_free::Value;

/// merge_config — 10 branches: both empty→{}, override-only key→added,
/// base-only key→preserved, both objects→recursive, both arrays+append→concat,
/// both arrays+replace→override, override null→removed, type mismatch→override,
/// same type→override, depth exceeded→error.
fn merge_config(
    base: &HashMap<String, Value>,
    overrides: &HashMap<String, Value>,
    array_strategy: &str,
    max_depth: usize,
    current_depth: usize,
) -> Result<HashMap<String, Value>, String> {
    if current_depth > max_depth {
        return Err("max depth exceeded".to_string());
    }

    let mut result = HashMap::new();

    for (key, base_val) in base {
        if let Some(override_val) = overrides.get(key) {
            if override_val.is_null() {
                continue;
            }

            if let (Some(base_map), Some(override_map)) =
                (base_val.as_object(), override_val.as_object())
            {
                let merged = merge_config(
                    base_map,
                    override_map,
                    array_strategy,
                    max_depth,
                    current_depth + 1,
                )?;
                result.insert(key.clone(), Value::Object(merged));
                continue;
            }

            if let (Some(base_arr), Some(override_arr)) =
                (base_val.as_array(), override_val.as_array())
            {
                if array_strategy == "append" {
                    let mut combined = base_arr.clone();
                    combined.extend(override_arr.iter().cloned());
                    result.insert(key.clone(), Value::Array(combined));
                } else {
                    result.insert(key.clone(), Value::Array(override_arr.clone()));
                }
                continue;
            }

            result.insert(key.clone(), override_val.clone());
        } else {
            result.insert(key.clone(), base_val.clone());
        }
    }

    for (key, override_val) in overrides {
        if !base.contains_key(key) && !override_val.is_null() {
            result.insert(key.clone(), override_val.clone());
        }
    }

    Ok(result)
}

struct TransformResult {
    status: &'static str,
    reason: Option<String>,
    normalized: Option<HashMap<String, String>>,
}

/// transform_record — 9 branches: missing id→rejected, missing type→rejected,
/// user+no email→rejected, user+invalid email→rejected, user+valid→accepted,
/// order+no amount→rejected, order+amount≤0→rejected, order+valid→accepted,
/// unknown type→rejected.
fn transform_record(record: &HashMap<String, String>) -> TransformResult {
    let id = match record.get("id") {
        Some(v) if !v.is_empty() => v,
        _ => {
            return TransformResult {
                status: "rejected",
                reason: Some("missing id".to_string()),
                normalized: None,
            }
        }
    };

    let record_type = match record.get("type") {
        Some(v) if !v.is_empty() => v.clone(),
        _ => {
            return TransformResult {
                status: "rejected",
                reason: Some("missing type".to_string()),
                normalized: None,
            }
        }
    };

    if record_type == "user" {
        let email = match record.get("email") {
            Some(v) if !v.is_empty() => v,
            _ => {
                return TransformResult {
                    status: "rejected",
                    reason: Some("user needs email".to_string()),
                    normalized: None,
                }
            }
        };

        if !is_simple_email(email) {
            return TransformResult {
                status: "rejected",
                reason: Some("invalid email".to_string()),
                normalized: None,
            };
        }

        let mut normalized = HashMap::new();
        normalized.insert("id".to_string(), id.clone());
        normalized.insert("type".to_string(), "user".to_string());
        normalized.insert("email".to_string(), email.to_lowercase());
        return TransformResult {
            status: "accepted",
            reason: None,
            normalized: Some(normalized),
        };
    }

    if record_type == "order" {
        let amount_str = match record.get("amount") {
            Some(v) if !v.is_empty() => v,
            _ => {
                return TransformResult {
                    status: "rejected",
                    reason: Some("order needs amount".to_string()),
                    normalized: None,
                }
            }
        };

        let amount: f64 = match amount_str.parse() {
            Ok(v) if v > 0.0 => v,
            _ => {
                return TransformResult {
                    status: "rejected",
                    reason: Some("non-positive amount".to_string()),
                    normalized: None,
                }
            }
        };

        let rounded = (amount * 100.0).round() / 100.0;
        let mut normalized = HashMap::new();
        normalized.insert("id".to_string(), id.clone());
        normalized.insert("type".to_string(), "order".to_string());
        normalized.insert("amount".to_string(), format!("{rounded}"));
        return TransformResult {
            status: "accepted",
            reason: None,
            normalized: Some(normalized),
        };
    }

    TransformResult {
        status: "rejected",
        reason: Some("unknown type".to_string()),
        normalized: None,
    }
}

/// Simple email check: must contain @ with text on both sides and a dot in domain.
fn is_simple_email(email: &str) -> bool {
    let at_pos = match email.find('@') {
        Some(p) if p > 0 => p,
        _ => return false,
    };
    let domain = &email[at_pos + 1..];
    !domain.is_empty() && domain.contains('.')
}

fn main() {
    let mut record = HashMap::new();
    record.insert("id".to_string(), "1".to_string());
    record.insert("type".to_string(), "user".to_string());
    record.insert("email".to_string(), "Alice@Example.COM".to_string());
    let result = transform_record(&record);
    println!("status={}, normalized={:?}", result.status, result.normalized);
}
