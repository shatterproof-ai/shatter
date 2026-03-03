//! Test export: generate test files from behavior maps.
//!
//! Converts a [`BehaviorMap`] into a test file for a specific framework.
//! Supports Jest (TypeScript) and Go table-driven tests.

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

/// Generate Go table-driven test source from a behavior map.
///
/// Each behavior becomes a test case in a table-driven `t.Run()` subtest.
/// The generated file uses the standard Go testing package.
///
/// # Arguments
/// * `behavior_map` - The behavior map to generate tests from.
/// * `function_name` - The name of the function under test (used in test function name and calls).
/// * `package_name` - The Go package name for the file header.
pub fn generate_go_tests(
    behavior_map: &BehaviorMap,
    function_name: &str,
    package_name: &str,
) -> String {
    let mut out = String::new();

    out.push_str(&format!("package {package_name}\n\n"));
    out.push_str("import \"testing\"\n\n");

    // Test function name: TestFunctionName (capitalize first letter)
    let test_func_name = format!("Test{}", capitalize_first(function_name));

    out.push_str(&format!("func {test_func_name}(t *testing.T) {{\n"));

    if behavior_map.behaviors.is_empty() {
        out.push_str("\t// No behaviors observed\n");
        out.push_str("}\n");
        return out;
    }

    // Separate panic behaviors from normal return behaviors
    let (panic_behaviors, normal_behaviors): (Vec<_>, Vec<_>) = behavior_map
        .behaviors
        .iter()
        .partition(|b| b.thrown_error.is_some());

    if !normal_behaviors.is_empty() {
        out.push_str("\ttests := []struct {\n");
        out.push_str("\t\tname string\n");

        // Determine param types from first behavior
        let first = &normal_behaviors[0];
        for (i, arg) in first.input_args.iter().enumerate() {
            let go_type = go_type_from_value(arg);
            out.push_str(&format!("\t\targ{i}     {go_type}\n"));
        }

        // Determine return type from first behavior with a return value
        let has_return = normal_behaviors.iter().any(|b| b.return_value.is_some());
        if has_return {
            let return_type = normal_behaviors
                .iter()
                .find_map(|b| b.return_value.as_ref())
                .map(go_type_from_value)
                .unwrap_or_else(|| "interface{}".to_string());
            out.push_str(&format!("\t\texpected {return_type}\n"));
        }

        out.push_str("\t}{{\n");

        for behavior in &normal_behaviors {
            let test_name = build_test_name(behavior);
            let args: Vec<String> = behavior.input_args.iter().map(format_go_value).collect();

            if has_return {
                let expected = match &behavior.return_value {
                    Some(val) => format_go_value(val),
                    None => go_zero_value(
                        normal_behaviors
                            .iter()
                            .find_map(|b| b.return_value.as_ref())
                            .map(go_type_from_value)
                            .as_deref()
                            .unwrap_or("interface{}"),
                    ),
                };
                out.push_str(&format!(
                    "\t\t{{\"{test_name}\", {}, {expected}}},\n",
                    args.join(", ")
                ));
            } else {
                out.push_str(&format!(
                    "\t\t{{\"{test_name}\", {}}},\n",
                    args.join(", ")
                ));
            }
        }

        out.push_str("\t}\n");

        out.push_str("\tfor _, tt := range tests {\n");
        out.push_str("\t\tt.Run(tt.name, func(t *testing.T) {\n");

        let arg_refs: Vec<String> = (0..normal_behaviors[0].input_args.len())
            .map(|i| format!("tt.arg{i}"))
            .collect();
        let call = format!("{function_name}({})", arg_refs.join(", "));

        if has_return {
            out.push_str(&format!("\t\t\tgot := {call}\n"));
            out.push_str(&format!(
                "\t\t\tif got != tt.expected {{\n\t\t\t\tt.Errorf(\"{function_name}({}) = %v, want %v\", {}, got, tt.expected)\n\t\t\t}}\n",
                (0..normal_behaviors[0].input_args.len()).map(|_| "%v").collect::<Vec<_>>().join(", "),
                arg_refs.join(", "),
            ));
        } else {
            out.push_str(&format!("\t\t\t{call}\n"));
        }

        out.push_str("\t\t})\n");
        out.push_str("\t}\n");
    }

    // Generate panic test cases
    for behavior in &panic_behaviors {
        let test_name = build_test_name(behavior);
        let args: Vec<String> = behavior.input_args.iter().map(format_go_value).collect();

        out.push_str(&format!("\tt.Run(\"{test_name}\", func(t *testing.T) {{\n"));
        out.push_str("\t\tdefer func() {\n");
        out.push_str("\t\t\tif r := recover(); r == nil {\n");
        out.push_str("\t\t\t\tt.Errorf(\"expected panic but did not get one\")\n");
        out.push_str("\t\t\t}\n");
        out.push_str("\t\t}()\n");
        out.push_str(&format!(
            "\t\t{function_name}({})\n",
            args.join(", ")
        ));
        out.push_str("\t})\n");
    }

    out.push_str("}\n");
    out
}

/// Capitalize the first character of a string (for Go exported names).
fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

/// Map a JSON value to a Go type string.
fn go_type_from_value(value: &serde_json::Value) -> String {
    // Check for __complex_type tagged objects first
    if let Some(obj) = value.as_object()
        && let Some(tag) = obj.get("__complex_type").and_then(|t| t.as_str())
    {
        return go_type_from_complex(tag);
    }
    match value {
        serde_json::Value::String(_) => "string".to_string(),
        serde_json::Value::Bool(_) => "bool".to_string(),
        serde_json::Value::Number(n) => {
            if n.is_f64() && n.to_string().contains('.') {
                "float64".to_string()
            } else {
                "int".to_string()
            }
        }
        serde_json::Value::Null => "interface{}".to_string(),
        serde_json::Value::Array(_) => "interface{}".to_string(),
        serde_json::Value::Object(_) => "interface{}".to_string(),
    }
}

/// Map a complex type tag to its Go type name.
fn go_type_from_complex(tag: &str) -> String {
    match tag {
        "date" | "date_time" => "time.Time".to_string(),
        "duration" => "time.Duration".to_string(),
        "reg_exp" => "*regexp.Regexp".to_string(),
        "big_int" => "*big.Int".to_string(),
        "big_decimal" => "*big.Float".to_string(),
        "url" => "*url.URL".to_string(),
        "ip_address" => "net.IP".to_string(),
        "error" => "error".to_string(),
        "rune" => "rune".to_string(),
        "go_byte" => "byte".to_string(),
        _ => "interface{}".to_string(),
    }
}

/// Format a JSON value as a Go literal.
///
/// Detects `__complex_type` tagged objects and emits constructor calls.
fn format_go_value(value: &serde_json::Value) -> String {
    if let Some(go) = try_format_complex_go(value) {
        return go;
    }
    match value {
        serde_json::Value::String(s) => format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")),
        serde_json::Value::Null => "nil".to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        other => format!("/* unsupported: {} */nil", other),
    }
}

/// Try to format a `__complex_type` tagged JSON object as a Go constructor.
/// Returns `None` if the value is not a tagged complex type.
fn try_format_complex_go(value: &serde_json::Value) -> Option<String> {
    let obj = value.as_object()?;
    let tag = obj.get("__complex_type")?.as_str()?;
    match tag {
        "date" | "date_time" => {
            let v = obj.get("value")?;
            Some(format!("time.UnixMilli({})", v))
        }
        "duration" => {
            let ms = obj.get("ms").or_else(|| obj.get("value"))?;
            Some(format!("time.Duration({}) * time.Millisecond", ms))
        }
        "reg_exp" => {
            let source = obj.get("source")?.as_str()?;
            let escaped = source.replace('\\', "\\\\").replace('"', "\\\"");
            Some(format!("regexp.MustCompile(\"{escaped}\")"))
        }
        "big_int" => {
            let v = obj.get("value")?.as_str()?;
            Some(format!("func() *big.Int {{ n, _ := new(big.Int).SetString(\"{v}\", 10); return n }}()"))
        }
        "url" => {
            let v = obj.get("value")?.as_str()?;
            let escaped = v.replace('"', "\\\"");
            Some(format!("func() *url.URL {{ u, _ := url.Parse(\"{escaped}\"); return u }}()"))
        }
        "error" => {
            let msg = obj.get("message").and_then(|m| m.as_str()).unwrap_or("");
            let escaped = msg.replace('"', "\\\"");
            Some(format!("errors.New(\"{escaped}\")"))
        }
        "uuid" => {
            let v = obj.get("value")?.as_str()?;
            Some(format!("/* uuid */ \"{v}\""))
        }
        "ip_address" => {
            let v = obj.get("value")?.as_str()?;
            Some(format!("net.ParseIP(\"{v}\")"))
        }
        "rune" => {
            let v = obj.get("value")?.as_str()?;
            if let Some(ch) = v.chars().next() {
                Some(format!("'{ch}'"))
            } else {
                Some("0".to_string())
            }
        }
        "go_byte" => {
            let v = obj.get("value")?;
            Some(format!("byte({})", v))
        }
        _ => None,
    }
}

/// Return the Go zero value for a type name.
fn go_zero_value(type_name: &str) -> String {
    match type_name {
        "string" => "\"\"".to_string(),
        "int" => "0".to_string(),
        "float64" => "0.0".to_string(),
        "bool" => "false".to_string(),
        _ => "nil".to_string(),
    }
}

/// Generate Vitest test source from a behavior map.
///
/// Similar to Jest but uses `import { describe, it, expect } from 'vitest'`
/// and imports the function from the module path.
pub fn generate_vitest_tests(
    behavior_map: &BehaviorMap,
    function_name: &str,
    module_path: &str,
) -> String {
    let mut out = String::new();

    out.push_str("import { describe, it, expect } from 'vitest';\n");
    out.push_str(&format!(
        "import {{ {function_name} }} from '{module_path}';\n\n"
    ));

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
///
/// Detects `__complex_type` tagged objects and emits constructor calls.
fn format_value(value: &serde_json::Value) -> String {
    if let Some(js) = try_format_complex_js(value) {
        return js;
    }
    match value {
        serde_json::Value::String(s) => format!("'{}'", escape_single_quotes(s)),
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        // For arrays/objects, use JSON.stringify-compatible output
        other => other.to_string(),
    }
}

/// Try to format a `__complex_type` tagged JSON object as a JavaScript constructor.
/// Returns `None` if the value is not a tagged complex type.
fn try_format_complex_js(value: &serde_json::Value) -> Option<String> {
    let obj = value.as_object()?;
    let tag = obj.get("__complex_type")?.as_str()?;
    match tag {
        "date" | "date_time" => {
            let v = obj.get("value")?;
            Some(format!("new Date({})", v))
        }
        "duration" => {
            let ms = obj.get("ms").or_else(|| obj.get("value"))?;
            Some(format!("/* Duration */ {}", ms))
        }
        "reg_exp" => {
            let source = obj.get("source")?.as_str()?;
            let flags = obj.get("flags").and_then(|f| f.as_str()).unwrap_or("");
            Some(format!("/{source}/{flags}"))
        }
        "big_int" => {
            let v = obj.get("value")?.as_str()?;
            Some(format!("BigInt('{v}')"))
        }
        "url" => {
            let v = obj.get("value")?.as_str()?;
            Some(format!("new URL('{}')", escape_single_quotes(v)))
        }
        "buffer" => {
            let v = obj.get("value")?.as_str()?;
            let enc = obj.get("encoding").and_then(|e| e.as_str()).unwrap_or("base64");
            Some(format!("Buffer.from('{v}', '{enc}')"))
        }
        "error" => {
            let class = obj.get("class").and_then(|c| c.as_str()).unwrap_or("Error");
            let msg = obj.get("message").and_then(|m| m.as_str()).unwrap_or("");
            Some(format!("new {class}('{}')", escape_single_quotes(msg)))
        }
        "uuid" | "path" | "email" | "mime_type" | "locale" | "sem_ver" => {
            let v = obj.get("value")?.as_str()?;
            Some(format!("'{}'", escape_single_quotes(v)))
        }
        "symbol" => {
            let desc = obj.get("description").and_then(|d| d.as_str()).unwrap_or("");
            Some(format!("Symbol('{}')", escape_single_quotes(desc)))
        }
        "option" => {
            let present = obj.get("present")?.as_bool()?;
            if present {
                let inner = obj.get("value")?;
                Some(format_value(inner))
            } else {
                Some("undefined".to_string())
            }
        }
        _ => None, // fall through to default JSON formatting
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
            dependency_trace: None,
        }
    }

    #[test]
    fn generates_import_statement() {
        let map = BehaviorMap {
            function_id: "add".to_string(),
            behaviors: vec![],
            fingerprint: None,
        };
        let output = generate_jest_tests(&map, "add", "./src/math");
        assert!(output.starts_with("import { add } from './src/math';\n"));
    }

    #[test]
    fn generates_describe_block() {
        let map = BehaviorMap {
            function_id: "add".to_string(),
            behaviors: vec![],
            fingerprint: None,
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
            fingerprint: None,
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
            fingerprint: None,
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
            fingerprint: None,
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
            fingerprint: None,
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
            fingerprint: None,
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
            fingerprint: None,
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

    // ── Go test generation ──────────────────────────────────────────────

    #[test]
    fn go_generates_package_and_import() {
        let map = BehaviorMap {
            function_id: "add".to_string(),
            behaviors: vec![],
            fingerprint: None,
        };
        let output = generate_go_tests(&map, "add", "examples");
        assert!(output.starts_with("package examples\n"), "output: {output}");
        assert!(output.contains("import \"testing\"\n"), "output: {output}");
    }

    #[test]
    fn go_generates_test_function_name_capitalized() {
        let map = BehaviorMap {
            function_id: "classifyNumber".to_string(),
            behaviors: vec![make_behavior(0, vec![json!(5)], Some(json!("positive")), None)],
            fingerprint: None,
        };
        let output = generate_go_tests(&map, "classifyNumber", "examples");
        assert!(
            output.contains("func TestClassifyNumber(t *testing.T)"),
            "output: {output}"
        );
    }

    #[test]
    fn go_empty_behavior_map_has_comment() {
        let map = BehaviorMap {
            function_id: "noop".to_string(),
            behaviors: vec![],
            fingerprint: None,
        };
        let output = generate_go_tests(&map, "noop", "main");
        assert!(output.contains("// No behaviors observed"), "output: {output}");
    }

    #[test]
    fn go_generates_table_driven_test_with_int_return() {
        let map = BehaviorMap {
            function_id: "abs".to_string(),
            behaviors: vec![
                make_behavior(0, vec![json!(5)], Some(json!(5)), None),
                make_behavior(1, vec![json!(-3)], Some(json!(3)), None),
                make_behavior(2, vec![json!(0)], Some(json!(0)), None),
            ],
            fingerprint: None,
        };
        let output = generate_go_tests(&map, "abs", "math");

        assert!(output.contains("tests := []struct {"), "output: {output}");
        assert!(output.contains("name string"), "output: {output}");
        assert!(output.contains("arg0     int"), "output: {output}");
        assert!(output.contains("expected int"), "output: {output}");
        assert!(output.contains("t.Run(tt.name,"), "output: {output}");
        assert!(output.contains("got := abs(tt.arg0)"), "output: {output}");
        assert!(output.contains("if got != tt.expected"), "output: {output}");

        // Should have 3 test cases
        let case_count = output.matches("{\"returns").count();
        assert_eq!(case_count, 3, "expected 3 test cases, output:\n{output}");
    }

    #[test]
    fn go_generates_string_return_values() {
        let map = BehaviorMap {
            function_id: "classifyNumber".to_string(),
            behaviors: vec![
                make_behavior(0, vec![json!(5)], Some(json!("positive")), None),
                make_behavior(1, vec![json!(-3)], Some(json!("negative")), None),
            ],
            fingerprint: None,
        };
        let output = generate_go_tests(&map, "classifyNumber", "examples");

        assert!(output.contains("expected string"), "output: {output}");
        assert!(output.contains("\"positive\""), "output: {output}");
        assert!(output.contains("\"negative\""), "output: {output}");
    }

    #[test]
    fn go_generates_bool_values() {
        let map = BehaviorMap {
            function_id: "isPositive".to_string(),
            behaviors: vec![
                make_behavior(0, vec![json!(5)], Some(json!(true)), None),
                make_behavior(1, vec![json!(-1)], Some(json!(false)), None),
            ],
            fingerprint: None,
        };
        let output = generate_go_tests(&map, "isPositive", "math");

        assert!(output.contains("expected bool"), "output: {output}");
        assert!(output.contains("true"), "output: {output}");
        assert!(output.contains("false"), "output: {output}");
    }

    #[test]
    fn go_generates_float_values() {
        let map = BehaviorMap {
            function_id: "half".to_string(),
            behaviors: vec![make_behavior(0, vec![json!(3.14)], Some(json!(1.57)), None)],
            fingerprint: None,
        };
        let output = generate_go_tests(&map, "half", "math");

        assert!(output.contains("arg0     float64"), "output: {output}");
        assert!(output.contains("expected float64"), "output: {output}");
    }

    #[test]
    fn go_generates_nil_return_for_null() {
        let map = BehaviorMap {
            function_id: "maybeNil".to_string(),
            behaviors: vec![make_behavior(0, vec![json!(null)], Some(json!(null)), None)],
            fingerprint: None,
        };
        let output = generate_go_tests(&map, "maybeNil", "main");

        assert!(output.contains("nil"), "output: {output}");
    }

    #[test]
    fn go_generates_multiple_params() {
        let map = BehaviorMap {
            function_id: "add".to_string(),
            behaviors: vec![make_behavior(0, vec![json!(1), json!(2)], Some(json!(3)), None)],
            fingerprint: None,
        };
        let output = generate_go_tests(&map, "add", "math");

        assert!(output.contains("arg0     int"), "output: {output}");
        assert!(output.contains("arg1     int"), "output: {output}");
        assert!(output.contains("got := add(tt.arg0, tt.arg1)"), "output: {output}");
    }

    #[test]
    fn go_generates_panic_test_with_defer_recover() {
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
            fingerprint: None,
        };
        let output = generate_go_tests(&map, "divide", "math");

        assert!(output.contains("defer func()"), "output: {output}");
        assert!(output.contains("recover()"), "output: {output}");
        assert!(
            output.contains("expected panic but did not get one"),
            "output: {output}"
        );
        assert!(output.contains("divide(1, 0)"), "output: {output}");
    }

    #[test]
    fn go_mixed_normal_and_panic_behaviors() {
        let map = BehaviorMap {
            function_id: "safeDivide".to_string(),
            behaviors: vec![
                make_behavior(0, vec![json!(10), json!(2)], Some(json!(5)), None),
                make_behavior(
                    1,
                    vec![json!(1), json!(0)],
                    None,
                    Some(ErrorInfo {
                        error_type: "Error".to_string(),
                        message: "division by zero".to_string(),
                        stack: None,
                    }),
                ),
            ],
            fingerprint: None,
        };
        let output = generate_go_tests(&map, "safeDivide", "math");

        // Should have both table-driven normal test AND panic subtest
        assert!(output.contains("tests := []struct {"), "output: {output}");
        assert!(output.contains("defer func()"), "output: {output}");
    }

    #[test]
    fn go_type_detection_from_values() {
        assert_eq!(go_type_from_value(&json!(42)), "int");
        assert_eq!(go_type_from_value(&json!(3.14)), "float64");
        assert_eq!(go_type_from_value(&json!("hello")), "string");
        assert_eq!(go_type_from_value(&json!(true)), "bool");
        assert_eq!(go_type_from_value(&json!(null)), "interface{}");
    }

    #[test]
    fn go_format_value_all_types() {
        assert_eq!(format_go_value(&json!(42)), "42");
        assert_eq!(format_go_value(&json!(3.14)), "3.14");
        assert_eq!(format_go_value(&json!(true)), "true");
        assert_eq!(format_go_value(&json!(false)), "false");
        assert_eq!(format_go_value(&json!(null)), "nil");
        assert_eq!(format_go_value(&json!("hello")), "\"hello\"");
    }

    #[test]
    fn go_escapes_double_quotes_in_strings() {
        assert_eq!(format_go_value(&json!("say \"hi\"")), "\"say \\\"hi\\\"\"");
    }

    #[test]
    fn capitalize_first_works() {
        assert_eq!(capitalize_first("hello"), "Hello");
        assert_eq!(capitalize_first("classifyNumber"), "ClassifyNumber");
        assert_eq!(capitalize_first("A"), "A");
        assert_eq!(capitalize_first(""), "");
    }

    // ── Complex type formatting ─────────────────────────────────────────

    #[test]
    fn format_value_date_produces_js_constructor() {
        let val = json!({"__complex_type": "date", "value": 1704067200000_i64});
        assert_eq!(format_value(&val), "new Date(1704067200000)");
    }

    #[test]
    fn format_value_regexp_produces_literal() {
        let val = json!({"__complex_type": "reg_exp", "source": "\\d+", "flags": "g"});
        assert_eq!(format_value(&val), "/\\d+/g");
    }

    #[test]
    fn format_value_bigint_produces_constructor() {
        let val = json!({"__complex_type": "big_int", "value": "99999999999999999999"});
        assert_eq!(format_value(&val), "BigInt('99999999999999999999')");
    }

    #[test]
    fn format_value_url_produces_constructor() {
        let val = json!({"__complex_type": "url", "value": "https://example.com"});
        assert_eq!(format_value(&val), "new URL('https://example.com')");
    }

    #[test]
    fn format_value_error_produces_constructor() {
        let val = json!({"__complex_type": "error", "class": "TypeError", "message": "oops"});
        assert_eq!(format_value(&val), "new TypeError('oops')");
    }

    #[test]
    fn format_value_uuid_produces_string() {
        let val = json!({"__complex_type": "uuid", "value": "550e8400-e29b-41d4-a716-446655440000"});
        assert_eq!(format_value(&val), "'550e8400-e29b-41d4-a716-446655440000'");
    }

    #[test]
    fn format_value_symbol_produces_constructor() {
        let val = json!({"__complex_type": "symbol", "description": "mySymbol"});
        assert_eq!(format_value(&val), "Symbol('mySymbol')");
    }

    #[test]
    fn format_value_option_some() {
        let val = json!({"__complex_type": "option", "present": true, "value": 42});
        assert_eq!(format_value(&val), "42");
    }

    #[test]
    fn format_value_option_none() {
        let val = json!({"__complex_type": "option", "present": false});
        assert_eq!(format_value(&val), "undefined");
    }

    #[test]
    fn format_value_unknown_complex_falls_through() {
        let val = json!({"__complex_type": "some_future_type", "value": "x"});
        // Should fall through to default JSON formatting
        let result = format_value(&val);
        assert!(result.contains("__complex_type"), "should contain raw JSON: {result}");
    }

    #[test]
    fn go_format_value_date_produces_constructor() {
        let val = json!({"__complex_type": "date", "value": 1704067200000_i64});
        assert_eq!(format_go_value(&val), "time.UnixMilli(1704067200000)");
    }

    #[test]
    fn go_format_value_regexp_produces_compile() {
        let val = json!({"__complex_type": "reg_exp", "source": "\\d+"});
        assert_eq!(format_go_value(&val), "regexp.MustCompile(\"\\\\d+\")");
    }

    #[test]
    fn go_format_value_error_produces_errors_new() {
        let val = json!({"__complex_type": "error", "message": "bad input"});
        assert_eq!(format_go_value(&val), "errors.New(\"bad input\")");
    }

    #[test]
    fn go_type_from_complex_date() {
        let val = json!({"__complex_type": "date", "value": 0});
        assert_eq!(go_type_from_value(&val), "time.Time");
    }

    #[test]
    fn go_type_from_complex_regexp() {
        let val = json!({"__complex_type": "reg_exp", "source": ".*"});
        assert_eq!(go_type_from_value(&val), "*regexp.Regexp");
    }

    #[test]
    fn go_type_from_complex_unknown_is_interface() {
        let val = json!({"__complex_type": "some_future_type", "value": "x"});
        assert_eq!(go_type_from_value(&val), "interface{}");
    }

    // ── Vitest test generation ────────────────────────────────────────────

    #[test]
    fn vitest_generates_vitest_import() {
        let map = BehaviorMap {
            function_id: "add".to_string(),
            behaviors: vec![],
            fingerprint: None,
        };
        let output = generate_vitest_tests(&map, "add", "./src/math");
        assert!(output.contains("import { describe, it, expect } from 'vitest';"));
    }

    #[test]
    fn vitest_generates_function_import() {
        let map = BehaviorMap {
            function_id: "add".to_string(),
            behaviors: vec![],
            fingerprint: None,
        };
        let output = generate_vitest_tests(&map, "add", "./src/math");
        assert!(output.contains("import { add } from './src/math';"));
    }

    #[test]
    fn vitest_generates_describe_block() {
        let map = BehaviorMap {
            function_id: "add".to_string(),
            behaviors: vec![],
            fingerprint: None,
        };
        let output = generate_vitest_tests(&map, "add", "./src/math");
        assert!(output.contains("describe('add', () => {"));
        assert!(output.ends_with("});\n"));
    }

    #[test]
    fn vitest_generates_it_block_for_return_value() {
        let map = BehaviorMap {
            function_id: "add".to_string(),
            behaviors: vec![make_behavior(0, vec![json!(1), json!(2)], Some(json!(3)), None)],
            fingerprint: None,
        };
        let output = generate_vitest_tests(&map, "add", "./src/math");
        assert!(output.contains("it('returns 3 for input (1, 2)'"));
        assert!(output.contains("const result = add(1, 2);"));
        assert!(output.contains("expect(result).toEqual(3);"));
    }

    #[test]
    fn vitest_generates_thrown_error_test() {
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
            fingerprint: None,
        };
        let output = generate_vitest_tests(&map, "divide", "./src/math");
        assert!(output.contains("expect(() => divide(1, 0)).toThrow();"));
    }

    #[test]
    fn vitest_generates_multiple_behaviors() {
        let map = BehaviorMap {
            function_id: "abs".to_string(),
            behaviors: vec![
                make_behavior(0, vec![json!(5)], Some(json!(5)), None),
                make_behavior(1, vec![json!(-3)], Some(json!(3)), None),
                make_behavior(2, vec![json!(0)], Some(json!(0)), None),
            ],
            fingerprint: None,
        };
        let output = generate_vitest_tests(&map, "abs", "./src/math");
        let it_count = output.matches("  it('").count();
        assert_eq!(it_count, 3, "expected 3 it blocks, output:\n{output}");
    }
}
