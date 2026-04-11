//! Build script: generates `string_ops_generated.rs` from `data/string-ops.yaml`.
//!
//! Validates the YAML spec (no duplicate method names, valid sorts/styles) and
//! emits a `StringOp` enum with `z3_sort()`, `resolve_string_op()`, and
//! sort-grouped constant arrays.

use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::path::Path;

#[derive(Deserialize)]
struct Spec {
    operations: Vec<Operation>,
}

#[derive(Deserialize)]
struct Operation {
    name: String,
    z3_sort: String,
    aliases: Vec<Alias>,
}

#[derive(Deserialize)]
struct Alias {
    // Parsed for validation completeness; not used in codegen since aliases are
    // grouped by canonical operation, not by language.
    #[allow(dead_code)]
    language: String,
    method: String,
    style: String,
}

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let yaml_path = Path::new(&manifest_dir).join("data/string-ops.yaml");
    println!("cargo::rerun-if-changed={}", yaml_path.display());

    let yaml_content = fs::read_to_string(&yaml_path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", yaml_path.display()));
    let spec: Spec = serde_yaml::from_str(&yaml_content)
        .unwrap_or_else(|e| panic!("failed to parse {}: {e}", yaml_path.display()));

    validate(&spec);

    let out_dir = env::var("OUT_DIR").unwrap();
    let out_path = Path::new(&out_dir).join("string_ops_generated.rs");
    let code = generate(&spec);
    fs::write(&out_path, code)
        .unwrap_or_else(|e| panic!("failed to write {}: {e}", out_path.display()));
}

fn validate(spec: &Spec) {
    let valid_sorts = ["Bool", "Int", "Str"];
    let valid_styles = ["receiver", "free"];
    let mut seen_methods: HashMap<String, String> = HashMap::new();

    for op in &spec.operations {
        assert!(
            valid_sorts.contains(&op.z3_sort.as_str()),
            "operation '{}' has invalid z3_sort '{}' (expected one of {valid_sorts:?})",
            op.name,
            op.z3_sort,
        );

        for alias in &op.aliases {
            assert!(
                valid_styles.contains(&alias.style.as_str()),
                "alias '{}' in operation '{}' has invalid style '{}' (expected one of {valid_styles:?})",
                alias.method,
                op.name,
                alias.style,
            );

            if let Some(prev_op) = seen_methods.get(&alias.method) {
                if prev_op != &op.name {
                    panic!(
                        "duplicate method name '{}': defined in both '{}' and '{}'",
                        alias.method, prev_op, op.name,
                    );
                }
                // Same operation, different language — allowed (e.g. Go "len" and Rust "len")
            } else {
                seen_methods.insert(alias.method.clone(), op.name.clone());
            }
        }
    }
}

fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

fn generate(spec: &Spec) -> String {
    let mut code = String::new();

    // --- StringOp enum ---
    code.push_str("/// Canonical string operations, generated from `data/string-ops.yaml`.\n");
    code.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]\n");
    code.push_str("#[allow(dead_code)] // Variants used via resolve_string_op() in solver\n");
    code.push_str("enum StringOp {\n");
    for op in &spec.operations {
        code.push_str(&format!("    {},\n", to_pascal_case(&op.name)));
    }
    code.push_str("}\n\n");

    // --- StringOp::z3_sort() ---
    code.push_str("impl StringOp {\n");
    code.push_str("    /// Returns the Z3 sort this operation produces.\n");
    code.push_str("    fn z3_sort(self) -> Sort {\n");
    code.push_str("        match self {\n");
    for op in &spec.operations {
        code.push_str(&format!(
            "            StringOp::{} => Sort::{},\n",
            to_pascal_case(&op.name),
            op.z3_sort,
        ));
    }
    code.push_str("        }\n");
    code.push_str("    }\n");
    code.push_str("}\n\n");

    // --- resolve_string_op() ---
    code.push_str(
        "/// Resolve a method name (as reported by a frontend) to a canonical `StringOp`.\n",
    );
    code.push_str("fn resolve_string_op(method: &str) -> Option<StringOp> {\n");
    code.push_str("    match method {\n");
    let mut emitted_methods = HashSet::new();
    for op in &spec.operations {
        let variant = to_pascal_case(&op.name);
        for alias in &op.aliases {
            if emitted_methods.insert(alias.method.clone()) {
                code.push_str(&format!(
                    "        \"{}\" => Some(StringOp::{}),\n",
                    alias.method, variant,
                ));
            }
        }
    }
    code.push_str("        _ => None,\n");
    code.push_str("    }\n");
    code.push_str("}\n\n");

    // --- Sort-grouped constant arrays ---
    let mut bool_methods = Vec::new();
    let mut int_methods = Vec::new();
    let mut str_methods = Vec::new();

    // Collect unique method names per sort (avoid duplicates from Go len / Rust len)
    let mut seen = HashSet::new();
    for op in &spec.operations {
        for alias in &op.aliases {
            if seen.insert(alias.method.clone()) {
                match op.z3_sort.as_str() {
                    "Bool" => bool_methods.push(alias.method.clone()),
                    "Int" => int_methods.push(alias.method.clone()),
                    "Str" => str_methods.push(alias.method.clone()),
                    _ => unreachable!(),
                }
            }
        }
    }

    fn emit_array(code: &mut String, name: &str, doc: &str, methods: &[String]) {
        code.push_str(&format!("/// {doc}\n"));
        code.push_str("#[allow(dead_code)] // Available for consumers outside solver dispatch\n");
        code.push_str(&format!("const {name}: &[&str] = &[\n"));
        for m in methods {
            code.push_str(&format!("    \"{m}\",\n"));
        }
        code.push_str("];\n\n");
    }

    emit_array(
        &mut code,
        "STRING_BOOL_METHODS",
        "String method names that return a Bool in Z3 (contains, startsWith, endsWith).",
        &bool_methods,
    );
    emit_array(
        &mut code,
        "STRING_INT_METHODS",
        "String method names that return an Int in Z3 (indexOf, length).",
        &int_methods,
    );
    emit_array(
        &mut code,
        "STRING_STR_METHODS",
        "String method names that return a String in Z3 (charAt, slice, concat, etc.).",
        &str_methods,
    );

    code
}
