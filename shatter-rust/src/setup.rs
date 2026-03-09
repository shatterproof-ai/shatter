//! Setup file compilation and subprocess execution for the Rust frontend.
//!
//! Setup files are Rust source files compiled to binaries and executed as
//! subprocesses. The subprocess receives setup parameters as JSON on stdin
//! and returns a JSON context object on stdout. This context is passed to
//! subsequent Execute commands so frontends can restore setup state.

use std::io;
use std::path::Path;
use std::process::Command;

use crate::protocol::{SetupContextStack, SetupLevel};

/// Result of executing a setup file subprocess.
#[derive(Debug)]
pub struct SetupResult {
    /// Opaque context value returned by the setup subprocess (JSON on stdout).
    pub context: serde_json::Value,
}

/// Errors that can occur during setup file compilation or execution.
#[derive(Debug)]
pub enum SetupError {
    FileNotFound(String),
    CompilationFailed(String),
    ExecutionFailed(String),
    InvalidOutput(String),
}

impl std::fmt::Display for SetupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FileNotFound(path) => write!(f, "setup file not found: {path}"),
            Self::CompilationFailed(msg) => write!(f, "setup compilation failed: {msg}"),
            Self::ExecutionFailed(msg) => write!(f, "setup execution failed: {msg}"),
            Self::InvalidOutput(msg) => write!(f, "setup produced invalid output: {msg}"),
        }
    }
}

impl std::error::Error for SetupError {}

impl From<SetupError> for io::Error {
    fn from(e: SetupError) -> Self {
        io::Error::other(e.to_string())
    }
}

/// Compile a Rust setup file to a temporary binary.
/// Returns the path to the compiled binary.
fn compile_setup_file(file_path: &Path) -> Result<std::path::PathBuf, SetupError> {
    if !file_path.exists() {
        return Err(SetupError::FileNotFound(
            file_path.to_string_lossy().into_owned(),
        ));
    }

    let out_dir = std::env::temp_dir().join(format!("shatter-setup-{}", std::process::id()));
    std::fs::create_dir_all(&out_dir).map_err(|e| {
        SetupError::CompilationFailed(format!("failed to create output directory: {e}"))
    })?;

    let stem = file_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("setup");
    let binary_path = out_dir.join(stem);

    let output = Command::new("rustc")
        .arg(file_path)
        .arg("-o")
        .arg(&binary_path)
        .output()
        .map_err(|e| SetupError::CompilationFailed(format!("failed to invoke rustc: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(SetupError::CompilationFailed(stderr.into_owned()));
    }

    Ok(binary_path)
}

/// Execute a compiled setup binary as a subprocess.
/// Passes setup parameters as JSON on stdin, reads context JSON from stdout.
fn run_setup_binary(
    binary_path: &Path,
    level: SetupLevel,
    scope: &str,
    parent_context: Option<&SetupContextStack>,
) -> Result<SetupResult, SetupError> {
    let input = serde_json::json!({
        "level": level,
        "scope": scope,
        "parent_context": parent_context,
    });
    let input_json = serde_json::to_string(&input)
        .map_err(|e| SetupError::ExecutionFailed(format!("failed to serialize input: {e}")))?;

    let mut child = Command::new(binary_path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| SetupError::ExecutionFailed(format!("failed to spawn setup binary: {e}")))?;

    if let Some(stdin) = child.stdin.take() {
        use std::io::Write;
        let mut stdin = stdin;
        let _ = stdin.write_all(input_json.as_bytes());
    }

    let output = child
        .wait_with_output()
        .map_err(|e| SetupError::ExecutionFailed(format!("failed to wait for setup binary: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(SetupError::ExecutionFailed(format!(
            "setup binary exited with {}: {}",
            output.status, stderr
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let context: serde_json::Value = serde_json::from_str(stdout.trim()).map_err(|e| {
        SetupError::InvalidOutput(format!("expected JSON on stdout, got: {e}"))
    })?;

    Ok(SetupResult { context })
}

/// Run a complete setup: compile the file and execute the resulting binary.
/// Returns the opaque context value for downstream Execute commands.
pub fn run_setup(
    file_path: &Path,
    level: SetupLevel,
    scope: &str,
    parent_context: Option<&SetupContextStack>,
) -> Result<SetupResult, SetupError> {
    let binary_path = compile_setup_file(file_path)?;
    let result = run_setup_binary(&binary_path, level, scope, parent_context);

    // Best-effort cleanup of the compiled binary
    let _ = std::fs::remove_file(&binary_path);

    result
}

/// Tear down state for a given scope and level.
/// Currently a no-op since teardown state is managed by the core engine.
/// Future: could invoke a teardown file or function.
pub fn run_teardown(
    _level: SetupLevel,
    _scope: &str,
) -> Result<(), SetupError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compile_nonexistent_file_returns_file_not_found() {
        let result = compile_setup_file(Path::new("/nonexistent/setup.rs"));
        assert!(matches!(result, Err(SetupError::FileNotFound(_))));
    }

    #[test]
    fn compile_invalid_rust_returns_compilation_error() {
        let dir = std::env::temp_dir().join("shatter-test-setup-compile");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("bad_setup.rs");
        std::fs::write(&file, "this is not valid rust!!!").unwrap();

        let result = compile_setup_file(&file);
        assert!(matches!(result, Err(SetupError::CompilationFailed(_))));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn compile_and_run_valid_setup_file() {
        let dir = std::env::temp_dir().join("shatter-test-setup-run");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("hello_setup.rs");
        // A minimal setup that reads stdin (ignores it) and prints JSON to stdout.
        // Using escaped braces in println! format string to produce {"initialized": true}.
        let source = "fn main() {\n\
            let mut input = String::new();\n\
            let _ = std::io::stdin().read_line(&mut input);\n\
            println!(\"{{\\\"initialized\\\": true}}\");\n\
        }\n";
        std::fs::write(&file, source).unwrap();

        let result = run_setup(&file, SetupLevel::Function, "myFunc", None);
        match result {
            Ok(setup_result) => {
                assert_eq!(setup_result.context["initialized"], true);
            }
            Err(SetupError::CompilationFailed(msg)) if msg.contains("rustc") => {
                // rustc not available in this environment — skip
            }
            Err(e) => panic!("unexpected error: {e}"),
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_teardown_succeeds() {
        let result = run_teardown(SetupLevel::Function, "myFunc");
        assert!(result.is_ok());
    }
}
