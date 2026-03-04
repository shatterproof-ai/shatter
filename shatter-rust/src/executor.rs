//! Execute instrumented Rust code via subprocess compilation.
//!
//! Instruments the target function, generates a `main()` harness that links
//! `shatter_rust_runtime`, compiles to a binary in a temp directory, runs it,
//! and parses the JSON `ExecuteResult` from stdout.

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::instrument;

/// Errors from the execute pipeline.
#[derive(Debug)]
pub enum ExecuteError {
    /// Source file not found or unreadable.
    FileError(String),
    /// Instrumentation failed (parse error, etc.).
    InstrumentError(String),
    /// Failed to write temp project files.
    IoError(io::Error),
    /// `cargo build` failed with compiler output.
    CompilationFailed(String),
    /// Binary produced no parseable output.
    OutputParseError(String),
}

impl std::fmt::Display for ExecuteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FileError(e) => write!(f, "file error: {e}"),
            Self::InstrumentError(e) => write!(f, "instrumentation error: {e}"),
            Self::IoError(e) => write!(f, "I/O error: {e}"),
            Self::CompilationFailed(e) => write!(f, "compilation failed: {e}"),
            Self::OutputParseError(e) => write!(f, "output parse error: {e}"),
        }
    }
}

impl From<io::Error> for ExecuteError {
    fn from(e: io::Error) -> Self {
        Self::IoError(e)
    }
}

/// Result of executing an instrumented function. Uses `serde_json::Value`
/// for fields to stay wire-compatible without duplicating runtime types.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExecuteResult {
    pub return_value: Option<Value>,
    pub thrown_error: Option<Value>,
    #[serde(default)]
    pub branch_path: Vec<Value>,
    #[serde(default)]
    pub lines_executed: Vec<u32>,
    #[serde(default)]
    pub calls_to_external: Vec<Value>,
    #[serde(default)]
    pub path_constraints: Vec<Value>,
    #[serde(default)]
    pub side_effects: Vec<Value>,
    pub performance: Value,
}

const DEFAULT_BUILD_TIMEOUT_SECS: u64 = 30;

/// Locate the shatter-rust-runtime crate by walking up from the shatter-rust binary.
fn find_runtime_crate_path() -> Result<PathBuf, ExecuteError> {
    // Try SHATTER_RUNTIME_PATH env var first (for testing and deployment).
    if let Ok(p) = std::env::var("SHATTER_RUNTIME_PATH") {
        let path = PathBuf::from(&p);
        if path.join("Cargo.toml").exists() {
            return Ok(path);
        }
    }

    // Walk up from current exe to find shatter-rust-runtime as a sibling.
    if let Ok(exe) = std::env::current_exe() {
        let mut dir = exe.parent().map(Path::to_path_buf);
        for _ in 0..5 {
            if let Some(d) = &dir {
                let candidate = d.join("shatter-rust-runtime");
                if candidate.join("Cargo.toml").exists() {
                    return Ok(candidate);
                }
                dir = d.parent().map(Path::to_path_buf);
            }
        }
    }

    Err(ExecuteError::FileError(
        "cannot locate shatter-rust-runtime crate; set SHATTER_RUNTIME_PATH".to_string(),
    ))
}

/// Extracted function signature for harness generation.
struct FnSignature {
    param_names: Vec<String>,
    param_types: Vec<String>,
    return_type: Option<String>,
}

/// Extract parameter info from a Rust source file for a specific function.
fn extract_fn_signature(
    source: &str,
    function_name: &str,
) -> Result<FnSignature, ExecuteError> {
    use quote::ToTokens;

    let file = syn::parse_file(source)
        .map_err(|e| ExecuteError::InstrumentError(format!("parse error: {e}")))?;

    for item in &file.items {
        if let syn::Item::Fn(item_fn) = item
            && item_fn.sig.ident == function_name
        {
            let mut param_names = Vec::new();
            let mut param_types = Vec::new();

            for arg in &item_fn.sig.inputs {
                if let syn::FnArg::Typed(pat_type) = arg {
                    let name = pat_type.pat.to_token_stream().to_string();
                    let ty = pat_type.ty.to_token_stream().to_string();
                    param_names.push(name);
                    param_types.push(ty);
                }
            }

            let return_type = match &item_fn.sig.output {
                syn::ReturnType::Default => None,
                syn::ReturnType::Type(_, ty) => Some(ty.to_token_stream().to_string()),
            };

            return Ok(FnSignature { param_names, param_types, return_type });
        }
    }

    Err(ExecuteError::InstrumentError(format!(
        "function not found: {function_name}"
    )))
}

/// Generate a Cargo.toml for the temp project.
fn generate_cargo_toml(runtime_path: &Path) -> String {
    let runtime_path_str = runtime_path.display().to_string().replace('\\', "/");
    format!(
        r#"[package]
name = "shatter-exec-temp"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = {{ version = "1", features = ["derive"] }}
serde_json = "1"
shatter-rust-runtime = {{ path = "{runtime_path_str}" }}
"#
    )
}

/// Generate the main.rs harness that calls the target function.
fn generate_harness(
    instrumented_source: &str,
    function_name: &str,
    param_names: &[String],
    param_types: &[String],
    return_type: Option<&str>,
    inputs_json: &str,
    mocks_json: &str,
) -> String {
    let mut h = String::with_capacity(4096);

    h.push_str("use serde_json::Value;\n\n");
    h.push_str(instrumented_source);
    h.push_str("\n\nfn main() {\n");

    // Parse inputs
    h.push_str(&format!(
        "    let inputs_json = r#\"{}\"#;\n",
        inputs_json
    ));
    h.push_str(
        "    let inputs: Vec<Value> = serde_json::from_str(inputs_json).unwrap_or_default();\n\n",
    );

    // Parse and register mocks
    h.push_str(&format!(
        "    let mocks_json = r#\"{}\"#;\n",
        mocks_json
    ));
    h.push_str("    let mocks: Vec<Value> = serde_json::from_str(mocks_json).unwrap_or_default();\n");
    h.push_str("    for mock in &mocks {\n");
    h.push_str("        if let (Some(symbol), Some(return_values)) = (\n");
    h.push_str("            mock.get(\"symbol\").and_then(|s| s.as_str()),\n");
    h.push_str("            mock.get(\"return_values\").and_then(|v| v.as_array()),\n");
    h.push_str("        ) {\n");
    h.push_str(
        "            shatter_rust_runtime::register_mock(symbol, return_values.clone());\n",
    );
    h.push_str("        }\n");
    h.push_str("    }\n\n");

    // Reset runtime state
    h.push_str("    shatter_rust_runtime::reset();\n\n");

    // Deserialize each input parameter
    for (i, (name, ty)) in param_names.iter().zip(param_types.iter()).enumerate() {
        let clean_name = name.strip_prefix("mut ").unwrap_or(name).trim();
        h.push_str(&format!(
            "    let {clean_name}: {ty} = serde_json::from_value(inputs[{i}].clone()).unwrap_or_default();\n"
        ));
    }
    h.push('\n');

    // Build the argument list
    let arg_list: Vec<&str> = param_names
        .iter()
        .map(|n| n.strip_prefix("mut ").unwrap_or(n).trim())
        .collect();
    let args = arg_list.join(", ");

    // Call the function inside catch_unwind, measuring time
    h.push_str("    let start = std::time::Instant::now();\n");
    h.push_str("    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {\n");
    h.push_str(&format!("        {function_name}({args})\n"));
    h.push_str("    }));\n");
    h.push_str("    let wall_time_ms = start.elapsed().as_secs_f64() * 1000.0;\n\n");

    // Flush runtime results
    h.push_str(
        "    let runtime_json = shatter_rust_runtime::flush_results();\n",
    );
    h.push_str(
        "    let mut exec_result: Value = serde_json::from_str(&runtime_json).unwrap_or(Value::Object(Default::default()));\n",
    );
    h.push_str("    let obj = exec_result.as_object_mut().unwrap();\n\n");

    // Set return_value or thrown_error
    h.push_str("    match result {\n");
    if return_type.is_some() {
        h.push_str("        Ok(ret_val) => {\n");
        h.push_str(
            "            obj.insert(\"return_value\".into(), serde_json::to_value(&ret_val).unwrap_or(Value::Null));\n",
        );
        h.push_str("        }\n");
    } else {
        h.push_str("        Ok(()) => {\n");
        h.push_str("            obj.insert(\"return_value\".into(), Value::Null);\n");
        h.push_str("        }\n");
    }
    h.push_str("        Err(panic_info) => {\n");
    h.push_str("            let msg = if let Some(s) = panic_info.downcast_ref::<&str>() {\n");
    h.push_str("                s.to_string()\n");
    h.push_str("            } else if let Some(s) = panic_info.downcast_ref::<String>() {\n");
    h.push_str("                s.clone()\n");
    h.push_str("            } else {\n");
    h.push_str("                format!(\"{:?}\", panic_info)\n");
    h.push_str("            };\n");
    h.push_str("            obj.insert(\"thrown_error\".into(), serde_json::json!({\n");
    h.push_str("                \"error_type\": \"runtime_error\",\n");
    h.push_str("                \"message\": msg,\n");
    h.push_str("            }));\n");
    h.push_str("        }\n");
    h.push_str("    }\n\n");

    // Set performance metrics
    h.push_str("    obj.insert(\"performance\".into(), serde_json::json!({\n");
    h.push_str("        \"wall_time_ms\": wall_time_ms,\n");
    h.push_str("        \"cpu_time_us\": 0,\n");
    h.push_str("        \"heap_used_bytes\": 0,\n");
    h.push_str("        \"heap_allocated_bytes\": 0,\n");
    h.push_str("    }));\n\n");

    // Ensure side_effects is present
    h.push_str("    obj.entry(\"side_effects\").or_insert(serde_json::json!([]));\n\n");

    // Print the result JSON to stdout
    h.push_str("    println!(\"{}\", serde_json::to_string(&exec_result).unwrap());\n");
    h.push_str("}\n");

    h
}

/// Execute an instrumented Rust function by compiling and running a temp project.
///
/// Returns the parsed `ExecuteResult` on success. Compilation and runtime errors
/// are reported as `ExecuteError` variants that the handler maps to protocol responses.
pub fn execute_function(
    file_path: &str,
    function_name: &str,
    inputs: &[Value],
    mocks: &[Value],
    timeout_ms: u64,
) -> Result<ExecuteResult, ExecuteError> {
    let path = Path::new(file_path);
    if !path.exists() {
        return Err(ExecuteError::FileError(format!(
            "file not found: {file_path}"
        )));
    }

    let source = std::fs::read_to_string(path)
        .map_err(|e| ExecuteError::FileError(format!("cannot read {file_path}: {e}")))?;

    // Extract function signature for harness generation
    let sig = extract_fn_signature(&source, function_name)?;

    if inputs.len() != sig.param_names.len() {
        return Err(ExecuteError::InstrumentError(format!(
            "expected {} inputs for {function_name}, got {}",
            sig.param_names.len(),
            inputs.len()
        )));
    }

    // Instrument the source targeting the specific function
    let instr_result = instrument::instrument_source(&source, Some(function_name))
        .map_err(|e| ExecuteError::InstrumentError(e.to_string()))?;

    // Find the runtime crate
    let runtime_path = find_runtime_crate_path()?;

    // Serialize inputs and mocks for embedding
    let inputs_json = serde_json::to_string(inputs)
        .map_err(|e| ExecuteError::InstrumentError(format!("cannot serialize inputs: {e}")))?;
    let mocks_json = serde_json::to_string(mocks)
        .map_err(|e| ExecuteError::InstrumentError(format!("cannot serialize mocks: {e}")))?;

    // Generate the harness
    let harness = generate_harness(
        &instr_result.source,
        function_name,
        &sig.param_names,
        &sig.param_types,
        sig.return_type.as_deref(),
        &inputs_json,
        &mocks_json,
    );

    // Create temp directory with unique name
    let temp_dir = std::env::temp_dir().join(format!(
        "shatter-exec-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(temp_dir.join("src"))?;

    // Write project files
    let cargo_toml = generate_cargo_toml(&runtime_path);
    std::fs::write(temp_dir.join("Cargo.toml"), cargo_toml)?;
    std::fs::write(temp_dir.join("src/main.rs"), &harness)?;

    // Compile
    let build_timeout = Duration::from_secs(DEFAULT_BUILD_TIMEOUT_SECS);
    let build_start = Instant::now();
    let build_output = Command::new("cargo")
        .args(["build", "--release"])
        .current_dir(&temp_dir)
        .output()
        .map_err(|e| ExecuteError::CompilationFailed(format!("failed to run cargo: {e}")))?;

    if build_start.elapsed() > build_timeout {
        let _ = std::fs::remove_dir_all(&temp_dir);
        return Err(ExecuteError::CompilationFailed(
            "build timed out".to_string(),
        ));
    }

    if !build_output.status.success() {
        let stderr = String::from_utf8_lossy(&build_output.stderr);
        let _ = std::fs::remove_dir_all(&temp_dir);
        return Err(ExecuteError::CompilationFailed(stderr.into_owned()));
    }

    // Find the compiled binary
    let binary_name = if cfg!(windows) {
        "shatter-exec-temp.exe"
    } else {
        "shatter-exec-temp"
    };
    let binary_path = temp_dir.join("target/release").join(binary_name);

    if !binary_path.exists() {
        let _ = std::fs::remove_dir_all(&temp_dir);
        return Err(ExecuteError::CompilationFailed(
            "compiled binary not found".to_string(),
        ));
    }

    // Run the binary with timeout
    let exec_timeout = Duration::from_millis(timeout_ms);
    let run_start = Instant::now();
    let run_output = Command::new(&binary_path)
        .current_dir(&temp_dir)
        .output()
        .map_err(|e| {
            let _ = std::fs::remove_dir_all(&temp_dir);
            ExecuteError::OutputParseError(format!("failed to run binary: {e}"))
        })?;
    let wall_time_ms = run_start.elapsed().as_secs_f64() * 1000.0;

    // Clean up temp dir
    let _ = std::fs::remove_dir_all(&temp_dir);

    // Check for timeout
    if run_start.elapsed() > exec_timeout {
        return Ok(ExecuteResult {
            return_value: None,
            thrown_error: Some(serde_json::json!({
                "error_type": "timeout",
                "message": format!("execution timed out after {}ms", timeout_ms),
            })),
            branch_path: vec![],
            lines_executed: vec![],
            calls_to_external: vec![],
            path_constraints: vec![],
            side_effects: vec![],
            performance: serde_json::json!({
                "wall_time_ms": wall_time_ms,
                "cpu_time_us": 0,
                "heap_used_bytes": 0,
                "heap_allocated_bytes": 0,
            }),
        });
    }

    // Parse stdout
    let stdout = String::from_utf8_lossy(&run_output.stdout);
    let stderr_str = String::from_utf8_lossy(&run_output.stderr);

    // Check for runtime crash (non-zero exit without output)
    if !run_output.status.success() && stdout.trim().is_empty() {
        return Ok(ExecuteResult {
            return_value: None,
            thrown_error: Some(serde_json::json!({
                "error_type": "runtime_error",
                "message": if stderr_str.is_empty() {
                    format!("process exited with {}", run_output.status)
                } else {
                    stderr_str.into_owned()
                },
            })),
            branch_path: vec![],
            lines_executed: vec![],
            calls_to_external: vec![],
            path_constraints: vec![],
            side_effects: vec![],
            performance: serde_json::json!({
                "wall_time_ms": wall_time_ms,
                "cpu_time_us": 0,
                "heap_used_bytes": 0,
                "heap_allocated_bytes": 0,
            }),
        });
    }

    // Parse the JSON output from stdout
    let result: ExecuteResult = serde_json::from_str(stdout.trim()).map_err(|e| {
        ExecuteError::OutputParseError(format!(
            "failed to parse execute result: {e}\nstdout: {stdout}\nstderr: {stderr_str}"
        ))
    })?;

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_runtime_crate_via_env() {
        let runtime_path = find_runtime_crate_path();
        assert!(
            runtime_path.is_ok(),
            "should find runtime crate: {:?}",
            runtime_path.err()
        );
    }

    #[test]
    fn extract_fn_signature_simple() {
        let source = "fn classify_number(n: i32) -> &'static str { \"\" }";
        let sig = extract_fn_signature(source, "classify_number").unwrap();
        assert_eq!(sig.param_names, vec!["n"]);
        assert_eq!(sig.param_types, vec!["i32"]);
        assert!(sig.return_type.is_some());
    }

    #[test]
    fn extract_fn_signature_multiple_params() {
        let source = "fn add(a: i32, b: i32) -> i32 { a + b }";
        let sig = extract_fn_signature(source, "add").unwrap();
        assert_eq!(sig.param_names, vec!["a", "b"]);
        assert_eq!(sig.param_types, vec!["i32", "i32"]);
        assert_eq!(sig.return_type.as_deref(), Some("i32"));
    }

    #[test]
    fn extract_fn_signature_no_return() {
        let source = "fn noop() {}";
        let sig = extract_fn_signature(source, "noop").unwrap();
        assert!(sig.param_names.is_empty());
        assert!(sig.param_types.is_empty());
        assert!(sig.return_type.is_none());
    }

    #[test]
    fn extract_fn_signature_not_found() {
        let source = "fn other() {}";
        let result = extract_fn_signature(source, "missing");
        assert!(result.is_err());
    }

    #[test]
    fn generate_cargo_toml_includes_runtime_dep() {
        let toml = generate_cargo_toml(Path::new("/home/user/shatter-rust-runtime"));
        assert!(toml.contains("shatter-rust-runtime"));
        assert!(toml.contains("/home/user/shatter-rust-runtime"));
    }

    #[test]
    fn generate_harness_contains_function_call() {
        let harness = generate_harness(
            "fn classify_number(n: i32) -> &'static str { \"\" }",
            "classify_number",
            &["n".to_string()],
            &["i32".to_string()],
            Some("& 'static str"),
            "[42]",
            "[]",
        );
        assert!(harness.contains("classify_number(n)"));
        assert!(harness.contains("catch_unwind"));
        assert!(harness.contains("flush_results"));
        assert!(harness.contains("shatter_rust_runtime::reset()"));
    }

    #[test]
    fn generate_harness_void_function() {
        let harness = generate_harness(
            "fn noop() {}",
            "noop",
            &[],
            &[],
            None,
            "[]",
            "[]",
        );
        assert!(harness.contains("noop()"));
        assert!(harness.contains("Ok(())"));
    }

    #[test]
    fn execute_nonexistent_file_returns_error() {
        let result = execute_function("/nonexistent/file.rs", "f", &[], &[], 5000);
        assert!(result.is_err());
        if let Err(ExecuteError::FileError(msg)) = result {
            assert!(msg.contains("not found"));
        } else {
            panic!("expected FileError");
        }
    }

    #[test]
    fn execute_wrong_input_count_returns_error() {
        let dir = std::env::temp_dir().join("shatter-test-exec-count");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("test.rs");
        std::fs::write(&file, "fn add(a: i32, b: i32) -> i32 { a + b }").unwrap();

        let result = execute_function(
            &file.to_string_lossy(),
            "add",
            &[serde_json::json!(1)], // only 1 input, needs 2
            &[],
            5000,
        );
        assert!(result.is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
