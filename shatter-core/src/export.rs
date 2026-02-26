//! Test export: generate test files from behavior maps.
//!
//! Converts a [`BehaviorMap`] into a test file for a specific framework.
//! Currently supports Jest (TypeScript).

use crate::behavior::{Behavior, BehaviorMap};

/// Generate Jest test source from a behavior map.
///
/// Each behavior becomes an `it` block inside a `describe` block for the function.
/// The generated file includes an import for the function under test.
///
/// # Arguments
/// * `behavior_map` - The behavior map to generate tests from.
/// * `function_name` - The name of the function under test (used in `describe`).
/// * `module_path` - The module path for the import statement (e.g. `"./src/math"`).
pub fn generate_jest_tests(
    behavior_map: &BehaviorMap,
    function_name: &str,
    module_path: &str,
) -> String {
    let mut out = String::new();

    // Import statement
    out.push_str(&format!(
        "import {{ {function_name} }} from '{module_path}';\n\n"
    ));

    // Describe block
    out.push_str(&format!("describe('{function_name}', () => {{\n"));

    if behavior_map.behaviors.is_empty() {
        out.push_str("  // No behaviors observed\n");
    }

    for behavior in &behavior_map.behaviors {
        let test_name = build_test_name(behavior);
        let body = build_test_body(function_name, behavior);

        out.push_str(&format!("  it('{test_name}', () => {{\n"));
        out.push_str(&body);
        out.push_str("  });\n\n");
    }

    out.push_str("});\n");
    out
}

/// Build a descriptive test name from a behavior.
fn build_test_name(behavior: &Behavior) -> String {
    let inputs = format_args_short(&behavior.input_args);

    if let Some(ref error) = behavior.thrown_error {
        return format!(
            "throws {} for input ({})",
            escape_single_quotes(&error.message),
            inputs
        );
    }

    match &behavior.return_value {
        Some(val) => format!(
            "returns {} for input ({})",
            format_value_short(val),
            inputs
        ),
        None => format!("returns undefined for input ({inputs})"),
    }
}

/// Build the body of an `it` block.
fn build_test_body(function_name: &str, behavior: &Behavior) -> String {
    let args = behavior
        .input_args
        .iter()
        .map(format_value)
        .collect::<Vec<_>>()
        .join(", ");

    let mut body = String::new();

    if behavior.thrown_error.is_some() {
        body.push_str(&format!(
            "    expect(() => {function_name}({args})).toThrow();\n"
        ));
    } else {
        let expected = match &behavior.return_value {
            Some(val) => format_value(val),
            None => "undefined".to_string(),
        };
        body.push_str(&format!(
            "    const result = {function_name}({args});\n"
        ));
        body.push_str(&format!("    expect(result).toEqual({expected});\n"));
    }

    body
}

/// Format a JSON value for embedding in JavaScript source.
fn format_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => format!("'{}'", escape_single_quotes(s)),
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        // For arrays/objects, use JSON.stringify-compatible output
        other => other.to_string(),
    }
}

/// Shortened value for test names.
fn format_value_short(value: &serde_json::Value) -> String {
    let s = format_value(value);
    if s.len() > 30 {
        format!("{}...", &s[..27])
    } else {
        s
    }
}

/// Shortened args list for test names.
fn format_args_short(args: &[serde_json::Value]) -> String {
    let parts: Vec<String> = args.iter().map(format_value_short).collect();
    let joined = parts.join(", ");
    if joined.len() > 50 {
        format!("{}...", &joined[..47])
    } else {
        joined
    }
}

/// Escape single quotes for JavaScript string literals.
fn escape_single_quotes(s: &str) -> String {
    s.replace('\'', "\\'")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behavior::{Behavior, BehaviorMap};
    use crate::execution_record::ErrorInfo;
    use serde_json::json;

    fn make_behavior(
        id: u32,
        inputs: Vec<serde_json::Value>,
        return_value: Option<serde_json::Value>,
        error: Option<ErrorInfo>,
    ) -> Behavior {
        Behavior {
            id,
            input_args: inputs,
            return_value,
            thrown_error: error,
            branch_path: vec![],
            side_effects: vec![],
        }
    }

    #[test]
    fn generates_import_statement() {
        let map = BehaviorMap {
            function_id: "add".to_string(),
            behaviors: vec![],
        };
        let output = generate_jest_tests(&map, "add", "./src/math");
        assert!(output.starts_with("import { add } from './src/math';\n"));
    }

    #[test]
    fn generates_describe_block() {
        let map = BehaviorMap {
            function_id: "add".to_string(),
            behaviors: vec![],
        };
        let output = generate_jest_tests(&map, "add", "./src/math");
        assert!(output.contains("describe('add', () => {"));
        assert!(output.ends_with("});\n"));
    }

    #[test]
    fn generates_it_block_for_return_value() {
        let map = BehaviorMap {
            function_id: "add".to_string(),
            behaviors: vec![make_behavior(0, vec![json!(1), json!(2)], Some(json!(3)), None)],
        };
        let output = generate_jest_tests(&map, "add", "./src/math");

        assert!(output.contains("it('returns 3 for input (1, 2)'"), "output: {output}");
        assert!(output.contains("const result = add(1, 2);"));
        assert!(output.contains("expect(result).toEqual(3);"));
    }

    #[test]
    fn generates_it_block_for_string_return() {
        let map = BehaviorMap {
            function_id: "classify".to_string(),
            behaviors: vec![make_behavior(
                0,
                vec![json!(5)],
                Some(json!("positive")),
                None,
            )],
        };
        let output = generate_jest_tests(&map, "classify", "./src/classifier");
        assert!(output.contains("expect(result).toEqual('positive');"));
    }

    #[test]
    fn generates_it_block_for_thrown_error() {
        let map = BehaviorMap {
            function_id: "divide".to_string(),
            behaviors: vec![make_behavior(
                0,
                vec![json!(1), json!(0)],
                None,
                Some(ErrorInfo {
                    error_type: "Error".to_string(),
                    message: "division by zero".to_string(),
                    stack: None,
                }),
            )],
        };
        let output = generate_jest_tests(&map, "divide", "./src/math");
        assert!(output.contains("throws division by zero for input (1, 0)"), "output: {output}");
        assert!(output.contains("expect(() => divide(1, 0)).toThrow();"));
    }

    #[test]
    fn generates_multiple_behaviors() {
        let map = BehaviorMap {
            function_id: "abs".to_string(),
            behaviors: vec![
                make_behavior(0, vec![json!(5)], Some(json!(5)), None),
                make_behavior(1, vec![json!(-3)], Some(json!(3)), None),
                make_behavior(2, vec![json!(0)], Some(json!(0)), None),
            ],
        };
        let output = generate_jest_tests(&map, "abs", "./src/math");

        // Should have 3 it blocks
        let it_count = output.matches("  it('").count();
        assert_eq!(it_count, 3, "expected 3 it blocks, output:\n{output}");
    }

    #[test]
    fn handles_null_return_value() {
        let map = BehaviorMap {
            function_id: "doSomething".to_string(),
            behaviors: vec![make_behavior(0, vec![json!("input")], None, None)],
        };
        let output = generate_jest_tests(&map, "doSomething", "./src/actions");
        assert!(output.contains("returns undefined for input"));
        assert!(output.contains("expect(result).toEqual(undefined);"));
    }

    #[test]
    fn escapes_single_quotes_in_strings() {
        let map = BehaviorMap {
            function_id: "greet".to_string(),
            behaviors: vec![make_behavior(
                0,
                vec![json!("it's")],
                Some(json!("hello it's")),
                None,
            )],
        };
        let output = generate_jest_tests(&map, "greet", "./src/greet");
        assert!(output.contains("'it\\'s'"), "output: {output}");
    }

    #[test]
    fn format_value_handles_all_json_types() {
        assert_eq!(format_value(&json!(42)), "42");
        assert_eq!(format_value(&json!(3.14)), "3.14");
        assert_eq!(format_value(&json!(true)), "true");
        assert_eq!(format_value(&json!(false)), "false");
        assert_eq!(format_value(&json!(null)), "null");
        assert_eq!(format_value(&json!("hello")), "'hello'");
        assert_eq!(format_value(&json!([1, 2])), "[1,2]");
    }
}
