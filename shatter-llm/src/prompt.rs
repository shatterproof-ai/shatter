//! Prompt construction and JSON Schema derivation for the LLM seed oracle.

use serde_json::{Value, json};
use shatter_core::oracle::OracleContext;
use shatter_core::types::{ParamInfo, TypeInfo};

/// Build a text prompt asking the LLM for `candidates_per_query` input
/// vectors that might flip the unsolved branch condition in `ctx`.
pub fn build_prompt(ctx: &OracleContext, candidates_per_query: u32) -> String {
    let param_types_line = ctx
        .param_types
        .iter()
        .map(|p| format!("{}: {}", p.name, type_label(&p.typ)))
        .collect::<Vec<_>>()
        .join(", ");

    let attempted_preview: Vec<&Vec<Value>> = ctx.attempted.iter().take(5).collect();
    let attempted_json = serde_json::to_string(&attempted_preview)
        .unwrap_or_else(|_| "[]".to_string());

    format!(
        "You are a test input generator for a concolic execution engine.\n\
\n\
## Function under test\n\
{src}\n\
\n\
## Parameter types\n\
{params}\n\
\n\
## Unsolved branch condition\n\
{predicate}\n\
\n\
## Inputs already attempted (did not satisfy the condition)\n\
{attempted}\n\
\n\
## Instructions\n\
Return ONLY a JSON array of {n} input objects.\n\
Each object must have one key per parameter with a value of the correct type.\n\
Do not include explanation or prose.\n\
\n\
Example format:\n\
[{{\"x\": 42, \"s\": \"hello@world\", \"flag\": true}}, ...]\n",
        src = ctx.function_source,
        params = param_types_line,
        predicate = ctx.condition.predicate,
        attempted = attempted_json,
        n = candidates_per_query,
    )
}

/// Build a JSON Schema (`{"type":"array","items":{"type":"object",...}}`) for
/// structured-output adapters like Anthropic `tool_use` or OpenAI JSON mode.
pub fn build_schema(param_types: &[ParamInfo]) -> Value {
    let mut properties = serde_json::Map::new();
    let mut required = Vec::with_capacity(param_types.len());
    for p in param_types {
        properties.insert(p.name.clone(), type_to_schema(&p.typ));
        required.push(Value::String(p.name.clone()));
    }
    json!({
        "type": "array",
        "items": {
            "type": "object",
            "properties": Value::Object(properties),
            "required": Value::Array(required),
            "additionalProperties": false,
        }
    })
}

fn type_to_schema(t: &TypeInfo) -> Value {
    match t {
        TypeInfo::Int { .. } => json!({ "type": "integer" }),
        TypeInfo::Float => json!({ "type": "number" }),
        TypeInfo::Str => json!({ "type": "string" }),
        TypeInfo::Bool => json!({ "type": "boolean" }),
        TypeInfo::Array { element } => json!({
            "type": "array",
            "items": type_to_schema(element),
        }),
        TypeInfo::Object { fields } => {
            let mut properties = serde_json::Map::new();
            let mut required = Vec::with_capacity(fields.len());
            for (name, ft) in fields {
                properties.insert(name.clone(), type_to_schema(ft));
                required.push(Value::String(name.clone()));
            }
            json!({
                "type": "object",
                "properties": Value::Object(properties),
                "required": Value::Array(required),
                "additionalProperties": false,
            })
        }
        TypeInfo::Union { variants, .. } => {
            let any_of: Vec<Value> = variants.iter().map(type_to_schema).collect();
            json!({ "anyOf": any_of })
        }
        TypeInfo::Nullable { inner } => {
            json!({ "anyOf": [type_to_schema(inner), { "type": "null" }] })
        }
        TypeInfo::Complex { inner, .. } => match inner {
            Some(inner) => type_to_schema(inner),
            None => json!({}),
        },
        TypeInfo::Opaque { .. } | TypeInfo::Unknown => json!({}),
    }
}

fn type_label(t: &TypeInfo) -> String {
    match t {
        TypeInfo::Int { .. } => "int".to_string(),
        TypeInfo::Float => "float".to_string(),
        TypeInfo::Str => "string".to_string(),
        TypeInfo::Bool => "bool".to_string(),
        TypeInfo::Array { element } => format!("array<{}>", type_label(element)),
        TypeInfo::Object { fields } => {
            let body = fields
                .iter()
                .map(|(n, t)| format!("{}: {}", n, type_label(t)))
                .collect::<Vec<_>>()
                .join(", ");
            format!("object{{{body}}}")
        }
        TypeInfo::Union { variants, .. } => variants
            .iter()
            .map(type_label)
            .collect::<Vec<_>>()
            .join(" | "),
        TypeInfo::Nullable { inner } => format!("{}?", type_label(inner)),
        TypeInfo::Complex { kind, .. } => format!("{kind:?}"),
        TypeInfo::Opaque { label, .. } => format!("opaque<{label}>"),
        TypeInfo::Unknown => "unknown".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shatter_core::oracle::FailedCondition;

    fn ctx() -> OracleContext {
        OracleContext {
            function_source: "fn f(x: i64) -> bool { x > 10 }".to_string(),
            param_types: vec![ParamInfo {
                name: "x".to_string(),
                typ: TypeInfo::Int { int_width: None, int_signed: None },
                type_name: None,
            }],
            condition: FailedCondition {
                predicate: "x > 10".to_string(),
                location: "src/f.rs:1".to_string(),
            },
            attempted: vec![vec![json!(0)], vec![json!(-1)]],
        }
    }

    #[test]
    fn build_prompt_includes_predicate_and_count() {
        let p = build_prompt(&ctx(), 4);
        assert!(p.contains("x > 10"));
        assert!(p.contains("JSON array of 4"));
        assert!(p.contains("x: int"));
    }

    #[test]
    fn build_schema_array_of_objects() {
        let params = vec![
            ParamInfo {
                name: "x".to_string(),
                typ: TypeInfo::Int { int_width: None, int_signed: None },
                type_name: None,
            },
            ParamInfo {
                name: "s".to_string(),
                typ: TypeInfo::Str,
                type_name: None,
            },
        ];
        let schema = build_schema(&params);
        assert_eq!(schema["type"], "array");
        assert_eq!(schema["items"]["type"], "object");
        assert_eq!(schema["items"]["properties"]["x"]["type"], "integer");
        assert_eq!(schema["items"]["properties"]["s"]["type"], "string");
        let required = schema["items"]["required"].as_array().unwrap();
        assert!(required.iter().any(|v| v == "x"));
        assert!(required.iter().any(|v| v == "s"));
    }
}
