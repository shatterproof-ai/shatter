//! Parse LLM responses into validated, deduplicated [`InputVector`]s.

use serde_json::Value;
use shatter_core::oracle::InputVector;
use shatter_core::types::{ParamInfo, TypeInfo};

/// Parse a text LLM response: find the first JSON array, type-validate each
/// candidate object against `param_types`, and drop any candidate that
/// duplicates an entry in `attempted`.
///
/// Returns an empty `Vec` on JSON-parse failure (caller may retry with a
/// simplified prompt). Provenance is the orchestrator's responsibility and is
/// not attached here.
pub fn parse_response(
    raw: &str,
    param_types: &[ParamInfo],
    attempted: &[InputVector],
) -> Vec<InputVector> {
    let Some(array) = extract_first_json_array(raw) else {
        return Vec::new();
    };
    parse_response_structured(array, param_types, attempted)
}

/// Same pipeline as [`parse_response`] but starting from an already-parsed
/// JSON value (for structured-output adapters that do their own parsing).
pub fn parse_response_structured(
    value: Value,
    param_types: &[ParamInfo],
    attempted: &[InputVector],
) -> Vec<InputVector> {
    let Value::Array(items) = value else {
        return Vec::new();
    };
    let mut out: Vec<InputVector> = Vec::with_capacity(items.len());
    for item in items {
        let Value::Object(map) = item else { continue };
        let mut vector: InputVector = Vec::with_capacity(param_types.len());
        let mut ok = true;
        for p in param_types {
            let Some(v) = map.get(&p.name) else {
                ok = false;
                break;
            };
            if !type_matches(&p.typ, v) {
                ok = false;
                break;
            }
            vector.push(v.clone());
        }
        if !ok {
            continue;
        }
        if attempted.iter().any(|a| a == &vector) {
            continue;
        }
        out.push(vector);
    }
    out
}

fn extract_first_json_array(raw: &str) -> Option<Value> {
    let bytes = raw.as_bytes();
    let mut start = None;
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'[' {
            start = Some(i);
            break;
        }
    }
    let start = start?;
    let mut depth: i32 = 0;
    let mut in_string = false;
    let mut escape = false;
    for (i, &b) in bytes.iter().enumerate().skip(start) {
        if in_string {
            if escape {
                escape = false;
            } else if b == b'\\' {
                escape = true;
            } else if b == b'"' {
                in_string = false;
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    let slice = &raw[start..=i];
                    return serde_json::from_str::<Value>(slice).ok();
                }
            }
            _ => {}
        }
    }
    None
}

fn type_matches(t: &TypeInfo, v: &Value) -> bool {
    match t {
        TypeInfo::Int => v.is_i64() || v.is_u64(),
        TypeInfo::Float => v.is_number(),
        TypeInfo::Str => v.is_string(),
        TypeInfo::Bool => v.is_boolean(),
        TypeInfo::Array { element } => match v {
            Value::Array(items) => items.iter().all(|i| type_matches(element, i)),
            _ => false,
        },
        TypeInfo::Object { fields } => match v {
            Value::Object(map) => fields
                .iter()
                .all(|(n, ft)| map.get(n).is_some_and(|fv| type_matches(ft, fv))),
            _ => false,
        },
        TypeInfo::Union { variants } => variants.iter().any(|vt| type_matches(vt, v)),
        TypeInfo::Nullable { inner } => v.is_null() || type_matches(inner, v),
        TypeInfo::Complex { inner, .. } => match inner {
            Some(inner) => type_matches(inner, v),
            None => true,
        },
        TypeInfo::Opaque { .. } | TypeInfo::Unknown => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn pi(name: &str, typ: TypeInfo) -> ParamInfo {
        ParamInfo {
            name: name.to_string(),
            typ,
            type_name: None,
        }
    }

    #[test]
    fn parses_simple_array() {
        let raw = r#"Sure! [{"x": 1, "s": "a"}, {"x": 2, "s": "b"}] done."#;
        let got = parse_response(
            raw,
            &[pi("x", TypeInfo::Int), pi("s", TypeInfo::Str)],
            &[],
        );
        assert_eq!(got.len(), 2);
        assert_eq!(got[0], vec![json!(1), json!("a")]);
        assert_eq!(got[1], vec![json!(2), json!("b")]);
    }

    #[test]
    fn drops_type_mismatched_candidates() {
        let raw = r#"[{"x": "not int", "s": "a"}, {"x": 7, "s": "b"}]"#;
        let got = parse_response(
            raw,
            &[pi("x", TypeInfo::Int), pi("s", TypeInfo::Str)],
            &[],
        );
        assert_eq!(got.len(), 1);
        assert_eq!(got[0], vec![json!(7), json!("b")]);
    }

    #[test]
    fn drops_duplicates_of_attempted() {
        let raw = r#"[{"x": 1}, {"x": 2}, {"x": 3}]"#;
        let attempted = vec![vec![json!(2)]];
        let got = parse_response(raw, &[pi("x", TypeInfo::Int)], &attempted);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0], vec![json!(1)]);
        assert_eq!(got[1], vec![json!(3)]);
    }

    #[test]
    fn non_json_input_returns_empty() {
        let got = parse_response("no json here at all", &[pi("x", TypeInfo::Int)], &[]);
        assert!(got.is_empty());
    }

    #[test]
    fn malformed_json_returns_empty() {
        let got = parse_response("[{x: 1,", &[pi("x", TypeInfo::Int)], &[]);
        assert!(got.is_empty());
    }

    #[test]
    fn missing_param_drops_candidate() {
        let raw = r#"[{"x": 1}, {"y": 2}]"#;
        let got = parse_response(
            raw,
            &[pi("x", TypeInfo::Int), pi("y", TypeInfo::Int)],
            &[],
        );
        assert!(got.is_empty());
    }

    #[test]
    fn nested_array_and_string_brackets_ok() {
        let raw = r#"[{"xs": [1, 2, 3], "label": "has ] bracket"}]"#;
        let got = parse_response(
            raw,
            &[
                pi(
                    "xs",
                    TypeInfo::Array {
                        element: Box::new(TypeInfo::Int),
                    },
                ),
                pi("label", TypeInfo::Str),
            ],
            &[],
        );
        assert_eq!(got.len(), 1);
    }

    #[test]
    fn nullable_accepts_null() {
        let raw = r#"[{"x": null}, {"x": 5}]"#;
        let got = parse_response(
            raw,
            &[pi(
                "x",
                TypeInfo::Nullable {
                    inner: Box::new(TypeInfo::Int),
                },
            )],
            &[],
        );
        assert_eq!(got.len(), 2);
    }

    #[test]
    fn structured_pipeline_validates() {
        let v = json!([{"x": 1}, {"x": "no"}]);
        let got = parse_response_structured(v, &[pi("x", TypeInfo::Int)], &[]);
        assert_eq!(got.len(), 1);
    }
}
