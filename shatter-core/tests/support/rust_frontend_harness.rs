use std::collections::HashSet;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};

use shatter_core::frontend::{Frontend, FrontendConfig};
use shatter_core::protocol::{Command as ProtoCommand, FunctionAnalysis, ResponseResult};

/// Path to the Rust frontend binary, resolved from the workspace root.
fn rust_frontend_path() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let standalone = manifest_dir.join("../shatter-rust/target/debug/shatter-rust");
    if standalone.exists() {
        return standalone;
    }
    let workspace = manifest_dir.join("../target/debug/shatter-rust");
    if workspace.exists() {
        return workspace;
    }
    panic!(
        "Rust frontend not built. Run `cargo build --manifest-path shatter-rust/Cargo.toml`.\n\
         Checked: {}\n         {}",
        standalone.display(),
        workspace.display()
    );
}

/// Resolve a workspace-relative path from the shatter-core crate.
pub fn workspace_path(relative: &str) -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join(relative)
}

/// Serialize Rust frontend integration tests so temp Cargo builds do not
/// compete for CPU and I/O during the pre-push hook.
pub fn lock_rust_frontend_test() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub async fn with_rust_frontend_test_lock<T>(body: impl Future<Output = T>) -> T {
    let _guard = lock_rust_frontend_test();
    body.await
}

/// Spawn a real Rust frontend subprocess with SHATTER_RUNTIME_PATH set
/// so the executor can find the runtime crate for compilation.
pub async fn spawn_rust_frontend() -> Frontend {
    let frontend_path = rust_frontend_path();
    let runtime_path = workspace_path("../shatter-rust-runtime");
    assert!(
        runtime_path.join("Cargo.toml").exists(),
        "shatter-rust-runtime not found at {}",
        runtime_path.display()
    );

    let mut config = FrontendConfig::new(frontend_path);
    config.request_timeout = std::time::Duration::from_secs(120);
    config.env_vars.push((
        "SHATTER_RUNTIME_PATH".to_string(),
        runtime_path.to_string_lossy().into_owned(),
    ));
    config
        .env_vars
        .push(("SHATTER_EXEC_TIMEOUT".to_string(), "60".to_string()));
    config
        .env_vars
        .push(("SHATTER_BUILD_TIMEOUT".to_string(), "120".to_string()));

    Frontend::spawn(&config)
        .await
        .expect("failed to spawn Rust frontend")
}

/// Analyze a function and return its analysis.
pub async fn analyze_function(
    frontend: &mut Frontend,
    file: &str,
    function_name: &str,
) -> FunctionAnalysis {
    let response = frontend
        .send(ProtoCommand::Analyze {
            file: file.to_string(),
            function: Some(function_name.to_string()),
            project_root: None,
        })
        .await
        .expect("analyze command failed");

    match response.result {
        ResponseResult::Analyze { functions } => functions
            .into_iter()
            .find(|f| f.name == function_name)
            .unwrap_or_else(|| panic!("function '{function_name}' not found in analysis results")),
        other => panic!("expected Analyze response, got: {other:?}"),
    }
}

/// Instrument a function and assert success.
pub async fn instrument_function(frontend: &mut Frontend, file: &str, function_name: &str) {
    let response = frontend
        .send(ProtoCommand::Instrument {
            file: file.to_string(),
            function: function_name.to_string(),
            mocks: vec![],
            project_root: None,
        })
        .await
        .expect("instrument command failed");

    match response.result {
        ResponseResult::Instrument { instrumented, .. } => {
            assert!(instrumented, "instrumentation returned false");
        }
        ResponseResult::Error { code, message, .. } => {
            panic!("instrument error ({code:?}): {message}");
        }
        other => panic!("expected Instrument response, got: {other:?}"),
    }
}

/// Execute a function with given inputs via the raw protocol.
pub async fn execute_function_raw(
    frontend: &mut Frontend,
    file: &str,
    function_name: &str,
    inputs: Vec<serde_json::Value>,
) -> shatter_core::protocol::ExecuteResult {
    let request_json = serde_json::json!({
        "protocol_version": "0.1.0",
        "id": 0,
        "command": "execute",
        "file": file,
        "function": function_name,
        "inputs": inputs,
        "mocks": []
    });

    let response = frontend
        .send_raw(request_json)
        .await
        .expect("execute command failed");

    match response.result {
        ResponseResult::Execute(result) => *result,
        ResponseResult::Error { code, message, .. } => {
            panic!("execute error ({code:?}): {message}");
        }
        other => panic!("expected Execute response, got: {other:?}"),
    }
}

/// Collect distinct return value strings from a set of execution results.
pub fn collect_return_values(results: &[shatter_core::protocol::ExecuteResult]) -> HashSet<String> {
    results
        .iter()
        .map(|exec| {
            if let Some(ref err) = exec.thrown_error {
                format!("ERROR:{}", err.message)
            } else {
                match &exec.return_value {
                    Some(v) => v.to_string(),
                    None => "null".to_string(),
                }
            }
        })
        .collect()
}
