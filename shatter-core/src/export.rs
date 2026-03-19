//! Test export: generate test files from behavior maps.
//!
//! Converts a [`BehaviorMap`] into a test file for a specific framework.
//! Supports Jest (TypeScript) and Go table-driven tests.
//!
//! ## MC/DC annotations
//!
//! When MC/DC data is available (exploration was run with `--mcdc`), the export
//! functions can annotate test cases with comments documenting which MC/DC
//! independence pairs they satisfy. Call [`find_mcdc_pairs`] to derive
//! annotations from a behavior map's branch paths, then pass the result to
//! the `_with_annotations` variants of the export functions.

use crate::behavior::{Behavior, BehaviorMap};
use crate::execution_record::BranchDecision;

// ---------------------------------------------------------------------------
// MC/DC annotation types
// ---------------------------------------------------------------------------

/// An annotation describing an MC/DC independence pair satisfied by a behavior.
///
/// When a test case participates in an independence pair for condition `i` of
/// decision `branch_id`, this annotation is attached to that test. It is purely
/// informational — the test assertion itself is unchanged.
#[derive(Debug, Clone, PartialEq)]
pub struct McdcAnnotation {
    /// Branch ID of the compound decision.
    pub branch_id: u32,
    /// Source line of the decision (from the branch path).
    pub line: u32,
    /// Zero-based index of the condition within the decision.
    pub condition_index: usize,
    /// Input args of the paired behavior (the other half of the independence pair).
    pub paired_inputs: Vec<serde_json::Value>,
    /// Decision outcome in this behavior (`true` or `false`).
    pub this_outcome: bool,
    /// Decision outcome in the paired behavior.
    pub paired_outcome: bool,
}

impl McdcAnnotation {
    /// Render this annotation as one or two comment lines suitable for TypeScript/Jest.
    ///
    /// Produces:
    /// ```text
    /// // MC/DC: condition 0 independently affects decision at line 15
    /// // Pair: [5, 3] → true vs [-1, 3] → false
    /// ```
    pub fn to_ts_comment(&self, indent: &str) -> String {
        let paired_args = format_args_short(&self.paired_inputs);
        let this_outcome = self.this_outcome;
        let paired_outcome = self.paired_outcome;
        format!(
            "{indent}// MC/DC: condition {} independently affects decision at line {}\n\
             {indent}// Pair: this → {this_outcome} vs [{paired_args}] → {paired_outcome}\n",
            self.condition_index, self.line,
        )
    }

    /// Render this annotation as comment lines suitable for Go tests.
    ///
    /// Produces:
    /// ```text
    /// // MC/DC: condition 0 independently affects decision at line 15
    /// // Pair: this → true vs [paired args] → false
    /// ```
    pub fn to_go_comment(&self, indent: &str) -> String {
        // Go uses the same comment format; delegate to the TS variant.
        self.to_ts_comment(indent)
    }
}

/// Find MC/DC independence pairs from a behavior map's branch paths.
///
/// Scans each behavior's `branch_path` for `BranchDecision` entries that carry
/// `conditions` data (present when MC/DC mode was active). For each compound
/// decision found across all behaviors, checks whether any pair of behaviors
/// satisfies the unique-cause masking MC/DC criterion for some condition `i`:
///
/// - Condition `i` has opposite concrete values in the two behaviors.
/// - All other non-masked conditions have the same values in both behaviors.
/// - The decision outcome (taken/not-taken) differs.
///
/// Returns a list of `(behavior_index, annotation)` pairs. A single behavior
/// may appear more than once if it participates in independence pairs for
/// multiple conditions or decisions. The `behavior_index` is the position of
/// the behavior in `behavior_map.behaviors`.
///
/// When MC/DC data is absent (no behavior has any `conditions` data), returns
/// an empty Vec.
pub fn find_mcdc_pairs(behavior_map: &BehaviorMap) -> Vec<(usize, McdcAnnotation)> {
    use std::collections::HashMap;

    // Index behaviors by branch_id → Vec<(behavior_index, &BranchDecision)>
    let mut by_branch: HashMap<u32, Vec<(usize, &BranchDecision)>> = HashMap::new();
    for (bidx, behavior) in behavior_map.behaviors.iter().enumerate() {
        for decision in &behavior.branch_path {
            if decision.conditions.is_some() {
                by_branch
                    .entry(decision.branch_id)
                    .or_default()
                    .push((bidx, decision));
            }
        }
    }

    if by_branch.is_empty() {
        return vec![];
    }

    let mut result: Vec<(usize, McdcAnnotation)> = Vec::new();

    // For each decision, find independence pairs.
    for entries in by_branch.values() {
        // Determine number of conditions from the first entry.
        let Some((_, first_dec)) = entries.first() else {
            continue;
        };
        let Some(ref first_conds) = first_dec.conditions else {
            continue;
        };
        let num_conditions = first_conds.len();

        // For each condition i, find the first pair that satisfies independence.
        // Track which (behavior_a, behavior_b) pairs we've already emitted for this
        // condition to avoid duplicate annotations.
        for cond_i in 0..num_conditions {
            'pair: for a in 0..entries.len() {
                for b in (a + 1)..entries.len() {
                    let (bidx_a, dec_a) = entries[a];
                    let (bidx_b, dec_b) = entries[b];

                    let conds_a = match dec_a.conditions.as_deref() {
                        Some(c) => c,
                        None => continue,
                    };
                    let conds_b = match dec_b.conditions.as_deref() {
                        Some(c) => c,
                        None => continue,
                    };

                    // Condition i must be observed with opposite concrete values.
                    let val_a = conds_a
                        .iter()
                        .find(|c| c.condition_index as usize == cond_i)
                        .and_then(|c| if c.masked { None } else { c.value });
                    let val_b = conds_b
                        .iter()
                        .find(|c| c.condition_index as usize == cond_i)
                        .and_then(|c| if c.masked { None } else { c.value });

                    let (va, vb) = match (val_a, val_b) {
                        (Some(a), Some(b)) => (a, b),
                        _ => continue,
                    };
                    if va == vb {
                        continue;
                    }

                    // Decision outcomes must differ.
                    if dec_a.taken == dec_b.taken {
                        continue;
                    }

                    // All other non-masked conditions must agree.
                    let mut all_others_agree = true;
                    for j in 0..num_conditions {
                        if j == cond_i {
                            continue;
                        }
                        let ja = conds_a
                            .iter()
                            .find(|c| c.condition_index as usize == j)
                            .and_then(|c| if c.masked { None } else { c.value });
                        let jb = conds_b
                            .iter()
                            .find(|c| c.condition_index as usize == j)
                            .and_then(|c| if c.masked { None } else { c.value });
                        match (ja, jb) {
                            (Some(ca), Some(cb)) if ca != cb => {
                                all_others_agree = false;
                                break;
                            }
                            _ => {}
                        }
                    }

                    if all_others_agree {
                        let inputs_a =
                            behavior_map.behaviors[bidx_a].input_args.clone();
                        let inputs_b =
                            behavior_map.behaviors[bidx_b].input_args.clone();
                        // Annotate behavior_a: paired with behavior_b's inputs.
                        result.push((
                            bidx_a,
                            McdcAnnotation {
                                branch_id: dec_a.branch_id,
                                line: dec_a.line,
                                condition_index: cond_i,
                                paired_inputs: inputs_b.clone(),
                                this_outcome: dec_a.taken,
                                paired_outcome: dec_b.taken,
                            },
                        ));
                        // Annotate behavior_b: paired with behavior_a's inputs.
                        result.push((
                            bidx_b,
                            McdcAnnotation {
                                branch_id: dec_b.branch_id,
                                line: dec_b.line,
                                condition_index: cond_i,
                                paired_inputs: inputs_a,
                                this_outcome: dec_b.taken,
                                paired_outcome: dec_a.taken,
                            },
                        ));
                        break 'pair;
                    }
                }
            }
        }
    }

    result
}

// ---------------------------------------------------------------------------
// Test generation
// ---------------------------------------------------------------------------

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
    generate_jest_tests_with_annotations(behavior_map, function_name, module_path, &[])
}

/// Like [`generate_jest_tests`] but inserts MC/DC pair annotations above each
/// test case that participates in an independence pair.
///
/// # Arguments
/// * `annotations` - Per-behavior annotations returned by [`find_mcdc_pairs`].
///   Each entry is `(behavior_index, annotation)`. When empty, output is identical
///   to [`generate_jest_tests`].
pub fn generate_jest_tests_with_annotations(
    behavior_map: &BehaviorMap,
    function_name: &str,
    module_path: &str,
    annotations: &[(usize, McdcAnnotation)],
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

    for (idx, behavior) in behavior_map.behaviors.iter().enumerate() {
        // Collect all MC/DC annotations for this behavior index.
        for (_, ann) in annotations.iter().filter(|(i, _)| *i == idx) {
            out.push_str(&ann.to_ts_comment("  "));
        }

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
    generate_go_tests_with_annotations(behavior_map, function_name, package_name, &[])
}

/// Like [`generate_go_tests`] but inserts MC/DC pair annotations as comments
/// above each panic test case and as a header comment block for table-driven cases.
///
/// # Arguments
/// * `annotations` - Per-behavior annotations returned by [`find_mcdc_pairs`].
///   When empty, output is identical to [`generate_go_tests`].
pub fn generate_go_tests_with_annotations(
    behavior_map: &BehaviorMap,
    function_name: &str,
    package_name: &str,
    annotations: &[(usize, McdcAnnotation)],
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

    // Separate panic behaviors (with original indices) from normal behaviors.
    let indexed_behaviors: Vec<(usize, &Behavior)> =
        behavior_map.behaviors.iter().enumerate().collect();
    let (panic_behaviors, normal_behaviors): (Vec<_>, Vec<_>) =
        indexed_behaviors.iter().partition(|(_, b)| b.thrown_error.is_some());

    if !normal_behaviors.is_empty() {
        // Emit MC/DC annotation block for table-driven cases, before the table.
        // Go struct literals don't support per-row comments, so we emit a grouped
        // comment block listing which test cases participate in MC/DC pairs.
        let table_annotations: Vec<(usize, &McdcAnnotation)> = annotations
            .iter()
            .filter(|(i, _)| normal_behaviors.iter().any(|(bidx, _)| bidx == i))
            .map(|(i, a)| (*i, a))
            .collect();
        if !table_annotations.is_empty() {
            for (bidx, ann) in &table_annotations {
                let behavior = &behavior_map.behaviors[*bidx];
                let test_name = build_test_name(behavior);
                out.push_str(&format!("\t// MC/DC: \"{test_name}\"\n"));
                out.push_str(&format!(
                    "\t//   condition {} independently affects decision at line {}\n",
                    ann.condition_index, ann.line
                ));
                let paired_args = format_args_short(&ann.paired_inputs);
                out.push_str(&format!(
                    "\t//   this → {} vs [{paired_args}] → {}\n",
                    ann.this_outcome, ann.paired_outcome
                ));
            }
        }

        out.push_str("\ttests := []struct {\n");
        out.push_str("\t\tname string\n");

        // Determine param types from first behavior
        let (_, first) = &normal_behaviors[0];
        for (i, arg) in first.input_args.iter().enumerate() {
            let go_type = go_type_from_value(arg);
            out.push_str(&format!("\t\targ{i}     {go_type}\n"));
        }

        // Determine return type from first behavior with a return value
        let has_return = normal_behaviors.iter().any(|(_, b)| b.return_value.is_some());
        if has_return {
            let return_type = normal_behaviors
                .iter()
                .find_map(|(_, b)| b.return_value.as_ref())
                .map(go_type_from_value)
                .unwrap_or_else(|| "interface{}".to_string());
            out.push_str(&format!("\t\texpected {return_type}\n"));
        }

        out.push_str("\t}{{\n");

        for (_, behavior) in &normal_behaviors {
            let test_name = build_test_name(behavior);
            let args: Vec<String> = behavior.input_args.iter().map(format_go_value).collect();

            if has_return {
                let expected = match &behavior.return_value {
                    Some(val) => format_go_value(val),
                    None => go_zero_value(
                        normal_behaviors
                            .iter()
                            .find_map(|(_, b)| b.return_value.as_ref())
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

        let arg_refs: Vec<String> = (0..normal_behaviors[0].1.input_args.len())
            .map(|i| format!("tt.arg{i}"))
            .collect();
        let call = format!("{function_name}({})", arg_refs.join(", "));

        if has_return {
            out.push_str(&format!("\t\t\tgot := {call}\n"));
            out.push_str(&format!(
                "\t\t\tif got != tt.expected {{\n\t\t\t\tt.Errorf(\"{function_name}({}) = %v, want %v\", {}, got, tt.expected)\n\t\t\t}}\n",
                (0..normal_behaviors[0].1.input_args.len()).map(|_| "%v").collect::<Vec<_>>().join(", "),
                arg_refs.join(", "),
            ));
        } else {
            out.push_str(&format!("\t\t\t{call}\n"));
        }

        out.push_str("\t\t})\n");
        out.push_str("\t}\n");
    }

    // Generate panic test cases, with per-case MC/DC annotations.
    for (bidx, behavior) in &panic_behaviors {
        for (_, ann) in annotations.iter().filter(|(i, _)| i == bidx) {
            out.push_str(&ann.to_go_comment("\t"));
        }

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
    generate_vitest_tests_with_annotations(behavior_map, function_name, module_path, &[])
}

/// Like [`generate_vitest_tests`] but inserts MC/DC pair annotations above each
/// test case that participates in an independence pair.
///
/// # Arguments
/// * `annotations` - Per-behavior annotations returned by [`find_mcdc_pairs`].
///   When empty, output is identical to [`generate_vitest_tests`].
pub fn generate_vitest_tests_with_annotations(
    behavior_map: &BehaviorMap,
    function_name: &str,
    module_path: &str,
    annotations: &[(usize, McdcAnnotation)],
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

    for (idx, behavior) in behavior_map.behaviors.iter().enumerate() {
        for (_, ann) in annotations.iter().filter(|(i, _)| *i == idx) {
            out.push_str(&ann.to_ts_comment("  "));
        }

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
            mock_values: vec![],
        }
    }

    #[test]
    fn generates_import_statement() {
        let map = BehaviorMap {
            function_id: "add".to_string(),
            behaviors: vec![],
            fingerprint: None,
            nondeterministic_fields: vec![],
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
            nondeterministic_fields: vec![],
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
            nondeterministic_fields: vec![],
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
            nondeterministic_fields: vec![],
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
                    stack: None, error_category: None }),
            )],
            fingerprint: None,
            nondeterministic_fields: vec![],
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
            nondeterministic_fields: vec![],
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
            nondeterministic_fields: vec![],
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
            nondeterministic_fields: vec![],
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
            nondeterministic_fields: vec![],
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
            nondeterministic_fields: vec![],
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
            nondeterministic_fields: vec![],
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
            nondeterministic_fields: vec![],
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
            nondeterministic_fields: vec![],
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
            nondeterministic_fields: vec![],
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
            nondeterministic_fields: vec![],
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
            nondeterministic_fields: vec![],
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
            nondeterministic_fields: vec![],
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
                    stack: None, error_category: None }),
            )],
            fingerprint: None,
            nondeterministic_fields: vec![],
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
                        stack: None, error_category: None }),
                ),
            ],
            fingerprint: None,
            nondeterministic_fields: vec![],
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
            nondeterministic_fields: vec![],
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
            nondeterministic_fields: vec![],
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
            nondeterministic_fields: vec![],
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
            nondeterministic_fields: vec![],
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
                    stack: None, error_category: None }),
            )],
            fingerprint: None,
            nondeterministic_fields: vec![],
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
            nondeterministic_fields: vec![],
        };
        let output = generate_vitest_tests(&map, "abs", "./src/math");
        let it_count = output.matches("  it('").count();
        assert_eq!(it_count, 3, "expected 3 it blocks, output:\n{output}");
    }

    // ── MC/DC annotation tests ───────────────────────────────────────────

    fn make_behavior_with_conditions(
        id: u32,
        inputs: Vec<serde_json::Value>,
        return_value: Option<serde_json::Value>,
        branch_id: u32,
        line: u32,
        taken: bool,
        conditions: Vec<crate::execution_record::ConditionOutcome>,
    ) -> Behavior {
        use crate::execution_record::{BranchDecision, SymConstraint};
        Behavior {
            id,
            input_args: inputs,
            return_value,
            thrown_error: None,
            branch_path: vec![BranchDecision {
                branch_id,
                line,
                taken,
                constraint: SymConstraint::Unknown { hint: String::new() },
                conditions: Some(conditions),
            }],
            side_effects: vec![],
            dependency_trace: None,
            mock_values: vec![],
        }
    }

    fn make_condition_outcome(
        condition_index: u32,
        value: Option<bool>,
        masked: bool,
    ) -> crate::execution_record::ConditionOutcome {
        use crate::execution_record::SymConstraint;
        crate::execution_record::ConditionOutcome {
            condition_index,
            value,
            masked,
            constraint: SymConstraint::Unknown { hint: String::new() },
        }
    }

    /// Build a BehaviorMap from two behaviors that form an independence pair
    /// for condition 0 of decision at branch_id=0, line=15.
    /// Behavior 0: inputs=[5, 3], conditions=[cond0=T, cond1=T], taken=true
    /// Behavior 1: inputs=[-1, 3], conditions=[cond0=F, cond1=T], taken=false
    /// → condition 0 independently affects the decision.
    fn make_mcdc_behavior_map() -> BehaviorMap {
        let b0 = make_behavior_with_conditions(
            0,
            vec![json!(5), json!(3)],
            Some(json!("can drive")),
            0,   // branch_id
            15,  // line
            true, // taken
            vec![
                make_condition_outcome(0, Some(true), false),
                make_condition_outcome(1, Some(true), false),
            ],
        );
        let b1 = make_behavior_with_conditions(
            1,
            vec![json!(-1), json!(3)],
            Some(json!("cannot drive")),
            0,    // branch_id
            15,   // line
            false, // taken
            vec![
                make_condition_outcome(0, Some(false), false),
                make_condition_outcome(1, Some(true), false),
            ],
        );
        BehaviorMap {
            function_id: "classify".to_string(),
            behaviors: vec![b0, b1],
            fingerprint: None,
            nondeterministic_fields: vec![],
        }
    }

    #[test]
    fn find_mcdc_pairs_detects_independence_pair() {
        let map = make_mcdc_behavior_map();
        let pairs = find_mcdc_pairs(&map);

        // Condition 0 has a pair (two behaviors satisfy independence for it).
        // Condition 1 does not (same value in both observations; no pair found).
        assert!(!pairs.is_empty(), "expected at least one pair to be found");

        // Both behavior 0 and behavior 1 should be annotated.
        let bidx_0_count = pairs.iter().filter(|(i, _)| *i == 0).count();
        let bidx_1_count = pairs.iter().filter(|(i, _)| *i == 1).count();
        assert!(bidx_0_count > 0, "behavior 0 should be annotated");
        assert!(bidx_1_count > 0, "behavior 1 should be annotated");

        // Verify annotation content for behavior 0.
        let (_, ann0) = pairs.iter().find(|(i, _)| *i == 0).unwrap();
        assert_eq!(ann0.branch_id, 0);
        assert_eq!(ann0.line, 15);
        assert_eq!(ann0.condition_index, 0);
        assert_eq!(ann0.this_outcome, true);
        assert_eq!(ann0.paired_outcome, false);
    }

    #[test]
    fn find_mcdc_pairs_returns_empty_without_conditions_data() {
        // Behaviors with no `conditions` data (non-MC/DC mode).
        let map = BehaviorMap {
            function_id: "add".to_string(),
            behaviors: vec![
                make_behavior(0, vec![json!(1), json!(2)], Some(json!(3)), None),
                make_behavior(1, vec![json!(-1), json!(0)], Some(json!(-1)), None),
            ],
            fingerprint: None,
            nondeterministic_fields: vec![],
        };
        let pairs = find_mcdc_pairs(&map);
        assert!(
            pairs.is_empty(),
            "expected no pairs when MC/DC data is absent, got: {pairs:?}"
        );
    }

    #[test]
    fn jest_export_contains_mcdc_annotation_when_present() {
        let map = make_mcdc_behavior_map();
        let annotations = find_mcdc_pairs(&map);
        assert!(!annotations.is_empty(), "test setup: expected pairs to be found");

        let output =
            generate_jest_tests_with_annotations(&map, "classify", "./src/classify", &annotations);

        assert!(
            output.contains("// MC/DC: condition 0 independently affects decision at line 15"),
            "expected MC/DC annotation comment in output:\n{output}"
        );
        assert!(
            output.contains("// Pair: this →"),
            "expected MC/DC pair comment in output:\n{output}"
        );
        // Test assertions must still be present and unchanged.
        assert!(
            output.contains("expect(result).toEqual("),
            "test assertion must remain in output:\n{output}"
        );
    }

    #[test]
    fn jest_export_no_annotation_when_mcdc_data_absent() {
        let map = BehaviorMap {
            function_id: "add".to_string(),
            behaviors: vec![
                make_behavior(0, vec![json!(1), json!(2)], Some(json!(3)), None),
                make_behavior(1, vec![json!(-1), json!(0)], Some(json!(-1)), None),
            ],
            fingerprint: None,
            nondeterministic_fields: vec![],
        };
        // Standard generate_jest_tests (no annotations).
        let output = generate_jest_tests(&map, "add", "./src/math");
        assert!(
            !output.contains("// MC/DC"),
            "expected no MC/DC annotation in output without MC/DC data:\n{output}"
        );
    }

    #[test]
    fn vitest_export_contains_mcdc_annotation_when_present() {
        let map = make_mcdc_behavior_map();
        let annotations = find_mcdc_pairs(&map);

        let output = generate_vitest_tests_with_annotations(
            &map,
            "classify",
            "./src/classify",
            &annotations,
        );

        assert!(
            output.contains("// MC/DC: condition 0 independently affects decision at line 15"),
            "expected MC/DC annotation comment in output:\n{output}"
        );
    }

    #[test]
    fn vitest_export_no_annotation_when_mcdc_data_absent() {
        let map = BehaviorMap {
            function_id: "add".to_string(),
            behaviors: vec![make_behavior(0, vec![json!(1)], Some(json!(2)), None)],
            fingerprint: None,
            nondeterministic_fields: vec![],
        };
        let output = generate_vitest_tests(&map, "add", "./src/math");
        assert!(!output.contains("// MC/DC"), "expected no MC/DC annotation:\n{output}");
    }

    #[test]
    fn go_export_contains_mcdc_annotation_when_present() {
        let map = make_mcdc_behavior_map();
        let annotations = find_mcdc_pairs(&map);

        let output =
            generate_go_tests_with_annotations(&map, "classify", "main", &annotations);

        assert!(
            output.contains("// MC/DC:"),
            "expected MC/DC comment in Go output:\n{output}"
        );
        assert!(
            output.contains("independently affects decision at line 15"),
            "expected line reference in Go MC/DC comment:\n{output}"
        );
    }

    #[test]
    fn go_export_no_annotation_when_mcdc_data_absent() {
        let map = BehaviorMap {
            function_id: "add".to_string(),
            behaviors: vec![
                make_behavior(0, vec![json!(1), json!(2)], Some(json!(3)), None),
            ],
            fingerprint: None,
            nondeterministic_fields: vec![],
        };
        let output = generate_go_tests(&map, "add", "math");
        assert!(!output.contains("// MC/DC"), "expected no MC/DC annotation:\n{output}");
    }

    // ── Property-based tests ─────────────────────────────────────────────

    mod proptests {
        use super::*;
        use crate::test_arbitraries::arb_behavior_map;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn jest_export_structural_validity(
                map in arb_behavior_map(),
                fname in "[a-zA-Z_][a-zA-Z0-9_]{0,12}",
            ) {
                let output = generate_jest_tests(&map, &fname, "./src/module");

                let import_expect = format!("import {{ {} }}", fname);
                let describe_expect = format!("describe('{}',", fname);
                prop_assert!(output.contains(&import_expect));
                prop_assert!(output.contains(&describe_expect));

                let it_count = output.matches("  it('").count();
                prop_assert_eq!(it_count, map.behaviors.len());
            }

            #[test]
            fn go_export_structural_validity(
                map in arb_behavior_map(),
                fname in "[a-zA-Z_][a-zA-Z0-9_]{0,12}",
            ) {
                let output = generate_go_tests(&map, &fname, "main");

                prop_assert!(output.contains("package main"));
                prop_assert!(output.contains("import \"testing\""));
                prop_assert!(output.contains("func Test"));
            }

            #[test]
            fn vitest_export_structural_validity(
                map in arb_behavior_map(),
                fname in "[a-zA-Z_][a-zA-Z0-9_]{0,12}",
            ) {
                let output = generate_vitest_tests(&map, &fname, "./src/module");

                let import_expect = format!("import {{ {} }}", fname);
                let describe_expect = format!("describe('{}',", fname);
                let vitest_import = "import { describe, it, expect } from 'vitest'";
                prop_assert!(output.contains(vitest_import));
                prop_assert!(output.contains(&import_expect));
                prop_assert!(output.contains(&describe_expect));

                let it_count = output.matches("  it('").count();
                prop_assert_eq!(it_count, map.behaviors.len());
            }
        }
    }
}
