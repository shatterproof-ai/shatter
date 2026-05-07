//! Execute instrumented Rust code via persistent harness subprocess.
//!
//! Instruments the target function, generates a `main()` harness that links
//! `shatter_rust_runtime`, compiles it once per unique (file, function, mocks) triple,
//! then keeps the subprocess alive to accept repeated JSON-over-stdin execute requests.
//! Only the first call for a given function triggers `cargo build`; subsequent calls
//! with different inputs reuse the running subprocess and skip recompilation.

use std::collections::{HashMap, HashSet};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, mpsc};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::instrument;
use crate::timing::TimingCollector;

/// Wrap instrumented source in `mod user_code { ... }` with all top-level items
/// made `pub`, so the harness `main()` can call the target function without
/// name collisions (e.g. duplicate `main()` from the original source).
fn wrap_in_module(source: &str) -> Result<String, ExecuteError> {
    use quote::ToTokens;

    let mut file = syn::parse_file(source)
        .map_err(|e| ExecuteError::InstrumentError(format!("parse error: {e}")))?;

    // Build the `serde::Serialize` derive attribute to inject on structs/enums.
    let serialize_derive: syn::Attribute = syn::parse_quote!(#[derive(serde::Serialize)]);

    for item in &mut file.items {
        match item {
            syn::Item::Fn(f) => f.vis = syn::Visibility::Public(syn::token::Pub::default()),
            syn::Item::Struct(s) => {
                s.vis = syn::Visibility::Public(syn::token::Pub::default());
                if !has_serialize_derive(&s.attrs) {
                    s.attrs.push(serialize_derive.clone());
                }
            }
            syn::Item::Enum(e) => {
                e.vis = syn::Visibility::Public(syn::token::Pub::default());
                if !has_serialize_derive(&e.attrs) {
                    e.attrs.push(serialize_derive.clone());
                }
            }
            syn::Item::Type(t) => t.vis = syn::Visibility::Public(syn::token::Pub::default()),
            syn::Item::Const(c) => c.vis = syn::Visibility::Public(syn::token::Pub::default()),
            syn::Item::Static(s) => s.vis = syn::Visibility::Public(syn::token::Pub::default()),
            syn::Item::Trait(t) => t.vis = syn::Visibility::Public(syn::token::Pub::default()),
            syn::Item::Mod(m) => m.vis = syn::Visibility::Public(syn::token::Pub::default()),
            _ => {}
        }
    }

    let tokens = file.to_token_stream().to_string();
    Ok(format!(
        "#[allow(dead_code)]\nmod user_code {{\n{tokens}\n}}"
    ))
}

/// Extract names of all `static mut` items from the top level of a Rust source file.
///
/// These are Rust's explicit mutable global variables. The harness snapshots them
/// before and after the function call to detect global state changes, emitting
/// `global_state_change` side effects for any that differ.
///
/// Only top-level items are considered; statics inside nested modules are skipped.
/// If the source cannot be parsed, returns an empty list (execution can still proceed).
fn extract_static_mut_items(source: &str) -> Vec<String> {
    let file = match syn::parse_file(source) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };

    file.items
        .iter()
        .filter_map(|item| {
            if let syn::Item::Static(s) = item
                && matches!(s.mutability, syn::StaticMutability::Mut(_))
            {
                return Some(s.ident.to_string());
            }
            None
        })
        .collect()
}

/// Check whether a list of attributes already contains `#[derive(...Serialize...)]`.
fn has_serialize_derive(attrs: &[syn::Attribute]) -> bool {
    use quote::ToTokens;
    for attr in attrs {
        if attr.path().is_ident("derive") {
            let tokens = attr.to_token_stream().to_string();
            if tokens.contains("Serialize") {
                return true;
            }
        }
    }
    false
}

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
    /// Function has parameters that cannot be constructed (trait objects, etc.).
    NonExecutable(String),
}

impl std::fmt::Display for ExecuteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FileError(e) => write!(f, "file error: {e}"),
            Self::InstrumentError(e) => write!(f, "instrumentation error: {e}"),
            Self::IoError(e) => write!(f, "I/O error: {e}"),
            Self::CompilationFailed(e) => write!(f, "compilation failed: {e}"),
            Self::OutputParseError(e) => write!(f, "output parse error: {e}"),
            Self::NonExecutable(e) => write!(f, "non-executable: {e}"),
        }
    }
}

impl From<io::Error> for ExecuteError {
    fn from(e: io::Error) -> Self {
        Self::IoError(e)
    }
}

/// Cache key for a compiled harness subprocess.
///
/// Two executions share a harness when they target the same function in the same
/// source file with identical mocks. Different mocks require a separate compiled
/// binary because mocks are baked into the harness source at compile time.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HarnessKey {
    file_path: String,
    function_name: String,
    /// FNV-like hash of the serialized mocks array. Different mocks → different binary.
    mocks_hash: u64,
}

impl HarnessKey {
    /// Returns the source file path this harness was built for.
    pub fn file_path(&self) -> &str {
        &self.file_path
    }

    /// Create a key for testing.
    #[cfg(test)]
    pub fn new_test(file_path: &str, function_name: &str) -> Self {
        Self {
            file_path: file_path.to_string(),
            function_name: function_name.to_string(),
            mocks_hash: 0,
        }
    }
}

/// Signature data for a single compatible function used in dispatch harness generation.
struct CompatFn {
    name: String,
    param_names: Vec<String>,
    param_types: Vec<String>,
    return_type: Option<String>,
    /// True if the function is declared `async fn`.
    is_async: bool,
}

/// Cache key for a crate-backed file-level dispatch harness.
/// One harness per (file, source_hash, mocks) — handles all compatible functions via dispatch.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CrateHarnessKey {
    file_path: String,
    source_hash: u64,
    mocks_hash: u64,
}

pub struct CrateHarnessEntry {
    pub harness: PersistentHarness,
    compatible_functions: HashSet<String>,
}

impl CrateHarnessKey {
    /// Returns the source file path this harness was built for.
    pub fn file_path(&self) -> &str {
        &self.file_path
    }

    /// Create a key for testing.
    #[cfg(test)]
    pub fn new_test(file_path: &str) -> Self {
        Self {
            file_path: file_path.to_string(),
            source_hash: 0,
            mocks_hash: 0,
        }
    }
}

#[cfg(test)]
impl CrateHarnessEntry {
    /// Create an entry for testing.
    pub fn new_test(harness: PersistentHarness) -> Self {
        Self {
            harness,
            compatible_functions: HashSet::new(),
        }
    }
}

pub type CrateHarnessCache = Mutex<HashMap<CrateHarnessKey, CrateHarnessEntry>>;

/// Cache key for a crate-bridge harness.
///
/// One harness binary per (crate root, wrapper source hash, mocks) — keyed by crate root
/// rather than individual file so the same binary serves all bridge-enabled functions
/// in the same crate.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CrateBridgeHarnessKey {
    crate_root: PathBuf,
    /// Hash of the generated `__shatter.rs` wrapper module content.
    wrapper_hash: u64,
    mocks_hash: u64,
}

impl CrateBridgeHarnessKey {
    /// Returns the crate root path this harness was built for.
    pub fn crate_root(&self) -> &Path {
        &self.crate_root
    }

    /// Create a key for testing.
    #[cfg(test)]
    pub fn new_test(crate_root: PathBuf) -> Self {
        Self {
            crate_root,
            wrapper_hash: 0,
            mocks_hash: 0,
        }
    }
}

#[cfg(test)]
impl CrateBridgeHarnessEntry {
    /// Create an entry for testing.
    pub fn new_test(harness: PersistentHarness) -> Self {
        Self {
            harness,
            compatible_functions: HashSet::new(),
        }
    }
}

/// A running crate-bridge harness entry.
pub struct CrateBridgeHarnessEntry {
    pub harness: PersistentHarness,
    /// Names of the functions exposed in the wrapper (dispatch table).
    compatible_functions: HashSet<String>,
}

pub type CrateBridgeHarnessCache = Mutex<HashMap<CrateBridgeHarnessKey, CrateBridgeHarnessEntry>>;

/// A compiled, running harness subprocess ready to accept execute requests via stdin.
pub struct PersistentHarness {
    /// The subprocess handle (used to kill on timeout/cleanup).
    pub child: std::process::Child,
    /// Write end of the subprocess's stdin pipe.
    stdin: std::process::ChildStdin,
    /// Channel receiving JSON response lines from the reader thread.
    response_rx: mpsc::Receiver<String>,
    /// Harness build directory (kept alive; binary lives here).
    pub harness_dir: PathBuf,
}

impl PersistentHarness {
    /// Returns true if the harness build directory still exists on disk.
    pub fn is_alive(&self) -> bool {
        self.harness_dir.exists()
    }

    /// Create a dummy harness for testing cleanup/recovery logic.
    /// Spawns `sleep 3600` as a placeholder subprocess.
    #[cfg(test)]
    pub fn new_dummy(harness_dir: PathBuf) -> Self {
        use std::process::{Command, Stdio};
        let mut child = Command::new("sleep")
            .arg("3600")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("failed to spawn dummy subprocess");
        let stdin = child.stdin.take().expect("stdin");
        let (_tx, rx) = mpsc::channel();
        Self {
            child,
            stdin,
            response_rx: rx,
            harness_dir,
        }
    }

    /// Send `inputs` to the subprocess and wait for a JSON response, with timeout.
    ///
    /// On timeout, kills the subprocess and returns a timeout `ExecuteResult`.
    /// On subprocess crash (channel disconnected), returns `OutputParseError`.
    fn execute(&mut self, inputs: &[Value], timeout_ms: u64) -> Result<ExecuteResult, ExecuteError> {
        // Serialize request as {"inputs":[...]} newline
        let req = serde_json::json!({"inputs": inputs});
        let mut req_bytes = serde_json::to_vec(&req)
            .map_err(|e| ExecuteError::IoError(io::Error::other(e.to_string())))?;
        req_bytes.push(b'\n');
        self.stdin.write_all(&req_bytes)?;
        self.stdin.flush()?;

        // Wait for a response line with timeout
        let timeout = Duration::from_millis(timeout_ms);
        match self.response_rx.recv_timeout(timeout) {
            Ok(line) => serde_json::from_str(&line).map_err(|e| {
                ExecuteError::OutputParseError(format!(
                    "failed to parse execute result: {e}\nline: {line}"
                ))
            }),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let _ = self.child.kill();
                let _ = self.child.wait();
                Ok(ExecuteResult {
                    return_value: None,
                    thrown_error: Some(serde_json::json!({
                        "error_type": "timeout",
                        "message": format!("execution timed out after {timeout_ms}ms"),
                    })),
                    branch_path: vec![],
                    lines_executed: vec![],
                    calls_to_external: vec![],
                    path_constraints: vec![],
                    side_effects: vec![],
                    performance: serde_json::json!({
                        "wall_time_ms": timeout_ms as f64,
                        "cpu_time_us": 0,
                        "heap_used_bytes": 0,
                        "heap_allocated_bytes": 0,
                    }),
                })
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(ExecuteError::OutputParseError(
                "harness subprocess terminated unexpectedly".to_string(),
            )),
        }
    }

    /// Send `function_name` + `inputs` to a dispatch harness and wait for a JSON response.
    /// Protocol: {"function": "name", "inputs": [...]} → same response format as execute().
    fn execute_dispatch(
        &mut self,
        function_name: &str,
        inputs: &[Value],
        timeout_ms: u64,
    ) -> Result<ExecuteResult, ExecuteError> {
        let req = serde_json::json!({"function": function_name, "inputs": inputs});
        let mut req_bytes = serde_json::to_vec(&req)
            .map_err(|e| ExecuteError::IoError(io::Error::other(e.to_string())))?;
        req_bytes.push(b'\n');
        self.stdin.write_all(&req_bytes)?;
        self.stdin.flush()?;

        let timeout = Duration::from_millis(timeout_ms);
        match self.response_rx.recv_timeout(timeout) {
            Ok(line) => serde_json::from_str(&line).map_err(|e| {
                ExecuteError::OutputParseError(format!(
                    "failed to parse execute result: {e}\nline: {line}"
                ))
            }),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let _ = self.child.kill();
                let _ = self.child.wait();
                Ok(ExecuteResult {
                    return_value: None,
                    thrown_error: Some(serde_json::json!({
                        "error_type": "timeout",
                        "message": format!("execution timed out after {timeout_ms}ms"),
                    })),
                    branch_path: vec![],
                    lines_executed: vec![],
                    calls_to_external: vec![],
                    path_constraints: vec![],
                    side_effects: vec![],
                    performance: serde_json::json!({
                        "wall_time_ms": timeout_ms as f64,
                        "cpu_time_us": 0,
                        "heap_used_bytes": 0,
                        "heap_allocated_bytes": 0,
                    }),
                })
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(ExecuteError::OutputParseError(
                "harness subprocess terminated unexpectedly".to_string(),
            )),
        }
    }
}

/// Shared in-process cache of compiled harness subprocesses.
///
/// Keyed by `HarnessKey`; a `Mutex` provides interior mutability so
/// `handle_execute` can hold `&self` while mutating the cache.
pub type HarnessCache = Mutex<HashMap<HarnessKey, PersistentHarness>>;

/// Compute a stable u64 hash of the mocks array by hashing its JSON representation.
fn mocks_hash(mocks: &[Value]) -> u64 {
    use std::hash::{Hash, Hasher};
    let s = serde_json::to_string(mocks).unwrap_or_default();
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

/// Hash the source content for stable binary cache invalidation.
fn source_hash(content: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    content.hash(&mut h);
    h.finish()
}

/// Walk up from `file_path` to find the nearest directory containing a
/// `Cargo.toml` with a `[package]` section (not just a workspace root).
/// Returns `None` for standalone files or workspace-root-only manifests.
fn find_crate_root(file_path: &str) -> Option<PathBuf> {
    let mut dir = PathBuf::from(file_path);
    dir.pop();
    loop {
        let cargo_toml = dir.join("Cargo.toml");
        if cargo_toml.exists() {
            let is_crate = std::fs::read_to_string(&cargo_toml)
                .ok()
                .map(|c| c.contains("[package]"))
                .unwrap_or(false);
            return if is_crate { Some(dir) } else { None };
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Content-addressed stable directory for a crate-backed dispatch harness.
/// The directory path is deterministic: same file+source+mocks → same path.
fn stable_crate_harness_dir(file_path: &str, src_hash: u64, mocks_hash: u64) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    file_path.hash(&mut h);
    src_hash.hash(&mut h);
    mocks_hash.hash(&mut h);
    let key = h.finish();
    harness_cache_root()
        .map(|c| c.join("rust").join("bin-only").join(format!("{key:016x}")))
        .unwrap_or_else(|| std::env::temp_dir().join(format!("shatter-bin-only-{key:016x}")))
}

/// Extract the `[dependencies]` section lines from a Cargo.toml file.
/// Returns the raw lines (not including the `[dependencies]` header) ready to
/// append to a generated Cargo.toml.
fn extract_dependencies_section(cargo_toml: &str) -> String {
    let mut in_deps = false;
    let mut result = String::new();
    for line in cargo_toml.lines() {
        let trimmed = line.trim();
        if trimmed == "[dependencies]" {
            in_deps = true;
            continue;
        }
        if in_deps {
            if trimmed.starts_with('[') {
                break;
            }
            if !trimmed.is_empty() && !trimmed.starts_with('#') {
                result.push_str(line);
                result.push('\n');
            }
        }
    }
    result
}

/// Generate a Cargo.toml that includes `shatter-rust-runtime` + serde + serde_json
/// PLUS all deps from the user's crate, so the instrumented source can reference
/// external types (e.g. `regex::Regex`) that are available in the user's crate.
fn generate_cargo_toml_with_user_deps(user_cargo_toml: &str, runtime_path: &Path, needs_tokio: bool, needs_axum: bool) -> String {
    let forwarded = extract_dependencies_section(user_cargo_toml);
    // Axum handlers are always async, so tokio is implied by axum.
    let needs_tokio = needs_tokio || needs_axum;
    let axum_keys: &[&str] = if needs_axum {
        &["axum", "tower", "http", "http-body-util"]
    } else {
        &[]
    };
    // Filter out deps the harness template already injects to avoid duplicate TOML keys.
    let filtered: String = forwarded
        .lines()
        .filter(|line| {
            // Extract the dep name: everything before the first '=', ' ', or '.'
            let key = line.split(['=', ' ', '.']).next().unwrap_or("").trim();
            key != "serde" && key != "serde_json" && key != "libc" && key != "shatter-rust-runtime"
                && (!needs_tokio || key != "tokio")
                && !axum_keys.contains(&key)
        })
        .map(|line| format!("{line}\n"))
        .collect();
    let runtime_path_str = runtime_path.display().to_string().replace('\\', "/");
    let tokio_dep = if needs_tokio {
        "tokio = { version = \"1\", features = [\"full\"] }\n"
    } else {
        ""
    };
    let axum_deps = if needs_axum {
        concat!(
            "axum = { version = \"0.7\", features = [\"json\"] }\n",
            "tower = { version = \"0.5\", features = [\"util\"] }\n",
            "http = \"1\"\n",
            "http-body-util = \"0.1\"\n",
        )
    } else {
        ""
    };
    format!(
        r#"[package]
name = "shatter-exec-temp"
version = "0.1.0"
edition = "2021"

[workspace]

[dependencies]
serde = {{ version = "1", features = ["derive"] }}
serde_json = "1"
libc = "0.2"
shatter-rust-runtime = {{ path = "{runtime_path_str}" }}
{tokio_dep}{axum_deps}{filtered}
"#
    )
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

/// Check if harness should be compiled in release mode.
/// Reads `SHATTER_HARNESS_RELEASE` env var — `"1"` or `"true"` (case-insensitive) enables release.
fn harness_release_mode() -> bool {
    std::env::var("SHATTER_HARNESS_RELEASE")
        .ok()
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// Resolve `p` to an absolute path by prepending `current_dir` if relative.
///
/// Does not require the path to exist. Used to ensure `CARGO_TARGET_DIR` is
/// always an absolute path — Cargo resolves a relative `CARGO_TARGET_DIR`
/// relative to the build process's CWD (harness_dir), which differs from the
/// frontend process CWD, causing binary lookup failures.
fn to_absolute(p: PathBuf) -> PathBuf {
    if p.is_absolute() {
        p
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(&p))
            .unwrap_or(p)
    }
}

/// Read the harness cache root from `SHATTER_HARNESS_CACHE`.
/// Returns `None` if unset or empty. Always returns an absolute path so that
/// all derived paths (`standalone_target_dir`, `make_harness_dir`, etc.) are
/// absolute — this prevents Cargo from resolving `CARGO_TARGET_DIR` relative
/// to the build subprocess CWD instead of the frontend process CWD.
fn harness_cache_root() -> Option<PathBuf> {
    std::env::var("SHATTER_HARNESS_CACHE")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .map(to_absolute)
}

/// Read the harness scratch root from `SHATTER_HARNESS_SCRATCH`.
/// Returns `None` if unset or empty.
#[cfg(test)]
fn harness_scratch_root() -> Option<PathBuf> {
    std::env::var("SHATTER_HARNESS_SCRATCH")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
}

/// Create a per-request scratch directory for standalone harness execution.
///
/// Uses `SHATTER_HARNESS_SCRATCH/rust-<pid>-<id>/` when the env var is set,
/// falling back to a raw temp directory. The caller is responsible for
/// removing the directory after the request completes.
#[cfg(test)]
fn make_request_scratch() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let id = format!(
        "rust-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    );
    harness_scratch_root()
        .map(|s| s.join(&id))
        .unwrap_or_else(|| std::env::temp_dir().join(format!("shatter-exec-{id}")))
}

/// Create a unique persistent directory for a compiled harness binary.
///
/// Unlike `make_request_scratch`, this directory is NOT cleaned up after each
/// request — the compiled binary must remain accessible for the lifetime of the
/// persistent subprocess. Cleanup happens in `PersistentHarnessManager::close_all()`.
fn make_harness_dir() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let id = format!(
        "rust-harness-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    );
    harness_cache_root()
        .map(|c| c.join("rust").join("harnesses").join(&id))
        .unwrap_or_else(|| std::env::temp_dir().join(format!("shatter-harness-{id}")))
}

/// Return the shared `CARGO_TARGET_DIR` for standalone harness builds.
///
/// Placing the target directory inside `SHATTER_HARNESS_CACHE` allows compiled
/// dependency artifacts to persist across requests, so only the changed harness
/// source (`main.rs`) needs to be recompiled each time.
///
/// Returns `None` when no cache root is configured; callers fall back to a
/// per-request target directory inside scratch.
fn standalone_target_dir() -> Option<PathBuf> {
    harness_cache_root().map(|c| c.join("rust").join("standalone").join("target"))
}

/// Check if `cargo check` should be skipped before build.
/// Reads `SHATTER_SKIP_CHECK` env var — `"1"` or `"true"` (case-insensitive) skips check.
fn skip_cargo_check() -> bool {
    std::env::var("SHATTER_SKIP_CHECK")
        .ok()
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// Run `cargo check` for fast type/borrow validation before a full build.
///
/// Catches errors ~3x faster than `cargo build` by skipping codegen and linking.
/// Shares the same `CARGO_TARGET_DIR` so check metadata is reused by the subsequent build.
/// Set `SHATTER_SKIP_CHECK=1` to bypass the check step.
fn cargo_check_before_build(
    working_dir: &Path,
    target_dir: &Path,
    release: bool,
    timing: Option<&mut TimingCollector>,
) -> Result<(), ExecuteError> {
    if skip_cargo_check() {
        return Ok(());
    }

    let mut check_args = vec!["check"];
    if release {
        check_args.push("--release");
    }

    let check_output = if let Some(t) = timing {
        let args = check_args.clone();
        t.record("execute.check", |_| {
            Command::new("cargo")
                .args(&args)
                .current_dir(working_dir)
                .env("CARGO_TARGET_DIR", target_dir)
                .output()
                .map_err(|e| {
                    ExecuteError::CompilationFailed(format!("failed to run cargo check: {e}"))
                })
        })?
    } else {
        Command::new("cargo")
            .args(&check_args)
            .current_dir(working_dir)
            .env("CARGO_TARGET_DIR", target_dir)
            .output()
            .map_err(|e| {
                ExecuteError::CompilationFailed(format!("failed to run cargo check: {e}"))
            })?
    };

    if !check_output.status.success() {
        let stderr = String::from_utf8_lossy(&check_output.stderr);
        return Err(ExecuteError::CompilationFailed(stderr.into_owned()));
    }

    Ok(())
}

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
    /// True if the function has type parameters (e.g. `fn foo<T>(...)`).
    has_generics: bool,
    /// Names of type parameters for error messages (e.g. `["T", "U"]`).
    generic_names: Vec<String>,
    /// True if the function is declared `async fn`.
    is_async: bool,
}

/// File-level context needed for bin_only compatibility analysis.
struct FnContext {
    sig: FnSignature,
    /// Names of structs, enums, and type aliases defined at the top level of the file.
    local_type_names: HashSet<String>,
    /// True if the file has `use` items referencing `super::` or `crate::` paths.
    has_module_path_uses: bool,
}

/// Extract function signature and file-level context from a Rust source file.
///
/// Parses the source once and returns both the function signature and the file-level
/// metadata needed for compatibility checking, avoiding a redundant parse.
fn extract_fn_context(source: &str, function_name: &str) -> Result<FnContext, ExecuteError> {
    use quote::ToTokens;

    let file = syn::parse_file(source)
        .map_err(|e| ExecuteError::InstrumentError(format!("parse error: {e}")))?;

    // Collect file-level metadata for compatibility analysis.
    let local_type_names = collect_local_type_names(&file);
    let has_module_path_uses = has_module_path_uses(&file);

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

            let generic_names: Vec<String> = item_fn
                .sig
                .generics
                .params
                .iter()
                .filter_map(|p| {
                    if let syn::GenericParam::Type(tp) = p {
                        Some(tp.ident.to_string())
                    } else {
                        None
                    }
                })
                .collect();
            let has_generics = !generic_names.is_empty();

            let is_async = item_fn.sig.asyncness.is_some();

            return Ok(FnContext {
                sig: FnSignature {
                    param_names,
                    param_types,
                    return_type,
                    has_generics,
                    generic_names,
                    is_async,
                },
                local_type_names,
                has_module_path_uses,
            });
        }
    }

    Err(ExecuteError::InstrumentError(format!(
        "function not found: {function_name}"
    )))
}

/// Extract `FnContext` for every top-level `fn` item in the source file.
/// Functions that fail signature extraction are silently skipped (non-blocking).
fn extract_all_fn_contexts(source: &str) -> Vec<(String, FnContext)> {
    use quote::ToTokens;

    let file = match syn::parse_file(source) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };

    let local_type_names = collect_local_type_names(&file);
    let has_module_path_uses = has_module_path_uses(&file);

    file.items
        .iter()
        .filter_map(|item| {
            let item_fn = match item {
                syn::Item::Fn(f) => f,
                _ => return None,
            };
            let function_name = item_fn.sig.ident.to_string();

            let mut param_names = Vec::new();
            let mut param_types = Vec::new();
            for arg in &item_fn.sig.inputs {
                if let syn::FnArg::Typed(pt) = arg {
                    param_names.push(pt.pat.to_token_stream().to_string());
                    param_types.push(pt.ty.to_token_stream().to_string());
                }
            }
            let return_type = match &item_fn.sig.output {
                syn::ReturnType::Default => None,
                syn::ReturnType::Type(_, ty) => Some(ty.to_token_stream().to_string()),
            };
            let generic_names: Vec<String> = item_fn
                .sig
                .generics
                .params
                .iter()
                .filter_map(|p| {
                    if let syn::GenericParam::Type(tp) = p {
                        Some(tp.ident.to_string())
                    } else {
                        None
                    }
                })
                .collect();
            let has_generics = !generic_names.is_empty();

            let is_async = item_fn.sig.asyncness.is_some();

            Some((
                function_name,
                FnContext {
                    sig: FnSignature {
                        param_names,
                        param_types,
                        return_type,
                        has_generics,
                        generic_names,
                        is_async,
                    },
                    local_type_names: local_type_names.clone(),
                    has_module_path_uses,
                },
            ))
        })
        .collect()
}

/// Collect names of structs, enums, and type aliases defined at the top level.
fn collect_local_type_names(file: &syn::File) -> HashSet<String> {
    let mut names = HashSet::new();
    for item in &file.items {
        match item {
            syn::Item::Struct(s) => {
                names.insert(s.ident.to_string());
            }
            syn::Item::Enum(e) => {
                names.insert(e.ident.to_string());
            }
            syn::Item::Type(t) => {
                names.insert(t.ident.to_string());
            }
            _ => {}
        }
    }
    names
}

/// Check whether the file has `use` items referencing module-local paths
/// (`super::`, `crate::`) that won't resolve in an isolated harness.
fn has_module_path_uses(file: &syn::File) -> bool {
    use quote::ToTokens;
    for item in &file.items {
        if let syn::Item::Use(u) = item {
            let path_str = u.tree.to_token_stream().to_string();
            if path_str.starts_with("super ::") || path_str.starts_with("crate ::") {
                return true;
            }
        }
    }
    false
}

/// Known primitive and standard library types that are available in an isolated harness
/// (only `serde` + `serde_json` + `shatter-rust-runtime` as dependencies).
fn is_primitive_or_std_type(name: &str) -> bool {
    matches!(
        name,
        "i8" | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "isize"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
            | "f32"
            | "f64"
            | "bool"
            | "char"
            | "String"
            | "str"
            | "Vec"
            | "HashMap"
            | "HashSet"
            | "BTreeMap"
            | "BTreeSet"
            | "Option"
            | "Result"
            | "Box"
            | "Rc"
            | "Arc"
            | "PhantomData"
            | "Duration"
            | "PathBuf"
            | "OsString"
            | "()"
    )
}

/// Extract the root type name from a type string, stripping references, wrappers, etc.
///
/// Examples: `"&str"` → `"str"`, `"Vec<MyStruct>"` → `"Vec"`,
/// `"&mut HashMap<String, i32>"` → `"HashMap"`, `"Box<dyn Foo>"` → `"Box"`.
fn extract_root_type_name(ty: &str) -> &str {
    let s = ty.trim();
    // Strip leading `&`, `&mut`, lifetime refs
    let s = s.strip_prefix("&mut ").unwrap_or(s);
    let s = s.strip_prefix('&').unwrap_or(s);
    let s = s.trim();
    // Strip lifetime like `'static ` or `'a `
    let s = if s.starts_with('\'') {
        s.find(' ').map_or(s, |i| &s[i + 1..])
    } else {
        s
    };
    let s = s.trim();
    // Take up to first `<` or space (get the root name only)
    let end = s
        .find(['<', ' '])
        .unwrap_or(s.len());
    let root = &s[..end];
    if root.is_empty() { s } else { root }
}

/// Check whether a function can execute in bin_only harness mode.
///
/// Collects all incompatibilities and returns a single `NonExecutable` error
/// listing every problem and suggesting `crate_bridge` as an alternative.
///
/// `crate_backed` — when `true`, the harness forwards the user crate's dependencies,
/// so external crate types resolve. The external-type check (issue #3) is skipped.
fn check_bin_only_compatibility(
    function_name: &str,
    ctx: &FnContext,
    crate_backed: bool,
) -> Result<(), ExecuteError> {
    let mut issues: Vec<String> = Vec::new();

    // 1. Generic type parameters — harness can't pick concrete types.
    if ctx.sig.has_generics {
        issues.push(format!(
            "generic type parameters [{}]: harness cannot instantiate concrete types",
            ctx.sig.generic_names.join(", ")
        ));
    }

    // 2. Trait object parameters — can't be deserialized from JSON.
    for (name, ty) in ctx.sig.param_names.iter().zip(ctx.sig.param_types.iter()) {
        if is_trait_object_type(ty) {
            issues.push(format!(
                "parameter `{name}` has trait object type `{ty}`: cannot be deserialized from JSON"
            ));
        }
    }

    // 3. External crate types — not available in isolated harness.
    // Skipped for crate-backed mode because user deps are forwarded.
    if !crate_backed {
        for (name, ty) in ctx.sig.param_names.iter().zip(ctx.sig.param_types.iter()) {
            // Skip trait objects (already reported above).
            if is_trait_object_type(ty) {
                continue;
            }
            let root = extract_root_type_name(ty);
            if !root.is_empty()
                && !is_primitive_or_std_type(root)
                && !ctx.local_type_names.contains(root)
                // Tuple types like `(i32, i32)` start with `(`
                && !root.starts_with('(')
                // Array/slice types like `[u8 ; 4]`
                && !root.starts_with('[')
            {
                if ctx.has_module_path_uses {
                    issues.push(format!(
                        "parameter `{name}` uses type `{root}` imported via module path: \
                         won't resolve in isolated harness"
                    ));
                } else {
                    issues.push(format!(
                        "parameter `{name}` uses external type `{root}`: \
                         not available in isolated harness (only serde + serde_json)"
                    ));
                }
            }
        }
    }

    if issues.is_empty() {
        return Ok(());
    }

    let mut msg = format!("bin_only harness incompatible with `{function_name}`:\n");
    for issue in &issues {
        msg.push_str(&format!("  - {issue}\n"));
    }
    msg.push_str("\nHint: use crate_bridge mode to execute within the original crate context");

    Err(ExecuteError::NonExecutable(msg))
}

/// Generate a Cargo.toml for the temp project.
fn generate_cargo_toml(runtime_path: &Path, needs_tokio: bool, needs_axum: bool) -> String {
    let runtime_path_str = runtime_path.display().to_string().replace('\\', "/");
    // Axum handlers are always async, so tokio is implied by axum.
    let needs_tokio = needs_tokio || needs_axum;
    let tokio_dep = if needs_tokio {
        "tokio = { version = \"1\", features = [\"full\"] }\n"
    } else {
        ""
    };
    let axum_deps = if needs_axum {
        concat!(
            "axum = { version = \"0.7\", features = [\"json\"] }\n",
            "tower = { version = \"0.5\", features = [\"util\"] }\n",
            "http = \"1\"\n",
            "http-body-util = \"0.1\"\n",
        )
    } else {
        ""
    };
    format!(
        r#"[package]
name = "shatter-exec-temp"
version = "0.1.0"
edition = "2021"

[workspace]

[dependencies]
serde = {{ version = "1", features = ["derive"] }}
serde_json = "1"
libc = "0.2"
shatter-rust-runtime = {{ path = "{runtime_path_str}" }}
{tokio_dep}{axum_deps}"#
    )
}

/// Map a reference parameter type to its owned equivalent for deserialization.
///
/// `&str` and `&String` can't be deserialized from `serde_json::Value` because
/// they require a borrow from the deserializer's input buffer. We deserialize
/// to the owned type and borrow when calling the function.
/// Returns true if the type string contains a trait object (`dyn Trait`).
/// Trait objects cannot be deserialized from JSON, so functions with such
/// parameters must be marked non-executable.
fn is_trait_object_type(ty: &str) -> bool {
    // Normalize spaces and check for `dyn ` keyword preceded by a word boundary.
    // Covers `&dyn Foo`, `Box<dyn Foo>`, `&mut dyn Foo + Bar`, etc.
    let normalized = ty.replace('\n', " ");
    normalized.contains("dyn ")
}

/// Map a reference parameter type to its owned equivalent for deserialization.
///
/// Returns `Some((owned_deser_type, owned_var_type, borrow_expr))` where:
/// - `owned_deser_type` is the type to deserialize into
/// - `owned_var_type` is the intermediate variable type (may differ for slices)
/// - `borrow_expr` is how to convert the owned value to the reference the function expects
///
/// Simple cases like `&str` → `String` with `&name_owned`.
/// Slice cases like `&[&str]` → `Vec<String>` need a two-step conversion.
struct OwnedTypeMapping {
    /// Type to deserialize into (e.g., `String`, `Vec<String>`)
    deser_type: &'static str,
    /// Whether the function call needs a slice borrow (e.g., `&name_owned` vs
    /// a more complex conversion for `&[&str]`)
    needs_slice_conversion: bool,
}

fn owned_type_for_ref(ty: &str) -> Option<OwnedTypeMapping> {
    let normalized = ty.replace(' ', "");
    match normalized.as_str() {
        "&str" | "&'staticstr" => Some(OwnedTypeMapping {
            deser_type: "String",
            needs_slice_conversion: false,
        }),
        "&String" | "&'staticString" => Some(OwnedTypeMapping {
            deser_type: "String",
            needs_slice_conversion: false,
        }),
        "&[&str]" | "&[&'staticstr]" => Some(OwnedTypeMapping {
            deser_type: "Vec<String>",
            needs_slice_conversion: true,
        }),
        _ => None,
    }
}

/// Generate the main.rs harness that calls the target function.
///
/// Wraps instrumented source in `mod user_code` to avoid name collisions
/// (e.g. duplicate `fn main()` when the source file has its own `main`).
///
/// The generated harness runs a persistent loop, reading one JSON request per
/// line from stdin and writing one JSON response per line to stdout, allowing
/// it to serve multiple execute calls without recompilation.
///
/// `static_mut_names` lists the names of `static mut` items in the source.
/// The harness snapshots each before and after the function call and emits
/// `global_state_change` side effects for any whose serialized value differs.
/// Variables that fail `serde_json::to_value` (e.g. non-Serialize types) are
/// silently skipped — execution is never blocked by unserializable statics.
#[allow(clippy::too_many_arguments)]
fn generate_harness(
    instrumented_source: &str,
    function_name: &str,
    param_names: &[String],
    param_types: &[String],
    return_type: Option<&str>,
    mocks_json: &str,
    static_mut_names: &[String],
    is_async: bool,
) -> Result<String, ExecuteError> {
    let module_block = wrap_in_module(instrumented_source)?;
    let mut h = String::with_capacity(4096);

    h.push_str("#![allow(unused_imports)]\n");
    h.push_str("use serde_json::Value;\n\n");
    h.push_str(&module_block);
    h.push_str("\n\nfn main() {\n");
    h.push_str(&format!(
        "    shatter_rust_runtime::run_harness_loop(r#\"{}\"#, |inputs| {{\n",
        mocks_json
    ));

    // Snapshot mutable globals before execution.
    if !static_mut_names.is_empty() {
        for name in static_mut_names {
            h.push_str(&format!(
                "        let __before_{name} = unsafe {{ serde_json::to_value(&user_code::{name}).ok() }};\n"
            ));
        }
        h.push('\n');
    }

    // Deserialize each input parameter from the inputs array.
    for (i, (name, ty)) in param_names.iter().zip(param_types.iter()).enumerate() {
        let clean_name = name.strip_prefix("mut ").unwrap_or(name).trim();
        if let Some(mapping) = owned_type_for_ref(ty) {
            h.push_str(&format!(
                "        let {clean_name}_owned: {} = serde_json::from_value(inputs[{i}].clone()).unwrap_or_default();\n",
                mapping.deser_type
            ));
            if mapping.needs_slice_conversion {
                h.push_str(&format!(
                    "        let {clean_name}_refs: Vec<&str> = {clean_name}_owned.iter().map(|s| s.as_str()).collect();\n"
                ));
            }
        } else {
            h.push_str(&format!(
                "        let {clean_name}: {ty} = serde_json::from_value(inputs[{i}].clone()).unwrap_or_default();\n"
            ));
        }
    }
    h.push('\n');

    // Build the argument list
    let arg_list: Vec<String> = param_names
        .iter()
        .zip(param_types.iter())
        .map(|(n, ty)| {
            let clean = n.strip_prefix("mut ").unwrap_or(n).trim();
            if let Some(mapping) = owned_type_for_ref(ty) {
                if mapping.needs_slice_conversion {
                    format!("&{clean}_refs")
                } else {
                    format!("&{clean}_owned")
                }
            } else {
                clean.to_string()
            }
        })
        .collect();
    let args = arg_list.join(", ");

    // Redirect stdout/stderr to temp files for console_output capture.
    h.push_str("        let __capture_dir = std::env::temp_dir();\n");
    h.push_str("        let __pid = std::process::id();\n");
    h.push_str("        let __stdout_path = __capture_dir.join(format!(\"shatter-{__pid}-stdout\"));\n");
    h.push_str("        let __stderr_path = __capture_dir.join(format!(\"shatter-{__pid}-stderr\"));\n");
    h.push_str("        let __stdout_file = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(true).open(&__stdout_path);\n");
    h.push_str("        let __stderr_file = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(true).open(&__stderr_path);\n");
    h.push_str("        let __orig_stdout = unsafe { libc::dup(1) };\n");
    h.push_str("        let __orig_stderr = unsafe { libc::dup(2) };\n");
    h.push_str("        if let (Ok(ref __sf), Ok(ref __ef)) = (&__stdout_file, &__stderr_file) {\n");
    h.push_str("            use std::os::unix::io::AsRawFd;\n");
    h.push_str("            unsafe { libc::dup2(__sf.as_raw_fd(), 1); }\n");
    h.push_str("            unsafe { libc::dup2(__ef.as_raw_fd(), 2); }\n");
    h.push_str("        }\n\n");

    // Call the function with panic recovery and timing via the runtime helper.
    if is_async {
        h.push_str("        let __tokio_rt = tokio::runtime::Runtime::new().unwrap();\n");
        h.push_str("        let (result, wall_time_ms) = shatter_rust_runtime::execute_with_timing(std::panic::AssertUnwindSafe(|| {\n");
        h.push_str(&format!(
            "            __tokio_rt.block_on(user_code::{function_name}({args}))\n"
        ));
        h.push_str("        }));\n\n");
    } else {
        h.push_str("        let (result, wall_time_ms) = shatter_rust_runtime::execute_with_timing(std::panic::AssertUnwindSafe(|| {\n");
        h.push_str(&format!(
            "            user_code::{function_name}({args})\n"
        ));
        h.push_str("        }));\n\n");
    }

    // Restore original stdout/stderr before writing JSON response.
    h.push_str("        unsafe { libc::dup2(__orig_stdout, 1); libc::close(__orig_stdout); }\n");
    h.push_str("        unsafe { libc::dup2(__orig_stderr, 2); libc::close(__orig_stderr); }\n\n");

    // Read captured console output.
    h.push_str("        let mut __captured_stdout = String::new();\n");
    h.push_str("        let mut __captured_stderr = String::new();\n");
    h.push_str("        if let Ok(mut __f) = __stdout_file {\n");
    h.push_str("            use std::io::{Read, Seek, SeekFrom};\n");
    h.push_str("            let _ = __f.seek(SeekFrom::Start(0));\n");
    h.push_str("            let _ = __f.read_to_string(&mut __captured_stdout);\n");
    h.push_str("        }\n");
    h.push_str("        if let Ok(mut __f) = __stderr_file {\n");
    h.push_str("            use std::io::{Read, Seek, SeekFrom};\n");
    h.push_str("            let _ = __f.seek(SeekFrom::Start(0));\n");
    h.push_str("            let _ = __f.read_to_string(&mut __captured_stderr);\n");
    h.push_str("        }\n");
    h.push_str("        let _ = std::fs::remove_file(&__stdout_path);\n");
    h.push_str("        let _ = std::fs::remove_file(&__stderr_path);\n\n");

    // Map result to return_value / thrown_error
    h.push_str("        let (ret_val, err_val) = match result {\n");
    if return_type.is_some() {
        h.push_str("            Ok(ref v) => (Some(serde_json::to_value(v).unwrap_or(Value::Null)), None),\n");
    } else {
        h.push_str("            Ok(()) => (Some(Value::Null), None),\n");
    }
    h.push_str("            Err(ref msg) => (None, Some(serde_json::json!({\"error_type\": \"runtime_error\", \"message\": msg}))),\n");
    h.push_str("        };\n\n");

    // Build side_effects: console_output first, then thrown_error, then global_state_change.
    h.push_str("        let mut __side_effects: Vec<serde_json::Value> = Vec::new();\n");
    h.push_str("        for __line in __captured_stdout.lines() {\n");
    h.push_str("            if !__line.is_empty() {\n");
    h.push_str("                let __msg: String = __line.chars().take(4096).collect();\n");
    h.push_str("                __side_effects.push(serde_json::json!({\"kind\": \"console_output\", \"level\": \"log\", \"message\": __msg}));\n");
    h.push_str("            }\n");
    h.push_str("        }\n");
    h.push_str("        for __line in __captured_stderr.lines() {\n");
    h.push_str("            if !__line.is_empty() {\n");
    h.push_str("                let __msg: String = __line.chars().take(4096).collect();\n");
    h.push_str("                __side_effects.push(serde_json::json!({\"kind\": \"console_output\", \"level\": \"error\", \"message\": __msg}));\n");
    h.push_str("            }\n");
    h.push_str("        }\n");
    h.push_str("        if let Err(ref msg) = result {\n");
    h.push_str("            __side_effects.push(serde_json::json!({\"kind\": \"thrown_error\", \"error_type\": \"runtime_error\", \"message\": msg, \"stack\": null}));\n");
    h.push_str("        }\n\n");

    // Detect global state changes by comparing before/after snapshots of mutable statics.
    if !static_mut_names.is_empty() {
        for name in static_mut_names {
            h.push_str(&format!(
                "        let __after_{name} = unsafe {{ serde_json::to_value(&user_code::{name}).ok() }};\n"
            ));
            h.push_str(&format!(
                "        if let (Some(__b), Some(__a)) = (__before_{name}, __after_{name}) {{\n"
            ));
            h.push_str("            if __b != __a {\n");
            h.push_str(&format!(
                "                __side_effects.push(serde_json::json!({{\"kind\":\"global_state_change\",\"variable\":\"{name}\",\"before\":__b,\"after\":__a}}));\n"
            ));
            h.push_str("            }\n");
            h.push_str("        }\n");
        }
        h.push('\n');
        h.push_str("        shatter_rust_runtime::build_result_json(ret_val, err_val, wall_time_ms, __side_effects)\n");
    } else {
        h.push_str("        shatter_rust_runtime::build_result_json(ret_val, err_val, wall_time_ms, __side_effects)\n");
    }

    h.push_str("    });\n"); // end closure + run_harness_loop call
    h.push_str("}\n");

    Ok(h)
}

/// Generate a multi-function dispatch harness for crate-backed execution.
///
/// The harness reads `{"function": "name", "inputs": [...]}` from stdin and
/// dispatches to the corresponding function via a `match` arm. All compatible
/// functions from the file are included.
///
/// `fns` is a slice of `CompatFn` descriptors for all compatible functions.
fn generate_dispatch_harness(
    instrumented_source: &str,
    fns: &[CompatFn],
    mocks_json: &str,
    static_mut_names: &[String],
) -> Result<String, ExecuteError> {
    let module_block = wrap_in_module(instrumented_source)?;
    let mut h = String::with_capacity(8192);

    h.push_str("#![allow(unused_imports)]\n");
    h.push_str("use serde_json::Value;\n\n");
    h.push_str(&module_block);
    h.push_str("\n\nfn main() {\n");
    h.push_str(&format!(
        "    shatter_rust_runtime::run_dispatch_loop(r#\"{}\"#, |function_name, inputs| {{\n",
        mocks_json
    ));

    // Snapshot mutable globals before dispatch.
    if !static_mut_names.is_empty() {
        for name in static_mut_names {
            h.push_str(&format!(
                "        let __before_{name} = unsafe {{ serde_json::to_value(&user_code::{name}).ok() }};\n"
            ));
        }
        h.push('\n');
    }

    // Redirect stdout/stderr for console capture.
    h.push_str("        let __capture_dir = std::env::temp_dir();\n");
    h.push_str("        let __pid = std::process::id();\n");
    h.push_str("        let __stdout_path = __capture_dir.join(format!(\"shatter-{__pid}-stdout\"));\n");
    h.push_str("        let __stderr_path = __capture_dir.join(format!(\"shatter-{__pid}-stderr\"));\n");
    h.push_str("        let __stdout_file = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(true).open(&__stdout_path);\n");
    h.push_str("        let __stderr_file = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(true).open(&__stderr_path);\n");
    h.push_str("        let __orig_stdout = unsafe { libc::dup(1) };\n");
    h.push_str("        let __orig_stderr = unsafe { libc::dup(2) };\n");
    h.push_str("        if let (Ok(ref __sf), Ok(ref __ef)) = (&__stdout_file, &__stderr_file) {\n");
    h.push_str("            use std::os::unix::io::AsRawFd;\n");
    h.push_str("            unsafe { libc::dup2(__sf.as_raw_fd(), 1); }\n");
    h.push_str("            unsafe { libc::dup2(__ef.as_raw_fd(), 2); }\n");
    h.push_str("        }\n\n");

    // Build the dispatch match — each arm deserializes params, calls, and returns result.
    h.push_str("        let (ret_val, err_val, wall_time_ms): (Option<Value>, Option<Value>, f64) = match function_name {\n");
    for fn_info in fns {
        let fn_name = &fn_info.name;
        let param_names = &fn_info.param_names;
        let param_types = &fn_info.param_types;
        let return_type = &fn_info.return_type;
        h.push_str(&format!("            {:?} => {{\n", fn_name.as_str()));
        // Deserialize each parameter.
        for (i, (name, ty)) in param_names.iter().zip(param_types.iter()).enumerate() {
            let clean_name = name.strip_prefix("mut ").unwrap_or(name).trim();
            if let Some(mapping) = owned_type_for_ref(ty) {
                h.push_str(&format!(
                    "                let {clean_name}_owned: {} = serde_json::from_value(inputs[{i}].clone()).unwrap_or_default();\n",
                    mapping.deser_type
                ));
                if mapping.needs_slice_conversion {
                    h.push_str(&format!(
                        "                let {clean_name}_refs: Vec<&str> = {clean_name}_owned.iter().map(|s| s.as_str()).collect();\n"
                    ));
                }
            } else {
                h.push_str(&format!(
                    "                let {clean_name}: {ty} = serde_json::from_value(inputs[{i}].clone()).unwrap_or_default();\n"
                ));
            }
        }
        // Build argument list.
        let arg_list: Vec<String> = param_names
            .iter()
            .zip(param_types.iter())
            .map(|(n, ty)| {
                let clean = n.strip_prefix("mut ").unwrap_or(n).trim();
                if let Some(mapping) = owned_type_for_ref(ty) {
                    if mapping.needs_slice_conversion {
                        format!("&{clean}_refs")
                    } else {
                        format!("&{clean}_owned")
                    }
                } else {
                    clean.to_string()
                }
            })
            .collect();
        let args = arg_list.join(", ");
        // Call with timing + panic recovery via runtime helper.
        if fn_info.is_async {
            h.push_str("                let __tokio_rt = tokio::runtime::Runtime::new().unwrap();\n");
            h.push_str("                let (result, wt) = shatter_rust_runtime::execute_with_timing(std::panic::AssertUnwindSafe(|| {\n");
            h.push_str(&format!(
                "                    __tokio_rt.block_on(user_code::{fn_name}({args}))\n"
            ));
            h.push_str("                }));\n");
        } else {
            h.push_str("                let (result, wt) = shatter_rust_runtime::execute_with_timing(std::panic::AssertUnwindSafe(|| {\n");
            h.push_str(&format!(
                "                    user_code::{fn_name}({args})\n"
            ));
            h.push_str("                }));\n");
        }
        // Map result.
        h.push_str("                match result {\n");
        if return_type.is_some() {
            h.push_str("                    Ok(ref v) => (Some(serde_json::to_value(v).unwrap_or(Value::Null)), None, wt),\n");
        } else {
            h.push_str("                    Ok(()) => (Some(Value::Null), None, wt),\n");
        }
        h.push_str("                    Err(ref msg) => (None, Some(serde_json::json!({\"error_type\": \"runtime_error\", \"message\": msg})), wt),\n");
        h.push_str("                }\n");
        h.push_str("            }\n");
    }
    // Unknown function fallback.
    h.push_str("            unknown => {\n");
    h.push_str("                (None, Some(serde_json::json!({\"error_type\": \"not_supported\", \"message\": format!(\"function not in dispatch table: {}\", unknown)})), 0.0)\n");
    h.push_str("            }\n");
    h.push_str("        };\n\n");

    // Restore stdout/stderr and read captured console output.
    h.push_str("        unsafe { libc::dup2(__orig_stdout, 1); libc::close(__orig_stdout); }\n");
    h.push_str("        unsafe { libc::dup2(__orig_stderr, 2); libc::close(__orig_stderr); }\n");
    h.push_str("        let mut __captured_stdout = String::new();\n");
    h.push_str("        let mut __captured_stderr = String::new();\n");
    h.push_str("        if let Ok(mut __f) = __stdout_file {\n");
    h.push_str("            use std::io::{Read, Seek, SeekFrom};\n");
    h.push_str("            let _ = __f.seek(SeekFrom::Start(0));\n");
    h.push_str("            let _ = __f.read_to_string(&mut __captured_stdout);\n");
    h.push_str("        }\n");
    h.push_str("        if let Ok(mut __f) = __stderr_file {\n");
    h.push_str("            use std::io::{Read, Seek, SeekFrom};\n");
    h.push_str("            let _ = __f.seek(SeekFrom::Start(0));\n");
    h.push_str("            let _ = __f.read_to_string(&mut __captured_stderr);\n");
    h.push_str("        }\n");
    h.push_str("        let _ = std::fs::remove_file(&__stdout_path);\n");
    h.push_str("        let _ = std::fs::remove_file(&__stderr_path);\n\n");

    // Build side effects: console_output, then thrown_error, then global_state_change.
    h.push_str("        let mut __side_effects: Vec<serde_json::Value> = Vec::new();\n");
    h.push_str("        for __line in __captured_stdout.lines() {\n");
    h.push_str("            if !__line.is_empty() {\n");
    h.push_str("                let __msg: String = __line.chars().take(4096).collect();\n");
    h.push_str("                __side_effects.push(serde_json::json!({\"kind\": \"console_output\", \"level\": \"log\", \"message\": __msg}));\n");
    h.push_str("            }\n");
    h.push_str("        }\n");
    h.push_str("        for __line in __captured_stderr.lines() {\n");
    h.push_str("            if !__line.is_empty() {\n");
    h.push_str("                let __msg: String = __line.chars().take(4096).collect();\n");
    h.push_str("                __side_effects.push(serde_json::json!({\"kind\": \"console_output\", \"level\": \"error\", \"message\": __msg}));\n");
    h.push_str("            }\n");
    h.push_str("        }\n");
    h.push_str("        if let Some(ref err) = err_val {\n");
    h.push_str("            if let Some(msg) = err.get(\"message\").and_then(|m| m.as_str()) {\n");
    h.push_str("                __side_effects.push(serde_json::json!({\"kind\": \"thrown_error\", \"error_type\": \"runtime_error\", \"message\": msg, \"stack\": null}));\n");
    h.push_str("            }\n");
    h.push_str("        }\n\n");

    // Global state changes.
    if !static_mut_names.is_empty() {
        for name in static_mut_names {
            h.push_str(&format!(
                "        let __after_{name} = unsafe {{ serde_json::to_value(&user_code::{name}).ok() }};\n"
            ));
            h.push_str(&format!(
                "        if let (Some(__b), Some(__a)) = (__before_{name}, __after_{name}) {{\n"
            ));
            h.push_str("            if __b != __a {\n");
            h.push_str(&format!(
                "                __side_effects.push(serde_json::json!({{\"kind\":\"global_state_change\",\"variable\":\"{name}\",\"before\":__b,\"after\":__a}}));\n"
            ));
            h.push_str("            }\n");
            h.push_str("        }\n");
        }
        h.push('\n');
        h.push_str("        shatter_rust_runtime::build_result_json(ret_val, err_val, wall_time_ms, __side_effects)\n");
    } else {
        h.push_str("        shatter_rust_runtime::build_result_json(ret_val, err_val, wall_time_ms, __side_effects)\n");
    }

    h.push_str("    });\n"); // end closure + run_dispatch_loop call
    h.push_str("}\n");

    Ok(h)
}

/// Compile the harness source and spawn it as a persistent subprocess.
///
/// The compiled binary lives in `harness_dir`. A reader thread is spawned
/// to forward response lines from the subprocess stdout to a channel, so
/// `PersistentHarness::execute()` can use `recv_timeout` for deadline control.
fn build_and_spawn_harness(
    harness_source: &str,
    harness_dir: &Path,
    runtime_path: &Path,
    needs_tokio: bool,
    needs_axum: bool,
    mut timing: Option<&mut TimingCollector>,
) -> Result<PersistentHarness, ExecuteError> {
    let src_dir = harness_dir.join("src");
    std::fs::create_dir_all(&src_dir)?;

    let cargo_toml = generate_cargo_toml(runtime_path, needs_tokio, needs_axum);
    std::fs::write(harness_dir.join("Cargo.toml"), &cargo_toml)?;
    std::fs::write(src_dir.join("main.rs"), harness_source)?;

    // Compile
    let build_timeout_secs = std::env::var("SHATTER_BUILD_TIMEOUT")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|&v| v > 0)
        .unwrap_or(DEFAULT_BUILD_TIMEOUT_SECS);
    let build_timeout = Duration::from_secs(build_timeout_secs);

    let release = harness_release_mode();
    let mut cargo_args = vec!["build"];
    if release {
        cargo_args.push("--release");
    }

    // Use a persistent target dir for dep caching, shared across harnesses.
    let target_dir = standalone_target_dir()
        .unwrap_or_else(|| harness_dir.join("target"));
    if let Some(parent) = target_dir.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    // Fast validation: cargo check catches type/borrow errors ~3x faster than build.
    cargo_check_before_build(
        harness_dir,
        &target_dir,
        release,
        timing.as_deref_mut(),
    )?;

    let build_start = Instant::now();
    let build_output = if let Some(t) = timing {
        t.record("execute.build", |_| {
            Command::new("cargo")
                .args(&cargo_args)
                .current_dir(harness_dir)
                .env("CARGO_TARGET_DIR", &target_dir)
                .output()
                .map_err(|e| ExecuteError::CompilationFailed(format!("failed to run cargo: {e}")))
        })?
    } else {
        Command::new("cargo")
            .args(&cargo_args)
            .current_dir(harness_dir)
            .env("CARGO_TARGET_DIR", &target_dir)
            .output()
            .map_err(|e| ExecuteError::CompilationFailed(format!("failed to run cargo: {e}")))?
    };

    if build_start.elapsed() > build_timeout {
        return Err(ExecuteError::CompilationFailed("build timed out".to_string()));
    }
    if !build_output.status.success() {
        return Err(ExecuteError::CompilationFailed(
            String::from_utf8_lossy(&build_output.stderr).into_owned(),
        ));
    }

    // Locate binary
    let binary_name = if cfg!(windows) { "shatter-exec-temp.exe" } else { "shatter-exec-temp" };
    let profile_dir = if release { "release" } else { "debug" };
    let binary_path = target_dir.join(profile_dir).join(binary_name);
    if !binary_path.exists() {
        return Err(ExecuteError::CompilationFailed("compiled binary not found".to_string()));
    }

    // Spawn the subprocess with stdin/stdout pipes
    let mut child = Command::new(&binary_path)
        .current_dir(harness_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(ExecuteError::IoError)?;

    let stdin = child.stdin.take().ok_or_else(|| {
        ExecuteError::IoError(io::Error::other("no stdin pipe"))
    })?;
    let stdout = child.stdout.take().ok_or_else(|| {
        ExecuteError::IoError(io::Error::other("no stdout pipe"))
    })?;

    // Reader thread: forwards JSON response lines from subprocess stdout to a channel.
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        use std::io::BufRead;
        let reader = std::io::BufReader::new(stdout);
        for line in reader.lines() {
            match line {
                Ok(l) if !l.is_empty() => {
                    if tx.send(l).is_err() {
                        break;
                    }
                }
                _ => break,
            }
        }
    });

    Ok(PersistentHarness {
        child,
        stdin,
        response_rx: rx,
        harness_dir: harness_dir.to_path_buf(),
    })
}

/// Compile a crate-backed dispatch harness and spawn it as a persistent subprocess.
///
/// Unlike `build_and_spawn_harness()`, this:
/// - Uses a per-harness target dir at `harness_dir/target/` for stable binary storage
/// - Skips `cargo build` if the binary exists and `src/main.rs` content is unchanged
fn build_and_spawn_crate_harness(
    harness_source: &str,
    cargo_toml_content: &str,
    harness_dir: &Path,
    mut timing: Option<&mut TimingCollector>,
) -> Result<PersistentHarness, ExecuteError> {
    let src_dir = harness_dir.join("src");
    std::fs::create_dir_all(&src_dir)?;

    let release = harness_release_mode();
    let profile_dir = if release { "release" } else { "debug" };
    let binary_name = if cfg!(windows) { "shatter-exec-temp.exe" } else { "shatter-exec-temp" };
    let target_dir = harness_dir.join("target");
    let binary_path = target_dir.join(profile_dir).join(binary_name);

    // Skip recompile if binary exists and harness source is unchanged.
    let main_rs_path = src_dir.join("main.rs");
    let already_built = binary_path.exists()
        && std::fs::read_to_string(&main_rs_path)
            .ok()
            .as_deref()
            == Some(harness_source);

    if !already_built {
        std::fs::write(harness_dir.join("Cargo.toml"), cargo_toml_content)?;
        std::fs::write(&main_rs_path, harness_source)?;

        // Fast validation: cargo check catches type/borrow errors ~3x faster than build.
        cargo_check_before_build(
            harness_dir,
            &target_dir,
            release,
            timing.as_deref_mut(),
        )?;

        let build_timeout_secs = std::env::var("SHATTER_BUILD_TIMEOUT")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|&v| v > 0)
            .unwrap_or(DEFAULT_BUILD_TIMEOUT_SECS);
        let build_timeout = Duration::from_secs(build_timeout_secs);

        let mut cargo_args = vec!["build"];
        if release {
            cargo_args.push("--release");
        }

        let build_start = Instant::now();
        let build_output = if let Some(t) = timing {
            t.record("execute.build", |_| {
                Command::new("cargo")
                    .args(&cargo_args)
                    .current_dir(harness_dir)
                    .env("CARGO_TARGET_DIR", &target_dir)
                    .output()
                    .map_err(|e| ExecuteError::CompilationFailed(format!("failed to run cargo: {e}")))
            })?
        } else {
            Command::new("cargo")
                .args(&cargo_args)
                .current_dir(harness_dir)
                .env("CARGO_TARGET_DIR", &target_dir)
                .output()
                .map_err(|e| ExecuteError::CompilationFailed(format!("failed to run cargo: {e}")))?
        };

        if build_start.elapsed() > build_timeout {
            return Err(ExecuteError::CompilationFailed("build timed out".to_string()));
        }
        if !build_output.status.success() {
            return Err(ExecuteError::CompilationFailed(
                String::from_utf8_lossy(&build_output.stderr).into_owned(),
            ));
        }
    }

    if !binary_path.exists() {
        return Err(ExecuteError::CompilationFailed("compiled binary not found".to_string()));
    }

    let mut child = Command::new(&binary_path)
        .current_dir(harness_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(ExecuteError::IoError)?;

    let stdin = child.stdin.take().ok_or_else(|| {
        ExecuteError::IoError(io::Error::other("no stdin pipe"))
    })?;
    let stdout = child.stdout.take().ok_or_else(|| {
        ExecuteError::IoError(io::Error::other("no stdout pipe"))
    })?;

    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        use std::io::BufRead;
        let reader = std::io::BufReader::new(stdout);
        for line in reader.lines() {
            match line {
                Ok(l) if !l.is_empty() => {
                    if tx.send(l).is_err() {
                        break;
                    }
                }
                _ => break,
            }
        }
    });

    Ok(PersistentHarness {
        child,
        stdin,
        response_rx: rx,
        harness_dir: harness_dir.to_path_buf(),
    })
}

// ─── crate_bridge implementation ─────────────────────────────────────────────

/// Content-addressed stable directory for a crate-bridge harness.
///
/// The directory path is deterministic: same crate root + wrapper hash + mocks → same path.
fn stable_crate_bridge_dir(crate_root: &Path, wrapper_hash: u64, mh: u64) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    crate_root.hash(&mut h);
    wrapper_hash.hash(&mut h);
    mh.hash(&mut h);
    let key = h.finish();
    harness_cache_root()
        .map(|c| c.join("rust").join("crate-bridge").join(format!("{key:016x}")))
        .unwrap_or_else(|| std::env::temp_dir().join(format!("shatter-crate-bridge-{key:016x}")))
}

/// Locate the `lib.rs` entry point for a crate.
///
/// Checks `[lib] path` in Cargo.toml first, falls back to `src/lib.rs`.
fn find_lib_rs(crate_root: &Path) -> Option<PathBuf> {
    let cargo_toml_path = crate_root.join("Cargo.toml");
    if let Ok(content) = std::fs::read_to_string(&cargo_toml_path) {
        // Look for `[lib]` section with a `path = "..."` override.
        let mut in_lib = false;
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed == "[lib]" {
                in_lib = true;
                continue;
            }
            if in_lib {
                if trimmed.starts_with('[') {
                    break;
                }
                if let Some(rest) = trimmed.strip_prefix("path") {
                    let rest = rest.trim().trim_start_matches('=').trim();
                    let path_val = rest.trim_matches('"').trim_matches('\'');
                    let candidate = crate_root.join(path_val);
                    if candidate.exists() {
                        return Some(candidate);
                    }
                }
            }
        }
    }
    let default = crate_root.join("src").join("lib.rs");
    if default.exists() { Some(default) } else { None }
}

/// Append `#[cfg(feature = "shatter-crate-bridge")] pub mod __shatter;` to lib.rs
/// if the declaration is not already present (idempotent).
fn inject_lib_module_declaration(lib_rs_path: &Path) -> Result<(), ExecuteError> {
    const MARKER: &str = "pub mod __shatter;";
    let content = std::fs::read_to_string(lib_rs_path)
        .map_err(|e| ExecuteError::IoError(io::Error::other(format!("cannot read lib.rs: {e}"))))?;
    if content.contains(MARKER) {
        return Ok(());
    }
    let declaration = "\n#[cfg(feature = \"shatter-crate-bridge\")]\npub mod __shatter;\n";
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(lib_rs_path)
        .map_err(ExecuteError::IoError)?;
    file.write_all(declaration.as_bytes())
        .map_err(ExecuteError::IoError)
}

/// Add the `shatter-crate-bridge` feature plus optional `serde_json` and
/// `shatter-rust-runtime` dependencies to the user's Cargo.toml (idempotent).
///
/// The `__shatter.rs` wrapper module calls both of these crates directly, so
/// they must be present as optional deps gated by the feature.
///
/// Injection strategy avoids duplicate TOML section headers:
/// - Appends a `[features]` block if no feature marker is present.
/// - Appends `[dependencies]` only when no `[dependencies]` section exists yet;
///   otherwise inserts dep lines directly after the existing header.
fn inject_crate_bridge_feature(
    cargo_toml_path: &Path,
    runtime_path: &Path,
) -> Result<(), ExecuteError> {
    const FEATURE_MARKER: &str = "shatter-crate-bridge";
    let content = std::fs::read_to_string(cargo_toml_path)
        .map_err(|e| ExecuteError::IoError(io::Error::other(format!("cannot read Cargo.toml: {e}"))))?;

    let needs_feature = !content.contains(FEATURE_MARKER);
    let needs_serde_json = !content.contains("serde_json");
    let needs_runtime = !content.contains("shatter-rust-runtime");

    if !needs_feature && !needs_serde_json && !needs_runtime {
        return Ok(());
    }

    let mut new_content = content.clone();

    // Insert new optional deps into the [dependencies] section (or add the section).
    let mut deps_to_add = String::new();
    if needs_serde_json {
        deps_to_add.push_str("serde_json = { version = \"1\", optional = true }\n");
    }
    if needs_runtime {
        let runtime_str = runtime_path.display().to_string().replace('\\', "/");
        deps_to_add.push_str(&format!(
            "shatter-rust-runtime = {{ path = \"{runtime_str}\", optional = true }}\n"
        ));
    }

    if !deps_to_add.is_empty() {
        if let Some(pos) = new_content.find("[dependencies]") {
            let insert_at = pos + "[dependencies]".len();
            let insert_at = new_content[insert_at..].find('\n')
                .map(|n| insert_at + n + 1)
                .unwrap_or(new_content.len());
            new_content.insert_str(insert_at, &deps_to_add);
        } else {
            new_content.push_str("\n[dependencies]\n");
            new_content.push_str(&deps_to_add);
        }
    }

    if needs_feature {
        let mut feature_deps = Vec::new();
        if needs_serde_json { feature_deps.push("\"dep:serde_json\""); }
        if needs_runtime { feature_deps.push("\"dep:shatter-rust-runtime\""); }
        let dep_list = feature_deps.join(", ");
        new_content.push_str(&format!("\n[features]\nshatter-crate-bridge = [{dep_list}]\n"));
    }

    std::fs::write(cargo_toml_path, new_content).map_err(ExecuteError::IoError)
}

/// Generate the `__shatter.rs` wrapper module content.
///
/// The module exposes:
/// - One `pub fn shatter_wrap_<name>(inputs: Vec<Value>) -> Value` per compatible function,
///   each calling the real function via `super::<name>()` to access private items.
/// - A `pub fn shatter_run_harness()` entry point with the stdin/stdout dispatch loop.
///   The stable driver binary just calls `<user_crate>::__shatter::shatter_run_harness()`.
fn generate_crate_bridge_wrapper(
    fns: &[CompatFn],
    mocks_json: &str,
    static_mut_names: &[String],
) -> String {
    let mut w = String::with_capacity(8192);
    w.push_str("// Generated by shatter-rust crate_bridge — do not edit\n");
    w.push_str("#![allow(unused_imports, dead_code, clippy::all)]\n");
    w.push_str("use serde_json::Value;\n\n");

    // Per-function wrapper: deserialise inputs, call via super::, return JSON.
    for fn_info in fns {
        let fn_name = &fn_info.name;
        let param_names = &fn_info.param_names;
        let param_types = &fn_info.param_types;
        let return_type = &fn_info.return_type;

        w.push_str(&format!(
            "pub fn shatter_wrap_{fn_name}(inputs: Vec<Value>) -> Value {{\n"
        ));

        for (i, (name, ty)) in param_names.iter().zip(param_types.iter()).enumerate() {
            let clean = name.strip_prefix("mut ").unwrap_or(name).trim();
            if let Some(mapping) = owned_type_for_ref(ty) {
                w.push_str(&format!(
                    "    let {clean}_owned: {} = serde_json::from_value(inputs[{i}].clone()).unwrap_or_default();\n",
                    mapping.deser_type
                ));
                if mapping.needs_slice_conversion {
                    w.push_str(&format!(
                        "    let {clean}_refs: Vec<&str> = {clean}_owned.iter().map(|s| s.as_str()).collect();\n"
                    ));
                }
            } else {
                w.push_str(&format!(
                    "    let {clean}: {ty} = serde_json::from_value(inputs[{i}].clone()).unwrap_or_default();\n"
                ));
            }
        }

        let arg_list: Vec<String> = param_names.iter().zip(param_types.iter()).map(|(n, ty)| {
            let clean = n.strip_prefix("mut ").unwrap_or(n).trim();
            if let Some(mapping) = owned_type_for_ref(ty) {
                if mapping.needs_slice_conversion { format!("&{clean}_refs") } else { format!("&{clean}_owned") }
            } else {
                clean.to_string()
            }
        }).collect();
        let args = arg_list.join(", ");

        // Snapshot static mut before call.
        for name in static_mut_names {
            w.push_str(&format!(
                "    let __before_{name} = unsafe {{ serde_json::to_value(&super::{name}).ok() }};\n"
            ));
        }

        w.push_str("    shatter_rust_runtime::reset();\n");
        w.push_str("    let start = std::time::Instant::now();\n");
        if fn_info.is_async {
            w.push_str("    let __tokio_rt = tokio::runtime::Runtime::new().unwrap();\n");
            w.push_str("    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {\n");
            w.push_str(&format!("        __tokio_rt.block_on(super::{fn_name}({args}))\n"));
            w.push_str("    }));\n");
        } else {
            w.push_str("    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {\n");
            w.push_str(&format!("        super::{fn_name}({args})\n"));
            w.push_str("    }));\n");
        }
        w.push_str("    let wall_time_ms = start.elapsed().as_secs_f64() * 1000.0;\n");
        w.push_str("    let runtime_json = shatter_rust_runtime::flush_results();\n");
        w.push_str("    let mut exec_result: Value = serde_json::from_str(&runtime_json).unwrap_or(Value::Object(Default::default()));\n");
        w.push_str("    let obj = exec_result.as_object_mut().unwrap();\n");

        if return_type.is_some() {
            w.push_str("    match result {\n");
            w.push_str("        Ok(ref ret_val) => { obj.insert(\"return_value\".into(), serde_json::to_value(ret_val).unwrap_or(Value::Null)); }\n");
        } else {
            w.push_str("    match result {\n");
            w.push_str("        Ok(()) => { obj.insert(\"return_value\".into(), Value::Null); }\n");
        }
        w.push_str("        Err(ref panic_info) => {\n");
        w.push_str("            let msg = if let Some(s) = panic_info.downcast_ref::<&str>() { s.to_string() } else if let Some(s) = panic_info.downcast_ref::<String>() { s.clone() } else { format!(\"{:?}\", panic_info) };\n");
        w.push_str("            obj.insert(\"thrown_error\".into(), serde_json::json!({\"error_type\": \"runtime_error\", \"message\": msg}));\n");
        w.push_str("        }\n");
        w.push_str("    }\n");
        w.push_str("    obj.insert(\"performance\".into(), serde_json::json!({\"wall_time_ms\": wall_time_ms, \"cpu_time_us\": 0, \"heap_used_bytes\": 0, \"heap_allocated_bytes\": 0}));\n");

        // Thrown error side effect.
        w.push_str("    if let Err(ref panic_info) = result {\n");
        w.push_str("        let msg = if let Some(s) = panic_info.downcast_ref::<&str>() { s.to_string() } else if let Some(s) = panic_info.downcast_ref::<String>() { s.clone() } else { format!(\"{:?}\", panic_info) };\n");
        w.push_str("        let __se = obj.entry(\"side_effects\").or_insert(serde_json::json!([]));\n");
        w.push_str("        if let Some(__arr) = __se.as_array_mut() { __arr.push(serde_json::json!({\"kind\": \"thrown_error\", \"error_type\": \"runtime_error\", \"message\": msg, \"stack\": null})); }\n");
        w.push_str("    }\n");

        // Detect global state changes.
        if !static_mut_names.is_empty() {
            w.push_str("    let mut __global_se: Vec<Value> = Vec::new();\n");
            for name in static_mut_names {
                w.push_str(&format!(
                    "    let __after_{name} = unsafe {{ serde_json::to_value(&super::{name}).ok() }};\n"
                ));
                w.push_str(&format!(
                    "    if let (Some(__b), Some(__a)) = (__before_{name}, __after_{name}) {{\n"
                ));
                w.push_str("        if __b != __a {\n");
                w.push_str(&format!(
                    "            __global_se.push(serde_json::json!({{\"kind\":\"global_state_change\",\"variable\":\"{name}\",\"before\":__b,\"after\":__a}}));\n"
                ));
                w.push_str("        }\n");
                w.push_str("    }\n");
            }
            w.push_str("    let __se = obj.entry(\"side_effects\").or_insert(serde_json::json!([]));\n");
            w.push_str("    if let Some(__arr) = __se.as_array_mut() { __arr.extend(__global_se); }\n");
            w.push_str("    drop(obj);\n");
        } else {
            w.push_str("    obj.entry(\"side_effects\").or_insert(serde_json::json!([]));\n");
            w.push_str("    drop(obj);\n");
        }
        w.push_str("    exec_result\n");
        w.push_str("}\n\n");
    }

    // shatter_run_harness: the stable entry point called by the driver binary.
    w.push_str("/// Stable entry point called by the crate-bridge driver binary.\n");
    w.push_str("/// Reads `{\"function\": \"name\", \"inputs\": [...]}` lines from stdin,\n");
    w.push_str("/// dispatches to the appropriate wrapper, and writes JSON results to stdout.\n");
    w.push_str("pub fn shatter_run_harness() {\n");
    w.push_str("    use std::io::BufRead;\n");
    w.push_str(&format!("    let mocks_json = r#\"{}\"#;\n", mocks_json));
    w.push_str("    let mocks: Vec<Value> = serde_json::from_str(mocks_json).unwrap_or_default();\n");
    w.push_str("    for mock in &mocks {\n");
    w.push_str("        if let (Some(symbol), Some(return_values)) = (\n");
    w.push_str("            mock.get(\"symbol\").and_then(|s| s.as_str()),\n");
    w.push_str("            mock.get(\"return_values\").and_then(|v| v.as_array()),\n");
    w.push_str("        ) {\n");
    w.push_str("            shatter_rust_runtime::register_mock(symbol, return_values.clone());\n");
    w.push_str("        }\n");
    w.push_str("    }\n\n");
    w.push_str("    let stdin = std::io::stdin();\n");
    w.push_str("    let mut reader = std::io::BufReader::new(stdin.lock());\n");
    w.push_str("    loop {\n");
    w.push_str("        let mut line = String::new();\n");
    w.push_str("        match reader.read_line(&mut line) {\n");
    w.push_str("            Ok(0) | Err(_) => break,\n");
    w.push_str("            Ok(_) => {}\n");
    w.push_str("        }\n");
    w.push_str("        let line = line.trim();\n");
    w.push_str("        if line.is_empty() { continue; }\n");
    w.push_str("        let req: Value = serde_json::from_str(line).unwrap_or_default();\n");
    w.push_str("        let function_name = req[\"function\"].as_str().unwrap_or(\"\");\n");
    w.push_str("        let inputs = req[\"inputs\"].as_array().cloned().unwrap_or_default();\n");
    w.push_str("        let exec_result = match function_name {\n");
    for fn_info in fns {
        let fn_name = &fn_info.name;
        w.push_str(&format!("            {:?} => shatter_wrap_{fn_name}(inputs),\n", fn_name.as_str()));
    }
    w.push_str("            unknown => serde_json::json!({\"return_value\": null, \"thrown_error\": {\"error_type\": \"not_supported\", \"message\": format!(\"function not in crate_bridge dispatch table: {}\", unknown)}, \"branch_path\": [], \"lines_executed\": [], \"calls_to_external\": [], \"path_constraints\": [], \"side_effects\": [], \"performance\": {\"wall_time_ms\": 0.0, \"cpu_time_us\": 0, \"heap_used_bytes\": 0, \"heap_allocated_bytes\": 0}}),\n");
    w.push_str("        };\n");
    w.push_str("        println!(\"{}\", serde_json::to_string(&exec_result).unwrap());\n");
    w.push_str("        let _ = std::io::Write::flush(&mut std::io::stdout());\n");
    w.push_str("    }\n");
    w.push_str("}\n");

    w
}

/// Generate the stable driver binary `main.rs`.
///
/// The body never changes — it just calls the wrapper module's entry point.
/// The binary is "stable" because its source is constant regardless of which
/// function is being tested.
fn generate_crate_bridge_bin(crate_name: &str) -> String {
    format!(
        "fn main() {{\n    {}::__shatter::shatter_run_harness();\n}}\n",
        crate_name
    )
}

/// Generate a Cargo.toml for the crate-bridge driver binary.
///
/// The driver depends on the user's crate (by path) with the
/// `shatter-crate-bridge` feature enabled, so it compiles `__shatter.rs`.
fn generate_crate_bridge_cargo_toml(crate_name: &str, crate_root: &Path, needs_tokio: bool) -> String {
    let crate_path = crate_root.display().to_string().replace('\\', "/");
    let tokio_dep = if needs_tokio {
        "tokio = { version = \"1\", features = [\"full\"] }\n"
    } else {
        ""
    };
    format!(
        r#"[package]
name = "shatter-crate-bridge-exec"
version = "0.1.0"
edition = "2021"

[workspace]

[dependencies]
{crate_name} = {{ path = "{crate_path}", features = ["shatter-crate-bridge"] }}
{tokio_dep}"#
    )
}

/// Extract the `name` field from a `[package]` section of a Cargo.toml string.
fn extract_crate_name(cargo_toml: &str) -> Option<String> {
    let mut in_package = false;
    for line in cargo_toml.lines() {
        let trimmed = line.trim();
        if trimmed == "[package]" {
            in_package = true;
            continue;
        }
        if in_package {
            if trimmed.starts_with('[') {
                break;
            }
            if let Some(rest) = trimmed.strip_prefix("name") {
                let rest = rest.trim().trim_start_matches('=').trim();
                let name = rest.trim_matches('"').trim_matches('\'').to_string();
                if !name.is_empty() {
                    return Some(name);
                }
            }
        }
    }
    None
}

/// Compile the crate-bridge driver binary and spawn it as a persistent subprocess.
///
/// The driver binary lives in `harness_dir`. If the binary already exists and
/// `src/main.rs` is unchanged, recompilation is skipped (stable cache).
fn build_and_spawn_crate_bridge_harness(
    driver_source: &str,
    cargo_toml_content: &str,
    harness_dir: &Path,
    mut timing: Option<&mut TimingCollector>,
) -> Result<PersistentHarness, ExecuteError> {
    let src_dir = harness_dir.join("src");
    std::fs::create_dir_all(&src_dir)?;

    let release = harness_release_mode();
    let profile_dir = if release { "release" } else { "debug" };
    let binary_name = if cfg!(windows) { "shatter-crate-bridge-exec.exe" } else { "shatter-crate-bridge-exec" };
    let target_dir = harness_dir.join("target");
    let binary_path = target_dir.join(profile_dir).join(binary_name);

    let main_rs_path = src_dir.join("main.rs");
    let already_built = binary_path.exists()
        && std::fs::read_to_string(&main_rs_path).ok().as_deref() == Some(driver_source);

    if !already_built {
        std::fs::write(harness_dir.join("Cargo.toml"), cargo_toml_content)?;
        std::fs::write(&main_rs_path, driver_source)?;

        cargo_check_before_build(harness_dir, &target_dir, release, timing.as_deref_mut())?;

        let build_timeout_secs = std::env::var("SHATTER_BUILD_TIMEOUT")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|&v| v > 0)
            .unwrap_or(DEFAULT_BUILD_TIMEOUT_SECS);
        let build_timeout = Duration::from_secs(build_timeout_secs);

        let mut cargo_args = vec!["build"];
        if release {
            cargo_args.push("--release");
        }

        let build_start = Instant::now();
        let build_output = if let Some(t) = timing {
            t.record("execute.build", |_| {
                Command::new("cargo")
                    .args(&cargo_args)
                    .current_dir(harness_dir)
                    .env("CARGO_TARGET_DIR", &target_dir)
                    .output()
                    .map_err(|e| ExecuteError::CompilationFailed(format!("failed to run cargo: {e}")))
            })?
        } else {
            Command::new("cargo")
                .args(&cargo_args)
                .current_dir(harness_dir)
                .env("CARGO_TARGET_DIR", &target_dir)
                .output()
                .map_err(|e| ExecuteError::CompilationFailed(format!("failed to run cargo: {e}")))?
        };

        if build_start.elapsed() > build_timeout {
            return Err(ExecuteError::CompilationFailed("build timed out".to_string()));
        }
        if !build_output.status.success() {
            return Err(ExecuteError::CompilationFailed(
                String::from_utf8_lossy(&build_output.stderr).into_owned(),
            ));
        }
    }

    if !binary_path.exists() {
        return Err(ExecuteError::CompilationFailed("compiled crate-bridge binary not found".to_string()));
    }

    let mut child = Command::new(&binary_path)
        .current_dir(harness_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(ExecuteError::IoError)?;

    let stdin = child.stdin.take().ok_or_else(|| ExecuteError::IoError(io::Error::other("no stdin pipe")))?;
    let stdout = child.stdout.take().ok_or_else(|| ExecuteError::IoError(io::Error::other("no stdout pipe")))?;

    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        use std::io::BufRead;
        let reader = std::io::BufReader::new(stdout);
        for line in reader.lines() {
            match line {
                Ok(l) if !l.is_empty() => {
                    if tx.send(l).is_err() { break; }
                }
                _ => break,
            }
        }
    });

    Ok(PersistentHarness {
        child,
        stdin,
        response_rx: rx,
        harness_dir: harness_dir.to_path_buf(),
    })
}

/// Execute a function via the crate_bridge harness mode.
///
/// Injects a feature-gated `__shatter.rs` wrapper module into the user's library
/// crate, compiles a thin driver binary that depends on the user crate, and
/// dispatches execution through `__shatter::shatter_run_harness()`. This allows
/// calling crate-private functions that the bin_only harness cannot reach.
#[allow(clippy::too_many_arguments)]
fn execute_function_crate_bridge(
    file_path: &str,
    function_name: &str,
    inputs: &[Value],
    mocks: &[Value],
    timeout_ms: u64,
    mut timing: Option<&mut TimingCollector>,
    bridge_cache: &CrateBridgeHarnessCache,
    crate_root: &Path,
) -> Result<ExecuteResult, ExecuteError> {
    let source = std::fs::read_to_string(file_path)
        .map_err(|e| ExecuteError::FileError(format!("cannot read {file_path}: {e}")))?;
    let mh = mocks_hash(mocks);

    // Collect all functions and build the wrapper to get a stable wrapper_hash.
    let all_fn_ctxs = extract_all_fn_contexts(&source);
    let static_mut_names = extract_static_mut_items(&source);

    // For crate_bridge, generic/dyn constraints still apply (can't deserialise them),
    // but external-type and module-path restrictions are lifted.
    let compatible_fns: Vec<CompatFn> = all_fn_ctxs
        .iter()
        .filter_map(|(name, ctx)| {
            // Only block on generics and trait objects; allow external + module-path refs.
            let has_generics = ctx.sig.has_generics;
            let has_dyn = ctx.sig.param_types.iter().any(|t| is_trait_object_type(t));
            if has_generics || has_dyn {
                None
            } else {
                Some(CompatFn {
                    name: name.clone(),
                    param_names: ctx.sig.param_names.clone(),
                    param_types: ctx.sig.param_types.clone(),
                    return_type: ctx.sig.return_type.clone(),
                    is_async: ctx.sig.is_async,
                })
            }
        })
        .collect();

    if !compatible_fns.iter().any(|f| f.name == function_name) {
        // Give a precise error for the requested function.
        if let Some((_, ctx)) = all_fn_ctxs.iter().find(|(n, _)| n == function_name) {
            if ctx.sig.has_generics {
                return Err(ExecuteError::NonExecutable(format!(
                    "crate_bridge: function `{function_name}` has generic type parameters — cannot deserialise concrete inputs"
                )));
            }
            if ctx.sig.param_types.iter().any(|t| is_trait_object_type(t)) {
                return Err(ExecuteError::NonExecutable(format!(
                    "crate_bridge: function `{function_name}` has trait object parameters — cannot deserialise from JSON"
                )));
            }
        }
        return Err(ExecuteError::NonExecutable(format!(
            "function `{function_name}` not found in `{file_path}`"
        )));
    }

    let expected_inputs = compatible_fns.iter()
        .find(|f| f.name == function_name)
        .map(|f| f.param_names.len())
        .unwrap_or(0);
    if inputs.len() != expected_inputs {
        return Err(ExecuteError::InstrumentError(format!(
            "expected {expected_inputs} inputs for {function_name}, got {}",
            inputs.len()
        )));
    }

    let mocks_json = serde_json::to_string(mocks)
        .map_err(|e| ExecuteError::InstrumentError(format!("cannot serialize mocks: {e}")))?;

    // Instrument the source for branch/coverage tracking (whole file, no filter).
    let instr_result = if let Some(timing) = timing.as_deref_mut() {
        timing.record("execute.instrument", |timing| {
            instrument::instrument_source_with_timing(&source, None, Some(timing))
                .map_err(|e| ExecuteError::InstrumentError(e.to_string()))
        })?
    } else {
        instrument::instrument_source(&source, None)
            .map_err(|e| ExecuteError::InstrumentError(e.to_string()))?
    };

    let wrapper_content = generate_crate_bridge_wrapper(&compatible_fns, &mocks_json, &static_mut_names);
    let wrapper_hash = source_hash(&wrapper_content);

    let key = CrateBridgeHarnessKey {
        crate_root: crate_root.to_path_buf(),
        wrapper_hash,
        mocks_hash: mh,
    };

    // Fast path: harness already running and function in dispatch table.
    {
        let mut map = bridge_cache.lock().unwrap();
        if let Some(entry) = map.get_mut(&key)
            && entry.compatible_functions.contains(function_name)
        {
            let result = entry.harness.execute_dispatch(function_name, inputs, timeout_ms)?;
            if result.thrown_error.as_ref()
                .and_then(|e| e.get("error_type"))
                .and_then(|v| v.as_str()) == Some("timeout")
            {
                map.remove(&key);
            }
            return Ok(result);
        }
    }

    // Slow path: inject wrapper into user crate and build driver binary.
    let user_cargo_toml_path = crate_root.join("Cargo.toml");
    let user_cargo_toml = std::fs::read_to_string(&user_cargo_toml_path)
        .map_err(|e| ExecuteError::FileError(format!("cannot read Cargo.toml: {e}")))?;

    let crate_name = extract_crate_name(&user_cargo_toml).unwrap_or_else(|| "user_crate".to_string());
    let runtime_path = find_runtime_crate_path()?;

    // Write the instrumented source back to the original file so instrumentation
    // is active when the crate compiles.
    std::fs::write(file_path, &instr_result.source)
        .map_err(|e| ExecuteError::IoError(io::Error::other(format!("cannot write instrumented source: {e}"))))?;

    // Write the wrapper module: `__shatter.rs` with pub wrappers + shatter_run_harness().
    let shatter_module_path = crate_root.join("src").join("__shatter.rs");
    std::fs::write(&shatter_module_path, &wrapper_content)
        .map_err(ExecuteError::IoError)?;

    // Inject mod declaration into lib.rs (idempotent).
    if let Some(lib_rs) = find_lib_rs(crate_root) {
        inject_lib_module_declaration(&lib_rs)?;
    } else {
        return Err(ExecuteError::NonExecutable(
            "crate_bridge: cannot find lib.rs — only library crates are supported".to_string(),
        ));
    }

    // Inject feature + optional serde_json + shatter-rust-runtime into user Cargo.toml (idempotent).
    inject_crate_bridge_feature(&user_cargo_toml_path, &runtime_path)?;

    let needs_tokio = compatible_fns.iter().any(|f| f.is_async);
    let driver_source = generate_crate_bridge_bin(&crate_name.replace('-', "_"));
    let driver_cargo_toml = generate_crate_bridge_cargo_toml(&crate_name, crate_root, needs_tokio);

    let harness_dir = stable_crate_bridge_dir(crate_root, wrapper_hash, mh);
    std::fs::create_dir_all(&harness_dir)?;

    let mut harness = if let Some(timing) = timing {
        timing.record("execute.build", |timing| {
            build_and_spawn_crate_bridge_harness(&driver_source, &driver_cargo_toml, &harness_dir, Some(timing))
        })?
    } else {
        build_and_spawn_crate_bridge_harness(&driver_source, &driver_cargo_toml, &harness_dir, None)?
    };

    let result = harness.execute_dispatch(function_name, inputs, timeout_ms)?;

    let timed_out = result.thrown_error.as_ref()
        .and_then(|e| e.get("error_type"))
        .and_then(|v| v.as_str()) == Some("timeout");

    if !timed_out {
        let compatible_set: HashSet<String> = compatible_fns.iter().map(|f| f.name.clone()).collect();
        bridge_cache.lock().unwrap().insert(key, CrateBridgeHarnessEntry {
            harness,
            compatible_functions: compatible_set,
        });
    } else {
        let _ = std::fs::remove_dir_all(&harness_dir);
    }

    Ok(result)
}

/// Execute a function from a crate-backed Rust file using the stable bin-only dispatch harness.
///
/// One harness handles all compatible functions in the file; different inputs for the same
/// function reuse the running subprocess without recompilation. The compiled binary is stored
/// at a stable content-addressed path and reused across process restarts.
#[allow(clippy::too_many_arguments)]
fn execute_function_crate_backed(
    file_path: &str,
    function_name: &str,
    inputs: &[Value],
    mocks: &[Value],
    timeout_ms: u64,
    mut timing: Option<&mut TimingCollector>,
    crate_cache: &CrateHarnessCache,
    crate_root: &Path,
) -> Result<ExecuteResult, ExecuteError> {
    let source = std::fs::read_to_string(file_path)
        .map_err(|e| ExecuteError::FileError(format!("cannot read {file_path}: {e}")))?;
    let src_hash = source_hash(&source);
    let mh = mocks_hash(mocks);

    let key = CrateHarnessKey {
        file_path: file_path.to_string(),
        source_hash: src_hash,
        mocks_hash: mh,
    };

    // Fast path: dispatch harness running, function in dispatch table.
    {
        let mut map = crate_cache.lock().unwrap();
        if let Some(entry) = map.get_mut(&key) {
            if entry.compatible_functions.contains(function_name) {
                let result = entry.harness.execute_dispatch(function_name, inputs, timeout_ms)?;
                if result.thrown_error.as_ref().and_then(|e| e.get("error_type")).and_then(|v| v.as_str()) == Some("timeout") {
                    map.remove(&key);
                }
                return Ok(result);
            }
            // Function exists in cache but not in dispatch table → not executable in bin_only mode.
            return Err(ExecuteError::NonExecutable(format!(
                "function `{function_name}` is not compatible with bin_only harness mode\n\nHint: use crate_bridge mode to execute within the original crate context"
            )));
        }
    }

    // Slow path: compile a new dispatch harness for this file.
    let all_fn_ctxs = extract_all_fn_contexts(&source);
    let static_mut_names = extract_static_mut_items(&source);

    // Check which functions are compatible with crate-backed bin_only mode.
    let compatible_fns: Vec<CompatFn> = all_fn_ctxs
        .iter()
        .filter_map(|(name, ctx)| {
            if check_bin_only_compatibility(name, ctx, true).is_ok() {
                Some(CompatFn {
                    name: name.clone(),
                    param_names: ctx.sig.param_names.clone(),
                    param_types: ctx.sig.param_types.clone(),
                    return_type: ctx.sig.return_type.clone(),
                    is_async: ctx.sig.is_async,
                })
            } else {
                None
            }
        })
        .collect();

    if !compatible_fns.iter().any(|f| f.name == function_name) {
        // Give a precise error by extracting the specific incompatibilities.
        if let Some((_, ctx)) = all_fn_ctxs.iter().find(|(n, _)| n == function_name) {
            check_bin_only_compatibility(function_name, ctx, true)?; // will return Err
        }
        return Err(ExecuteError::NonExecutable(format!(
            "function `{function_name}` not found or not compatible with bin_only harness mode"
        )));
    }

    let expected_inputs = compatible_fns.iter()
        .find(|f| f.name == function_name)
        .map(|f| f.param_names.len())
        .unwrap_or(0);
    if inputs.len() != expected_inputs {
        return Err(ExecuteError::InstrumentError(format!(
            "expected {expected_inputs} inputs for {function_name}, got {}",
            inputs.len()
        )));
    }

    let instr_result = if let Some(timing) = timing.as_deref_mut() {
        timing.record("execute.instrument", |timing| {
            instrument::instrument_source_with_timing(&source, None, Some(timing))
                .map_err(|e| ExecuteError::InstrumentError(e.to_string()))
        })?
    } else {
        instrument::instrument_source(&source, None)
            .map_err(|e| ExecuteError::InstrumentError(e.to_string()))?
    };

    let mocks_json = serde_json::to_string(mocks).map_err(|e| {
        ExecuteError::InstrumentError(format!("cannot serialize mocks: {e}"))
    })?;

    let needs_tokio = compatible_fns.iter().any(|f| f.is_async);
    let harness_source = generate_dispatch_harness(
        &instr_result.source,
        &compatible_fns,
        &mocks_json,
        &static_mut_names,
    )?;

    let user_cargo_toml_path = crate_root.join("Cargo.toml");
    let user_cargo_toml = std::fs::read_to_string(&user_cargo_toml_path)
        .unwrap_or_default();

    let runtime_path = find_runtime_crate_path()?;
    let cargo_toml_content = generate_cargo_toml_with_user_deps(&user_cargo_toml, &runtime_path, needs_tokio, false);

    let harness_dir = stable_crate_harness_dir(file_path, src_hash, mh);
    std::fs::create_dir_all(&harness_dir)?;

    let mut harness = if let Some(timing) = timing {
        timing.record("execute.build", |timing| {
            build_and_spawn_crate_harness(&harness_source, &cargo_toml_content, &harness_dir, Some(timing))
        })?
    } else {
        build_and_spawn_crate_harness(&harness_source, &cargo_toml_content, &harness_dir, None)?
    };

    let result = harness.execute_dispatch(function_name, inputs, timeout_ms)?;

    let timed_out = result.thrown_error.as_ref()
        .and_then(|e| e.get("error_type"))
        .and_then(|v| v.as_str()) == Some("timeout");

    if !timed_out {
        let compatible_set: HashSet<String> = compatible_fns.iter().map(|f| f.name.clone()).collect();
        crate_cache.lock().unwrap().insert(key, CrateHarnessEntry {
            harness,
            compatible_functions: compatible_set,
        });
    } else {
        let _ = std::fs::remove_dir_all(&harness_dir);
    }

    Ok(result)
}

/// Pre-build and cache a harness for a given (file, function, mocks) triple
/// without sending any inputs. This is the backend for the `prepare` command.
///
/// On success the harness subprocess is alive and cached — the next `execute`
/// call for the same triple will hit the fast path and skip compilation.
#[allow(clippy::too_many_arguments)]
pub fn prepare_harness(
    file_path: &str,
    function_name: &str,
    mocks: &[Value],
    timeout_ms: u64,
    harness_mode: Option<&str>,
    cache: &HarnessCache,
    crate_cache: &CrateHarnessCache,
    bridge_cache: &CrateBridgeHarnessCache,
) -> Result<(), ExecuteError> {
    // Build a dummy input vector that matches the function's arity.
    // We send one request so the harness is fully initialised and cached.
    let path = Path::new(file_path);
    if !path.exists() {
        return Err(ExecuteError::FileError(format!("file not found: {file_path}")));
    }

    let source = std::fs::read_to_string(path)
        .map_err(|e| ExecuteError::FileError(format!("cannot read {file_path}: {e}")))?;

    // For crate_bridge and crate_backed paths, the harness serves all functions
    // in the file — we just need to build it. Pick the target function to
    // validate it exists and is compatible.
    if harness_mode == Some("crate_bridge") {
        let crate_root = find_crate_root(file_path).ok_or_else(|| {
            ExecuteError::NonExecutable(
                "crate_bridge mode requires the target file to be inside a Cargo.toml crate".to_string(),
            )
        })?;
        // Build the bridge harness by executing with default inputs.
        let ctx = extract_fn_context(&source, function_name)?;
        let dummy_inputs: Vec<Value> = ctx.sig.param_names.iter().map(|_| Value::Null).collect();
        let _ = execute_function_crate_bridge(
            file_path, function_name, &dummy_inputs, mocks,
            timeout_ms, None, bridge_cache, &crate_root,
        )?;
        return Ok(());
    }

    if let Some(crate_root) = find_crate_root(file_path) {
        let ctx = extract_fn_context(&source, function_name)?;
        let dummy_inputs: Vec<Value> = ctx.sig.param_names.iter().map(|_| Value::Null).collect();
        let _ = execute_function_crate_backed(
            file_path, function_name, &dummy_inputs, mocks,
            timeout_ms, None, crate_cache, &crate_root,
        )?;
        return Ok(());
    }

    // Standalone harness: build, spawn, cache — but we still need to send one
    // request so the subprocess starts its read loop.
    let ctx = extract_fn_context(&source, function_name)?;
    let sig = &ctx.sig;
    check_bin_only_compatibility(function_name, &ctx, false)?;

    let dummy_inputs: Vec<Value> = sig.param_names.iter().map(|_| Value::Null).collect();
    let _ = execute_function(
        file_path, function_name, &dummy_inputs, mocks,
        timeout_ms, harness_mode, cache, crate_cache, bridge_cache,
    )?;
    Ok(())
}

/// Compute a prepare_id from file, function, and mocks (SHA-256, first 16 hex chars).
/// Matches the convention used by TS and Go frontends.
pub fn compute_prepare_id(file_path: &str, function_name: &str, mocks: &[Value]) -> String {
    use sha2::{Digest, Sha256};

    let mut mock_symbols: Vec<String> = mocks
        .iter()
        .filter_map(|m| m.get("symbol").and_then(|s| s.as_str()).map(String::from))
        .collect();
    mock_symbols.sort();

    let input = format!("{}:{}:{}", file_path, function_name, mock_symbols.join(","));
    let hash = Sha256::digest(input.as_bytes());
    hash.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

/// Execute an instrumented Rust function via a persistent harness subprocess.
///
/// On the first call for a given (file, function, mocks) triple, compiles the
/// harness and spawns the subprocess. Subsequent calls reuse the cached process.
#[allow(clippy::too_many_arguments)]
pub fn execute_function(
    file_path: &str,
    function_name: &str,
    inputs: &[Value],
    mocks: &[Value],
    timeout_ms: u64,
    harness_mode: Option<&str>,
    cache: &HarnessCache,
    crate_cache: &CrateHarnessCache,
    bridge_cache: &CrateBridgeHarnessCache,
) -> Result<ExecuteResult, ExecuteError> {
    execute_function_with_timing(file_path, function_name, inputs, mocks, timeout_ms, harness_mode, None, cache, crate_cache, bridge_cache)
}

#[allow(clippy::too_many_arguments)]
pub fn execute_function_with_timing(
    file_path: &str,
    function_name: &str,
    inputs: &[Value],
    mocks: &[Value],
    timeout_ms: u64,
    harness_mode: Option<&str>,
    mut timing: Option<&mut TimingCollector>,
    cache: &HarnessCache,
    crate_cache: &CrateHarnessCache,
    bridge_cache: &CrateBridgeHarnessCache,
) -> Result<ExecuteResult, ExecuteError> {
    let path = Path::new(file_path);
    if !path.exists() {
        return Err(ExecuteError::FileError(format!("file not found: {file_path}")));
    }

    // Explicit opt-in to crate_bridge mode: inject wrapper into the library crate.
    if harness_mode == Some("crate_bridge") {
        let crate_root = find_crate_root(file_path).ok_or_else(|| {
            ExecuteError::NonExecutable(
                "crate_bridge mode requires the target file to be inside a Cargo.toml crate".to_string(),
            )
        })?;
        return execute_function_crate_bridge(
            file_path,
            function_name,
            inputs,
            mocks,
            timeout_ms,
            timing,
            bridge_cache,
            &crate_root,
        );
    }

    // Route crate-backed files to the stable bin-only dispatch harness path.
    if let Some(crate_root) = find_crate_root(file_path) {
        return execute_function_crate_backed(
            file_path,
            function_name,
            inputs,
            mocks,
            timeout_ms,
            timing,
            crate_cache,
            &crate_root,
        );
    }

    // Compute cache key before doing any expensive work.
    let key = HarnessKey {
        file_path: file_path.to_string(),
        function_name: function_name.to_string(),
        mocks_hash: mocks_hash(mocks),
    };

    // Fast path: harness already compiled and running.
    {
        let mut map = cache.lock().unwrap();
        if let Some(harness) = map.get_mut(&key) {
            let result = harness.execute(inputs, timeout_ms)?;
            // If the harness timed out it killed itself; remove from cache.
            if result.thrown_error.as_ref().and_then(|e| e.get("error_type")).and_then(|v| v.as_str()) == Some("timeout") {
                map.remove(&key);
            }
            return Ok(result);
        }
    }

    // Slow path: first call for this (file, function, mocks) — compile and spawn.
    let source = if let Some(timing) = timing.as_deref_mut() {
        timing.record("execute.read_source", |_| {
            std::fs::read_to_string(path)
                .map_err(|e| ExecuteError::FileError(format!("cannot read {file_path}: {e}")))
        })?
    } else {
        std::fs::read_to_string(path)
            .map_err(|e| ExecuteError::FileError(format!("cannot read {file_path}: {e}")))?
    };

    let ctx = if let Some(timing) = timing.as_deref_mut() {
        timing.record("execute.extract_signature", |_| extract_fn_context(&source, function_name))?
    } else {
        extract_fn_context(&source, function_name)?
    };
    let sig = &ctx.sig;
    let static_mut_names = extract_static_mut_items(&source);
    check_bin_only_compatibility(function_name, &ctx, false)?;

    if inputs.len() != sig.param_names.len() {
        return Err(ExecuteError::InstrumentError(format!(
            "expected {} inputs for {function_name}, got {}",
            sig.param_names.len(),
            inputs.len()
        )));
    }

    let instr_result = if let Some(timing) = timing.as_deref_mut() {
        timing.record("execute.instrument", |timing| {
            instrument::instrument_source_with_timing(&source, Some(function_name), Some(timing))
                .map_err(|e| ExecuteError::InstrumentError(e.to_string()))
        })?
    } else {
        instrument::instrument_source(&source, Some(function_name))
            .map_err(|e| ExecuteError::InstrumentError(e.to_string()))?
    };

    let runtime_path = find_runtime_crate_path()?;

    let mocks_json = serde_json::to_string(mocks).map_err(|e| {
        ExecuteError::InstrumentError(format!("cannot serialize mocks: {e}"))
    })?;

    let harness_source = if let Some(timing) = timing.as_deref_mut() {
        timing.record("execute.generate_harness", |_| {
            generate_harness(
                &instr_result.source,
                function_name,
                &sig.param_names,
                &sig.param_types,
                sig.return_type.as_deref(),
                &mocks_json,
                &static_mut_names,
                sig.is_async,
            )
        })?
    } else {
        generate_harness(
            &instr_result.source,
            function_name,
            &sig.param_names,
            &sig.param_types,
            sig.return_type.as_deref(),
            &mocks_json,
            &static_mut_names,
            sig.is_async,
        )?
    };

    let harness_dir = make_harness_dir();
    std::fs::create_dir_all(&harness_dir)?;

    let mut harness = if let Some(timing) = timing {
        timing.record("execute.build", |timing| {
            build_and_spawn_harness(&harness_source, &harness_dir, &runtime_path, sig.is_async, false, Some(timing))
        })?
    } else {
        build_and_spawn_harness(&harness_source, &harness_dir, &runtime_path, sig.is_async, false, None)?
    };

    // Execute the first call
    let result = harness.execute(inputs, timeout_ms)?;

    // Cache the harness unless it timed out (killed itself).
    let timed_out = result.thrown_error.as_ref()
        .and_then(|e| e.get("error_type"))
        .and_then(|v| v.as_str()) == Some("timeout");
    if !timed_out {
        cache.lock().unwrap().insert(key, harness);
    } else {
        // Harness was killed by timeout; clean up its directory.
        let _ = std::fs::remove_dir_all(&harness_dir);
    }

    Ok(result)
}

// ---------------------------------------------------------------------------
// Axum handler execution
// ---------------------------------------------------------------------------

/// Generate the main.rs harness for an Axum handler.
///
/// Instead of calling the handler directly with deserialized params, the
/// harness builds a synthetic `http::Request`, mounts the handler on a
/// minimal `axum::Router`, and calls it via `tower::ServiceExt::oneshot`.
/// The HTTP response (status, headers, body) is normalized into the
/// standard `ExecuteResult` JSON format.
///
/// Input format: `inputs[0]` is a JSON object with keys:
///   `method`, `path`, `query`, `body`, `headers`, `state`
fn generate_axum_harness(
    instrumented_source: &str,
    function_name: &str,
    mappings: &[crate::adapters::AxumExtractorMapping],
    mocks_json: &str,
) -> Result<String, ExecuteError> {
    let module_block = wrap_in_module(instrumented_source)?;
    let mut h = String::with_capacity(8192);

    h.push_str("#![allow(unused_imports)]\n");
    h.push_str("use serde_json::Value;\n");
    h.push_str("use axum::{Router, routing};\n");
    h.push_str("use tower::ServiceExt;\n");
    h.push_str("use http_body_util::BodyExt;\n\n");
    h.push_str(&module_block);
    h.push_str("\n\nfn main() {\n");
    h.push_str(&format!(
        "    shatter_rust_runtime::run_harness_loop(r#\"{}\"#, |inputs| {{\n",
        mocks_json
    ));

    // Parse the input object from inputs[0].
    h.push_str("        let input_obj = inputs.first().and_then(|v| v.as_object()).cloned().unwrap_or_default();\n\n");

    // Extract HTTP method (default based on whether a body extractor is present).
    let has_body_extractor = mappings.iter().any(|m| {
        matches!(
            m.kind,
            crate::adapters::AxumExtractorKind::JsonBody | crate::adapters::AxumExtractorKind::FormBody
        )
    });
    let default_method = if has_body_extractor { "POST" } else { "GET" };
    h.push_str(&format!(
        "        let method_str = input_obj.get(\"method\").and_then(|v| v.as_str()).unwrap_or(\"{default_method}\");\n"
    ));

    // Extract path.
    let has_path_extractor = mappings
        .iter()
        .any(|m| m.kind == crate::adapters::AxumExtractorKind::PathParams);
    let default_path = if has_path_extractor { "/test/:p0" } else { "/test" };
    h.push_str(&format!(
        "        let path_value = input_obj.get(\"path\").and_then(|v| v.as_str()).unwrap_or(\"{default_path}\");\n"
    ));

    // Extract query string.
    h.push_str("        let query_str = input_obj.get(\"query\").map(|v| {\n");
    h.push_str("            if let Some(s) = v.as_str() { s.to_string() }\n");
    h.push_str("            else if let Some(obj) = v.as_object() {\n");
    h.push_str("                obj.iter().map(|(k, v)| format!(\"{}={}\", k, v.as_str().unwrap_or(&v.to_string()))).collect::<Vec<_>>().join(\"&\")\n");
    h.push_str("            } else { String::new() }\n");
    h.push_str("        }).unwrap_or_default();\n\n");

    // Build the URI.
    h.push_str("        let uri = if query_str.is_empty() {\n");
    h.push_str("            path_value.to_string()\n");
    h.push_str("        } else {\n");
    h.push_str("            format!(\"{}?{}\", path_value, query_str)\n");
    h.push_str("        };\n\n");

    // Build the request.
    h.push_str("        let body_json = input_obj.get(\"body\").cloned().unwrap_or(Value::Null);\n");
    h.push_str("        let body_bytes = if body_json.is_null() {\n");
    h.push_str("            axum::body::Body::empty()\n");
    h.push_str("        } else {\n");
    h.push_str("            axum::body::Body::from(serde_json::to_vec(&body_json).unwrap_or_default())\n");
    h.push_str("        };\n\n");

    h.push_str("        let request = http::Request::builder()\n");
    h.push_str("            .method(method_str)\n");
    h.push_str("            .uri(&uri)\n");
    h.push_str("            .header(\"content-type\", \"application/json\")\n");

    // Add custom headers.
    h.push_str("            ;\n");
    h.push_str("        let request = if let Some(hdrs) = input_obj.get(\"headers\").and_then(|v| v.as_object()) {\n");
    h.push_str("            let mut req = request;\n");
    h.push_str("            for (k, v) in hdrs {\n");
    h.push_str("                if let Some(val) = v.as_str() {\n");
    h.push_str("                    req = req.header(k.as_str(), val);\n");
    h.push_str("                }\n");
    h.push_str("            }\n");
    h.push_str("            req\n");
    h.push_str("        } else { request };\n");
    h.push_str("        let request = request.body(body_bytes).unwrap();\n\n");

    // Build the router with the handler.
    // Determine the route method based on the HTTP method.
    let has_state = mappings
        .iter()
        .any(|m| m.kind == crate::adapters::AxumExtractorKind::AppState);

    // Build route pattern from path extractor presence.
    let route_pattern = if has_path_extractor {
        default_path
    } else {
        "/test"
    };

    h.push_str(&format!(
        "        let app = Router::new().route(\"{route_pattern}\", routing::any(user_code::{function_name}));\n"
    ));

    // If State<T> is needed, deserialize state and attach via .with_state().
    if has_state {
        h.push_str("        let state_value = input_obj.get(\"state\").cloned().unwrap_or(Value::Object(serde_json::Map::new()));\n");
        h.push_str("        let app = app.with_state(serde_json::from_value(state_value).unwrap_or_default());\n");
    }

    // Execute via oneshot inside a Tokio runtime.
    h.push_str("\n        let __tokio_rt = tokio::runtime::Runtime::new().unwrap();\n");
    h.push_str("        let (result, wall_time_ms) = shatter_rust_runtime::execute_with_timing(std::panic::AssertUnwindSafe(|| {\n");
    h.push_str("            __tokio_rt.block_on(async {\n");
    h.push_str("                let response = app.oneshot(request).await.unwrap();\n");
    h.push_str("                let status = response.status().as_u16();\n");
    h.push_str("                let headers: serde_json::Map<String, Value> = response.headers().iter()\n");
    h.push_str("                    .map(|(k, v)| (k.to_string(), Value::String(v.to_str().unwrap_or(\"\").to_string())))\n");
    h.push_str("                    .collect();\n");
    h.push_str("                let body_bytes = response.into_body().collect().await.map(|b| b.to_bytes()).unwrap_or_default();\n");
    h.push_str("                let body_str = String::from_utf8_lossy(&body_bytes).to_string();\n");
    h.push_str("                let body_value = serde_json::from_str::<Value>(&body_str).unwrap_or(Value::String(body_str));\n");
    h.push_str("                serde_json::json!({\n");
    h.push_str("                    \"status\": status,\n");
    h.push_str("                    \"headers\": headers,\n");
    h.push_str("                    \"body\": body_value\n");
    h.push_str("                })\n");
    h.push_str("            })\n");
    h.push_str("        }));\n\n");

    // Build result JSON (simplified — no console capture or static snapshot for now).
    h.push_str("        let return_value = match &result {\n");
    h.push_str("            Ok(v) => Some(v.clone()),\n");
    h.push_str("            Err(_) => None,\n");
    h.push_str("        };\n");
    h.push_str("        let thrown_error = match &result {\n");
    h.push_str("            Ok(_) => None,\n");
    h.push_str("            Err(e) => {\n");
    h.push_str("                let msg = if let Some(s) = e.downcast_ref::<String>() { s.clone() }\n");
    h.push_str("                    else if let Some(s) = e.downcast_ref::<&str>() { s.to_string() }\n");
    h.push_str("                    else { \"unknown panic\".to_string() };\n");
    h.push_str("                Some(serde_json::json!({ \"error_type\": \"runtime_error\", \"message\": msg, \"stack\": null }))\n");
    h.push_str("            }\n");
    h.push_str("        };\n\n");

    h.push_str("        shatter_rust_runtime::build_result_json(\n");
    h.push_str("            return_value,\n");
    h.push_str("            thrown_error,\n");
    h.push_str("            vec![],\n");
    h.push_str("            vec![],\n");
    h.push_str("            vec![],\n");
    h.push_str("            vec![],\n");
    h.push_str("            vec![],\n");
    h.push_str("            wall_time_ms,\n");
    h.push_str("        )\n");
    h.push_str("    });\n");
    h.push_str("}\n");

    Ok(h)
}

/// Execute an Axum handler function via adapter-owned path.
///
/// Generates an Axum-specific harness that builds a minimal `Router`,
/// sends a synthetic `http::Request` via `tower::ServiceExt::oneshot`,
/// and normalizes the HTTP response.
#[allow(clippy::too_many_arguments)]
pub fn execute_axum_handler(
    file_path: &str,
    function_name: &str,
    inputs: &[Value],
    mocks: &[Value],
    timeout_ms: u64,
    mappings: &[crate::adapters::AxumExtractorMapping],
    cache: &HarnessCache,
    _crate_cache: &CrateHarnessCache,
    _bridge_cache: &CrateBridgeHarnessCache,
) -> Result<ExecuteResult, ExecuteError> {
    let path = Path::new(file_path);
    if !path.exists() {
        return Err(ExecuteError::FileError(format!("file not found: {file_path}")));
    }

    // Compute cache key.
    let key = HarnessKey {
        file_path: file_path.to_string(),
        function_name: function_name.to_string(),
        mocks_hash: mocks_hash(mocks),
    };

    // Fast path: harness already compiled and running.
    {
        let mut map = cache.lock().unwrap();
        if let Some(harness) = map.get_mut(&key) {
            let result = harness.execute(inputs, timeout_ms)?;
            if result.thrown_error.as_ref().and_then(|e| e.get("error_type")).and_then(|v| v.as_str()) == Some("timeout") {
                map.remove(&key);
            }
            return Ok(result);
        }
    }

    // Slow path: compile and spawn.
    let source = std::fs::read_to_string(path)
        .map_err(|e| ExecuteError::FileError(format!("cannot read {file_path}: {e}")))?;

    let instr_result = instrument::instrument_source(&source, Some(function_name))
        .map_err(|e| ExecuteError::InstrumentError(e.to_string()))?;

    let runtime_path = find_runtime_crate_path()?;

    let mocks_json = serde_json::to_string(mocks).map_err(|e| {
        ExecuteError::InstrumentError(format!("cannot serialize mocks: {e}"))
    })?;

    let harness_source = generate_axum_harness(
        &instr_result.source,
        function_name,
        mappings,
        &mocks_json,
    )?;

    let harness_dir = make_harness_dir();
    std::fs::create_dir_all(&harness_dir)?;

    let mut harness = build_and_spawn_harness(
        &harness_source,
        &harness_dir,
        &runtime_path,
        true,  // needs_tokio (always for axum)
        true,  // needs_axum
        None,
    )?;

    let result = harness.execute(inputs, timeout_ms)?;

    let timed_out = result.thrown_error.as_ref()
        .and_then(|e| e.get("error_type"))
        .and_then(|v| v.as_str()) == Some("timeout");
    if !timed_out {
        cache.lock().unwrap().insert(key, harness);
    } else {
        let _ = std::fs::remove_dir_all(&harness_dir);
    }

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
    fn extract_fn_context_simple() {
        let source = "fn classify_number(n: i32) -> &'static str { \"\" }";
        let ctx = extract_fn_context(source, "classify_number").unwrap();
        assert_eq!(ctx.sig.param_names, vec!["n"]);
        assert_eq!(ctx.sig.param_types, vec!["i32"]);
        assert!(ctx.sig.return_type.is_some());
        assert!(!ctx.sig.has_generics);
    }

    #[test]
    fn extract_fn_context_multiple_params() {
        let source = "fn add(a: i32, b: i32) -> i32 { a + b }";
        let ctx = extract_fn_context(source, "add").unwrap();
        assert_eq!(ctx.sig.param_names, vec!["a", "b"]);
        assert_eq!(ctx.sig.param_types, vec!["i32", "i32"]);
        assert_eq!(ctx.sig.return_type.as_deref(), Some("i32"));
    }

    #[test]
    fn extract_fn_context_no_return() {
        let source = "fn noop() {}";
        let ctx = extract_fn_context(source, "noop").unwrap();
        assert!(ctx.sig.param_names.is_empty());
        assert!(ctx.sig.param_types.is_empty());
        assert!(ctx.sig.return_type.is_none());
    }

    #[test]
    fn extract_fn_context_not_found() {
        let source = "fn other() {}";
        let result = extract_fn_context(source, "missing");
        assert!(result.is_err());
    }

    #[test]
    fn extract_fn_context_detects_generics() {
        let source = "fn identity<T: Clone>(x: T) -> T { x }";
        let ctx = extract_fn_context(source, "identity").unwrap();
        assert!(ctx.sig.has_generics);
        assert_eq!(ctx.sig.generic_names, vec!["T"]);
    }

    #[test]
    fn extract_fn_context_collects_local_types() {
        let source = "struct Point { x: f64, y: f64 }\nenum Color { Red, Blue }\nfn origin() -> Point { Point { x: 0.0, y: 0.0 } }";
        let ctx = extract_fn_context(source, "origin").unwrap();
        assert!(ctx.local_type_names.contains("Point"));
        assert!(ctx.local_type_names.contains("Color"));
    }

    #[test]
    fn extract_fn_context_detects_module_path_uses() {
        let source = "use crate::config::Config;\nfn init(c: Config) {}";
        let ctx = extract_fn_context(source, "init").unwrap();
        assert!(ctx.has_module_path_uses);
    }

    #[test]
    fn generate_cargo_toml_includes_runtime_dep() {
        let toml = generate_cargo_toml(Path::new("/home/user/shatter-rust-runtime"), false, false);
        assert!(toml.contains("[workspace]"));
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
            "[]",
            &[],
            false,
        )
        .unwrap();
        assert!(harness.contains("mod user_code"));
        assert!(harness.contains("user_code::classify_number(n)"));
        assert!(harness.contains("run_harness_loop"));
        assert!(harness.contains("execute_with_timing"));
        assert!(harness.contains("build_result_json"));
    }

    #[test]
    fn generate_harness_void_function() {
        let harness = generate_harness("fn noop() {}", "noop", &[], &[], None, "[]", &[], false).unwrap();
        assert!(harness.contains("user_code::noop()"));
        assert!(harness.contains("Ok(())"));
        assert!(harness.contains("run_harness_loop"));
    }

    // ── Async harness generation tests ──

    #[test]
    fn generate_harness_async_wraps_in_tokio_runtime() {
        let source = "async fn fetch(url: String) -> String { url }";
        let harness = generate_harness(
            source,
            "fetch",
            &["url".to_string()],
            &["String".to_string()],
            Some("String"),
            "[]",
            &[],
            true,
        )
        .unwrap();
        assert!(
            harness.contains("tokio::runtime::Runtime::new()"),
            "async harness must create Tokio runtime\n\nharness:\n{harness}"
        );
        assert!(
            harness.contains("block_on(user_code::fetch(url))"),
            "async harness must block_on the async call\n\nharness:\n{harness}"
        );
    }

    #[test]
    fn generate_harness_sync_no_tokio() {
        let source = "fn add(a: i32, b: i32) -> i32 { a + b }";
        let harness = generate_harness(
            source,
            "add",
            &["a".to_string(), "b".to_string()],
            &["i32".to_string(), "i32".to_string()],
            Some("i32"),
            "[]",
            &[],
            false,
        )
        .unwrap();
        assert!(
            !harness.contains("tokio"),
            "sync harness must not reference tokio\n\nharness:\n{harness}"
        );
    }

    #[test]
    fn generate_cargo_toml_with_tokio() {
        let toml = generate_cargo_toml(Path::new("/fake/runtime"), true, false);
        assert!(
            toml.contains("tokio"),
            "needs_tokio=true must include tokio dep\n\ntoml:\n{toml}"
        );
    }

    #[test]
    fn generate_cargo_toml_without_tokio() {
        let toml = generate_cargo_toml(Path::new("/fake/runtime"), false, false);
        assert!(
            !toml.contains("tokio"),
            "needs_tokio=false must not include tokio dep\n\ntoml:\n{toml}"
        );
    }

    // ── Axum Cargo.toml generation ──

    #[test]
    fn generate_cargo_toml_with_axum() {
        let toml = generate_cargo_toml(Path::new("/fake/runtime"), false, true);
        assert!(
            toml.contains("axum"),
            "needs_axum=true must include axum dep\n\ntoml:\n{toml}"
        );
        assert!(
            toml.contains("tower"),
            "needs_axum=true must include tower dep\n\ntoml:\n{toml}"
        );
        assert!(
            toml.contains("http-body-util"),
            "needs_axum=true must include http-body-util dep\n\ntoml:\n{toml}"
        );
        // Axum implies tokio
        assert!(
            toml.contains("tokio"),
            "needs_axum=true must imply tokio dep\n\ntoml:\n{toml}"
        );
    }

    #[test]
    fn generate_cargo_toml_without_axum() {
        let toml = generate_cargo_toml(Path::new("/fake/runtime"), true, false);
        assert!(
            !toml.contains("axum"),
            "needs_axum=false must not include axum dep\n\ntoml:\n{toml}"
        );
        assert!(
            !toml.contains("tower"),
            "needs_axum=false must not include tower dep\n\ntoml:\n{toml}"
        );
    }

    // ── Axum harness generation ──

    #[test]
    fn generate_axum_harness_contains_router() {
        use crate::adapters::{AxumExtractorKind, AxumExtractorMapping};

        let source = "use axum::extract::Json;\nasync fn create_user(Json(body): Json<String>) -> String { body }";
        let mappings = vec![AxumExtractorMapping {
            param_index: 0,
            kind: AxumExtractorKind::JsonBody,
            type_name: "Json".to_string(),
        }];
        let harness = generate_axum_harness(source, "create_user", &mappings, "[]").unwrap();
        assert!(
            harness.contains("Router::new()"),
            "axum harness must create Router\n\nharness:\n{harness}"
        );
        assert!(
            harness.contains("routing::any(user_code::create_user)"),
            "axum harness must mount handler\n\nharness:\n{harness}"
        );
        assert!(
            harness.contains("oneshot"),
            "axum harness must use oneshot\n\nharness:\n{harness}"
        );
    }

    #[test]
    fn generate_axum_harness_with_path_extractor_uses_path_route() {
        use crate::adapters::{AxumExtractorKind, AxumExtractorMapping};

        let source = "use axum::extract::Path;\nasync fn get_user(Path(id): Path<u64>) -> String { id.to_string() }";
        let mappings = vec![AxumExtractorMapping {
            param_index: 0,
            kind: AxumExtractorKind::PathParams,
            type_name: "Path".to_string(),
        }];
        let harness = generate_axum_harness(source, "get_user", &mappings, "[]").unwrap();
        assert!(
            harness.contains("/:p0"),
            "axum harness with Path extractor must use path template\n\nharness:\n{harness}"
        );
    }

    #[test]
    fn generate_axum_harness_json_body_defaults_to_post() {
        use crate::adapters::{AxumExtractorKind, AxumExtractorMapping};

        let source = "use axum::extract::Json;\nasync fn handler(Json(b): Json<String>) -> String { b }";
        let mappings = vec![AxumExtractorMapping {
            param_index: 0,
            kind: AxumExtractorKind::JsonBody,
            type_name: "Json".to_string(),
        }];
        let harness = generate_axum_harness(source, "handler", &mappings, "[]").unwrap();
        assert!(
            harness.contains("\"POST\""),
            "harness with Json body must default to POST\n\nharness:\n{harness}"
        );
    }

    #[test]
    fn generate_axum_harness_no_body_defaults_to_get() {
        use crate::adapters::{AxumExtractorKind, AxumExtractorMapping};

        let source = "use axum::extract::Query;\nasync fn handler(Query(q): Query<String>) -> String { q }";
        let mappings = vec![AxumExtractorMapping {
            param_index: 0,
            kind: AxumExtractorKind::QueryParams,
            type_name: "Query".to_string(),
        }];
        let harness = generate_axum_harness(source, "handler", &mappings, "[]").unwrap();
        assert!(
            harness.contains("\"GET\""),
            "harness without body extractor must default to GET\n\nharness:\n{harness}"
        );
    }

    #[test]
    fn generate_axum_harness_with_state_uses_with_state() {
        use crate::adapters::{AxumExtractorKind, AxumExtractorMapping};

        let source = "use axum::extract::State;\nasync fn handler(State(s): State<String>) -> String { s }";
        let mappings = vec![AxumExtractorMapping {
            param_index: 0,
            kind: AxumExtractorKind::AppState,
            type_name: "State".to_string(),
        }];
        let harness = generate_axum_harness(source, "handler", &mappings, "[]").unwrap();
        assert!(
            harness.contains("with_state"),
            "harness with State extractor must call with_state\n\nharness:\n{harness}"
        );
    }

    #[test]
    fn generate_axum_harness_captures_http_response() {
        use crate::adapters::{AxumExtractorKind, AxumExtractorMapping};

        let source = "use axum::extract::Json;\nasync fn handler(Json(b): Json<String>) -> String { b }";
        let mappings = vec![AxumExtractorMapping {
            param_index: 0,
            kind: AxumExtractorKind::JsonBody,
            type_name: "Json".to_string(),
        }];
        let harness = generate_axum_harness(source, "handler", &mappings, "[]").unwrap();
        assert!(
            harness.contains("status"),
            "harness must capture HTTP status\n\nharness:\n{harness}"
        );
        assert!(
            harness.contains("headers"),
            "harness must capture HTTP headers\n\nharness:\n{harness}"
        );
        assert!(
            harness.contains("body"),
            "harness must capture HTTP body\n\nharness:\n{harness}"
        );
    }

    #[test]
    fn extract_fn_context_detects_async() {
        let source = "async fn fetch() -> String { String::new() }";
        let ctx = extract_fn_context(source, "fetch").unwrap();
        assert!(ctx.sig.is_async, "async fn must set is_async=true");
    }

    #[test]
    fn extract_fn_context_sync_not_async() {
        let source = "fn add(a: i32, b: i32) -> i32 { a + b }";
        let ctx = extract_fn_context(source, "add").unwrap();
        assert!(!ctx.sig.is_async, "sync fn must set is_async=false");
    }

    #[test]
    fn extract_all_fn_contexts_detects_mixed_async() {
        let source = "fn sync_fn() {} async fn async_fn() -> i32 { 42 }";
        let ctxs = extract_all_fn_contexts(source);
        let sync_ctx = ctxs.iter().find(|(n, _)| n == "sync_fn").unwrap();
        let async_ctx = ctxs.iter().find(|(n, _)| n == "async_fn").unwrap();
        assert!(!sync_ctx.1.sig.is_async);
        assert!(async_ctx.1.sig.is_async);
    }

    /// Regression test for str-dcln: Result-returning functions must use
    /// `Ok(ref v)` to avoid partial move of the `result` variable.
    #[test]
    fn generate_harness_result_returning_uses_ref_binding() {
        let source = r#"fn safe_divide(a: f64, b: f64) -> Result<f64, String> {
            if b == 0.0 { Err("division by zero".to_string()) } else { Ok(a / b) }
        }"#;
        let harness = generate_harness(
            source,
            "safe_divide",
            &["a".to_string(), "b".to_string()],
            &["f64".to_string(), "f64".to_string()],
            Some("Result < f64 , String >"),
            "[]",
            &[],
            false,
        )
        .unwrap();
        assert!(
            harness.contains("Ok(ref v)"),
            "harness for Result-returning fn must use Ok(ref v) to avoid partial move\n\nharness:\n{harness}"
        );
        assert!(
            harness.contains("Err(ref msg)"),
            "harness must use Err(ref msg)\n\nharness:\n{harness}"
        );
    }

    /// Regression test for str-dcln: dispatch harness must use `Ok(ref v)`
    /// and `Err(ref msg)` to avoid partial move of the `result` variable.
    #[test]
    fn generate_dispatch_harness_result_returning_uses_ref_binding() {
        let source = r#"fn safe_divide(a: f64, b: f64) -> Result<f64, String> {
            if b == 0.0 { Err("division by zero".to_string()) } else { Ok(a / b) }
        }"#;
        let fns = vec![CompatFn {
            name: "safe_divide".to_string(),
            param_names: vec!["a".to_string(), "b".to_string()],
            param_types: vec!["f64".to_string(), "f64".to_string()],
            return_type: Some("Result < f64 , String >".to_string()),
            is_async: false,
        }];
        let harness = generate_dispatch_harness(source, &fns, "[]", &[]).unwrap();
        assert!(
            harness.contains("Ok(ref v)"),
            "dispatch harness for Result-returning fn must use Ok(ref v)\n\nharness:\n{harness}"
        );
        assert!(
            harness.contains("Err(ref msg)"),
            "dispatch harness must use Err(ref msg)\n\nharness:\n{harness}"
        );
    }

    /// Regression test for str-dcln: crate-bridge wrapper must use `Ok(ref ret_val)`
    /// to avoid partial move — `result` is reused after the match for side-effect capture.
    #[test]
    fn generate_crate_bridge_wrapper_result_returning_uses_ref_binding() {
        let fns = vec![CompatFn {
            name: "safe_divide".to_string(),
            param_names: vec!["a".to_string(), "b".to_string()],
            param_types: vec!["f64".to_string(), "f64".to_string()],
            return_type: Some("Result < f64 , String >".to_string()),
            is_async: false,
        }];
        let wrapper = generate_crate_bridge_wrapper(&fns, "[]", &[]);
        assert!(
            wrapper.contains("Ok(ref ret_val)"),
            "crate-bridge wrapper must use Ok(ref ret_val)\n\nwrapper:\n{wrapper}"
        );
        assert!(
            wrapper.contains("Err(ref panic_info)"),
            "crate-bridge wrapper must use Err(ref panic_info)\n\nwrapper:\n{wrapper}"
        );
    }

    /// Reproduction test for str-cfhk: source with `fn main()` must not
    /// produce a harness with two top-level `main` definitions.
    #[test]
    fn generate_harness_no_duplicate_main() {
        let source = r#"
fn classify_number(n: i32) -> &'static str {
    if n < 0 { "negative" } else { "non-negative" }
}

fn main() {
    println!("{}", classify_number(42));
}
"#;
        let harness = generate_harness(
            source,
            "classify_number",
            &["n".to_string()],
            &["i32".to_string()],
            Some("& 'static str"),
            "[]",
            &[],
            false,
        )
        .unwrap();

        // The user's main() should be inside mod user_code, not at top level
        assert!(harness.contains("mod user_code"));
        assert!(harness.contains("user_code::classify_number(n)"));

        // Count top-level `fn main()` — should be exactly 1 (the harness's)
        let top_level_mains = harness
            .lines()
            .filter(|line| {
                let trimmed = line.trim();
                // Top-level main: starts at column 0 (not indented inside mod)
                !line.starts_with(' ')
                    && !line.starts_with('\t')
                    && (trimmed == "fn main() {" || trimmed.starts_with("fn main()"))
            })
            .count();
        assert_eq!(
            top_level_mains, 1,
            "expected exactly 1 top-level fn main(), found {top_level_mains}\n\nharness:\n{harness}"
        );
        assert!(harness.contains("loop"));
    }

    #[test]
    fn owned_type_for_ref_maps_str_refs() {
        let check = |ty: &str, expected_deser: &str| {
            let m = owned_type_for_ref(ty).unwrap_or_else(|| panic!("expected Some for {ty}"));
            assert_eq!(m.deser_type, expected_deser, "deser_type mismatch for {ty}");
        };
        check("& str", "String");
        check("&str", "String");
        check("& 'static str", "String");
        check("&String", "String");
        check("& 'static String", "String");
        check("& [& str]", "Vec<String>");
        check("&[&str]", "Vec<String>");
        assert!(owned_type_for_ref("i32").is_none());
        assert!(owned_type_for_ref("String").is_none());
    }

    /// Reproduction test for str-jxap: `&str` parameters must deserialize
    /// to `String` then borrow, because `serde_json::from_value::<&str>()`
    /// fails (requires borrowing from the deserializer, not from a `Value`).
    #[test]
    fn generate_harness_str_ref_param_deserializes_to_owned() {
        let source = r#"fn greet(name: &str) -> String { format!("Hello, {name}!") }"#;
        let harness = generate_harness(
            source,
            "greet",
            &["name".to_string()],
            &["& str".to_string()],
            Some("String"),
            "[]",
            &[],
            false,
        )
        .unwrap();

        // Should deserialize to String (owned), not &str
        assert!(
            harness.contains("name_owned: String = serde_json::from_value"),
            "expected owned String deserialization\n\nharness:\n{harness}"
        );
        // Should pass &name_owned to the function
        assert!(
            harness.contains("user_code::greet(&name_owned)"),
            "expected &name_owned in function call\n\nharness:\n{harness}"
        );
        // Should NOT try to deserialize directly to &str
        assert!(
            !harness.contains("name: & str"),
            "should not deserialize to &str\n\nharness:\n{harness}"
        );
    }

    #[test]
    fn execute_nonexistent_file_returns_error() {
        let cache = std::sync::Mutex::new(std::collections::HashMap::new());
        let crate_cache = std::sync::Mutex::new(std::collections::HashMap::new());
        let bridge_cache = std::sync::Mutex::new(std::collections::HashMap::new());
        let result = execute_function("/nonexistent/file.rs", "f", &[], &[], 5000, None, &cache, &crate_cache, &bridge_cache);
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

        let cache = std::sync::Mutex::new(std::collections::HashMap::new());
        let crate_cache = std::sync::Mutex::new(std::collections::HashMap::new());
        let bridge_cache = std::sync::Mutex::new(std::collections::HashMap::new());
        let result = execute_function(
            &file.to_string_lossy(),
            "add",
            &[serde_json::json!(1)], // only 1 input, needs 2
            &[],
            5000,
            None,
            &cache,
            &crate_cache,
            &bridge_cache,
        );
        assert!(result.is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn extract_root_type_name_strips_wrappers() {
        assert_eq!(extract_root_type_name("i32"), "i32");
        assert_eq!(extract_root_type_name("&str"), "str");
        assert_eq!(extract_root_type_name("&mut String"), "String");
        assert_eq!(extract_root_type_name("Vec<i32>"), "Vec");
        assert_eq!(extract_root_type_name("HashMap<String, i32>"), "HashMap");
        assert_eq!(extract_root_type_name("Box<dyn Foo>"), "Box");
        assert_eq!(extract_root_type_name("& 'static str"), "str");
    }

    #[test]
    fn is_trait_object_detects_dyn_ref() {
        assert!(is_trait_object_type("& dyn DataStore"));
        assert!(is_trait_object_type("&dyn DataStore"));
        assert!(is_trait_object_type("&mut dyn Write"));
        assert!(is_trait_object_type("Box<dyn Handler>"));
        assert!(is_trait_object_type("&(dyn Debug + Send)"));
    }

    #[test]
    fn is_trait_object_rejects_non_dyn() {
        assert!(!is_trait_object_type("i32"));
        assert!(!is_trait_object_type("String"));
        assert!(!is_trait_object_type("&str"));
        assert!(!is_trait_object_type("Vec<i32>"));
        assert!(!is_trait_object_type("MyStruct"));
    }

    #[test]
    fn execute_trait_object_param_returns_non_executable() {
        let dir = std::env::temp_dir().join("shatter-test-exec-dyn");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("test.rs");
        std::fs::write(
            &file,
            "trait DataStore { fn get(&self) -> i32; }\nfn query(store: &dyn DataStore) -> i32 { store.get() }",
        )
        .unwrap();

        let cache = std::sync::Mutex::new(std::collections::HashMap::new());
        let crate_cache = std::sync::Mutex::new(std::collections::HashMap::new());
        let bridge_cache = std::sync::Mutex::new(std::collections::HashMap::new());
        let result = execute_function(
            &file.to_string_lossy(),
            "query",
            &[serde_json::json!(null)],
            &[],
            5000,
            None,
            &cache,
            &crate_cache,
            &bridge_cache,
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, ExecuteError::NonExecutable(_)),
            "expected NonExecutable, got: {err:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ─── bin_only compatibility check tests ───────────────────────────────────

    #[test]
    fn compat_generic_params_detected() {
        let source = "fn identity<T: Clone>(x: T) -> T { x }";
        let ctx = extract_fn_context(source, "identity").unwrap();
        let err = check_bin_only_compatibility("identity", &ctx, false).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("generic type parameters [T]"),
            "expected generic params message, got: {msg}"
        );
        assert!(msg.contains("crate_bridge"), "should suggest crate_bridge: {msg}");
    }

    #[test]
    fn compat_trait_object_detected() {
        let source = "trait Db { fn get(&self) -> i32; }\nfn query(db: &dyn Db) -> i32 { db.get() }";
        let ctx = extract_fn_context(source, "query").unwrap();
        let err = check_bin_only_compatibility("query", &ctx, false).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("trait object"),
            "expected trait object message, got: {msg}"
        );
        assert!(msg.contains("crate_bridge"), "should suggest crate_bridge: {msg}");
    }

    #[test]
    fn compat_external_type_detected() {
        let source = "fn process(conn: PgConnection) -> bool { true }";
        let ctx = extract_fn_context(source, "process").unwrap();
        let err = check_bin_only_compatibility("process", &ctx, false).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("external type") && msg.contains("PgConnection"),
            "expected external type message, got: {msg}"
        );
        assert!(msg.contains("crate_bridge"), "should suggest crate_bridge: {msg}");
    }

    #[test]
    fn compat_module_path_import_detected() {
        let source = "use crate::config::Config;\nfn init(c: Config) {}";
        let ctx = extract_fn_context(source, "init").unwrap();
        let err = check_bin_only_compatibility("init", &ctx, false).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("module path") && msg.contains("Config"),
            "expected module path message, got: {msg}"
        );
    }

    #[test]
    fn compat_multiple_issues_listed() {
        let source = "fn dispatch<T>(db: &dyn std::any::Any, val: T) {}";
        let ctx = extract_fn_context(source, "dispatch").unwrap();
        let err = check_bin_only_compatibility("dispatch", &ctx, false).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("generic type parameters"), "should list generics: {msg}");
        assert!(msg.contains("trait object"), "should list trait object: {msg}");
    }

    #[test]
    fn compat_primitives_pass() {
        let source = "fn add(a: i32, b: i32) -> i32 { a + b }";
        let ctx = extract_fn_context(source, "add").unwrap();
        assert!(check_bin_only_compatibility("add", &ctx, false).is_ok());
    }

    #[test]
    fn compat_std_types_pass() {
        let source = "use std::collections::HashMap;\nfn f(v: Vec<String>, m: HashMap<String, i32>) -> Option<bool> { None }";
        let ctx = extract_fn_context(source, "f").unwrap();
        assert!(check_bin_only_compatibility("f", &ctx, false).is_ok());
    }

    #[test]
    fn compat_local_struct_passes() {
        let source = "struct Point { x: f64, y: f64 }\nfn origin() -> Point { Point { x: 0.0, y: 0.0 } }";
        let ctx = extract_fn_context(source, "origin").unwrap();
        assert!(check_bin_only_compatibility("origin", &ctx, false).is_ok());
    }

    #[test]
    fn compat_local_struct_param_passes() {
        let source = "struct Config { debug: bool }\nfn setup(c: Config) -> bool { c.debug }";
        let ctx = extract_fn_context(source, "setup").unwrap();
        assert!(check_bin_only_compatibility("setup", &ctx, false).is_ok());
    }

    #[test]
    fn compat_ref_params_pass() {
        let source = "fn greet(name: &str) -> String { format!(\"hi {name}\") }";
        let ctx = extract_fn_context(source, "greet").unwrap();
        assert!(check_bin_only_compatibility("greet", &ctx, false).is_ok());
    }

    #[test]
    fn execute_generic_fn_returns_non_executable() {
        let dir = std::env::temp_dir().join("shatter-test-exec-generic");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("test.rs");
        std::fs::write(&file, "fn identity<T: Clone>(x: T) -> T { x.clone() }").unwrap();

        let cache = std::sync::Mutex::new(std::collections::HashMap::new());
        let crate_cache = std::sync::Mutex::new(std::collections::HashMap::new());
        let bridge_cache = std::sync::Mutex::new(std::collections::HashMap::new());
        let result = execute_function(
            &file.to_string_lossy(),
            "identity",
            &[serde_json::json!(42)],
            &[],
            5000,
            None,
            &cache,
            &crate_cache,
            &bridge_cache,
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, ExecuteError::NonExecutable(_)),
            "expected NonExecutable, got: {err:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn execute_external_type_returns_non_executable() {
        let dir = std::env::temp_dir().join("shatter-test-exec-exttype");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("test.rs");
        std::fs::write(&file, "fn process(conn: PgConnection) -> bool { true }").unwrap();

        let cache = std::sync::Mutex::new(std::collections::HashMap::new());
        let crate_cache = std::sync::Mutex::new(std::collections::HashMap::new());
        let bridge_cache = std::sync::Mutex::new(std::collections::HashMap::new());
        let result = execute_function(
            &file.to_string_lossy(),
            "process",
            &[serde_json::json!(null)],
            &[],
            5000,
            None,
            &cache,
            &crate_cache,
            &bridge_cache,
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, ExecuteError::NonExecutable(_)),
            "expected NonExecutable, got: {err:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ─── end compatibility check tests ──────────────────────────────────────────

    /// Bug: `&[&str]` parameter causes compilation error because
    /// `owned_type_for_ref` doesn't handle reference slices.
    /// The harness must deserialize to `Vec<String>` and convert.
    #[test]
    fn generate_harness_slice_ref_param_deserializes_to_vec() {
        let source =
            r#"fn negotiate(header: &str, supported: &[&str]) -> String { String::new() }"#;
        let harness = generate_harness(
            source,
            "negotiate",
            &["header".to_string(), "supported".to_string()],
            &["& str".to_string(), "& [& str]".to_string()],
            Some("String"),
            "[]",
            &[],
            false,
        )
        .unwrap();

        // Should NOT contain `& [& str]` in deserialization (not DeserializeOwned)
        assert!(
            !harness.contains("supported: & [& str]"),
            "should not deserialize to &[&str]\n\nharness:\n{harness}"
        );
        // Should deserialize to Vec<String> (owned)
        assert!(
            harness.contains("Vec<String>"),
            "expected Vec<String> deserialization\n\nharness:\n{harness}"
        );
    }

    /// Bug: user-defined return types without `Serialize` cause compilation
    /// error when the harness tries `serde_json::to_value(&ret_val)`.
    /// `wrap_in_module` must inject `#[derive(serde::Serialize)]` on user
    /// structs and enums so the harness can serialize return values.
    #[test]
    fn wrap_in_module_injects_serialize_derive() {
        let source = r#"
#[derive(Debug)]
struct MyResult { value: i32 }
fn compute(n: i32) -> MyResult { MyResult { value: n } }
"#;
        let wrapped = wrap_in_module(source).unwrap();

        // The wrapped module must add serde::Serialize to struct derives
        assert!(
            wrapped.contains("serde :: Serialize") || wrapped.contains("serde::Serialize"),
            "wrap_in_module must inject Serialize derive on structs\n\nwrapped:\n{wrapped}"
        );
    }

    // -------------------------------------------------------------------------
    // extract_static_mut_items tests
    // -------------------------------------------------------------------------

    #[test]
    fn extract_static_mut_items_finds_mutable_statics() {
        let source = r#"
static mut COUNTER: i32 = 0;
static mut TOTAL: f64 = 0.0;
fn increment() { unsafe { COUNTER += 1; } }
"#;
        let names = extract_static_mut_items(source);
        assert!(names.contains(&"COUNTER".to_string()), "expected COUNTER in {names:?}");
        assert!(names.contains(&"TOTAL".to_string()), "expected TOTAL in {names:?}");
    }

    #[test]
    fn extract_static_mut_items_ignores_immutable_statics() {
        let source = r#"
static MAX: i32 = 100;
static NAME: &str = "shatter";
fn check(x: i32) -> bool { x < MAX }
"#;
        let names = extract_static_mut_items(source);
        assert!(
            names.is_empty(),
            "immutable statics should not be returned, got: {names:?}"
        );
    }

    #[test]
    fn extract_static_mut_items_empty_on_parse_error() {
        let names = extract_static_mut_items("this is not valid rust ~~~");
        assert!(names.is_empty(), "parse error should yield empty list");
    }

    #[test]
    fn extract_static_mut_items_empty_when_no_statics() {
        let source = "fn add(a: i32, b: i32) -> i32 { a + b }";
        let names = extract_static_mut_items(source);
        assert!(names.is_empty());
    }

    // -------------------------------------------------------------------------
    // generate_harness global-state tests
    // -------------------------------------------------------------------------

    #[test]
    fn generate_harness_with_static_mut_includes_snapshot_code() {
        let source = r#"
static mut COUNTER: i32 = 0;
fn increment() -> i32 { unsafe { COUNTER += 1; COUNTER } }
"#;
        let harness = generate_harness(
            source,
            "increment",
            &[],
            &[],
            Some("i32"),
            "[]",
            &["COUNTER".to_string()],
            false,
        )
        .unwrap();

        // Before-snapshot code
        assert!(
            harness.contains("__before_COUNTER"),
            "harness must snapshot COUNTER before execution\n\nharness:\n{harness}"
        );
        // After-snapshot code
        assert!(
            harness.contains("__after_COUNTER"),
            "harness must snapshot COUNTER after execution\n\nharness:\n{harness}"
        );
        // global_state_change side effect emission
        assert!(
            harness.contains("global_state_change"),
            "harness must emit global_state_change\n\nharness:\n{harness}"
        );
        assert!(
            harness.contains("\"variable\":\"COUNTER\"") || harness.contains("\"variable\" : \"COUNTER\""),
            "harness must name the variable COUNTER\n\nharness:\n{harness}"
        );
    }

    #[test]
    fn generate_harness_no_static_mut_emits_no_snapshot_code() {
        let source = "fn add(a: i32, b: i32) -> i32 { a + b }";
        let harness = generate_harness(
            source,
            "add",
            &["a".to_string(), "b".to_string()],
            &["i32".to_string(), "i32".to_string()],
            Some("i32"),
            "[]",
            &[],
            false,
        )
        .unwrap();

        // No snapshot variables should appear
        assert!(
            !harness.contains("__before_"),
            "no before-snapshots when no mutable statics\n\nharness:\n{harness}"
        );
        assert!(
            !harness.contains("__after_"),
            "no after-snapshots when no mutable statics\n\nharness:\n{harness}"
        );
        // build_result_json call must still be present (handles side_effects)
        assert!(
            harness.contains("build_result_json"),
            "build_result_json must be called\n\nharness:\n{harness}"
        );
    }

    #[test]
    fn generate_harness_contains_console_capture() {
        let source = "fn add(a: i32, b: i32) -> i32 { a + b }";
        let harness = generate_harness(
            source,
            "add",
            &["a".to_string(), "b".to_string()],
            &["i32".to_string(), "i32".to_string()],
            Some("i32"),
            "[]",
            &[],
            false,
        )
        .unwrap();

        assert!(
            harness.contains("libc::dup(1)"),
            "harness must save original stdout fd\n\nharness:\n{harness}"
        );
        assert!(
            harness.contains("libc::dup(2)"),
            "harness must save original stderr fd\n\nharness:\n{harness}"
        );
        assert!(
            harness.contains("libc::dup2"),
            "harness must redirect fds for capture\n\nharness:\n{harness}"
        );
        assert!(
            harness.contains("console_output"),
            "harness must emit console_output side effects\n\nharness:\n{harness}"
        );
    }

    #[test]
    fn generate_harness_contains_thrown_error_side_effect() {
        let source = "fn add(a: i32, b: i32) -> i32 { a + b }";
        let harness = generate_harness(
            source,
            "add",
            &["a".to_string(), "b".to_string()],
            &["i32".to_string(), "i32".to_string()],
            Some("i32"),
            "[]",
            &[],
            false,
        )
        .unwrap();

        // The harness should emit thrown_error as a side effect on panic.
        assert!(
            harness.contains(r#""kind": "thrown_error""#),
            "harness must emit thrown_error side effect on panic\n\nharness:\n{harness}"
        );
    }

    #[test]
    fn generate_harness_includes_libc_dependency() {
        let runtime_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("runtime");
        let toml = generate_cargo_toml(&runtime_path, false, false);
        assert!(
            toml.contains("libc"),
            "generated Cargo.toml must include libc dependency\n\ntoml:\n{toml}"
        );
    }

    // --- Standalone harness fallback tests ---

    #[test]
    fn standalone_target_dir_uses_cache_env() {
        // When SHATTER_HARNESS_CACHE is set, standalone_target_dir returns a path inside it.
        let _lock = crate::ENV_LOCK.lock().unwrap();
        let cache_root = std::env::temp_dir().join("shatter-test-cache-root");
        let cache_str = cache_root.to_string_lossy().into_owned();
        unsafe { std::env::set_var("SHATTER_HARNESS_CACHE", &cache_str) };

        let target = standalone_target_dir();
        // Restore before assertions so the var is cleared even on panic.
        unsafe { std::env::set_var("SHATTER_HARNESS_CACHE", "") };

        assert!(target.is_some(), "expected Some when SHATTER_HARNESS_CACHE is set");
        let target = target.unwrap();
        assert!(target.is_absolute(), "standalone_target_dir must be absolute, got {target:?}");
        assert!(
            target.starts_with(&cache_root),
            "target dir {target:?} should be under cache root {cache_str}"
        );
        assert!(
            target.ends_with("rust/standalone/target"),
            "target dir should end with rust/standalone/target, got {target:?}"
        );
    }

    #[test]
    fn standalone_target_dir_none_when_unset() {
        let _lock = crate::ENV_LOCK.lock().unwrap();
        unsafe { std::env::set_var("SHATTER_HARNESS_CACHE", "") };
        assert!(standalone_target_dir().is_none());
    }

    #[test]
    fn harness_cache_root_is_absolute_when_env_is_relative() {
        // Reproduce str-kdzq: when SHATTER_HARNESS_CACHE is a relative path,
        // harness_cache_root() must still return an absolute path so that
        // CARGO_TARGET_DIR resolves correctly when cargo runs in a subdirectory.
        let _lock = crate::ENV_LOCK.lock().unwrap();
        unsafe { std::env::set_var("SHATTER_HARNESS_CACHE", "relative/cache") };
        let root = harness_cache_root();
        unsafe { std::env::set_var("SHATTER_HARNESS_CACHE", "") };

        let root = root.expect("should return Some for non-empty env var");
        assert!(
            root.is_absolute(),
            "harness_cache_root must be absolute even with relative env var, got: {root:?}"
        );
    }

    #[test]
    fn standalone_target_dir_is_absolute_with_relative_cache() {
        // Reproduce str-kdzq: relative SHATTER_HARNESS_CACHE caused "compiled binary not found"
        // because CARGO_TARGET_DIR resolved relative to harness_dir (cargo's CWD), not the
        // frontend process CWD, so the binary was placed at a different path than expected.
        let _lock = crate::ENV_LOCK.lock().unwrap();
        unsafe { std::env::set_var("SHATTER_HARNESS_CACHE", "relative/harness/cache") };
        let target = standalone_target_dir();
        unsafe { std::env::set_var("SHATTER_HARNESS_CACHE", "") };

        let target = target.expect("should return Some when env var is set");
        assert!(
            target.is_absolute(),
            "standalone_target_dir must be absolute for CARGO_TARGET_DIR correctness, got: {target:?}"
        );
    }

    #[test]
    fn make_request_scratch_uses_scratch_env() {
        // When SHATTER_HARNESS_SCRATCH is set, make_request_scratch returns a path inside it.
        let _lock = crate::ENV_LOCK.lock().unwrap();
        let scratch_root = std::env::temp_dir().join("shatter-test-scratch-root");
        let scratch_str = scratch_root.to_string_lossy().into_owned();
        unsafe { std::env::set_var("SHATTER_HARNESS_SCRATCH", &scratch_str) };

        let scratch = make_request_scratch();
        unsafe { std::env::set_var("SHATTER_HARNESS_SCRATCH", "") };

        assert!(
            scratch.starts_with(&scratch_root),
            "scratch dir {scratch:?} should be under scratch root {scratch_str}"
        );
    }

    #[test]
    fn make_request_scratch_fallback_to_temp() {
        // When SHATTER_HARNESS_SCRATCH is empty/unset, make_request_scratch returns a temp-based path.
        let _lock = crate::ENV_LOCK.lock().unwrap();
        unsafe { std::env::set_var("SHATTER_HARNESS_SCRATCH", "") };
        let scratch = make_request_scratch();
        // Should not panic; path should contain "shatter-exec-"
        assert!(
            scratch.to_string_lossy().contains("shatter-exec-"),
            "fallback scratch should contain 'shatter-exec-', got: {scratch:?}"
        );
    }

    #[test]
    fn make_request_scratch_unique_per_call() {
        let _lock = crate::ENV_LOCK.lock().unwrap();
        unsafe { std::env::set_var("SHATTER_HARNESS_SCRATCH", "") };
        let a = make_request_scratch();
        let b = make_request_scratch();
        assert_ne!(a, b, "each call should produce a distinct scratch path");
    }

    // ─── prepare_id tests ──────────────────────────────────────────────────

    #[test]
    fn compute_prepare_id_is_16_hex_chars() {
        let id = compute_prepare_id("/tmp/test.rs", "add", &[]);
        assert_eq!(id.len(), 16, "prepare_id must be 16 hex chars, got: {id}");
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()), "must be hex: {id}");
    }

    #[test]
    fn compute_prepare_id_is_deterministic() {
        let a = compute_prepare_id("/tmp/test.rs", "add", &[]);
        let b = compute_prepare_id("/tmp/test.rs", "add", &[]);
        assert_eq!(a, b, "same inputs must produce same id");
    }

    #[test]
    fn compute_prepare_id_differs_for_different_functions() {
        let a = compute_prepare_id("/tmp/test.rs", "add", &[]);
        let b = compute_prepare_id("/tmp/test.rs", "sub", &[]);
        assert_ne!(a, b, "different functions must produce different ids");
    }

    #[test]
    fn compute_prepare_id_differs_for_different_mocks() {
        let no_mocks = compute_prepare_id("/tmp/test.rs", "add", &[]);
        let with_mocks = compute_prepare_id(
            "/tmp/test.rs",
            "add",
            &[serde_json::json!({"symbol": "foo::bar"})],
        );
        assert_ne!(no_mocks, with_mocks, "mocks should affect prepare_id");
    }

    #[test]
    fn compute_prepare_id_sorts_mock_symbols() {
        let a = compute_prepare_id(
            "/tmp/test.rs",
            "add",
            &[
                serde_json::json!({"symbol": "a::b"}),
                serde_json::json!({"symbol": "c::d"}),
            ],
        );
        let b = compute_prepare_id(
            "/tmp/test.rs",
            "add",
            &[
                serde_json::json!({"symbol": "c::d"}),
                serde_json::json!({"symbol": "a::b"}),
            ],
        );
        assert_eq!(a, b, "mock order should not affect prepare_id");
    }

    // ─── crate-backed harness helper tests ───────────────────────────────────

    #[test]
    fn find_crate_root_none_for_tmp() {
        // A file in /tmp should not find a crate root.
        let result = find_crate_root("/tmp/standalone.rs");
        assert!(result.is_none());
    }

    #[test]
    fn find_crate_root_finds_examples() {
        // examples/rust/src/arithmetic.rs is inside a crate.
        let examples_root = std::env::var("SHATTER_EXAMPLES_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::env::temp_dir().join("shatter-examples-main"));
        let examples_src = examples_root.join("rust/src/arithmetic.rs");
        if examples_src.exists() {
            let root = find_crate_root(&examples_src.to_string_lossy());
            assert!(root.is_some(), "should find crate root for examples/rust/src");
            let root = root.unwrap();
            assert!(root.join("Cargo.toml").exists());
        }
    }

    #[test]
    fn extract_dependencies_section_basic() {
        let toml = "[package]\nname = \"foo\"\n\n[dependencies]\nregex = \"1\"\nserde_json = \"1\"\n\n[dev-dependencies]\ncriterion = \"0.5\"\n";
        let deps = extract_dependencies_section(toml);
        assert!(deps.contains("regex"), "should include regex dep");
        assert!(deps.contains("serde_json"), "should include serde_json dep");
        assert!(!deps.contains("criterion"), "should not include dev-deps");
        assert!(!deps.contains("[package]"), "should not include package section");
    }

    #[test]
    fn extract_dependencies_section_empty() {
        let toml = "[package]\nname = \"foo\"\n";
        let deps = extract_dependencies_section(toml);
        assert!(deps.is_empty(), "should be empty when no [dependencies] section");
    }

    #[test]
    fn extract_dependencies_section_inline_table() {
        let toml = "[package]\nname = \"foo\"\n\n[dependencies]\nserde = { version = \"1\", features = [\"derive\"] }\n";
        let deps = extract_dependencies_section(toml);
        assert!(deps.contains("serde"), "should include inline table dep");
    }

    #[test]
    fn generate_cargo_toml_includes_user_deps() {
        let user_toml = "[package]\nname = \"my-crate\"\n\n[dependencies]\nregex = \"1\"\n";
        let runtime_path = std::path::Path::new("/fake/runtime");
        let result = generate_cargo_toml_with_user_deps(user_toml, runtime_path, false, false);
        assert!(result.contains("[workspace]"), "must opt generated harness out of parent workspaces");
        assert!(result.contains("shatter-rust-runtime"), "must include runtime");
        assert!(result.contains("regex"), "must include forwarded user dep");
        assert!(result.contains("serde"), "must include serde");
    }

    #[test]
    fn generate_cargo_toml_deduplicates_injected_deps() {
        // User crate already declares serde_json and serde — must not produce duplicate keys.
        let user_toml = "[package]\nname = \"my-crate\"\n\n[dependencies]\nregex = \"1\"\nserde_json = \"1\"\nserde = { version = \"1\", features = [\"derive\"] }\n";
        let runtime_path = std::path::Path::new("/fake/runtime");
        let result = generate_cargo_toml_with_user_deps(user_toml, runtime_path, false, false);
        assert!(result.contains("regex"), "must include forwarded user dep");
        let serde_json_count = result.matches("serde_json").count();
        assert_eq!(serde_json_count, 1, "serde_json must appear exactly once, got:\n{result}");
    }

    // ─── crate-backed integration tests ──────────────────────────────────────
    //
    // These tests create a minimal Rust crate in a temp dir, compile it via the
    // bin_only dispatch harness path, and verify end-to-end execution.  They
    // require `cargo` to be on PATH; they skip gracefully when it is not.
    //
    // Known-answer target: `add(a: i32, b: i32) -> i32`
    //   add(2, 3) → 5
    //   add(10, 20) → 30
    //   add(-1, 1) → 0
    //
    // Known-answer target: `double(x: i32) -> i32`
    //   double(7) → 14

    /// Write a minimal crate (Cargo.toml + src/lib.rs) to `dir` and return the
    /// path to the source file.  The crate has no external dependencies so
    /// compilation is fast.
    fn write_test_crate(dir: &std::path::Path, source: &str) -> PathBuf {
        let src_dir = dir.join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"shatter-test\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\n",
        )
        .unwrap();
        let src_file = src_dir.join("lib.rs");
        std::fs::write(&src_file, source).unwrap();
        src_file
    }

    fn cargo_build_unavailable(msg: &str) -> bool {
        msg.contains("cargo")
            || msg.contains("No such file")
            || msg.contains("spurious network error")
            || msg.contains("download of config.json failed")
            || msg.contains("Could not resolve host")
            || msg.contains("Could not resolve hostname")
    }

    #[test]
    fn crate_backed_execute_basic() {
        // Execute `add(2, 3)` from a crate-backed source file.
        // Verifies: crate-backed file routes to the bin_only harness and returns
        // the correct result.
        let dir = std::env::temp_dir().join("shatter-test-crate-basic");
        let src_file = write_test_crate(
            &dir,
            "pub fn add(a: i32, b: i32) -> i32 { a + b }\n",
        );

        let cache: HarnessCache = Mutex::new(HashMap::new());
        let crate_cache: CrateHarnessCache = Mutex::new(HashMap::new());
        let bridge_cache: CrateBridgeHarnessCache = Mutex::new(HashMap::new());

        let result = execute_function_with_timing(
            src_file.to_str().unwrap(),
            "add",
            &[serde_json::json!(2), serde_json::json!(3)],
            &[],
            30_000,
            None,
            None,
            &cache,
            &crate_cache,
            &bridge_cache,
        );

        let _ = std::fs::remove_dir_all(&dir);

        match result {
            Ok(r) => {
                assert_eq!(
                    r.return_value,
                    Some(serde_json::json!(5)),
                    "add(2, 3) should return 5"
                );
            }
            // cargo not available in this CI environment — skip
            Err(ExecuteError::CompilationFailed(msg))
                if cargo_build_unavailable(&msg) =>
            {
                eprintln!("skipping crate_backed_execute_basic: cargo unavailable ({msg})");
            }
            Err(e) => panic!("unexpected error: {e:?}"),
        }
    }

    #[test]
    fn crate_backed_second_call_reuses_cache() {
        // Execute `add` twice with different inputs via the same CrateHarnessCache.
        // Verifies: the second call hits the in-memory cache (no recompilation)
        // and returns the correct result.
        let dir = std::env::temp_dir().join("shatter-test-crate-cache");
        let src_file = write_test_crate(
            &dir,
            "pub fn add(a: i32, b: i32) -> i32 { a + b }\n",
        );

        let cache: HarnessCache = Mutex::new(HashMap::new());
        let crate_cache: CrateHarnessCache = Mutex::new(HashMap::new());
        let bridge_cache: CrateBridgeHarnessCache = Mutex::new(HashMap::new());

        // First call — slow path: compile and spawn.
        let first = execute_function_with_timing(
            src_file.to_str().unwrap(),
            "add",
            &[serde_json::json!(10), serde_json::json!(20)],
            &[],
            30_000,
            None,
            None,
            &cache,
            &crate_cache,
            &bridge_cache,
        );

        let cache_size_after_first = crate_cache.lock().unwrap().len();

        // Second call with different inputs — must hit the fast path.
        let second = execute_function_with_timing(
            src_file.to_str().unwrap(),
            "add",
            &[serde_json::json!(-1), serde_json::json!(1)],
            &[],
            30_000,
            None,
            None,
            &cache,
            &crate_cache,
            &bridge_cache,
        );

        let cache_size_after_second = crate_cache.lock().unwrap().len();
        let _ = std::fs::remove_dir_all(&dir);

        match (first, second) {
            (Ok(r1), Ok(r2)) => {
                assert_eq!(r1.return_value, Some(serde_json::json!(30)), "add(10, 20) should return 30");
                assert_eq!(r2.return_value, Some(serde_json::json!(0)), "add(-1, 1) should return 0");
                // The cache should have exactly one entry (same key for both calls).
                assert_eq!(cache_size_after_first, 1, "one cache entry after first call");
                assert_eq!(cache_size_after_second, 1, "still one entry after second call (cache hit)");
            }
            (Err(ExecuteError::CompilationFailed(msg)), _)
            | (_, Err(ExecuteError::CompilationFailed(msg)))
                if cargo_build_unavailable(&msg) =>
            {
                eprintln!("skipping crate_backed_second_call_reuses_cache: cargo unavailable ({msg})");
            }
            (Err(e), _) | (_, Err(e)) => panic!("unexpected error: {e:?}"),
        }
    }

    #[test]
    fn crate_backed_multiple_functions_same_binary() {
        // Execute two different functions (`add` and `double`) that live in the
        // same crate file.  Verifies: both are served by a single dispatch binary
        // and each returns the correct result.
        let dir = std::env::temp_dir().join("shatter-test-crate-multi");
        let src_file = write_test_crate(
            &dir,
            "pub fn add(a: i32, b: i32) -> i32 { a + b }\npub fn double(x: i32) -> i32 { x * 2 }\n",
        );

        let cache: HarnessCache = Mutex::new(HashMap::new());
        let crate_cache: CrateHarnessCache = Mutex::new(HashMap::new());
        let bridge_cache: CrateBridgeHarnessCache = Mutex::new(HashMap::new());

        let add_result = execute_function_with_timing(
            src_file.to_str().unwrap(),
            "add",
            &[serde_json::json!(4), serde_json::json!(6)],
            &[],
            30_000,
            None,
            None,
            &cache,
            &crate_cache,
            &bridge_cache,
        );

        let double_result = execute_function_with_timing(
            src_file.to_str().unwrap(),
            "double",
            &[serde_json::json!(7)],
            &[],
            30_000,
            None,
            None,
            &cache,
            &crate_cache,
            &bridge_cache,
        );

        let cache_size = crate_cache.lock().unwrap().len();
        let _ = std::fs::remove_dir_all(&dir);

        match (add_result, double_result) {
            (Ok(r1), Ok(r2)) => {
                assert_eq!(r1.return_value, Some(serde_json::json!(10)), "add(4, 6) should return 10");
                assert_eq!(r2.return_value, Some(serde_json::json!(14)), "double(7) should return 14");
                // Both functions share one dispatch binary → one cache entry.
                assert_eq!(cache_size, 1, "both functions share one crate cache entry");
            }
            (Err(ExecuteError::CompilationFailed(msg)), _)
            | (_, Err(ExecuteError::CompilationFailed(msg)))
                if cargo_build_unavailable(&msg) =>
            {
                eprintln!("skipping crate_backed_multiple_functions_same_binary: cargo unavailable ({msg})");
            }
            (Err(e), _) | (_, Err(e)) => panic!("unexpected error: {e:?}"),
        }
    }

    #[test]
    fn crate_backed_nonexistent_function_returns_error() {
        // Request execution of a function that does not exist in the crate file.
        // Verifies: returns NonExecutable immediately (no compilation attempted).
        let dir = std::env::temp_dir().join("shatter-test-crate-notfound");
        let src_file = write_test_crate(
            &dir,
            "pub fn add(a: i32, b: i32) -> i32 { a + b }\n",
        );

        let cache: HarnessCache = Mutex::new(HashMap::new());
        let crate_cache: CrateHarnessCache = Mutex::new(HashMap::new());
        let bridge_cache: CrateBridgeHarnessCache = Mutex::new(HashMap::new());

        let result = execute_function_with_timing(
            src_file.to_str().unwrap(),
            "nonexistent_fn",
            &[serde_json::json!(1)],
            &[],
            30_000,
            None,
            None,
            &cache,
            &crate_cache,
            &bridge_cache,
        );

        let _ = std::fs::remove_dir_all(&dir);

        match result {
            Err(ExecuteError::NonExecutable(msg)) => {
                assert!(
                    msg.contains("nonexistent_fn"),
                    "error should mention the function name, got: {msg}"
                );
            }
            other => panic!("expected NonExecutable error, got: {other:?}"),
        }
    }

    #[test]
    fn crate_backed_module_path_input_executes_via_bridge() {
        // Functions whose inputs are named through crate-local paths cannot run
        // in the wrapped bin_only harness because `crate::...` resolves against
        // the temporary harness crate. Default execute should route them through
        // crate_bridge instead of returning not_supported.
        let dir = std::env::temp_dir().join("shatter-test-crate-bridge-input");
        let src_file = write_test_crate(
            &dir,
            r#"
mod config {
    #[derive(serde::Deserialize, serde::Serialize)]
    pub(crate) struct Config {
        enabled: bool,
    }
}

use crate::config::Config;

fn enabled(config: Config) -> bool {
    config.enabled
}
"#,
        );

        let cache: HarnessCache = Mutex::new(HashMap::new());
        let crate_cache: CrateHarnessCache = Mutex::new(HashMap::new());
        let bridge_cache: CrateBridgeHarnessCache = Mutex::new(HashMap::new());

        let result = execute_function_with_timing(
            src_file.to_str().unwrap(),
            "enabled",
            &[serde_json::json!({ "enabled": true })],
            &[],
            30_000,
            None,
            None,
            &cache,
            &crate_cache,
            &bridge_cache,
        );

        let _ = std::fs::remove_dir_all(&dir);

        match result {
            Ok(r) => {
                assert_eq!(
                    r.return_value,
                    Some(serde_json::json!(true)),
                    "enabled({{ enabled: true }}) should return true"
                );
            }
            Err(ExecuteError::CompilationFailed(msg)) if cargo_build_unavailable(&msg) => {
                eprintln!(
                    "skipping crate_backed_module_path_input_executes_via_bridge: cargo unavailable ({msg})"
                );
            }
            Err(e) => panic!("unexpected error: {e:?}"),
        }
    }

    // -------------------------------------------------------------------------
    // cargo check helpers
    // -------------------------------------------------------------------------

    /// Tests skip_cargo_check env var parsing.
    /// Single test to avoid parallel env var mutation races.
    #[test]
    fn skip_cargo_check_env_parsing() {
        // SAFETY: test-only env mutation; single test avoids parallel races.
        unsafe { std::env::remove_var("SHATTER_SKIP_CHECK") };
        assert!(!skip_cargo_check(), "default should be false");

        unsafe { std::env::set_var("SHATTER_SKIP_CHECK", "1") };
        assert!(skip_cargo_check(), "'1' should enable skip");

        unsafe { std::env::set_var("SHATTER_SKIP_CHECK", "true") };
        assert!(skip_cargo_check(), "'true' should enable skip");

        unsafe { std::env::set_var("SHATTER_SKIP_CHECK", "TRUE") };
        assert!(skip_cargo_check(), "'TRUE' should enable skip");

        unsafe { std::env::set_var("SHATTER_SKIP_CHECK", "0") };
        assert!(!skip_cargo_check(), "'0' should not enable skip");

        unsafe { std::env::remove_var("SHATTER_SKIP_CHECK") };
    }

    // ─── crate_bridge helper tests ────────────────────────────────────────────

    #[test]
    fn crate_bridge_wrapper_contains_all_functions() {
        let fns = vec![
            CompatFn { name: "foo".to_string(), param_names: vec!["x".to_string()], param_types: vec!["i32".to_string()], return_type: Some("i32".to_string()), is_async: false },
            CompatFn { name: "bar".to_string(), param_names: vec![], param_types: vec![], return_type: None, is_async: false },
        ];
        let wrapper = generate_crate_bridge_wrapper(&fns, "[]", &[]);
        assert!(wrapper.contains("shatter_wrap_foo"), "wrapper must contain shatter_wrap_foo");
        assert!(wrapper.contains("shatter_wrap_bar"), "wrapper must contain shatter_wrap_bar");
    }

    #[test]
    fn crate_bridge_wrapper_uses_super_prefix() {
        let fns = vec![
            CompatFn { name: "my_fn".to_string(), param_names: vec!["n".to_string()], param_types: vec!["i32".to_string()], return_type: Some("i32".to_string()), is_async: false },
        ];
        let wrapper = generate_crate_bridge_wrapper(&fns, "[]", &[]);
        assert!(wrapper.contains("super::my_fn"), "wrapper must call super::my_fn, not bare my_fn");
    }

    #[test]
    fn crate_bridge_wrapper_has_run_harness_entry_point() {
        let fns = vec![
            CompatFn { name: "calc".to_string(), param_names: vec![], param_types: vec![], return_type: None, is_async: false },
        ];
        let wrapper = generate_crate_bridge_wrapper(&fns, "[]", &[]);
        assert!(wrapper.contains("pub fn shatter_run_harness()"), "wrapper must export shatter_run_harness");
        assert!(wrapper.contains("shatter_wrap_calc"), "dispatch in run_harness must call wrapper");
    }

    #[test]
    fn crate_bridge_wrapper_dispatch_includes_function_names() {
        let fns = vec![
            CompatFn { name: "alpha".to_string(), param_names: vec![], param_types: vec![], return_type: None, is_async: false },
            CompatFn { name: "beta".to_string(), param_names: vec![], param_types: vec![], return_type: None, is_async: false },
        ];
        let wrapper = generate_crate_bridge_wrapper(&fns, "[]", &[]);
        assert!(wrapper.contains("\"alpha\""), "dispatch must match on \"alpha\"");
        assert!(wrapper.contains("\"beta\""), "dispatch must match on \"beta\"");
    }

    #[test]
    fn crate_bridge_bin_is_stable() {
        let bin1 = generate_crate_bridge_bin("my_crate");
        let bin2 = generate_crate_bridge_bin("my_crate");
        assert_eq!(bin1, bin2, "driver bin must be deterministic");
        assert!(bin1.contains("shatter_run_harness"), "must call shatter_run_harness");
        assert!(bin1.contains("my_crate"), "must reference the user crate name");
    }

    #[test]
    fn crate_bridge_cargo_toml_has_feature_dep() {
        let toml = generate_crate_bridge_cargo_toml("my-crate", std::path::Path::new("/some/path"), false);
        assert!(toml.contains("shatter-crate-bridge"), "Cargo.toml must activate the shatter-crate-bridge feature");
        assert!(toml.contains("[workspace]"), "must opt out of parent workspace");
    }

    #[test]
    fn inject_lib_module_declaration_adds_mod() {
        let dir = std::env::temp_dir().join("shatter-test-inject-mod");
        std::fs::create_dir_all(&dir).unwrap();
        let lib_rs = dir.join("lib.rs");
        std::fs::write(&lib_rs, "pub fn foo() {}\n").unwrap();

        inject_lib_module_declaration(&lib_rs).unwrap();

        let content = std::fs::read_to_string(&lib_rs).unwrap();
        assert!(content.contains("pub mod __shatter;"), "must contain mod declaration");
        assert!(content.contains("shatter-crate-bridge"), "must be feature-gated");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn inject_lib_module_declaration_idempotent() {
        let dir = std::env::temp_dir().join("shatter-test-inject-mod-idem");
        std::fs::create_dir_all(&dir).unwrap();
        let lib_rs = dir.join("lib.rs");
        std::fs::write(&lib_rs, "pub fn foo() {}\n").unwrap();

        inject_lib_module_declaration(&lib_rs).unwrap();
        inject_lib_module_declaration(&lib_rs).unwrap();

        let content = std::fs::read_to_string(&lib_rs).unwrap();
        let count = content.matches("pub mod __shatter;").count();
        assert_eq!(count, 1, "declaration must appear exactly once, got {count}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn inject_crate_bridge_feature_adds_feature() {
        let dir = std::env::temp_dir().join("shatter-test-inject-feat");
        std::fs::create_dir_all(&dir).unwrap();
        let toml_path = dir.join("Cargo.toml");
        std::fs::write(
            &toml_path,
            "[package]\nname = \"test\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();

        inject_crate_bridge_feature(&toml_path, std::path::Path::new("/fake/runtime")).unwrap();

        let content = std::fs::read_to_string(&toml_path).unwrap();
        assert!(content.contains("shatter-crate-bridge"), "must add feature to Cargo.toml");
        assert!(content.contains("serde_json"), "must add serde_json optional dep");
        assert!(content.contains("shatter-rust-runtime"), "must add runtime optional dep");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn inject_crate_bridge_feature_idempotent() {
        let dir = std::env::temp_dir().join("shatter-test-inject-feat-idem");
        std::fs::create_dir_all(&dir).unwrap();
        let toml_path = dir.join("Cargo.toml");
        std::fs::write(
            &toml_path,
            "[package]\nname = \"test\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();

        let fake_runtime = std::path::Path::new("/fake/runtime");
        inject_crate_bridge_feature(&toml_path, fake_runtime).unwrap();
        inject_crate_bridge_feature(&toml_path, fake_runtime).unwrap();

        let content = std::fs::read_to_string(&toml_path).unwrap();
        let count = content.matches("shatter-crate-bridge").count();
        assert_eq!(count, 1, "feature must appear exactly once, got {count}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn extract_crate_name_from_package_section() {
        let toml = "[package]\nname = \"my-fancy-crate\"\nversion = \"0.1.0\"\n";
        let name = extract_crate_name(toml);
        assert_eq!(name, Some("my-fancy-crate".to_string()));
    }

    #[test]
    fn extract_crate_name_returns_none_for_no_package() {
        let toml = "[workspace]\nmembers = [\"crate-a\"]\n";
        assert_eq!(extract_crate_name(toml), None);
    }

    #[test]
    fn find_lib_rs_finds_default() {
        let dir = std::env::temp_dir().join("shatter-test-find-lib-rs");
        let src = dir.join("src");
        std::fs::create_dir_all(&src).unwrap();
        let lib = src.join("lib.rs");
        std::fs::write(&lib, "").unwrap();

        let found = find_lib_rs(&dir);
        assert_eq!(found, Some(lib));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn find_lib_rs_none_when_missing() {
        let dir = std::env::temp_dir().join("shatter-test-find-lib-rs-none");
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(find_lib_rs(&dir), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ─── crate_bridge integration test ───────────────────────────────────────
    //
    // Known-answer target: `secret_add(a: i32, b: i32) -> i32`
    //   secret_add(3, 4) → 7
    //
    // The function is NOT marked `pub`, verifying that crate_bridge can access
    // crate-private functions that bin_only cannot reach.

    #[test]
    fn crate_bridge_executes_private_function() {
        let dir = std::env::temp_dir().join("shatter-test-bridge-private");
        // Write a crate with a private function.
        let src = dir.join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"shatter-bridge-test\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\n",
        ).unwrap();
        std::fs::write(
            src.join("lib.rs"),
            "fn secret_add(a: i32, b: i32) -> i32 { a + b }\n",
        ).unwrap();

        let cache: HarnessCache = Mutex::new(HashMap::new());
        let crate_cache: CrateHarnessCache = Mutex::new(HashMap::new());
        let bridge_cache: CrateBridgeHarnessCache = Mutex::new(HashMap::new());

        let result = execute_function_with_timing(
            src.join("lib.rs").to_str().unwrap(),
            "secret_add",
            &[serde_json::json!(3), serde_json::json!(4)],
            &[],
            60_000,
            Some("crate_bridge"),
            None,
            &cache,
            &crate_cache,
            &bridge_cache,
        );

        let _ = std::fs::remove_dir_all(&dir);

        match result {
            Ok(r) => {
                assert_eq!(
                    r.return_value,
                    Some(serde_json::json!(7)),
                    "secret_add(3, 4) should return 7"
                );
            }
            Err(ExecuteError::CompilationFailed(msg))
                if cargo_build_unavailable(&msg) =>
            {
                eprintln!("skipping crate_bridge_executes_private_function: cargo unavailable ({msg})");
            }
            Err(e) => panic!("unexpected error: {e:?}"),
        }
    }
}
