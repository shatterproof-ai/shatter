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
#[cfg(test)]
use std::sync::atomic::{AtomicBool, Ordering};
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

    // Strip inner attributes that conflict with the std harness binary that
    // wraps user source. `#![no_std]` / `#![no_main]` and the `panic_impl`
    // lang item are the common collision sources for no_std user crates being
    // compiled into our std harness. See str-xt4h.
    file.attrs.retain(|a| !is_harness_incompatible_attr(a));

    // Build the `serde::Serialize` derive attribute to inject on structs/enums.
    let serialize_derive: syn::Attribute = syn::parse_quote!(#[derive(serde::Serialize)]);

    for item in &mut file.items {
        strip_harness_incompatible_item_attrs(item);
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

/// Attributes that must not survive being wrapped in our std harness binary.
///
/// `#[panic_handler]`, `#[lang = "..."]`, `#[start]`, `#[global_allocator]`,
/// and `#[panic_implementation]` collide with the lang items `std` already
/// provides; `#![no_std]` / `#![no_main]` flip compilation modes that don't
/// apply to a wrapped module. See str-xt4h.
fn is_harness_incompatible_attr(attr: &syn::Attribute) -> bool {
    let path = attr.path();
    path.is_ident("no_std")
        || path.is_ident("no_main")
        || path.is_ident("panic_handler")
        || path.is_ident("panic_implementation")
        || path.is_ident("lang")
        || path.is_ident("start")
        || path.is_ident("global_allocator")
}

/// Strip `#[panic_handler]`-style attributes from any item that supports them.
fn strip_harness_incompatible_item_attrs(item: &mut syn::Item) {
    let attrs: Option<&mut Vec<syn::Attribute>> = match item {
        syn::Item::Fn(f) => Some(&mut f.attrs),
        syn::Item::Static(s) => Some(&mut s.attrs),
        syn::Item::Const(c) => Some(&mut c.attrs),
        syn::Item::Mod(m) => Some(&mut m.attrs),
        syn::Item::Struct(s) => Some(&mut s.attrs),
        syn::Item::Enum(e) => Some(&mut e.attrs),
        syn::Item::Type(t) => Some(&mut t.attrs),
        syn::Item::Trait(t) => Some(&mut t.attrs),
        syn::Item::Impl(i) => Some(&mut i.attrs),
        syn::Item::Macro(m) => Some(&mut m.attrs),
        _ => None,
    };
    if let Some(attrs) = attrs {
        attrs.retain(|a| !is_harness_incompatible_attr(a));
    }
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
    /// Hash of native generator replay metadata baked into the harness source.
    native_replay_hash: u64,
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
            native_replay_hash: 0,
        }
    }
}

#[derive(Debug, Clone)]
struct NativeReplaySpec {
    input_index: usize,
    module_name: String,
    function_name: String,
    file_path: PathBuf,
    recipe: Value,
}

fn is_native_replay_marker(value: &Value) -> bool {
    value
        .as_object()
        .and_then(|obj| obj.get("__shatter_native"))
        .and_then(Value::as_bool)
        == Some(true)
}

fn is_rust_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c == '_' || c.is_ascii_alphabetic())
        && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

fn native_replay_specs(inputs: &[Value]) -> Result<Vec<Option<NativeReplaySpec>>, ExecuteError> {
    let mut specs = vec![None; inputs.len()];
    for (idx, value) in inputs.iter().enumerate() {
        if !is_native_replay_marker(value) {
            continue;
        }
        let replay = value
            .get("__shatter_replay")
            .and_then(Value::as_object)
            .ok_or_else(|| {
                ExecuteError::NonExecutable(format!(
                    "native replay metadata missing for input {idx}"
                ))
            })?;
        let language = replay
            .get("language")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                ExecuteError::NonExecutable(format!(
                    "native replay language missing for input {idx}"
                ))
            })?;
        if language != "rust" {
            return Err(ExecuteError::NonExecutable(format!(
                "native replay language `{language}` not supported for Rust input {idx}"
            )));
        }
        let file = replay
            .get("file")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                ExecuteError::NonExecutable(format!(
                    "native replay generator file missing for input {idx}"
                ))
            })?;
        let file_path = PathBuf::from(file);
        if !file_path.exists() {
            return Err(ExecuteError::FileError(format!(
                "native replay generator file not found for input {idx}: {}",
                file_path.display()
            )));
        }
        let file_path = std::fs::canonicalize(&file_path)
            .unwrap_or_else(|_| to_absolute(file_path));
        let function_name = replay
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                ExecuteError::NonExecutable(format!(
                    "native replay generator name missing for input {idx}"
                ))
            })?;
        if !is_rust_identifier(function_name) {
            return Err(ExecuteError::NonExecutable(format!(
                "native replay generator name is not a Rust identifier for input {idx}: {function_name}"
            )));
        }
        specs[idx] = Some(NativeReplaySpec {
            input_index: idx,
            module_name: format!("__shatter_native_gen_{idx}"),
            function_name: function_name.to_string(),
            file_path,
            recipe: replay.get("recipe").cloned().unwrap_or(Value::Null),
        });
    }
    Ok(specs)
}

fn native_replay_hash(specs: &[Option<NativeReplaySpec>]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for spec in specs.iter().flatten() {
        spec.input_index.hash(&mut hasher);
        spec.module_name.hash(&mut hasher);
        spec.function_name.hash(&mut hasher);
        spec.file_path.hash(&mut hasher);
        serde_json::to_string(&spec.recipe)
            .unwrap_or_default()
            .hash(&mut hasher);
    }
    hasher.finish()
}

fn shatter_rust_crate_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn rust_string_literal(value: &str) -> String {
    serde_json::to_string(value).expect("string literal serialization cannot fail")
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

/// Snapshot of files for test-only backup/restore assertions.
///
/// Previously used by `execute_function_crate_bridge` to restore user source
/// after in-place mutation (str-31j.1). Superseded by the staging-copy
/// approach (str-ja70) which never modifies original files.
#[cfg(test)]
pub struct BridgeSourceBackup {
    entries: Vec<(PathBuf, Option<Vec<u8>>)>,
    restored: AtomicBool,
}

#[cfg(test)]
impl BridgeSourceBackup {
    /// Snapshot the contents (or non-existence) of each path. Returns an
    /// error only if a path exists but cannot be read — in that case the
    /// caller must abort before any mutations.
    pub fn snapshot(paths: &[PathBuf]) -> io::Result<Self> {
        let mut entries = Vec::with_capacity(paths.len());
        for path in paths {
            match std::fs::read(path) {
                Ok(bytes) => entries.push((path.clone(), Some(bytes))),
                Err(e) if e.kind() == io::ErrorKind::NotFound => {
                    entries.push((path.clone(), None));
                }
                Err(e) => return Err(e),
            }
        }
        Ok(Self { entries, restored: AtomicBool::new(false) })
    }

    /// Restore every tracked path. Errors from individual restores are
    /// swallowed — we are usually on a failure path already and want to
    /// best-effort recover every file we can rather than abort on the first
    /// permission glitch.
    pub fn restore(&self) {
        if self.restored.swap(true, Ordering::SeqCst) {
            return;
        }
        for (path, original) in &self.entries {
            match original {
                Some(bytes) => {
                    let _ = std::fs::write(path, bytes);
                }
                None => {
                    let _ = std::fs::remove_file(path);
                }
            }
        }
    }

    /// Test helper: report whether `restore()` has run yet.
    #[cfg(test)]
    pub fn is_restored(&self) -> bool {
        self.restored.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
impl Drop for BridgeSourceBackup {
    fn drop(&mut self) {
        // Drop must never panic; restore() already swallows write errors.
        self.restore();
    }
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
                    loop_body_states: vec![],
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
                    loop_body_states: vec![],
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

fn cached_harness_matches(
    binary_path: &Path,
    main_rs_path: &Path,
    main_rs_content: &str,
    cargo_toml_path: &Path,
    cargo_toml_content: &str,
) -> bool {
    binary_path.exists()
        && std::fs::read_to_string(main_rs_path).ok().as_deref() == Some(main_rs_content)
        && std::fs::read_to_string(cargo_toml_path).ok().as_deref() == Some(cargo_toml_content)
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
fn generate_cargo_toml_with_user_deps(
    user_cargo_toml: &str,
    runtime_path: &Path,
    needs_tokio: bool,
    needs_axum: bool,
    needs_shatter_rust: bool,
) -> String {
    let forwarded = extract_dependencies_section(user_cargo_toml);
    // Axum handlers are always async, so tokio is implied by axum.
    let needs_tokio = needs_tokio || needs_axum;
    let axum_keys: &[&str] = if needs_axum {
        &["axum", "tower", "http", "http-body-util", "async-trait"]
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
                && (!needs_shatter_rust || key != "shatter-rust")
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
            "async-trait = \"0.1\"\n",
        )
    } else {
        ""
    };
    let shatter_rust_dep = if needs_shatter_rust {
        let shatter_rust_path = shatter_rust_crate_path().display().to_string().replace('\\', "/");
        format!("shatter-rust = {{ path = \"{shatter_rust_path}\" }}\n")
    } else {
        String::new()
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
{tokio_dep}{axum_deps}{shatter_rust_dep}{filtered}
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
    #[serde(default)]
    pub loop_body_states: Vec<Value>,
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
        if is_raw_pointer_or_extern_type(ty) {
            issues.push(format!(
                "parameter `{name}` has raw pointer / extern fn type `{ty}`: cannot be synthesised from JSON"
            ));
        }
    }

    // 3. Module-local crate/super paths — source is wrapped in `mod user_code`,
    // so absolute paths still point at the temporary harness crate.
    if ctx.has_module_path_uses {
        issues.push(
            "file imports types through crate/super module paths: \
             won't resolve in the wrapped bin_only harness"
                .to_string(),
        );
    }

    // 4. External crate types — not available in isolated harness.
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
fn generate_cargo_toml(
    runtime_path: &Path,
    needs_tokio: bool,
    needs_axum: bool,
    needs_shatter_rust: bool,
) -> String {
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
            "async-trait = \"0.1\"\n",
        )
    } else {
        ""
    };
    let shatter_rust_dep = if needs_shatter_rust {
        let shatter_rust_path = shatter_rust_crate_path().display().to_string().replace('\\', "/");
        format!("shatter-rust = {{ path = \"{shatter_rust_path}\" }}\n")
    } else {
        String::new()
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
{tokio_dep}{axum_deps}{shatter_rust_dep}"#
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

/// Returns true if the type string is a raw pointer or function pointer / extern
/// fn type that the harness cannot synthesise from JSON inputs. See str-xt4h.
fn is_raw_pointer_or_extern_type(ty: &str) -> bool {
    let normalized = ty.replace('\n', " ");
    // Collapse syn's token-stream spacing (`* const u8`) so substring matching
    // works regardless of whether the source had `*const`, `* const`, etc.
    let collapsed: String = normalized.split_whitespace().collect::<Vec<_>>().join(" ");
    let t = collapsed.as_str();
    if t.contains("*const ") || t.contains("*mut ") || t.contains("* const ") || t.contains("* mut ") {
        return true;
    }
    if t.starts_with("fn(") || t.contains(" fn(") {
        return true;
    }
    if t.starts_with("extern ") || t.contains(" extern ") {
        return true;
    }
    false
}

fn crate_bridge_unsupported_reason(fn_info: &CompatFn) -> Option<String> {
    if fn_info
        .param_types
        .iter()
        .any(|ty| is_trait_object_type(ty) || is_raw_pointer_or_extern_type(ty))
    {
        return Some(
            "function has trait-object, raw-pointer, or extern function parameters that cannot be synthesized from JSON"
                .to_string(),
        );
    }

    if let Some(reason) = axum_state_unsupported_reason(&fn_info.param_types) {
        return Some(reason);
    }

    None
}

fn crate_bridge_serde_bound_failure_reason(build_error: &str) -> Option<&'static str> {
    let from_value_bound = build_error.contains("serde_json::from_value")
        && (build_error.contains("DeserializeOwned")
            || build_error.contains("Deserialize<'de>")
            || build_error.contains("Deserialize < 'de >"));
    if from_value_bound {
        return Some(
            "function parameters are not JSON-harness compatible and may not implement DeserializeOwned",
        );
    }

    let to_value_bound = build_error.contains("serde_json::to_value")
        && build_error.contains("Serialize");
    if to_value_bound {
        return Some(
            "return type is not JSON-harness compatible and may not implement Serialize",
        );
    }

    None
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

/// Classification of an Axum extractor wrapper type seen on a function parameter.
///
/// The wrapper-generation paths (`generate_harness`, `generate_dispatch_harness`,
/// `generate_crate_bridge_wrapper`) cannot blindly call `serde_json::from_value`
/// on Axum extractor wrappers because:
/// - `axum::extract::Path<T>`, `Query<T>`, `Json<T>` are *not* `DeserializeOwned`
///   themselves (only their inner `T` typically is), so the naïve emit produces
///   `the trait bound \`Path<T>: DeserializeOwned\` is not satisfied`.
/// - `axum::extract::State<T>` does not implement `Deserialize` at all — its
///   value comes from the `Router::with_state()` plumbing, not from the request.
///
/// This classifier inspects the leaf path segment of the parameter type so it
/// matches both `Json<T>` (when imported via `use axum::Json`) and
/// `axum::extract::Json<T>` (fully qualified).
#[derive(Debug, Clone, PartialEq)]
enum AxumExtractor {
    /// `Path<T>` — supported: deserialize inner `T`, then wrap.
    Path(String),
    /// `Query<T>` — supported: deserialize inner `T`, then wrap.
    Query(String),
    /// `Json<T>` — supported: deserialize inner `T`, then wrap.
    Json(String),
    /// `State<T>` — unsupported by generic wrappers (no `Deserialize` impl).
    /// The wrapper must early-return with a clear "not supported" error instead
    /// of emitting an uncompilable `serde_json::from_value::<State<T>>` call.
    State(String),
}

fn classify_axum_extractor(ty: &str) -> Option<AxumExtractor> {
    // Strip any leading reference (`&`, `&mut`) before parsing — extractors
    // are usually passed by value but we don't want to fail on `&Path<T>`.
    let parsed: syn::Type = syn::parse_str(ty).ok()?;
    let path = match parsed {
        syn::Type::Path(syn::TypePath { path, .. }) => path,
        syn::Type::Reference(r) => match *r.elem {
            syn::Type::Path(syn::TypePath { path, .. }) => path,
            _ => return None,
        },
        _ => return None,
    };
    let seg = path.segments.last()?;
    let name = seg.ident.to_string();
    let inner_type = || -> Option<String> {
        match &seg.arguments {
            syn::PathArguments::AngleBracketed(args) => {
                args.args.iter().find_map(|a| match a {
                    syn::GenericArgument::Type(t) => {
                        use quote::ToTokens;
                        Some(t.to_token_stream().to_string())
                    }
                    _ => None,
                })
            }
            _ => None,
        }
    };
    match name.as_str() {
        "Path" => inner_type().map(AxumExtractor::Path),
        "Query" => inner_type().map(AxumExtractor::Query),
        "Json" => inner_type().map(AxumExtractor::Json),
        "State" => inner_type().map(AxumExtractor::State),
        _ => None,
    }
}

fn axum_path_segment_types(inner_ty: &str) -> Vec<String> {
    let parsed: syn::Type = match syn::parse_str(inner_ty) {
        Ok(parsed) => parsed,
        Err(_) => return vec![inner_ty.to_string()],
    };
    match parsed {
        syn::Type::Tuple(tuple) if !tuple.elems.is_empty() => tuple
            .elems
            .iter()
            .map(|elem| {
                use quote::ToTokens;
                elem.to_token_stream().to_string()
            })
            .collect(),
        other => {
            use quote::ToTokens;
            vec![other.to_token_stream().to_string()]
        }
    }
}

fn axum_default_path_segment(ty: &str, index: usize) -> String {
    let normalized = ty.replace(' ', "");
    if normalized.ends_with("Uuid") || normalized.contains("::Uuid") {
        format!("00000000-0000-0000-0000-{value:012}", value = index + 1)
    } else if matches!(
        normalized.as_str(),
        "u8" | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "isize"
    ) {
        (index + 1).to_string()
    } else {
        format!("p{index}")
    }
}

fn axum_route_pattern_for_path(inner_ty: &str) -> String {
    let segments = axum_path_segment_types(inner_ty);
    let placeholders = (0..segments.len())
        .map(|idx| format!("{{p{idx}}}"))
        .collect::<Vec<_>>()
        .join("/");
    format!("/test/{placeholders}")
}

fn axum_default_path_value_for_path(inner_ty: &str) -> String {
    let segments = axum_path_segment_types(inner_ty)
        .iter()
        .enumerate()
        .map(|(idx, ty)| axum_default_path_segment(ty, idx))
        .collect::<Vec<_>>()
        .join("/");
    format!("/test/{segments}")
}

fn axum_json_default_value(type_sources: &[(Option<&str>, &str)], inner_ty: &str) -> Value {
    let structs = collect_struct_defs(type_sources);
    let mut seen = HashSet::new();
    axum_json_default_for_type_str(inner_ty, &structs, &mut seen)
}

fn collect_struct_defs(type_sources: &[(Option<&str>, &str)]) -> HashMap<String, syn::ItemStruct> {
    let mut found = Vec::new();
    let mut bare_counts: HashMap<String, usize> = HashMap::new();
    for (module_path, source) in type_sources {
        let Ok(file) = syn::parse_file(source) else {
            continue;
        };
        for item in file.items {
            if let syn::Item::Struct(item_struct) = item {
                let bare = item_struct.ident.to_string();
                *bare_counts.entry(bare).or_insert(0) += 1;
                found.push((module_path.map(str::to_string), item_struct));
            }
        }
    }
    let mut structs = HashMap::new();
    for (module_path, item_struct) in found {
        let bare = item_struct.ident.to_string();
        if bare_counts.get(&bare) == Some(&1) {
            structs.insert(bare.clone(), item_struct.clone());
        }
        if let Some(module_path) = module_path {
            structs.insert(format!("{module_path}::{bare}"), item_struct.clone());
            structs.insert(format!("crate::{module_path}::{bare}"), item_struct);
        }
    }
    structs
}

fn axum_json_default_for_type_str(
    ty: &str,
    structs: &HashMap<String, syn::ItemStruct>,
    seen: &mut HashSet<String>,
) -> Value {
    let Ok(parsed) = syn::parse_str::<syn::Type>(ty) else {
        return Value::Object(serde_json::Map::new());
    };
    axum_json_default_for_type(&parsed, structs, seen)
}

fn axum_json_default_for_type(
    ty: &syn::Type,
    structs: &HashMap<String, syn::ItemStruct>,
    seen: &mut HashSet<String>,
) -> Value {
    let syn::Type::Path(type_path) = ty else {
        return Value::Null;
    };
    let Some(segment) = type_path.path.segments.last() else {
        return Value::Null;
    };
    let bare_name = segment.ident.to_string();
    match bare_name.as_str() {
        "String" => return Value::String(String::new()),
        "bool" => return Value::Bool(false),
        "u8" | "u16" | "u32" | "u64" | "u128" | "usize" | "i8" | "i16" | "i32" | "i64"
        | "i128" | "isize" => return serde_json::json!(0),
        "f32" | "f64" => return serde_json::json!(0.0),
        "Uuid" => return Value::String("00000000-0000-0000-0000-000000000001".to_string()),
        "NaiveDate" => return Value::String("1970-01-01".to_string()),
        "DateTime" => return Value::String("1970-01-01T00:00:00Z".to_string()),
        "Vec" | "HashSet" | "BTreeSet" => return Value::Array(Vec::new()),
        "HashMap" | "BTreeMap" => return Value::Object(serde_json::Map::new()),
        "Option" => return Value::Null,
        _ => {}
    }

    for key in axum_type_lookup_keys(type_path) {
        if !seen.insert(key.clone()) {
            return Value::Null;
        }
        if let Some(item_struct) = structs.get(&key) {
            let value = axum_json_default_for_struct(item_struct, structs, seen);
            seen.remove(&key);
            return value;
        }
        seen.remove(&key);
    }
    Value::Object(serde_json::Map::new())
}

fn axum_type_lookup_keys(type_path: &syn::TypePath) -> Vec<String> {
    let mut keys = Vec::new();
    let path = type_path
        .path
        .segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect::<Vec<_>>()
        .join("::");
    if !path.is_empty() {
        keys.push(path);
    }
    if let Some(last) = type_path.path.segments.last() {
        let bare = last.ident.to_string();
        if keys.first() != Some(&bare) {
            keys.push(bare);
        }
    }
    keys
}

fn axum_json_default_for_struct(
    item_struct: &syn::ItemStruct,
    structs: &HashMap<String, syn::ItemStruct>,
    seen: &mut HashSet<String>,
) -> Value {
    match &item_struct.fields {
        syn::Fields::Named(fields) => {
            let rename_all_camel = serde_rename_all_camel_case(&item_struct.attrs);
            let mut object = serde_json::Map::new();
            for field in &fields.named {
                let Some(ident) = &field.ident else {
                    continue;
                };
                if serde_field_has_attr(&field.attrs, &["skip", "skip_deserializing"]) {
                    continue;
                }
                let value = axum_json_default_for_type(&field.ty, structs, seen);
                if serde_field_has_attr(&field.attrs, &["flatten"]) {
                    if let Value::Object(flattened) = value {
                        object.extend(flattened);
                    }
                    continue;
                }
                if serde_field_has_attr(&field.attrs, &["default"]) || type_is_option(&field.ty) {
                    continue;
                }
                let key = serde_field_name(ident.to_string(), &field.attrs, rename_all_camel);
                object.insert(key, value);
            }
            Value::Object(object)
        }
        syn::Fields::Unnamed(fields) => Value::Array(
            fields
                .unnamed
                .iter()
                .map(|field| axum_json_default_for_type(&field.ty, structs, seen))
                .collect(),
        ),
        syn::Fields::Unit => Value::Null,
    }
}

fn type_is_option(ty: &syn::Type) -> bool {
    matches!(
        ty,
        syn::Type::Path(type_path)
            if type_path
                .path
                .segments
                .last()
                .is_some_and(|segment| segment.ident == "Option")
    )
}

fn serde_field_has_attr(attrs: &[syn::Attribute], names: &[&str]) -> bool {
    attrs.iter().any(|attr| {
        if !attr.path().is_ident("serde") {
            return false;
        }
        let mut found = false;
        let _ = attr.parse_nested_meta(|meta| {
            if names.iter().any(|name| meta.path.is_ident(name)) {
                found = true;
            }
            Ok(())
        });
        found
    })
}

fn serde_rename_all_camel_case(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        if !attr.path().is_ident("serde") {
            return false;
        }
        let mut found = false;
        let _ = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("rename_all") {
                let value = meta.value()?;
                let lit: syn::LitStr = value.parse()?;
                found = lit.value() == "camelCase";
            }
            Ok(())
        });
        found
    })
}

fn serde_field_name(name: String, attrs: &[syn::Attribute], rename_all_camel: bool) -> String {
    for attr in attrs {
        if !attr.path().is_ident("serde") {
            continue;
        }
        let mut rename = None;
        let _ = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("rename") {
                let value = meta.value()?;
                let lit: syn::LitStr = value.parse()?;
                rename = Some(lit.value());
            }
            Ok(())
        });
        if let Some(rename) = rename {
            return rename;
        }
    }
    if rename_all_camel {
        snake_to_lower_camel(&name)
    } else {
        name
    }
}

fn snake_to_lower_camel(name: &str) -> String {
    let mut parts = name.split('_');
    let Some(first) = parts.next() else {
        return String::new();
    };
    let mut camel = first.to_string();
    for part in parts {
        let mut chars = part.chars();
        if let Some(first_char) = chars.next() {
            camel.extend(first_char.to_uppercase());
            camel.push_str(chars.as_str());
        }
    }
    camel
}

/// Returns the fully-qualified Axum constructor expression to wrap an inner
/// deserialized value into the extractor type, e.g. `axum::extract::Path`.
fn axum_extractor_constructor(ext: &AxumExtractor) -> &'static str {
    match ext {
        AxumExtractor::Path(_) => "axum::extract::Path",
        AxumExtractor::Query(_) => "axum::extract::Query",
        AxumExtractor::Json(_) => "axum::Json",
        AxumExtractor::State(_) => "", // unreachable: State doesn't get constructed here
    }
}

/// If any parameter is an Axum `State<T>` extractor, return a message describing
/// why this function isn't executable via the generic wrapper path.
fn axum_state_unsupported_reason(param_types: &[String]) -> Option<String> {
    let states: Vec<String> = param_types
        .iter()
        .filter_map(|ty| match classify_axum_extractor(ty) {
            Some(AxumExtractor::State(_)) => Some(ty.clone()),
            _ => None,
        })
        .collect();
    if states.is_empty() {
        None
    } else {
        Some(format!(
            "axum extractor `{}` not constructible without an app-state generator; \
             configure a state generator or run via the Axum harness adapter",
            states.join("`, `")
        ))
    }
}

/// True if any parameter is an Axum extractor (handler-shaped function).
///
/// Used to decide whether to skip Serialize-on-return-value capture, which
/// would otherwise fail for `axum::Json<T>` and project error types that
/// don't implement `Serialize`.
fn is_axum_handler_shape(param_types: &[String]) -> bool {
    param_types.iter().any(|ty| classify_axum_extractor(ty).is_some())
}

/// True if a return type is, or wraps, an Axum response wrapper.
///
/// Handler-shaped functions can have no extractor parameters, e.g.
/// `async fn health() -> Json<Value>`. The generic JSON harness cannot
/// serialize Axum's outer response wrappers directly, so these returns are
/// treated like other Axum handlers and captured as `null` instead.
fn is_axum_return_shape(return_type: &str) -> bool {
    fn contains_axum_wrapper(ty: &syn::Type) -> bool {
        match ty {
            syn::Type::Path(type_path) => type_path.path.segments.iter().any(|seg| {
                matches!(seg.ident.to_string().as_str(), "Json" | "Response")
                    || match &seg.arguments {
                        syn::PathArguments::AngleBracketed(args) => args.args.iter().any(|arg| {
                            matches!(arg, syn::GenericArgument::Type(inner) if contains_axum_wrapper(inner))
                        }),
                        _ => false,
                    }
            }),
            syn::Type::Reference(type_ref) => contains_axum_wrapper(&type_ref.elem),
            syn::Type::Tuple(tuple) => tuple.elems.iter().any(contains_axum_wrapper),
            _ => false,
        }
    }

    syn::parse_str::<syn::Type>(return_type)
        .is_ok_and(|ty| contains_axum_wrapper(&ty))
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
    native_replays: &[Option<NativeReplaySpec>],
    is_async: bool,
) -> Result<String, ExecuteError> {
    let module_block = wrap_in_module(instrumented_source)?;
    let mut h = String::with_capacity(4096);

    h.push_str("#![allow(unused_imports)]\n");
    h.push_str("use serde_json::Value;\n\n");
    if native_replays.iter().any(Option::is_some) {
        h.push_str(
            "extern crate self as shatter_rust;\npub mod generators {\n    pub struct GeneratorResult {\n        pub id: String,\n        pub value: Box<dyn std::any::Any + Send>,\n        pub recipe: serde_json::Value,\n    }\n}\n\n",
        );
    }
    h.push_str(&module_block);
    h.push_str("\nuse crate::user_code::*;\n");
    for spec in native_replays.iter().flatten() {
        let file_path = rust_string_literal(&spec.file_path.display().to_string());
        h.push_str(&format!(
            "\n#[path = {file_path}]\nmod {};\n",
            spec.module_name
        ));
    }
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

    // If any parameter is an Axum `State<T>` we cannot synthesize it from JSON
    // (it has no `Deserialize` impl). Emit an early return with a clear error
    // rather than producing uncompilable `from_value::<State<T>>` code.
    if let Some(reason) = axum_state_unsupported_reason(param_types) {
        h.push_str(&format!(
            "        return shatter_rust_runtime::build_result_json(None, Some(serde_json::json!({{\"error_type\":\"not_supported\",\"message\":{:?}}})), 0.0, vec![]);\n",
            reason
        ));
        // Emit a dummy reference to silence unused-variable warnings on `inputs`.
        h.push_str("        #[allow(unreachable_code)] { let _ = inputs; }\n");
        h.push_str("    });\n");
        h.push_str("}\n");
        return Ok(h);
    }

    // Deserialize each input parameter from the inputs array.
    for (i, (name, ty)) in param_names.iter().zip(param_types.iter()).enumerate() {
        let clean_name = name.strip_prefix("mut ").unwrap_or(name).trim();
        if let Some(spec) = native_replays.get(i).and_then(|spec| spec.as_ref()) {
            let recipe_json = serde_json::to_string(&spec.recipe)
                .map_err(|e| ExecuteError::InstrumentError(format!("cannot serialize native replay recipe: {e}")))?;
            let recipe_literal = rust_string_literal(&recipe_json);
            h.push_str(&format!(
                "        let __recipe_{i}: serde_json::Value = serde_json::from_str({recipe_literal}).unwrap();\n"
            ));
            h.push_str(&format!(
                "        let __generated_{i} = {}::{}(Some(__recipe_{i}));\n",
                spec.module_name, spec.function_name
            ));
            h.push_str(&format!(
                "        let {clean_name}: {ty} = match __generated_{i}.value.downcast::<{ty}>() {{\n"
            ));
            h.push_str("            Ok(value) => *value,\n");
            h.push_str(&format!(
                "            Err(_) => return shatter_rust_runtime::build_result_json(None, Some(serde_json::json!({{\"error_type\":\"runtime_error\",\"message\": \"native replay downcast failed for input {i}: expected {ty}\"}})), 0.0, vec![]),\n"
            ));
            h.push_str("        };\n");
        } else if let Some(ext) = classify_axum_extractor(ty) {
            // Axum Path/Query/Json: deserialize inner T, then wrap with the
            // extractor constructor. The wrapper types themselves are not
            // `DeserializeOwned` so `from_value::<Path<T>>` would not compile.
            let inner = match &ext {
                AxumExtractor::Path(t) | AxumExtractor::Query(t) | AxumExtractor::Json(t) => t.clone(),
                AxumExtractor::State(_) => unreachable!("State handled above"),
            };
            let ctor = axum_extractor_constructor(&ext);
            h.push_str(&format!(
                "        let {clean_name}_inner: {inner} = match serde_json::from_value(inputs[{i}].clone()) {{\n"
            ));
            h.push_str("            Ok(value) => value,\n");
            h.push_str(&format!(
                "            Err(__err) => return shatter_rust_runtime::build_result_json(None, Some(serde_json::json!({{\"error_type\":\"runtime_error\",\"message\": format!(\"input {i} deserialization failed: {{}}\", __err)}})), 0.0, vec![]),\n"
            ));
            h.push_str("        };\n");
            h.push_str(&format!(
                "        let {clean_name} = {ctor}({clean_name}_inner);\n"
            ));
        } else if let Some(mapping) = owned_type_for_ref(ty) {
            h.push_str(&format!(
                "        let {clean_name}_owned: {} = match serde_json::from_value(inputs[{i}].clone()) {{\n",
                mapping.deser_type
            ));
            h.push_str("            Ok(value) => value,\n");
            h.push_str(&format!(
                "            Err(__err) => return shatter_rust_runtime::build_result_json(None, Some(serde_json::json!({{\"error_type\":\"runtime_error\",\"message\": format!(\"input {i} deserialization failed: {{}}\", __err)}})), 0.0, vec![]),\n"
            ));
            h.push_str("        };\n");
            if mapping.needs_slice_conversion {
                h.push_str(&format!(
                    "        let {clean_name}_refs: Vec<&str> = {clean_name}_owned.iter().map(|s| s.as_str()).collect();\n"
                ));
            }
        } else {
            h.push_str(&format!(
                "        let {clean_name}: {ty} = match serde_json::from_value(inputs[{i}].clone()) {{\n"
            ));
            h.push_str("            Ok(value) => value,\n");
            h.push_str(&format!(
                "            Err(__err) => return shatter_rust_runtime::build_result_json(None, Some(serde_json::json!({{\"error_type\":\"runtime_error\",\"message\": format!(\"input {i} deserialization failed: {{}}\", __err)}})), 0.0, vec![]),\n"
            ));
            h.push_str("        };\n");
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

    // Map result to return_value / thrown_error.
    //
    // For Axum-shaped handlers the return type may be `axum::Json<T>`,
    // `Result<axum::Json<T>, ProjectError>`, `impl IntoResponse`, etc., which
    // do not implement `Serialize` on the outer wrapper. Calling
    // `serde_json::to_value` directly would fail to compile. The generic
    // wrapper path can't synthesize an HTTP roundtrip, so we drop the return
    // value (callers should use the Axum harness adapter for response capture).
    let axum_shape = is_axum_handler_shape(param_types)
        || return_type.as_ref().is_some_and(|ty| is_axum_return_shape(ty));
    h.push_str("        let (ret_val, err_val) = match result {\n");
    if return_type.is_some() && !axum_shape {
        h.push_str("            Ok(ref v) => (Some(serde_json::to_value(v).unwrap_or(Value::Null)), None),\n");
    } else if return_type.is_some() {
        // Axum-shaped handler: skip Serialize-bound return capture.
        h.push_str("            Ok(_) => (Some(Value::Null), None),\n");
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
        h.push_str(&format!("            {:?} => 'shatter_arm: {{\n", fn_name.as_str()));
        // Axum `State<T>` short-circuits with a clear "not supported" before
        // emitting any uncompilable `from_value::<State<T>>` code.
        if let Some(reason) = axum_state_unsupported_reason(param_types) {
            h.push_str(&format!(
                "                break 'shatter_arm (None, Some(serde_json::json!({{\"error_type\":\"not_supported\",\"message\":{:?}}})), 0.0);\n",
                reason
            ));
            h.push_str("            }\n");
            continue;
        }
        let axum_shape = is_axum_handler_shape(param_types)
            || return_type.as_ref().is_some_and(|ty| is_axum_return_shape(ty));
        // Deserialize each parameter.
        for (i, (name, ty)) in param_names.iter().zip(param_types.iter()).enumerate() {
            let clean_name = name.strip_prefix("mut ").unwrap_or(name).trim();
            if let Some(ext) = classify_axum_extractor(ty) {
                let inner = match &ext {
                    AxumExtractor::Path(t) | AxumExtractor::Query(t) | AxumExtractor::Json(t) => t.clone(),
                AxumExtractor::State(_) => unreachable!("State handled above"),
                };
                let ctor = axum_extractor_constructor(&ext);
                h.push_str(&format!(
                    "                let {clean_name}_inner: {inner} = match serde_json::from_value(inputs[{i}].clone()) {{\n"
                ));
                h.push_str("                    Ok(value) => value,\n");
                h.push_str(&format!(
                    "                    Err(__err) => break 'shatter_arm (None, Some(serde_json::json!({{\"error_type\":\"runtime_error\",\"message\": format!(\"input {i} deserialization failed: {{}}\", __err)}})), 0.0),\n"
                ));
                h.push_str("                };\n");
                h.push_str(&format!(
                    "                let {clean_name} = {ctor}({clean_name}_inner);\n"
                ));
            } else if let Some(mapping) = owned_type_for_ref(ty) {
                h.push_str(&format!(
                    "                let {clean_name}_owned: {} = match serde_json::from_value(inputs[{i}].clone()) {{\n",
                    mapping.deser_type
                ));
                h.push_str("                    Ok(value) => value,\n");
                h.push_str(&format!(
                    "                    Err(__err) => break 'shatter_arm (None, Some(serde_json::json!({{\"error_type\":\"runtime_error\",\"message\": format!(\"input {i} deserialization failed: {{}}\", __err)}})), 0.0),\n"
                ));
                h.push_str("                };\n");
                if mapping.needs_slice_conversion {
                    h.push_str(&format!(
                        "                let {clean_name}_refs: Vec<&str> = {clean_name}_owned.iter().map(|s| s.as_str()).collect();\n"
                    ));
                }
            } else {
                h.push_str(&format!(
                    "                let {clean_name}: {ty} = match serde_json::from_value(inputs[{i}].clone()) {{\n"
                ));
                h.push_str("                    Ok(value) => value,\n");
                h.push_str(&format!(
                    "                    Err(__err) => break 'shatter_arm (None, Some(serde_json::json!({{\"error_type\":\"runtime_error\",\"message\": format!(\"input {i} deserialization failed: {{}}\", __err)}})), 0.0),\n"
                ));
                h.push_str("                };\n");
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
        // Map result. Axum handlers commonly return wrappers without
        // `Serialize`, so skip return-value capture for handler shapes.
        h.push_str("                match result {\n");
        if return_type.is_some() && !axum_shape {
            h.push_str("                    Ok(ref v) => (Some(serde_json::to_value(v).unwrap_or(Value::Null)), None, wt),\n");
        } else if return_type.is_some() {
            h.push_str("                    Ok(_) => (Some(Value::Null), None, wt),\n");
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
    needs_shatter_rust: bool,
    mut timing: Option<&mut TimingCollector>,
) -> Result<PersistentHarness, ExecuteError> {
    let src_dir = harness_dir.join("src");
    std::fs::create_dir_all(&src_dir)?;

    let cargo_toml =
        generate_cargo_toml(runtime_path, needs_tokio, needs_axum, needs_shatter_rust);
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
/// - Uses a per-harness target dir for bin-only harnesses and a shared target
///   dir for crate-bridge harnesses
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
    let package_name = extract_crate_name(cargo_toml_content)
        .unwrap_or_else(|| "shatter-exec-temp".to_string());
    let binary_name = cargo_binary_name(&package_name);
    let target_dir = crate_bridge_target_dir(harness_dir);
    let cargo_binary_path = target_dir.join(profile_dir).join(&binary_name);
    let binary_path = if target_dir == harness_dir.join("target") {
        cargo_binary_path.clone()
    } else {
        local_harness_binary_path(harness_dir, profile_dir, &binary_name)
    };

    // Skip recompile if binary exists and generated build inputs are unchanged.
    let main_rs_path = src_dir.join("main.rs");
    let cargo_toml_path = harness_dir.join("Cargo.toml");
    let already_built = cached_harness_matches(
        &binary_path,
        &main_rs_path,
        harness_source,
        &cargo_toml_path,
        cargo_toml_content,
    );

    if !already_built {
        std::fs::write(&cargo_toml_path, cargo_toml_content)?;
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
        copy_cargo_binary_to_harness(&cargo_binary_path, &binary_path)?;
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

fn crate_bridge_target_dir(harness_dir: &Path) -> PathBuf {
    if let Some(cache_root) = harness_cache_root() {
        let bridge_root = cache_root.join("rust").join("crate-bridge");
        if harness_dir.starts_with(&bridge_root) {
            return bridge_root.join("target");
        }
    }
    harness_dir.join("target")
}

fn cargo_binary_name(package_name: &str) -> String {
    if cfg!(windows) {
        format!("{package_name}.exe")
    } else {
        package_name.to_string()
    }
}

fn local_harness_binary_path(harness_dir: &Path, profile_dir: &str, binary_name: &str) -> PathBuf {
    harness_dir.join("bin").join(profile_dir).join(binary_name)
}

fn copy_cargo_binary_to_harness(
    cargo_binary_path: &Path,
    harness_binary_path: &Path,
) -> Result<(), ExecuteError> {
    if cargo_binary_path == harness_binary_path {
        return Ok(());
    }
    if let Some(parent) = harness_binary_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(cargo_binary_path, harness_binary_path).map_err(ExecuteError::IoError)?;
    Ok(())
}

fn crate_bridge_driver_package_name(prefix: &str, harness_dir: &Path) -> String {
    let suffix = harness_dir
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_else(|| "harness".into())
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();
    let suffix = suffix.trim_matches('-');
    if suffix.is_empty() {
        prefix.to_string()
    } else {
        format!("{prefix}-{suffix}")
    }
}

/// Create an isolated staging copy of a user crate for crate_bridge compilation.
///
/// Copies the source tree, `Cargo.toml`, `build.rs`, and `.cargo/` to
/// `staging_root` so the crate_bridge driver can compile the staging copy
/// without ever touching the original files (str-ja70).
///
/// Returns the staging crate root path.
fn create_crate_staging_copy(
    crate_root: &Path,
    staging_root: &Path,
) -> io::Result<PathBuf> {
    let staging_crate = staging_root.join("crate-shadow");
    // Copy src/ tree recursively.
    let src_dir = crate_root.join("src");
    if src_dir.is_dir() {
        copy_dir_recursive(&src_dir, &staging_crate.join("src"))?;
    }
    // Copy Cargo.toml with relative paths resolved to absolute.
    let cargo_toml_path = crate_root.join("Cargo.toml");
    if cargo_toml_path.exists() {
        let content = std::fs::read_to_string(&cargo_toml_path)?;
        let ws_pkg = if has_workspace_inheritance(&content) {
            find_workspace_root(crate_root)
                .and_then(|ws| extract_workspace_package_section(&ws))
        } else {
            None
        };
        let resolved = resolve_cargo_toml_paths(&content, crate_root, ws_pkg.as_deref());
        std::fs::create_dir_all(&staging_crate)?;
        std::fs::write(staging_crate.join("Cargo.toml"), resolved)?;
    }
    // Copy build.rs if it exists.
    let build_rs = crate_root.join("build.rs");
    if build_rs.exists() {
        std::fs::copy(&build_rs, staging_crate.join("build.rs"))?;
    }
    // Copy .cargo/ config if it exists.
    let dot_cargo = crate_root.join(".cargo");
    if dot_cargo.is_dir() {
        copy_dir_recursive(&dot_cargo, &staging_crate.join(".cargo"))?;
    }
    // str-gc0r: copy non-Rust compile-time assets referenced by macros like
    // `include_str!`, `include_bytes!`, and `sqlx::migrate!`. Without these the
    // shadow fails to compile even though the original crate compiles fine.
    copy_compile_time_assets(crate_root, &staging_crate)?;
    Ok(staging_crate)
}

/// str-gc0r: scan all `.rs` files under the crate root for compile-time macros
/// that pull in external files, and copy those files/directories into the
/// staging crate at the same relative location.
///
/// Supported patterns:
/// - `include_str!("path")` / `include_bytes!("path")` — relative to the `.rs` file
/// - `sqlx::migrate!("./path")` / `sqlx::migrate!()` — relative to `CARGO_MANIFEST_DIR`
///
/// Paths that escape the crate root, that don't exist, or that already live
/// under `src/` (already copied) are skipped silently.
fn copy_compile_time_assets(crate_root: &Path, staging_crate: &Path) -> io::Result<()> {
    let src_dir = crate_root.join("src");
    if !src_dir.is_dir() {
        return Ok(());
    }
    let crate_root_canon = crate_root.canonicalize().unwrap_or_else(|_| crate_root.to_path_buf());

    let mut copied: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
    let mut copy_asset = |abs: &Path| -> io::Result<()> {
        // Canonicalize for escape check; if canonicalization fails (e.g. broken
        // symlink) treat as absent and skip.
        let canon = match abs.canonicalize() {
            Ok(p) => p,
            Err(_) => return Ok(()),
        };
        if !canon.starts_with(&crate_root_canon) {
            return Ok(()); // escapes crate root — skip
        }
        let rel = match canon.strip_prefix(&crate_root_canon) {
            Ok(r) => r.to_path_buf(),
            Err(_) => return Ok(()),
        };
        // src/ already copied wholesale.
        if rel.starts_with("src") {
            return Ok(());
        }
        if !copied.insert(rel.clone()) {
            return Ok(());
        }
        let dst = staging_crate.join(&rel);
        if canon.is_dir() {
            copy_dir_recursive(&canon, &dst)?;
        } else if canon.is_file() {
            if let Some(parent) = dst.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(&canon, &dst)?;
        }
        Ok(())
    };

    // Walk all .rs files under src/.
    let mut stack = vec![src_dir.clone()];
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|s| s.to_str()) == Some("rs") {
                let content = match std::fs::read_to_string(&path) {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                let file_dir = path.parent().unwrap_or(&src_dir);
                for asset in find_compile_time_asset_paths(&content) {
                    let base = match asset.base {
                        AssetBase::File => file_dir.to_path_buf(),
                        AssetBase::Manifest => crate_root.to_path_buf(),
                    };
                    let abs = base.join(&asset.path);
                    copy_asset(&abs)?;
                }
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AssetBase {
    /// Path is relative to the file containing the macro (include_str!, include_bytes!).
    File,
    /// Path is relative to CARGO_MANIFEST_DIR (sqlx::migrate!).
    Manifest,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AssetRef {
    path: String,
    base: AssetBase,
}

/// Find compile-time asset path arguments in source text. Best-effort textual
/// scan — handles the common shapes:
///   include_str!("path")    include_bytes!("path")    sqlx::migrate!("path")
///   sqlx::migrate!()        — implicit "./migrations"
fn find_compile_time_asset_paths(content: &str) -> Vec<AssetRef> {
    let mut out = Vec::new();
    for (macro_name, base) in [
        ("include_str!", AssetBase::File),
        ("include_bytes!", AssetBase::File),
        ("sqlx::migrate!", AssetBase::Manifest),
    ] {
        let mut search_from = 0;
        while let Some(idx) = content[search_from..].find(macro_name) {
            let abs_idx = search_from + idx;
            search_from = abs_idx + macro_name.len();
            // Skip if this is part of a longer identifier (e.g. `my_include_str!`).
            if abs_idx > 0 {
                let prev = content.as_bytes()[abs_idx - 1];
                if prev.is_ascii_alphanumeric() || prev == b'_' {
                    continue;
                }
            }
            // After the macro name we expect optional whitespace then `(`.
            let rest = &content[search_from..];
            let rest = rest.trim_start();
            let after_paren = match rest.strip_prefix('(') {
                Some(s) => s,
                None => continue,
            };
            let after_paren = after_paren.trim_start();
            // sqlx::migrate!() with no arg defaults to "./migrations".
            if after_paren.starts_with(')') && matches!(base, AssetBase::Manifest) {
                out.push(AssetRef { path: "migrations".to_string(), base });
                continue;
            }
            // Otherwise look for a double-quoted string literal.
            let body = match after_paren.strip_prefix('"') {
                Some(s) => s,
                None => continue,
            };
            if let Some(end) = body.find('"') {
                let path = &body[..end];
                if !path.is_empty() {
                    out.push(AssetRef { path: path.to_string(), base });
                }
            }
        }
    }
    out
}

/// Recursively copy a directory tree.
fn copy_dir_recursive(src: &Path, dst: &Path) -> io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

/// Walk up from `crate_root` to find the nearest workspace root — a directory
/// whose `Cargo.toml` contains a `[workspace]` section.
fn find_workspace_root(crate_root: &Path) -> Option<PathBuf> {
    let mut dir = crate_root.parent()?;
    loop {
        let cargo_toml = dir.join("Cargo.toml");
        if cargo_toml.exists()
            && let Ok(content) = std::fs::read_to_string(&cargo_toml)
        {
            for line in content.lines() {
                let t = line.trim();
                if t == "[workspace]" || t.starts_with("[workspace]") {
                    return Some(dir.to_path_buf());
                }
            }
        }
        if dir.parent().is_none_or(|p| p == dir) {
            return None;
        }
        dir = dir.parent().unwrap();
    }
}

/// Extract the `[workspace.package]` section from a workspace root Cargo.toml.
/// Returns lines like `edition = "2021"\nrust-version = "1.70"\n`.
fn extract_workspace_package_section(workspace_root: &Path) -> Option<String> {
    let content = std::fs::read_to_string(workspace_root.join("Cargo.toml")).ok()?;
    let mut in_section = false;
    let mut result = String::new();
    for line in content.lines() {
        let t = line.trim();
        if t == "[workspace.package]" {
            in_section = true;
            continue;
        }
        if in_section {
            if t.starts_with('[') {
                break;
            }
            if !t.is_empty() {
                result.push_str(line);
                result.push('\n');
            }
        }
    }
    if result.is_empty() { None } else { Some(result) }
}

/// Check whether a Cargo.toml manifest uses `*.workspace = true` inheritance.
fn has_workspace_inheritance(content: &str) -> bool {
    content.lines().any(|line| {
        let t = line.trim();
        t.ends_with(".workspace = true") || t.contains("workspace = true")
    })
}

/// Resolve relative `path = "..."` entries in a Cargo.toml to absolute paths
/// based on `crate_root`, and inject `[workspace]` to prevent workspace
/// auto-discovery from the staging location. When `ws_pkg_fields` is provided,
/// includes a `[workspace.package]` section so inherited fields resolve.
fn resolve_cargo_toml_paths(content: &str, crate_root: &Path, ws_pkg_fields: Option<&str>) -> String {
    let mut result = String::with_capacity(content.len() + 256);
    let mut has_workspace = false;
    // Track whether we are inside a dependency section where `path = "..."`
    // refers to a local crate dependency (should be absolutised) vs a
    // non-dep section like `[lib]` or `[[bin]]` where `path` is a file
    // relative to the crate root (should stay relative so the staging copy
    // resolves its own copy).
    let mut in_dep_section = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "[workspace]" || trimmed.starts_with("[workspace]") {
            has_workspace = true;
        }
        // Detect section headers.
        if trimmed.starts_with('[') {
            in_dep_section = trimmed.contains("dependencies");
        }
        // Resolve relative path deps: `path = "..."` or `path = '...'`
        // Only inside [dependencies], [dev-dependencies], [build-dependencies],
        // or inline dep tables — NOT in [lib], [[bin]], etc.
        if in_dep_section
            && let Some(idx) = trimmed.find("path") {
                let after_path = trimmed[idx + 4..].trim();
                if let Some(rest) = after_path.strip_prefix('=') {
                    let rest = rest.trim();
                    let (quote, path_val) = if let Some(s) = rest.strip_prefix('"') {
                        ('"', s.split('"').next().unwrap_or(""))
                    } else if let Some(s) = rest.strip_prefix('\'') {
                        ('\'', s.split('\'').next().unwrap_or(""))
                    } else {
                        // Not a quoted path, pass through.
                        result.push_str(line);
                        result.push('\n');
                        continue;
                    };
                    let dep_path = Path::new(path_val);
                    if dep_path.is_relative() && !path_val.is_empty() {
                        let abs = crate_root.join(dep_path);
                        let abs_str = abs.display().to_string().replace('\\', "/");
                        let new_line = line.replacen(
                            &format!("{quote}{path_val}{quote}"),
                            &format!("{quote}{abs_str}{quote}"),
                            1,
                        );
                        result.push_str(&new_line);
                        result.push('\n');
                        continue;
                    }
                }
        }
        result.push_str(line);
        result.push('\n');
    }
    // Add [workspace] if not present, to isolate the staging copy from
    // any workspace root above the original crate.
    if !has_workspace {
        if let Some(fields) = ws_pkg_fields {
            result.push_str("\n[workspace]\nmembers = []\n\n[workspace.package]\n");
            result.push_str(fields);
        } else {
            result.push_str("\n[workspace]\nmembers = []\n");
        }
    }
    result
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
    w.push_str("use super::*;\n\n");

    // Per-function wrapper: deserialise inputs, call via super::, return JSON.
    for fn_info in fns {
        let fn_name = &fn_info.name;
        let param_names = &fn_info.param_names;
        let param_types = &fn_info.param_types;
        let return_type = &fn_info.return_type;

        w.push_str(&format!(
            "pub fn shatter_wrap_{fn_name}(inputs: Vec<Value>) -> Value {{\n"
        ));

        // Axum `State<T>` extractors are not constructible by the generic
        // wrapper (no `Deserialize` impl). Short-circuit with a clear
        // not-supported error instead of emitting `from_value::<State<T>>`.
        if let Some(reason) = axum_state_unsupported_reason(param_types) {
            w.push_str("    let _ = inputs;\n");
            w.push_str(&format!(
                "    return serde_json::json!({{\"return_value\": null, \"thrown_error\": {{\"error_type\": \"not_supported\", \"message\": {:?}}}, \"branch_path\": [], \"lines_executed\": [], \"calls_to_external\": [], \"path_constraints\": [], \"side_effects\": [], \"performance\": {{\"wall_time_ms\": 0.0, \"cpu_time_us\": 0, \"heap_used_bytes\": 0, \"heap_allocated_bytes\": 0}}}});\n",
                reason
            ));
            w.push_str("}\n\n");
            continue;
        }
        let axum_shape = is_axum_handler_shape(param_types)
            || return_type.as_ref().is_some_and(|ty| is_axum_return_shape(ty));

        for (i, (name, ty)) in param_names.iter().zip(param_types.iter()).enumerate() {
            let clean = name.strip_prefix("mut ").unwrap_or(name).trim();
            if let Some(ext) = classify_axum_extractor(ty) {
                let inner = match &ext {
                    AxumExtractor::Path(t) | AxumExtractor::Query(t) | AxumExtractor::Json(t) => t.clone(),
                AxumExtractor::State(_) => unreachable!("State handled above"),
                };
                let ctor = axum_extractor_constructor(&ext);
                w.push_str(&format!(
                    "    let {clean}_inner: {inner} = match serde_json::from_value(inputs[{i}].clone()) {{\n"
                ));
                w.push_str("        Ok(value) => value,\n");
                w.push_str(&format!(
                    "        Err(__err) => return serde_json::json!({{\"return_value\": null, \"thrown_error\": {{\"error_type\": \"runtime_error\", \"message\": format!(\"input {i} deserialization failed: {{}}\", __err)}}, \"branch_path\": [], \"lines_executed\": [], \"calls_to_external\": [], \"path_constraints\": [], \"side_effects\": [], \"performance\": {{\"wall_time_ms\": 0.0, \"cpu_time_us\": 0, \"heap_used_bytes\": 0, \"heap_allocated_bytes\": 0}}}}),\n"
                ));
                w.push_str("    };\n");
                w.push_str(&format!("    let {clean} = {ctor}({clean}_inner);\n"));
            } else if let Some(mapping) = owned_type_for_ref(ty) {
                w.push_str(&format!(
                    "    let {clean}_owned: {} = match serde_json::from_value(inputs[{i}].clone()) {{\n",
                    mapping.deser_type
                ));
                w.push_str("        Ok(value) => value,\n");
                w.push_str(&format!(
                    "        Err(__err) => return serde_json::json!({{\"return_value\": null, \"thrown_error\": {{\"error_type\": \"runtime_error\", \"message\": format!(\"input {i} deserialization failed: {{}}\", __err)}}, \"branch_path\": [], \"lines_executed\": [], \"calls_to_external\": [], \"path_constraints\": [], \"side_effects\": [], \"performance\": {{\"wall_time_ms\": 0.0, \"cpu_time_us\": 0, \"heap_used_bytes\": 0, \"heap_allocated_bytes\": 0}}}}),\n"
                ));
                w.push_str("    };\n");
                if mapping.needs_slice_conversion {
                    w.push_str(&format!(
                        "    let {clean}_refs: Vec<&str> = {clean}_owned.iter().map(|s| s.as_str()).collect();\n"
                    ));
                }
            } else {
                w.push_str(&format!(
                    "    let {clean}: {ty} = match serde_json::from_value(inputs[{i}].clone()) {{\n"
                ));
                w.push_str("        Ok(value) => value,\n");
                w.push_str(&format!(
                    "        Err(__err) => return serde_json::json!({{\"return_value\": null, \"thrown_error\": {{\"error_type\": \"runtime_error\", \"message\": format!(\"input {i} deserialization failed: {{}}\", __err)}}, \"branch_path\": [], \"lines_executed\": [], \"calls_to_external\": [], \"path_constraints\": [], \"side_effects\": [], \"performance\": {{\"wall_time_ms\": 0.0, \"cpu_time_us\": 0, \"heap_used_bytes\": 0, \"heap_allocated_bytes\": 0}}}}),\n"
                ));
                w.push_str("    };\n");
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

        if return_type.is_some() && !axum_shape {
            w.push_str("    match result {\n");
            w.push_str("        Ok(ref ret_val) => { obj.insert(\"return_value\".into(), serde_json::to_value(ret_val).unwrap_or(Value::Null)); }\n");
        } else if return_type.is_some() {
            // Axum handler shape: return type may be `axum::Json<T>` or
            // `Result<Json<T>, ProjectError>` without a `Serialize` bound on
            // the outer wrapper. Skip return capture and emit Null.
            w.push_str("    match result {\n");
            w.push_str("        Ok(_) => { obj.insert(\"return_value\".into(), Value::Null); }\n");
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

/// Marker lines bracketing the appended in-module wrapper inside the target
/// source file. Used by `strip_shatter_wrapper` to remove a prior wrapper
/// idempotently before re-instrumenting.
const SHATTER_WRAPPER_BEGIN: &str = "// __SHATTER_WRAPPER_BEGIN__";
const SHATTER_WRAPPER_END: &str = "// __SHATTER_WRAPPER_END__";

/// Remove any previously appended `__SHATTER_WRAPPER_BEGIN__ … END__` block(s)
/// from `source`. Leaves source unchanged if no markers are present.
fn strip_shatter_wrapper(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut rest = source;
    while let Some(b) = rest.find(SHATTER_WRAPPER_BEGIN) {
        out.push_str(
            rest[..b].trim_end_matches(['\n', '\r', ' ', '\t']),
        );
        let after_begin = b + SHATTER_WRAPPER_BEGIN.len();
        if let Some(e_rel) = rest[after_begin..].find(SHATTER_WRAPPER_END) {
            let after_end = after_begin + e_rel + SHATTER_WRAPPER_END.len();
            rest = match rest[after_end..].find('\n') {
                Some(nl) => &rest[after_end + nl + 1..],
                None => "",
            };
        } else {
            return source.to_string();
        }
    }
    out.push_str(rest);
    out
}

/// Generate the in-module wrapper block appended to the target source file.
///
/// The wrapper is a submodule (`mod __shatter_inner`) inside the target
/// function's parent module. Because the wrapper lives in that module's
/// scope, `use super::*` brings in module-local imports (e.g. `use
/// crate::Config`) and `super::<fn>` resolves private helpers — fixing
/// E0425 "cannot find function `…` in module `super`" errors that the
/// previous root-level wrapper produced for functions defined outside
/// lib.rs (str-31j.3).
///
/// The wrapper exports `#[no_mangle] pub extern "C" fn
/// shatter_crate_bridge_entry`. The crate-root `__shatter.rs` stub
/// declares the FFI symbol and forwards `shatter_run_harness()` to it,
/// preserving the existing driver-binary entry point.
fn generate_in_module_wrapper(
    fns: &[CompatFn],
    mocks_json: &str,
    static_mut_names: &[String],
) -> String {
    let body = generate_crate_bridge_wrapper(fns, mocks_json, static_mut_names);
    let mut w = String::with_capacity(body.len() + 512);
    w.push('\n');
    w.push_str(SHATTER_WRAPPER_BEGIN);
    w.push('\n');
    w.push_str("#[cfg(feature = \"shatter-crate-bridge\")]\n");
    w.push_str("#[allow(non_snake_case, dead_code, unused_imports, unused_variables, clippy::all)]\n");
    w.push_str("mod __shatter_inner {\n");
    w.push_str(&body);
    w.push_str("\n#[no_mangle]\n");
    w.push_str("pub extern \"C\" fn shatter_crate_bridge_entry() {\n");
    w.push_str("    shatter_run_harness();\n");
    w.push_str("}\n");
    w.push_str("}\n");
    w.push_str(SHATTER_WRAPPER_END);
    w.push('\n');
    w
}

/// Generate the crate-root `__shatter.rs` stub. The stub declares the FFI
/// symbol exported by the in-module wrapper and forwards
/// `shatter_run_harness()` to it. The driver binary keeps calling
/// `<crate>::__shatter::shatter_run_harness()` unchanged.
fn generate_crate_bridge_root_stub() -> String {
    "// Generated by shatter-rust crate_bridge — do not edit\n\
     #![allow(unused_imports, dead_code, clippy::all)]\n\
     unsafe extern \"C\" {\n\
         fn shatter_crate_bridge_entry();\n\
     }\n\
     \n\
     pub fn shatter_run_harness() {\n\
         unsafe { shatter_crate_bridge_entry(); }\n\
     }\n"
        .to_string()
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
fn generate_crate_bridge_cargo_toml(
    driver_package_name: &str,
    crate_name: &str,
    crate_root: &Path,
    needs_tokio: bool,
) -> String {
    let crate_path = crate_root.display().to_string().replace('\\', "/");
    let tokio_dep = if needs_tokio {
        "tokio = { version = \"1\", features = [\"full\"] }\n"
    } else {
        ""
    };
    format!(
        r#"[package]
name = "{driver_package_name}"
version = "0.1.0"
edition = "2021"

[workspace]
exclude = ["crate-shadow"]

[profile.dev]
debug = 0
incremental = false

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
    let package_name = extract_crate_name(cargo_toml_content)
        .unwrap_or_else(|| "shatter-crate-bridge-exec".to_string());
    let binary_name = cargo_binary_name(&package_name);
    let target_dir = crate_bridge_target_dir(harness_dir);
    let cargo_binary_path = target_dir.join(profile_dir).join(&binary_name);
    let binary_path = local_harness_binary_path(harness_dir, profile_dir, &binary_name);

    let main_rs_path = src_dir.join("main.rs");
    let cargo_toml_path = harness_dir.join("Cargo.toml");
    let already_built = cached_harness_matches(
        &binary_path,
        &main_rs_path,
        driver_source,
        &cargo_toml_path,
        cargo_toml_content,
    );

    if !already_built {
        std::fs::write(&cargo_toml_path, cargo_toml_content)?;
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
        copy_cargo_binary_to_harness(&cargo_binary_path, &binary_path)?;
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
    // Canonicalise crate_root so the generated harness Cargo.toml carries an
    // absolute `path = "..."` for the user crate. A relative crate_root (e.g.
    // `api/`, when the user invoked Shatter from a workspace root) would
    // otherwise be resolved by Cargo relative to the harness Cargo.toml, which
    // lives in the per-project harness cache and contains no such subdir. See
    // str-iqqo.
    let crate_root_buf = std::fs::canonicalize(crate_root)
        .unwrap_or_else(|_| to_absolute(crate_root.to_path_buf()));
    let crate_root = crate_root_buf.as_path();
    let raw_source = std::fs::read_to_string(file_path)
        .map_err(|e| ExecuteError::FileError(format!("cannot read {file_path}: {e}")))?;
    // Strip any wrapper left by a previous invocation so we re-parse a clean
    // source. The wrapper is appended to the target file (str-31j.3) and
    // would otherwise accumulate or confuse the syn parser on re-runs.
    let source = strip_shatter_wrapper(&raw_source);
    let mh = mocks_hash(mocks);

    // Collect all functions so we can locate the requested target. The
    // crate_bridge wrapper is intentionally built for one target function at a
    // time: one non-JSON-compatible sibling in the same file must not poison
    // execution for every other function in the dispatch harness.
    let all_fn_ctxs = extract_all_fn_contexts(&source);
    let static_mut_names = extract_static_mut_items(&source);

    let Some((_, ctx)) = all_fn_ctxs.iter().find(|(n, _)| n == function_name) else {
        return Err(ExecuteError::NonExecutable(format!(
            "function `{function_name}` not found in `{file_path}`"
        )));
    };
    if ctx.sig.has_generics {
        return Err(ExecuteError::NonExecutable(format!(
            "crate_bridge: function `{function_name}` has generic type parameters — cannot deserialise concrete inputs"
        )));
    }
    let fn_info = CompatFn {
        name: function_name.to_string(),
        param_names: ctx.sig.param_names.clone(),
        param_types: ctx.sig.param_types.clone(),
        return_type: ctx.sig.return_type.clone(),
        is_async: ctx.sig.is_async,
    };
    if let Some(reason) = crate_bridge_unsupported_reason(&fn_info) {
        return Err(ExecuteError::NonExecutable(format!(
            "crate_bridge: function `{function_name}` is not JSON-harness compatible: {reason}"
        )));
    }
    let expected_inputs = fn_info.param_names.len();
    let compatible_fns = vec![fn_info];
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

    // In-module wrapper appended to the target file so it sees module-local
    // imports and private items (str-31j.3). The crate-root `__shatter.rs` is
    // a small FFI stub that forwards to the in-module entry symbol.
    let in_module_wrapper = generate_in_module_wrapper(&compatible_fns, &mocks_json, &static_mut_names);
    let root_stub = generate_crate_bridge_root_stub();
    let wrapper_hash = source_hash(&format!("{in_module_wrapper}{root_stub}"));

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

    // Slow path: build driver binary against an isolated staging copy of the
    // user crate. The original source files are never modified (str-ja70).
    let user_cargo_toml_path = crate_root.join("Cargo.toml");
    let user_cargo_toml = std::fs::read_to_string(&user_cargo_toml_path)
        .map_err(|e| ExecuteError::FileError(format!("cannot read Cargo.toml: {e}")))?;

    let crate_name = extract_crate_name(&user_cargo_toml).unwrap_or_else(|| "user_crate".to_string());
    let runtime_path = find_runtime_crate_path()?;

    // Verify the user crate is a library before doing any I/O.
    let _lib_rs = find_lib_rs(crate_root).ok_or_else(|| ExecuteError::NonExecutable(
        "crate_bridge: cannot find lib.rs — only library crates are supported".to_string(),
    ))?;

    let harness_dir = stable_crate_bridge_dir(crate_root, wrapper_hash, mh);
    std::fs::create_dir_all(&harness_dir)?;

    // Create an isolated staging copy of the user crate. All mutations
    // (instrumented source, wrapper module, Cargo.toml feature injection)
    // happen inside this copy, leaving the original tree untouched (str-ja70).
    let staging_crate = create_crate_staging_copy(crate_root, &harness_dir)
        .map_err(|e| ExecuteError::IoError(
            io::Error::other(format!("cannot create staging copy: {e}"))
        ))?;

    // Map `file_path` from original crate to staging copy.
    let rel_file = Path::new(file_path).strip_prefix(crate_root)
        .unwrap_or(Path::new(file_path));
    let staging_file = staging_crate.join(rel_file);
    let staging_lib_rs = find_lib_rs(&staging_crate).ok_or_else(|| ExecuteError::NonExecutable(
        "crate_bridge: cannot find lib.rs in staging copy".to_string(),
    ))?;
    let staging_shatter_mod = staging_crate.join("src").join("__shatter.rs");
    let staging_cargo_toml = staging_crate.join("Cargo.toml");

    // Write the instrumented source + in-module wrapper to the staging copy.
    let mut target_contents = instr_result.source.clone();
    if !target_contents.ends_with('\n') {
        target_contents.push('\n');
    }
    target_contents.push_str(&in_module_wrapper);
    if let Some(parent) = staging_file.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| ExecuteError::IoError(io::Error::other(format!("cannot create staging directories: {e}"))))?;
    }
    std::fs::write(&staging_file, &target_contents)
        .map_err(|e| ExecuteError::IoError(io::Error::other(format!("cannot write instrumented source to staging: {e}"))))?;

    // Write the FFI stub at staging crate root.
    std::fs::write(&staging_shatter_mod, &root_stub)
        .map_err(ExecuteError::IoError)?;

    // Inject mod declaration into staging lib.rs (idempotent).
    inject_lib_module_declaration(&staging_lib_rs)?;

    // Inject feature + deps into staging Cargo.toml (idempotent).
    inject_crate_bridge_feature(&staging_cargo_toml, &runtime_path)?;

    let needs_tokio = compatible_fns.iter().any(|f| f.is_async);
    let driver_source = generate_crate_bridge_bin(&crate_name.replace('-', "_"));
    let driver_package_name =
        crate_bridge_driver_package_name("shatter-crate-bridge-exec", &harness_dir);
    let driver_cargo_toml =
        generate_crate_bridge_cargo_toml(&driver_package_name, &crate_name, &staging_crate, needs_tokio);

    // Shadow `timing` as mutable so we can `take()` it inside the branch.
    let mut timing = timing;

    let harness_result = if let Some(t) = timing.take() {
        t.record("execute.build", |t| {
            build_and_spawn_crate_bridge_harness(&driver_source, &driver_cargo_toml, &harness_dir, Some(t))
        })
    } else {
        build_and_spawn_crate_bridge_harness(&driver_source, &driver_cargo_toml, &harness_dir, None)
    };
    let mut harness = match harness_result {
        Ok(harness) => harness,
        Err(ExecuteError::CompilationFailed(msg)) => {
            let _ = std::fs::remove_dir_all(&harness_dir);
            if let Some(reason) = crate_bridge_serde_bound_failure_reason(&msg) {
                return Err(ExecuteError::NonExecutable(format!(
                    "crate_bridge: function `{function_name}` is not JSON-harness compatible: {reason}"
                )));
            }
            return Err(ExecuteError::CompilationFailed(msg));
        }
        Err(err) => {
            let _ = std::fs::remove_dir_all(&harness_dir);
            return Err(err);
        }
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
    let cargo_toml_content =
        generate_cargo_toml_with_user_deps(&user_cargo_toml, &runtime_path, needs_tokio, false, false);

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
        let result = execute_function_crate_backed(
            file_path, function_name, &dummy_inputs, mocks,
            timeout_ms, None, crate_cache, &crate_root,
        );
        match result {
            Ok(_) => {}
            Err(ExecuteError::NonExecutable(_)) if harness_mode.is_none() => {
                let _ = execute_function_crate_bridge(
                    file_path,
                    function_name,
                    &dummy_inputs,
                    mocks,
                    timeout_ms,
                    None,
                    bridge_cache,
                    &crate_root,
                )?;
            }
            Err(err) => return Err(err),
        }
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
    // If bin_only cannot represent the source layout, retry through crate_bridge
    // unless the caller explicitly requested a harness mode.
    if let Some(crate_root) = find_crate_root(file_path) {
        let result = if let Some(timing) = timing.as_mut() {
            execute_function_crate_backed(
                file_path,
                function_name,
                inputs,
                mocks,
                timeout_ms,
                Some(&mut **timing),
                crate_cache,
                &crate_root,
            )
        } else {
            execute_function_crate_backed(
                file_path,
                function_name,
                inputs,
                mocks,
                timeout_ms,
                None,
                crate_cache,
                &crate_root,
            )
        };

        match result {
            Ok(result) => return Ok(result),
            Err(ExecuteError::NonExecutable(_)) if harness_mode.is_none() => {
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
            Err(err) => return Err(err),
        }
    }

    // Compute cache key before doing any expensive work.
    let native_replays = native_replay_specs(inputs)?;
    let native_replay_hash = native_replay_hash(&native_replays);
    let key = HarnessKey {
        file_path: file_path.to_string(),
        function_name: function_name.to_string(),
        mocks_hash: mocks_hash(mocks),
        native_replay_hash,
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
                &native_replays,
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
            &native_replays,
            sig.is_async,
        )?
    };

    let harness_dir = make_harness_dir();
    std::fs::create_dir_all(&harness_dir)?;

    let mut harness = if let Some(timing) = timing {
        timing.record("execute.build", |timing| {
            build_and_spawn_harness(
                &harness_source,
                &harness_dir,
                &runtime_path,
                sig.is_async,
                false,
                false,
                Some(timing),
            )
        })?
    } else {
        build_and_spawn_harness(
            &harness_source,
            &harness_dir,
            &runtime_path,
            sig.is_async,
            false,
            false,
            None,
        )?
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
    param_types: &[String],
    native_replays: &[Option<NativeReplaySpec>],
    mocks_json: &str,
    type_sources: &[(Option<&str>, &str)],
) -> Result<String, ExecuteError> {
    let module_block = wrap_in_module(instrumented_source)?;
    let mut h = String::with_capacity(8192);

    h.push_str("#![allow(unused_imports)]\n");
    h.push_str("use serde_json::Value;\n");
    h.push_str("use axum::{Router, routing};\n");
    h.push_str("use tower::ServiceExt;\n");
    h.push_str("use http_body_util::BodyExt;\n\n");
    if native_replays.iter().any(Option::is_some) {
        h.push_str(
            "extern crate self as shatter_rust;\npub mod generators {\n    pub struct GeneratorResult {\n        pub id: String,\n        pub value: Box<dyn std::any::Any + Send>,\n        pub recipe: serde_json::Value,\n    }\n}\n\n",
        );
    }
    h.push_str(&module_block);
    h.push_str("\nuse crate::user_code::*;\n");
    for spec in native_replays.iter().flatten() {
        let file_path = rust_string_literal(&spec.file_path.display().to_string());
        h.push_str(&format!(
            "\n#[path = {file_path}]\nmod {};\n",
            spec.module_name
        ));
    }
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
    let path_param_index = mappings
        .iter()
        .find(|m| m.kind == crate::adapters::AxumExtractorKind::PathParams)
        .map(|m| m.param_index);
    let path_inner_type = path_param_index
        .and_then(|idx| param_types.get(idx))
        .and_then(|ty| match classify_axum_extractor(ty) {
            Some(AxumExtractor::Path(inner)) => Some(inner),
            _ => None,
        });
    let default_path = path_inner_type
        .as_deref()
        .map(axum_route_pattern_for_path)
        .unwrap_or_else(|| "/test".to_string());
    if let Some(idx) = path_param_index {
        let default_path_value = path_inner_type
            .as_deref()
            .map(axum_default_path_value_for_path)
            .unwrap_or_else(|| "/test/p0".to_string());
        let default_path_value_literal = rust_string_literal(&default_path_value);
        h.push_str(&format!(
            "        let path_value = input_obj.get(\"path\").and_then(|v| v.as_str()).map(str::to_string).unwrap_or_else(|| {{\n            if let Some(value) = inputs.get({idx}) {{\n                if !value.is_null() {{\n                    if let Some(segment) = value.as_str() {{\n                        return format!(\"/test/{{}}\", segment);\n                    }}\n                    if let Some(segments) = value.as_array() {{\n                        let path_segments = segments.iter().filter_map(|segment| {{\n                            if segment.is_null() {{\n                                None\n                            }} else if let Some(segment) = segment.as_str() {{\n                                Some(segment.to_string())\n                            }} else {{\n                                Some(segment.to_string().trim_matches('\"').to_string())\n                            }}\n                        }}).collect::<Vec<_>>();\n                        if !path_segments.is_empty() {{\n                            return format!(\"/test/{{}}\", path_segments.join(\"/\"));\n                        }}\n                    }}\n                    return format!(\"/test/{{}}\", value.to_string().trim_matches('\"'));\n                }}\n            }}\n            {default_path_value_literal}.to_string()\n        }});\n"
        ));
    } else {
        h.push_str(&format!(
            "        let path_value = input_obj.get(\"path\").and_then(|v| v.as_str()).unwrap_or(\"{default_path}\").to_string();\n"
        ));
    }

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
    let json_param_index = mappings
        .iter()
        .find(|m| m.kind == crate::adapters::AxumExtractorKind::JsonBody)
        .map(|m| m.param_index);
    let default_body_json = json_param_index
        .and_then(|idx| param_types.get(idx))
        .and_then(|ty| match classify_axum_extractor(ty) {
            Some(AxumExtractor::Json(inner)) => Some(axum_json_default_value(type_sources, &inner)),
            _ => None,
        })
        .unwrap_or(Value::Null);
    let default_body_json = serde_json::to_string(&default_body_json).map_err(|e| {
        ExecuteError::InstrumentError(format!("cannot serialize default axum JSON body: {e}"))
    })?;
    let default_body_json_literal = rust_string_literal(&default_body_json);
    if let Some(idx) = json_param_index {
        h.push_str(&format!(
            "        let body_json = input_obj.get(\"body\").cloned().unwrap_or_else(|| inputs.get({idx}).cloned().unwrap_or(Value::Null));\n"
        ));
        h.push_str(&format!(
            "        let body_json = if body_json.is_null() {{ serde_json::from_str::<Value>({default_body_json_literal}).unwrap_or(Value::Null) }} else {{ body_json }};\n"
        ));
        h.push_str("        let body_bytes = axum::body::Body::from(serde_json::to_vec(&body_json).unwrap_or_default());\n\n");
    } else {
        h.push_str("        let body_json = input_obj.get(\"body\").cloned().unwrap_or(Value::Null);\n");
        h.push_str("        let body_bytes = if body_json.is_null() {\n");
        h.push_str("            axum::body::Body::empty()\n");
        h.push_str("        } else {\n");
        h.push_str("            axum::body::Body::from(serde_json::to_vec(&body_json).unwrap_or_default())\n");
        h.push_str("        };\n\n");
    }

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
    h.push_str("        let mut request = request.body(body_bytes).unwrap();\n\n");

    // Build the router with the handler.
    // Determine the route method based on the HTTP method.
    let has_state = mappings
        .iter()
        .any(|m| m.kind == crate::adapters::AxumExtractorKind::AppState);

    if has_state {
        let state_mapping = mappings
            .iter()
            .find(|m| m.kind == crate::adapters::AxumExtractorKind::AppState)
            .expect("has_state implies a state mapping");
        let idx = state_mapping.param_index;
        let state_type = param_types
            .get(idx)
            .and_then(|ty| match classify_axum_extractor(ty) {
                Some(AxumExtractor::State(inner)) => Some(inner),
                _ => None,
            })
            .ok_or_else(|| {
                ExecuteError::InstrumentError(format!(
                    "cannot determine axum state type for input {idx}"
                ))
            })?;
        if native_replays.get(idx).and_then(|spec| spec.as_ref()).is_none() {
            h.push_str(&format!(
                "        return shatter_rust_runtime::build_result_json(None, Some(serde_json::json!({{\"error_type\":\"not_supported\",\"message\":\"axum State<{state_type}> requires native replay input {idx}\"}})), 0.0, vec![]);\n"
            ));
            h.push_str("    });\n");
            h.push_str("}\n");
            return Ok(h);
        }
    }

    // Build route pattern from path extractor presence.
    let route_pattern = default_path.as_str();

    // Execute native replay and the Axum request inside the same Tokio runtime.
    h.push_str("\n        let __tokio_rt = tokio::runtime::Runtime::new().unwrap();\n");
    h.push_str("        let (result, wall_time_ms) = shatter_rust_runtime::execute_with_timing(std::panic::AssertUnwindSafe(|| {\n");
    h.push_str("            __tokio_rt.block_on(async move {\n");

    for mapping in mappings {
        if mapping.kind != crate::adapters::AxumExtractorKind::Unsupported {
            continue;
        }
        let Some(spec) = native_replays
            .get(mapping.param_index)
            .and_then(|spec| spec.as_ref())
        else {
            continue;
        };
        let Some(ty) = param_types.get(mapping.param_index) else {
            continue;
        };
        let recipe_json = serde_json::to_string(&spec.recipe).map_err(|e| {
            ExecuteError::InstrumentError(format!("cannot serialize native replay recipe: {e}"))
        })?;
        let recipe_literal = rust_string_literal(&recipe_json);
        let idx = mapping.param_index;
        h.push_str(&format!(
            "        let __extension_recipe_{idx}: serde_json::Value = serde_json::from_str({recipe_literal}).unwrap();\n"
        ));
        h.push_str(&format!(
            "        let __extension_generated_{idx} = {}::{}(Some(__extension_recipe_{idx}));\n",
            spec.module_name, spec.function_name
        ));
        h.push_str(&format!(
            "        let __extension_value_{idx}: {ty} = match __extension_generated_{idx}.value.downcast::<{ty}>() {{\n"
        ));
        h.push_str("            Ok(value) => *value,\n");
        h.push_str(&format!(
            "            Err(_) => return shatter_rust_runtime::build_result_json(None, Some(serde_json::json!({{\"error_type\":\"runtime_error\",\"message\": \"native replay downcast failed for axum extension input {idx}: expected {ty}\"}})), 0.0, vec![]),\n"
        ));
        h.push_str("        };\n");
        h.push_str(&format!(
            "        request.extensions_mut().insert(__extension_value_{idx});\n"
        ));
    }

    h.push_str(&format!(
        "        let app = Router::new().route(\"{route_pattern}\", routing::any(user_code::{function_name}));\n"
    ));

    // If State<T> is needed, deserialize state and attach via .with_state().
    if has_state {
        let state_mapping = mappings
            .iter()
            .find(|m| m.kind == crate::adapters::AxumExtractorKind::AppState)
            .expect("has_state implies a state mapping");
        let idx = state_mapping.param_index;
        let state_type = param_types
            .get(idx)
            .and_then(|ty| match classify_axum_extractor(ty) {
                Some(AxumExtractor::State(inner)) => Some(inner),
                _ => None,
            })
            .ok_or_else(|| {
                ExecuteError::InstrumentError(format!(
                    "cannot determine axum state type for input {idx}"
                ))
            })?;
        let spec = native_replays
            .get(idx)
            .and_then(|spec| spec.as_ref())
            .expect("validated axum state native replay");
        let recipe_json = serde_json::to_string(&spec.recipe).map_err(|e| {
            ExecuteError::InstrumentError(format!("cannot serialize native replay recipe: {e}"))
        })?;
        let recipe_literal = rust_string_literal(&recipe_json);
        h.push_str(&format!(
            "        let __state_recipe_{idx}: serde_json::Value = serde_json::from_str({recipe_literal}).unwrap();\n"
        ));
        h.push_str(&format!(
            "        let __state_generated_{idx} = {}::{}(Some(__state_recipe_{idx}));\n",
            spec.module_name, spec.function_name
        ));
        h.push_str(&format!(
            "        let __state_value_{idx}: {state_type} = match __state_generated_{idx}.value.downcast::<{state_type}>() {{\n"
        ));
        h.push_str("            Ok(value) => *value,\n");
        h.push_str(&format!(
            "            Err(_) => return shatter_rust_runtime::build_result_json(None, Some(serde_json::json!({{\"error_type\":\"runtime_error\",\"message\": \"native replay downcast failed for axum state input {idx}: expected {state_type}\"}})), 0.0, vec![]),\n"
        ));
        h.push_str("        };\n");
        h.push_str(&format!(
            "        let app = app.with_state(__state_value_{idx});\n"
        ));
    }

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
    h.push_str("                let msg = e.clone();\n");
    h.push_str("                Some(serde_json::json!({ \"error_type\": \"runtime_error\", \"message\": msg, \"stack\": null }))\n");
    h.push_str("            }\n");
    h.push_str("        };\n\n");

    h.push_str("        shatter_rust_runtime::build_result_json(return_value, thrown_error, wall_time_ms, vec![])\n");
    h.push_str("    });\n");
    h.push_str("}\n");

    Ok(h)
}

fn crate_use_imports_for_harness(source: &str, crate_alias: &str) -> String {
    use quote::ToTokens;

    let Ok(file) = syn::parse_file(source) else {
        return String::new();
    };
    let mut imports = String::new();
    for item in file.items {
        let syn::Item::Use(use_item) = item else {
            continue;
        };
        let tokens = use_item.to_token_stream().to_string();
        if !tokens.contains("crate ::") {
            continue;
        }
        imports.push_str(&tokens.replace("crate ::", &format!("{crate_alias} ::")));
        imports.push('\n');
    }
    imports
}

fn module_path_for_crate_file(crate_root: &Path, file_path: &Path) -> Option<String> {
    let rel = file_path.strip_prefix(crate_root).ok()?;
    let rel = rel.strip_prefix("src").ok()?;
    let mut parts: Vec<String> = rel
        .iter()
        .map(|part| part.to_string_lossy().to_string())
        .collect();
    let last = parts.last_mut()?;
    if last == "lib.rs" || last == "main.rs" || last == "mod.rs" {
        parts.pop();
    } else if let Some(stripped) = last.strip_suffix(".rs") {
        *last = stripped.to_string();
    } else {
        return None;
    }
    Some(parts.join("::"))
}

fn module_file_for_module_path(crate_root: &Path, module_path: &str) -> Option<PathBuf> {
    let mut path = crate_root.join("src");
    for segment in module_path.split("::").filter(|segment| !segment.is_empty()) {
        path.push(segment);
    }
    let mod_rs = path.join("mod.rs");
    if mod_rs.exists() {
        return Some(mod_rs);
    }
    path.set_extension("rs");
    path.exists().then_some(path)
}

fn public_reexport_name_for_child(
    source: &str,
    child_module: &str,
    function_name: &str,
) -> Option<String> {
    let file = syn::parse_file(source).ok()?;
    file.items.into_iter().find_map(|item| {
        let syn::Item::Use(use_item) = item else {
            return None;
        };
        if !matches!(use_item.vis, syn::Visibility::Public(_)) {
            return None;
        }
        use_tree_reexports_child_function(&use_item.tree, child_module, function_name, false)
    })
}

fn use_tree_reexports_child_function(
    tree: &syn::UseTree,
    child_module: &str,
    function_name: &str,
    in_child_module: bool,
) -> Option<String> {
    match tree {
        syn::UseTree::Path(path) if !in_child_module && path.ident == "self" => {
            use_tree_reexports_child_function(&path.tree, child_module, function_name, false)
        }
        syn::UseTree::Path(path) if !in_child_module && path.ident == child_module => {
            use_tree_reexports_child_function(&path.tree, child_module, function_name, true)
        }
        syn::UseTree::Group(group) => group.items.iter().find_map(|tree| {
            use_tree_reexports_child_function(tree, child_module, function_name, in_child_module)
        }),
        syn::UseTree::Name(name) if in_child_module && name.ident == function_name => {
            Some(function_name.to_string())
        }
        syn::UseTree::Rename(rename) if in_child_module && rename.ident == function_name => {
            Some(rename.rename.to_string())
        }
        syn::UseTree::Glob(_) if in_child_module => Some(function_name.to_string()),
        _ => None,
    }
}

fn public_invocation_path_for_crate_file(
    crate_root: &Path,
    file_path: &Path,
    function_name: &str,
    crate_alias: &str,
    module_path: &str,
) -> Option<String> {
    let (parent_module_path, child_module) = module_path.rsplit_once("::")?;
    let parent_file = module_file_for_module_path(crate_root, parent_module_path)?;
    let source = std::fs::read_to_string(parent_file).ok()?;
    let export_name = public_reexport_name_for_child(&source, child_module, function_name)?;
    let canonical_source = std::fs::canonicalize(file_path).ok()?;
    let child_file = module_file_for_module_path(crate_root, module_path)?;
    let canonical_child = std::fs::canonicalize(child_file).ok()?;
    if canonical_source != canonical_child {
        return None;
    }
    Some(format!(
        "{crate_alias}::{parent_module_path}::{export_name}"
    ))
}

#[allow(clippy::too_many_arguments)]
fn generate_axum_crate_harness(
    source: &str,
    function_name: &str,
    crate_alias: &str,
    module_path: &str,
    public_invocation_target: Option<&str>,
    mappings: &[crate::adapters::AxumExtractorMapping],
    param_types: &[String],
    native_replays: &[Option<NativeReplaySpec>],
    mocks_json: &str,
    type_sources: &[(Option<&str>, &str)],
) -> Result<String, ExecuteError> {
    let mut harness = generate_axum_harness(
        "",
        function_name,
        mappings,
        param_types,
        native_replays,
        mocks_json,
        type_sources,
    )?;
    harness = harness.replace("#[allow(dead_code)]\nmod user_code {\n\n}", "");
    harness = harness.replace(
        "use crate::user_code::*;\n",
        &crate_use_imports_for_harness(source, crate_alias),
    );
    let public_module_path = if module_path
        .rsplit("::")
        .next()
        .is_some_and(|segment| segment == function_name)
    {
        module_path.rsplit_once("::").map(|(parent, _)| parent)
    } else {
        Some(module_path)
    };
    let target = public_invocation_target
        .map(str::to_string)
        .unwrap_or_else(|| {
            if public_module_path.is_none_or(str::is_empty) {
                format!("{crate_alias}::{function_name}")
            } else {
                format!(
                    "{crate_alias}::{}::{function_name}",
                    public_module_path.unwrap()
                )
            }
        });
    harness = harness.replace(
        &format!("routing::any(user_code::{function_name})"),
        &format!("routing::any({target})"),
    );
    Ok(harness)
}

fn generate_axum_crate_driver_cargo_toml(
    driver_package_name: &str,
    crate_name: &str,
    staging_crate: &Path,
    runtime_path: &Path,
    user_cargo_toml: &str,
) -> String {
    let crate_path = staging_crate.display().to_string().replace('\\', "/");
    let runtime_path = runtime_path.display().to_string().replace('\\', "/");
    let project_deps = rust_dependency_lines_for_driver(user_cargo_toml, crate_name);
    let has_dep = |name: &str| project_deps.iter().any(|(dep, _)| dep == name);
    let mut deps = String::new();
    for (_, line) in &project_deps {
        deps.push_str(line);
        deps.push('\n');
    }
    for (name, line) in [
        ("serde_json", r#"serde_json = "1""#),
        ("tokio", r#"tokio = { version = "1", features = ["full"] }"#),
        ("axum", r#"axum = { version = "0.8", features = ["json"] }"#),
        ("tower", r#"tower = { version = "0.5", features = ["util"] }"#),
        ("http", r#"http = "1""#),
        ("http-body-util", r#"http-body-util = "0.1""#),
        ("async-trait", r#"async-trait = "0.1""#),
    ] {
        if !has_dep(name) {
            deps.push_str(line);
            deps.push('\n');
        }
    }
    format!(
        r#"[package]
name = "{driver_package_name}"
version = "0.1.0"
edition = "2021"

[workspace]
exclude = ["crate-shadow"]

[profile.dev]
debug = 0
incremental = false

[dependencies]
{crate_name} = {{ path = "{crate_path}", features = ["shatter-crate-bridge"] }}
shatter-rust-runtime = {{ path = "{runtime_path}" }}
{deps}
"#
    )
}

fn rust_dependency_lines_for_driver(cargo_toml: &str, crate_name: &str) -> Vec<(String, String)> {
    let mut in_dependencies = false;
    let mut deps = Vec::new();
    for line in cargo_toml.lines() {
        let trimmed = line.trim();
        if trimmed == "[dependencies]" {
            in_dependencies = true;
            continue;
        }
        if in_dependencies && trimmed.starts_with('[') {
            break;
        }
        if !in_dependencies || trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((name, _)) = trimmed.split_once('=') else {
            continue;
        };
        let name = name.trim();
        if matches!(name, "shatter-rust-runtime") || name == crate_name {
            continue;
        }
        deps.push((name.to_string(), trimmed.to_string()));
    }
    deps
}

fn crate_rust_type_sources(crate_root: &Path) -> Vec<(Option<String>, String)> {
    let src = crate_root.join("src");
    let mut sources = Vec::new();
    collect_rust_sources(crate_root, &src, &mut sources);
    sources
}

fn collect_rust_sources(
    crate_root: &Path,
    dir: &Path,
    sources: &mut Vec<(Option<String>, String)>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_rust_sources(crate_root, &path, sources);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs")
            && let Ok(source) = std::fs::read_to_string(&path)
        {
            let module_path = module_path_for_crate_file(crate_root, &path);
            sources.push((module_path, source));
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn execute_axum_handler_crate_backed(
    file_path: &str,
    function_name: &str,
    inputs: &[Value],
    mocks: &[Value],
    timeout_ms: u64,
    mappings: &[crate::adapters::AxumExtractorMapping],
    cache: &HarnessCache,
    crate_root: &Path,
) -> Result<ExecuteResult, ExecuteError> {
    let crate_root_buf = std::fs::canonicalize(crate_root)
        .unwrap_or_else(|_| to_absolute(crate_root.to_path_buf()));
    let crate_root = crate_root_buf.as_path();
    let path = Path::new(file_path);
    let source_path =
        std::fs::canonicalize(path).unwrap_or_else(|_| to_absolute(path.to_path_buf()));
    let source = std::fs::read_to_string(path)
        .map_err(|e| ExecuteError::FileError(format!("cannot read {file_path}: {e}")))?;
    let ctx = extract_fn_context(&source, function_name)?;
    let native_replays = native_replay_specs(inputs)?;
    let native_replay_hash = native_replay_hash(&native_replays);

    let user_cargo_toml_path = crate_root.join("Cargo.toml");
    let user_cargo_toml = std::fs::read_to_string(&user_cargo_toml_path)
        .map_err(|e| ExecuteError::FileError(format!("cannot read Cargo.toml: {e}")))?;
    let crate_name = extract_crate_name(&user_cargo_toml).unwrap_or_else(|| "user_crate".to_string());
    let crate_alias = crate_name.replace('-', "_");
    let module_path = module_path_for_crate_file(crate_root, &source_path).ok_or_else(|| {
        ExecuteError::NonExecutable(format!(
            "axum crate harness cannot map `{}` to a crate module path",
            path.display()
        ))
    })?;
    let public_invocation_target = public_invocation_path_for_crate_file(
        crate_root,
        &source_path,
        function_name,
        &crate_alias,
        &module_path,
    );

    let mocks_json = serde_json::to_string(mocks).map_err(|e| {
        ExecuteError::InstrumentError(format!("cannot serialize mocks: {e}"))
    })?;
    let instr_result = instrument::instrument_source(&source, Some(function_name))
        .map_err(|e| ExecuteError::InstrumentError(e.to_string()))?;
    let type_sources = crate_rust_type_sources(crate_root);
    let type_source_refs = type_sources
        .iter()
        .map(|(module_path, source)| (module_path.as_deref(), source.as_str()))
        .collect::<Vec<_>>();
    let harness_source = generate_axum_crate_harness(
        &source,
        function_name,
        &crate_alias,
        &module_path,
        public_invocation_target.as_deref(),
        mappings,
        &ctx.sig.param_types,
        &native_replays,
        &mocks_json,
        &type_source_refs,
    )?;
    let wrapper_hash = source_hash(&format!("{}{}", instr_result.source, harness_source));
    let mh = mocks_hash(mocks);
    let key = HarnessKey {
        file_path: file_path.to_string(),
        function_name: function_name.to_string(),
        mocks_hash: mh,
        native_replay_hash,
    };

    {
        let mut map = cache.lock().unwrap();
        if let Some(harness) = map.get_mut(&key) {
            let result = harness.execute(inputs, timeout_ms)?;
            if result
                .thrown_error
                .as_ref()
                .and_then(|e| e.get("error_type"))
                .and_then(|v| v.as_str())
                == Some("timeout")
            {
                map.remove(&key);
            }
            return Ok(result);
        }
    }

    let runtime_path = find_runtime_crate_path()?;
    let harness_dir = stable_crate_bridge_dir(crate_root, wrapper_hash, mh);
    std::fs::create_dir_all(&harness_dir)?;
    let staging_crate = create_crate_staging_copy(crate_root, &harness_dir).map_err(|e| {
        ExecuteError::IoError(io::Error::other(format!("cannot create staging copy: {e}")))
    })?;
    let rel_file = path.strip_prefix(crate_root).unwrap_or(path);
    let staging_file = staging_crate.join(rel_file);
    if let Some(parent) = staging_file.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            ExecuteError::IoError(io::Error::other(format!(
                "cannot create staging directories: {e}"
            )))
        })?;
    }
    std::fs::write(&staging_file, &instr_result.source).map_err(|e| {
        ExecuteError::IoError(io::Error::other(format!(
            "cannot write instrumented source to staging: {e}"
        )))
    })?;
    inject_crate_bridge_feature(&staging_crate.join("Cargo.toml"), &runtime_path)?;

    let driver_package_name =
        crate_bridge_driver_package_name("shatter-exec-temp", &harness_dir);
    let driver_cargo_toml = generate_axum_crate_driver_cargo_toml(
        &driver_package_name,
        &crate_name,
        &staging_crate,
        &runtime_path,
        &user_cargo_toml,
    );
    let harness_result =
        build_and_spawn_crate_harness(&harness_source, &driver_cargo_toml, &harness_dir, None);
    let mut harness = match harness_result {
        Ok(harness) => harness,
        Err(err) => {
            let _ = std::fs::remove_dir_all(&harness_dir);
            return Err(err);
        }
    };

    let result = harness.execute(inputs, timeout_ms)?;
    let timed_out = result
        .thrown_error
        .as_ref()
        .and_then(|e| e.get("error_type"))
        .and_then(|v| v.as_str())
        == Some("timeout");
    if !timed_out {
        cache.lock().unwrap().insert(key, harness);
    } else {
        let _ = std::fs::remove_dir_all(&harness_dir);
    }

    Ok(result)
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
    if let Some(crate_root) = find_crate_root(file_path) {
        return execute_axum_handler_crate_backed(
            file_path,
            function_name,
            inputs,
            mocks,
            timeout_ms,
            mappings,
            cache,
            &crate_root,
        );
    }

    let native_replays = native_replay_specs(inputs)?;
    let native_replay_hash = native_replay_hash(&native_replays);

    // Compute cache key.
    let key = HarnessKey {
        file_path: file_path.to_string(),
        function_name: function_name.to_string(),
        mocks_hash: mocks_hash(mocks),
        native_replay_hash,
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
    let ctx = extract_fn_context(&source, function_name)?;

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
        &ctx.sig.param_types,
        &native_replays,
        &mocks_json,
        &[(None, source.as_str())],
    )?;

    let harness_dir = make_harness_dir();
    std::fs::create_dir_all(&harness_dir)?;

    let mut harness = build_and_spawn_harness(
        &harness_source,
        &harness_dir,
        &runtime_path,
        true,  // needs_tokio (always for axum)
        true,  // needs_axum
        false, // needs_shatter_rust
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

    fn is_offline_compile_error_message(msg: &str) -> bool {
        msg.contains("spurious network error")
            || msg.contains("download of config.json failed")
            || msg.contains("Could not resolve host")
            || msg.contains("Could not resolve hostname")
    }

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
        let toml = generate_cargo_toml(Path::new("/home/user/shatter-rust-runtime"), false, false, false);
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
        let harness =
            generate_harness("fn noop() {}", "noop", &[], &[], None, "[]", &[], &[], false)
                .unwrap();
        assert!(harness.contains("user_code::noop()"));
        assert!(harness.contains("Ok(())"));
        assert!(harness.contains("run_harness_loop"));
    }

    #[test]
    fn execute_replays_native_generator_inside_subprocess_harness() {
        let dir = tempfile::tempdir().expect("tempdir");
        let source_file = dir.path().join("standalone.rs");
        std::fs::write(
            &source_file,
            r#"
pub fn generated_len(value: String) -> usize {
    value.len()
}
"#,
        )
        .expect("write source");
        let generator_file = dir.path().join("generated_string.rs");
        std::fs::write(
            &generator_file,
            r#"
use shatter_rust::generators::GeneratorResult;

pub fn GeneratedString(recipe: Option<serde_json::Value>) -> GeneratorResult {
    let value = recipe
        .and_then(|v| v.get("value").and_then(|v| v.as_str()).map(ToString::to_string))
        .unwrap_or_else(|| "fallback".to_string());
    GeneratorResult {
        id: "generated-string".to_string(),
        value: Box::new(value),
        recipe: serde_json::json!({"value": "fallback"}),
    }
}
"#,
        )
        .expect("write generator");

        let input = serde_json::json!({
            "__shatter_native": true,
            "handle": "frontend-only-handle",
            "__shatter_replay": {
                "language": "rust",
                "file": generator_file,
                "name": "GeneratedString",
                "recipe": {"value": "abcdef"}
            }
        });

        let cache: HarnessCache = Mutex::new(HashMap::new());
        let crate_cache: CrateHarnessCache = Mutex::new(HashMap::new());
        let bridge_cache: CrateBridgeHarnessCache = Mutex::new(HashMap::new());
        let result = execute_function(
            &source_file.to_string_lossy(),
            "generated_len",
            &[input],
            &[],
            30_000,
            None,
            &cache,
            &crate_cache,
            &bridge_cache,
        );

        match result {
            Ok(result) => assert_eq!(result.return_value, Some(serde_json::json!(6))),
            Err(ExecuteError::CompilationFailed(msg)) if cargo_build_unavailable(&msg) => {
                eprintln!(
                    "skipping execute_replays_native_generator_inside_subprocess_harness: cargo unavailable ({msg})"
                );
            }
            Err(err) => panic!("execute failed: {err:?}"),
        }
    }

    #[test]
    fn execute_reports_native_replay_downcast_failure() {
        let dir = tempfile::tempdir().expect("tempdir");
        let source_file = dir.path().join("standalone.rs");
        std::fs::write(
            &source_file,
            r#"
pub fn generated_len(value: String) -> usize {
    value.len()
}
"#,
        )
        .expect("write source");
        let generator_file = dir.path().join("wrong_type.rs");
        std::fs::write(
            &generator_file,
            r#"
use shatter_rust::generators::GeneratorResult;

pub fn WrongType(_recipe: Option<serde_json::Value>) -> GeneratorResult {
    GeneratorResult {
        id: "wrong-type".to_string(),
        value: Box::new(123_i32),
        recipe: serde_json::Value::Null,
    }
}
"#,
        )
        .expect("write generator");

        let input = serde_json::json!({
            "__shatter_native": true,
            "handle": "frontend-only-handle",
            "__shatter_replay": {
                "language": "rust",
                "file": generator_file,
                "name": "WrongType",
                "recipe": null
            }
        });

        let cache: HarnessCache = Mutex::new(HashMap::new());
        let crate_cache: CrateHarnessCache = Mutex::new(HashMap::new());
        let bridge_cache: CrateBridgeHarnessCache = Mutex::new(HashMap::new());
        let result = execute_function(
            &source_file.to_string_lossy(),
            "generated_len",
            &[input],
            &[],
            30_000,
            None,
            &cache,
            &crate_cache,
            &bridge_cache,
        );

        match result {
            Ok(result) => {
                let message = result
                    .thrown_error
                    .as_ref()
                    .and_then(|err| err.get("message"))
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                assert!(
                    message.contains("native replay downcast failed for input 0: expected String"),
                    "unexpected thrown_error: {:?}",
                    result.thrown_error
                );
            }
            Err(ExecuteError::CompilationFailed(msg)) if cargo_build_unavailable(&msg) => {
                eprintln!(
                    "skipping execute_reports_native_replay_downcast_failure: cargo unavailable ({msg})"
                );
            }
            Err(err) => panic!("execute failed: {err:?}"),
        }
    }

    #[test]
    fn execute_axum_handler_replays_native_state_and_extension_values() {
        use crate::adapters::{AxumExtractorKind, AxumExtractorMapping};

        let dir = tempfile::tempdir().expect("tempdir");
        let source_file = dir.path().join("handler.rs");
        std::fs::write(
            &source_file,
            r#"
use axum::{extract::{FromRequestParts, State}, Json};
use axum::http::request::Parts;
use async_trait::async_trait;

#[derive(Clone)]
pub struct AppStateLike {
    pub prefix: String,
}

#[derive(Clone)]
pub struct CurrentAccountLike {
    pub id: u64,
}

#[async_trait]
impl<S> FromRequestParts<S> for CurrentAccountLike
where
    S: Send + Sync,
{
    type Rejection = &'static str;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<CurrentAccountLike>()
            .cloned()
            .ok_or("missing current account")
    }
}

#[derive(serde::Deserialize)]
pub struct Payload {
    pub name: String,
}

pub async fn combo(
    State(state): State<AppStateLike>,
    current: CurrentAccountLike,
    Json(payload): Json<Payload>,
) -> String {
    format!("{}:{}:{}", state.prefix, current.id, payload.name)
}
"#,
        )
        .expect("write source");

        let state_generator = dir.path().join("state_gen.rs");
        std::fs::write(
            &state_generator,
            r#"
use crate::user_code::AppStateLike;
use shatter_rust::generators::GeneratorResult;

pub fn AppStateLikeGen(recipe: Option<serde_json::Value>) -> GeneratorResult {
    let prefix = recipe
        .as_ref()
        .and_then(|v| v.get("prefix"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("state")
        .to_string();
    GeneratorResult {
        id: "app-state-like".to_string(),
        value: Box::new(AppStateLike { prefix }),
        recipe: recipe.unwrap_or(serde_json::Value::Null),
    }
}
"#,
        )
        .expect("write state generator");

        let current_generator = dir.path().join("current_gen.rs");
        std::fs::write(
            &current_generator,
            r#"
use crate::user_code::CurrentAccountLike;
use shatter_rust::generators::GeneratorResult;

pub fn CurrentAccountLikeGen(recipe: Option<serde_json::Value>) -> GeneratorResult {
    let id = recipe
        .as_ref()
        .and_then(|v| v.get("id"))
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    GeneratorResult {
        id: "current-account-like".to_string(),
        value: Box::new(CurrentAccountLike { id }),
        recipe: recipe.unwrap_or(serde_json::Value::Null),
    }
}
"#,
        )
        .expect("write current generator");

        let state_input = serde_json::json!({
            "__shatter_native": true,
            "handle": "frontend-state",
            "__shatter_replay": {
                "language": "rust",
                "file": state_generator,
                "name": "AppStateLikeGen",
                "recipe": {"prefix": "pack"}
            }
        });
        let current_input = serde_json::json!({
            "__shatter_native": true,
            "handle": "frontend-current",
            "__shatter_replay": {
                "language": "rust",
                "file": current_generator,
                "name": "CurrentAccountLikeGen",
                "recipe": {"id": 42}
            }
        });
        let mappings = vec![
            AxumExtractorMapping {
                param_index: 0,
                kind: AxumExtractorKind::AppState,
                type_name: "State".to_string(),
            },
            AxumExtractorMapping {
                param_index: 1,
                kind: AxumExtractorKind::Unsupported,
                type_name: "CurrentAccountLike".to_string(),
            },
            AxumExtractorMapping {
                param_index: 2,
                kind: AxumExtractorKind::JsonBody,
                type_name: "Json".to_string(),
            },
        ];
        let cache: HarnessCache = Mutex::new(HashMap::new());
        let crate_cache: CrateHarnessCache = Mutex::new(HashMap::new());
        let bridge_cache: CrateBridgeHarnessCache = Mutex::new(HashMap::new());
        let result = execute_axum_handler(
            &source_file.to_string_lossy(),
            "combo",
            &[
                state_input,
                current_input,
                serde_json::json!({"name": "kit"}),
            ],
            &[],
            30_000,
            &mappings,
            &cache,
            &crate_cache,
            &bridge_cache,
        );

        match result {
            Ok(result) => {
                assert_eq!(
                    result
                        .return_value
                        .as_ref()
                        .and_then(|v| v.get("status")),
                    Some(&serde_json::json!(200))
                );
                assert_eq!(
                    result.return_value.as_ref().and_then(|v| v.get("body")),
                    Some(&serde_json::json!("pack:42:kit"))
                );
            }
            Err(ExecuteError::CompilationFailed(msg)) if is_offline_compile_error_message(&msg) => {
                eprintln!(
                    "skipping execute_axum_handler_replays_native_state_and_extension_values: cargo unavailable ({msg})"
                );
            }
            Err(err) => panic!("execute failed: {err:?}"),
        }
    }

    #[test]
    fn execute_axum_handler_replays_native_state_inside_request_runtime() {
        use crate::adapters::{AxumExtractorKind, AxumExtractorMapping};

        let dir = tempfile::tempdir().expect("tempdir");
        let source_file = dir.path().join("handler.rs");
        std::fs::write(
            &source_file,
            r#"
use axum::extract::State;

#[derive(Clone)]
pub struct AppStateLike {
    pub value: String,
}

pub async fn handler(State(state): State<AppStateLike>) -> String {
    state.value
}
"#,
        )
        .expect("write source");

        let state_generator = dir.path().join("state_gen.rs");
        std::fs::write(
            &state_generator,
            r#"
use crate::user_code::AppStateLike;
use shatter_rust::generators::GeneratorResult;

pub fn RuntimeAwareState(_recipe: Option<serde_json::Value>) -> GeneratorResult {
    tokio::runtime::Handle::try_current()
        .expect("native replay ran outside tokio request runtime");
    GeneratorResult {
        id: "runtime-aware-state".to_string(),
        value: Box::new(AppStateLike { value: "runtime-ok".to_string() }),
        recipe: serde_json::Value::Null,
    }
}
"#,
        )
        .expect("write generator");

        let input = serde_json::json!({
            "__shatter_native": true,
            "handle": "runtime-state",
            "__shatter_replay": {
                "language": "rust",
                "file": state_generator,
                "name": "RuntimeAwareState",
                "recipe": null
            }
        });
        let mappings = vec![AxumExtractorMapping {
            param_index: 0,
            kind: AxumExtractorKind::AppState,
            type_name: "State".to_string(),
        }];
        let cache: HarnessCache = Mutex::new(HashMap::new());
        let crate_cache: CrateHarnessCache = Mutex::new(HashMap::new());
        let bridge_cache: CrateBridgeHarnessCache = Mutex::new(HashMap::new());

        let result = execute_axum_handler(
            &source_file.to_string_lossy(),
            "handler",
            &[input],
            &[],
            30_000,
            &mappings,
            &cache,
            &crate_cache,
            &bridge_cache,
        );

        match result {
            Ok(result) => {
                assert_eq!(
                    result
                        .return_value
                        .as_ref()
                        .and_then(|v| v.get("status")),
                    Some(&serde_json::json!(200))
                );
                assert_eq!(
                    result.return_value.as_ref().and_then(|v| v.get("body")),
                    Some(&serde_json::json!("runtime-ok"))
                );
            }
            Err(ExecuteError::CompilationFailed(msg)) if is_offline_compile_error_message(&msg) => {
                eprintln!(
                    "skipping execute_axum_handler_replays_native_state_inside_request_runtime: cargo unavailable ({msg})"
                );
            }
            Err(err) => panic!("execute failed: {err:?}"),
        }
    }

    #[test]
    fn execute_axum_handler_defaults_null_json_body_to_valid_payload() {
        use crate::adapters::{AxumExtractorKind, AxumExtractorMapping};

        let dir = tempfile::tempdir().expect("tempdir");
        let source_file = dir.path().join("handler.rs");
        std::fs::write(
            &source_file,
            r#"
use axum::extract::Json;

#[derive(serde::Deserialize)]
pub struct Payload {
    pub name: String,
    pub count: u32,
    pub enabled: bool,
}

pub async fn create(Json(payload): Json<Payload>) -> String {
    format!("{}:{}:{}", payload.name, payload.count, payload.enabled)
}
"#,
        )
        .expect("write source");

        let mappings = vec![AxumExtractorMapping {
            param_index: 0,
            kind: AxumExtractorKind::JsonBody,
            type_name: "Json".to_string(),
        }];
        let cache: HarnessCache = Mutex::new(HashMap::new());
        let crate_cache: CrateHarnessCache = Mutex::new(HashMap::new());
        let bridge_cache: CrateBridgeHarnessCache = Mutex::new(HashMap::new());

        let result = execute_axum_handler(
            &source_file.to_string_lossy(),
            "create",
            &[serde_json::Value::Null],
            &[],
            30_000,
            &mappings,
            &cache,
            &crate_cache,
            &bridge_cache,
        );

        match result {
            Ok(result) => {
                assert_eq!(
                    result
                        .return_value
                        .as_ref()
                        .and_then(|v| v.get("status")),
                    Some(&serde_json::json!(200)),
                    "null Json<T> input should produce a valid request body, got {result:?}"
                );
                assert_eq!(
                    result.return_value.as_ref().and_then(|v| v.get("body")),
                    Some(&serde_json::json!(":0:false"))
                );
            }
            Err(ExecuteError::CompilationFailed(msg)) if is_offline_compile_error_message(&msg) => {
                eprintln!(
                    "skipping execute_axum_handler_defaults_null_json_body_to_valid_payload: cargo unavailable ({msg})"
                );
            }
            Err(err) => panic!("execute failed: {err:?}"),
        }
    }

    #[test]
    fn execute_axum_handler_sends_literal_null_json_body_for_option_payload() {
        use crate::adapters::{AxumExtractorKind, AxumExtractorMapping};

        let dir = tempfile::tempdir().expect("tempdir");
        let source_file = dir.path().join("handler.rs");
        std::fs::write(
            &source_file,
            r#"
use axum::extract::Json;

pub async fn maybe(Json(payload): Json<Option<String>>) -> String {
    if payload.is_none() { "none".to_string() } else { "some".to_string() }
}
"#,
        )
        .expect("write source");

        let mappings = vec![AxumExtractorMapping {
            param_index: 0,
            kind: AxumExtractorKind::JsonBody,
            type_name: "Json".to_string(),
        }];
        let cache: HarnessCache = Mutex::new(HashMap::new());
        let crate_cache: CrateHarnessCache = Mutex::new(HashMap::new());
        let bridge_cache: CrateBridgeHarnessCache = Mutex::new(HashMap::new());

        let result = execute_axum_handler(
            &source_file.to_string_lossy(),
            "maybe",
            &[serde_json::Value::Null],
            &[],
            30_000,
            &mappings,
            &cache,
            &crate_cache,
            &bridge_cache,
        );

        match result {
            Ok(result) => {
                assert_eq!(
                    result
                        .return_value
                        .as_ref()
                        .and_then(|v| v.get("status")),
                    Some(&serde_json::json!(200)),
                    "null Json<Option<T>> should be sent as literal JSON null, got {result:?}"
                );
                assert_eq!(
                    result.return_value.as_ref().and_then(|v| v.get("body")),
                    Some(&serde_json::json!("none"))
                );
            }
            Err(ExecuteError::CompilationFailed(msg)) if is_offline_compile_error_message(&msg) => {
                eprintln!(
                    "skipping execute_axum_handler_sends_literal_null_json_body_for_option_payload: cargo unavailable ({msg})"
                );
            }
            Err(err) => panic!("execute failed: {err:?}"),
        }
    }

    #[test]
    fn axum_json_default_uses_qualified_structs_and_unique_bare_fallback() {
        let domain = r#"
pub struct Duplicate {
    pub label: String,
}
"#;
        let other = r#"
pub struct Duplicate {
    pub count: u32,
}
"#;
        let type_sources = &[(Some("domain"), domain), (Some("other"), other)];

        assert_eq!(
            axum_json_default_value(type_sources, "crate::domain::Duplicate"),
            serde_json::json!({"label": ""})
        );
        assert_eq!(
            axum_json_default_value(type_sources, "Duplicate"),
            serde_json::json!({})
        );
    }

    #[test]
    fn axum_json_default_respects_serde_skip_flatten_and_camel_case() {
        let source = r#"
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Outer {
    pub label: String,
    #[serde(skip_deserializing)]
    pub server_only: String,
    #[serde(flatten)]
    pub nested: Nested,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Nested {
    pub owner_person_id: String,
}
"#;

        assert_eq!(
            axum_json_default_value(&[(None, source)], "Outer"),
            serde_json::json!({"label": "", "ownerPersonId": ""})
        );
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
        let toml = generate_cargo_toml(Path::new("/fake/runtime"), true, false, false);
        assert!(
            toml.contains("tokio"),
            "needs_tokio=true must include tokio dep\n\ntoml:\n{toml}"
        );
    }

    #[test]
    fn generate_cargo_toml_without_tokio() {
        let toml = generate_cargo_toml(Path::new("/fake/runtime"), false, false, false);
        assert!(
            !toml.contains("tokio"),
            "needs_tokio=false must not include tokio dep\n\ntoml:\n{toml}"
        );
    }

    // ── Axum Cargo.toml generation ──

    // ── Axum extractor classification + wrapper generation (str-q8do) ──

    #[test]
    fn classify_axum_path_extractor_extracts_inner() {
        let ext = classify_axum_extractor("axum::extract::Path<Uuid>").unwrap();
        assert_eq!(ext, AxumExtractor::Path("Uuid".to_string()));
    }

    #[test]
    fn classify_axum_query_extractor_extracts_inner() {
        let ext = classify_axum_extractor("Query<ListBundlesQuery>").unwrap();
        assert_eq!(ext, AxumExtractor::Query("ListBundlesQuery".to_string()));
    }

    #[test]
    fn classify_axum_json_extractor_extracts_inner() {
        let ext = classify_axum_extractor("axum::Json<CreateBundle>").unwrap();
        assert_eq!(ext, AxumExtractor::Json("CreateBundle".to_string()));
    }

    #[test]
    fn classify_axum_state_extractor_returns_state() {
        let ext = classify_axum_extractor("State<AppState>").unwrap();
        assert_eq!(ext, AxumExtractor::State("AppState".to_string()));
    }

    #[test]
    fn classify_non_axum_type_returns_none() {
        assert!(classify_axum_extractor("Vec<u8>").is_none());
        assert!(classify_axum_extractor("i32").is_none());
        assert!(classify_axum_extractor("String").is_none());
    }

    #[test]
    fn generate_harness_axum_path_uses_inner_type_not_extractor() {
        // Regression: str-q8do — wrapper used to call
        // `serde_json::from_value::<Path<Uuid>>(...)`, which doesn't compile
        // because `Path<T>` isn't `DeserializeOwned`.
        let source =
            "use axum::extract::Path;\nasync fn h(Path(id): Path<u64>) -> String { id.to_string() }";
        let harness = generate_harness(
            source,
            "h",
            &["id".to_string()],
            &["Path<u64>".to_string()],
            Some("String"),
            "[]",
            &[],
            &[],
            true,
        )
        .unwrap();
        assert!(
            !harness.contains("let id: Path<u64>"),
            "wrapper must not declare param as the extractor type:\n{harness}"
        );
        assert!(
            harness.contains("id_inner: u64"),
            "wrapper must deserialize inner u64, not Path<u64>:\n{harness}"
        );
        assert!(
            harness.contains("axum::extract::Path(id_inner)"),
            "wrapper must reconstruct Path via inner value:\n{harness}"
        );
    }

    #[test]
    fn generate_harness_axum_state_emits_not_supported_early_return() {
        let source = "use axum::extract::State;\nasync fn h(State(s): State<AppState>) {}";
        let harness = generate_harness(
            source,
            "h",
            &["s".to_string()],
            &["State<AppState>".to_string()],
            None,
            "[]",
            &[],
            &[],
            true,
        )
        .unwrap();
        assert!(
            !harness.contains("serde_json::from_value(inputs[0]"),
            "wrapper must NOT try to deserialize State<AppState>:\n{harness}"
        );
        assert!(
            harness.contains("not_supported"),
            "wrapper must emit not_supported for State<T>:\n{harness}"
        );
    }

    #[test]
    fn generate_harness_axum_handler_skips_serialize_on_return() {
        // axum::Json<T> doesn't impl Serialize on its outer wrapper; the
        // generic generated wrapper must not require it for handler shapes.
        let source =
            "use axum::Json;\nasync fn h(Json(body): Json<String>) -> Json<String> { Json(body) }";
        let harness = generate_harness(
            source,
            "h",
            &["body".to_string()],
            &["Json<String>".to_string()],
            Some("Json<String>"),
            "[]",
            &[],
            &[],
            true,
        )
        .unwrap();
        assert!(
            !harness.contains("serde_json::to_value(v)"),
            "axum handler must not call to_value on framework return wrapper:\n{harness}"
        );
        assert!(
            harness.contains("Ok(_) => (Some(Value::Null), None)"),
            "axum handler should null out return value capture:\n{harness}"
        );
    }

    #[test]
    fn generate_crate_bridge_wrapper_axum_path_uses_inner_type() {
        let fns = vec![CompatFn {
            name: "h".to_string(),
            param_names: vec!["id".to_string()],
            param_types: vec!["Path<Uuid>".to_string()],
            return_type: Some("String".to_string()),
            is_async: true,
        }];
        let w = generate_crate_bridge_wrapper(&fns, "[]", &[]);
        assert!(
            !w.contains("let id: Path<Uuid>"),
            "crate-bridge wrapper must not declare param as extractor:\n{w}"
        );
        assert!(
            w.contains("id_inner: Uuid"),
            "crate-bridge wrapper must deserialize inner Uuid:\n{w}"
        );
        assert!(
            w.contains("axum::extract::Path(id_inner)"),
            "crate-bridge wrapper must reconstruct Path:\n{w}"
        );
    }

    #[test]
    fn generate_crate_bridge_wrapper_axum_state_is_not_supported() {
        let fns = vec![CompatFn {
            name: "h".to_string(),
            param_names: vec!["s".to_string(), "id".to_string()],
            param_types: vec!["State<AppState>".to_string(), "Path<Uuid>".to_string()],
            return_type: Some("Result<Json<Vec<u8>>, ApiError>".to_string()),
            is_async: true,
        }];
        let w = generate_crate_bridge_wrapper(&fns, "[]", &[]);
        assert!(
            !w.contains("serde_json::from_value(inputs[0]"),
            "must not try to deserialize State<AppState>:\n{w}"
        );
        assert!(
            w.contains("not_supported"),
            "must emit not_supported error:\n{w}"
        );
        // And must NOT emit `serde_json::to_value(ret_val)` for the return,
        // since `Result<Json<T>, ApiError>` need not be Serialize.
        assert!(
            !w.contains("serde_json::to_value(ret_val)"),
            "must not require Serialize on framework return for handler shapes:\n{w}"
        );
    }

    #[test]
    fn generate_crate_bridge_wrapper_axum_handler_skips_serialize_on_return() {
        // Return type `Result<Json<Vec<T>>, ApiError>` — neither variant is
        // Serialize. Wrapper must skip the `to_value` call.
        let fns = vec![CompatFn {
            name: "h".to_string(),
            param_names: vec!["body".to_string()],
            param_types: vec!["Json<CreateBundle>".to_string()],
            return_type: Some("Result<Json<Vec<BundleSummary>>, ApiError>".to_string()),
            is_async: true,
        }];
        let w = generate_crate_bridge_wrapper(&fns, "[]", &[]);
        assert!(
            !w.contains("serde_json::to_value(ret_val)"),
            "axum handler return must not require Serialize:\n{w}"
        );
        assert!(
            w.contains("Ok(_) => { obj.insert(\"return_value\".into(), Value::Null); }"),
            "axum handler return must be nulled out:\n{w}"
        );
    }

    #[test]
    fn generate_dispatch_harness_axum_path_uses_inner_type() {
        let fns = vec![CompatFn {
            name: "h".to_string(),
            param_names: vec!["id".to_string()],
            param_types: vec!["Path<u64>".to_string()],
            return_type: Some("String".to_string()),
            is_async: true,
        }];
        let source = "use axum::extract::Path;\nasync fn h(Path(id): Path<u64>) -> String { id.to_string() }";
        let h = generate_dispatch_harness(source, &fns, "[]", &[]).unwrap();
        assert!(
            !h.contains("let id: Path<u64>"),
            "dispatch harness must not declare param as extractor:\n{h}"
        );
        assert!(
            h.contains("id_inner: u64"),
            "dispatch harness must deserialize inner u64:\n{h}"
        );
        assert!(
            h.contains("axum::extract::Path(id_inner)"),
            "dispatch harness must reconstruct Path:\n{h}"
        );
    }

    #[test]
    fn generate_cargo_toml_with_axum() {
        let toml = generate_cargo_toml(Path::new("/fake/runtime"), false, true, false);
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
        let toml = generate_cargo_toml(Path::new("/fake/runtime"), true, false, false);
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
        let harness = generate_axum_harness(
            source,
            "create_user",
            &mappings,
            &["Json<String>".to_string()],
            &[None],
            "[]",
            &[(None, source)],
        )
        .unwrap();
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
        let harness = generate_axum_harness(
            source,
            "get_user",
            &mappings,
            &["Path<u64>".to_string()],
            &[None],
            "[]",
            &[(None, source)],
        )
        .unwrap();
        assert!(
            harness.contains("/{p0}"),
            "axum harness with Path extractor must use Axum 0.7+ path template\n\nharness:\n{harness}"
        );
        assert!(
            !harness.contains("/:p0"),
            "axum harness must not use legacy colon path template\n\nharness:\n{harness}"
        );
    }

    #[test]
    fn generate_axum_harness_with_tuple_path_uses_matching_route_segments() {
        use crate::adapters::{AxumExtractorKind, AxumExtractorMapping};

        let source = "use axum::extract::Path;\nasync fn get_user(Path((workspace_id, bundle_id)): Path<(Uuid, Uuid)>) -> String { format!(\"{workspace_id}:{bundle_id}\") }";
        let mappings = vec![AxumExtractorMapping {
            param_index: 0,
            kind: AxumExtractorKind::PathParams,
            type_name: "Path".to_string(),
        }];
        let harness = generate_axum_harness(
            source,
            "get_user",
            &mappings,
            &["Path<(Uuid, Uuid)>".to_string()],
            &[None],
            "[]",
            &[(None, source)],
        )
        .unwrap();
        assert!(
            harness.contains("/test/{p0}/{p1}"),
            "tuple Path extractor must mount the same number of route segments as tuple fields\n\nharness:\n{harness}"
        );
        assert!(
            harness.contains("00000000-0000-0000-0000-000000000001/00000000-0000-0000-0000-000000000002"),
            "null tuple Path input must fall back to parseable default UUID path segments\n\nharness:\n{harness}"
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
        let harness = generate_axum_harness(
            source,
            "handler",
            &mappings,
            &["Json<String>".to_string()],
            &[None],
            "[]",
            &[(None, source)],
        )
        .unwrap();
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
        let harness = generate_axum_harness(
            source,
            "handler",
            &mappings,
            &["Query<String>".to_string()],
            &[None],
            "[]",
            &[(None, source)],
        )
        .unwrap();
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
        let native_replays = vec![Some(NativeReplaySpec {
            input_index: 0,
            module_name: "__shatter_native_gen_0".to_string(),
            function_name: "StringState".to_string(),
            file_path: PathBuf::from("/tmp/string_state.rs"),
            recipe: serde_json::Value::Null,
        })];
        let harness = generate_axum_harness(
            source,
            "handler",
            &mappings,
            &["State<String>".to_string()],
            &native_replays,
            "[]",
            &[(None, source)],
        )
        .unwrap();
        assert!(
            harness.contains("with_state"),
            "harness with State extractor must call with_state\n\nharness:\n{harness}"
        );
    }

    #[test]
    fn public_invocation_path_uses_parent_reexport_for_private_child_module() {
        let dir = tempfile::tempdir().expect("tempdir");
        let src = dir.path().join("src");
        std::fs::create_dir_all(src.join("handlers")).expect("create handlers dir");
        std::fs::write(src.join("lib.rs"), "pub mod handlers;\n").expect("write lib");
        std::fs::write(
            src.join("handlers").join("mod.rs"),
            "mod workspaces;\npub use workspaces::{update_workspace, workspaces};\n",
        )
        .expect("write handlers mod");
        let source_file = src.join("handlers").join("workspaces.rs");
        std::fs::write(
            &source_file,
            "pub async fn workspaces() {}\npub async fn update_workspace() {}\n",
        )
        .expect("write workspaces");

        let target = public_invocation_path_for_crate_file(
            dir.path(),
            &source_file,
            "update_workspace",
            "pickpackit_api",
            "handlers::workspaces",
        );

        assert_eq!(
            target.as_deref(),
            Some("pickpackit_api::handlers::update_workspace")
        );
    }

    #[test]
    fn generate_axum_crate_harness_uses_public_invocation_target() {
        use crate::adapters::{AxumExtractorKind, AxumExtractorMapping};

        let mappings = vec![AxumExtractorMapping {
            param_index: 0,
            kind: AxumExtractorKind::PathParams,
            type_name: "Path".to_string(),
        }];
        let harness = generate_axum_crate_harness(
            "use axum::extract::Path;\npub async fn update_workspace(Path(id): Path<u64>) -> String { id.to_string() }",
            "update_workspace",
            "pickpackit_api",
            "handlers::workspaces",
            Some("pickpackit_api::handlers::update_workspace"),
            &mappings,
            &["Path<u64>".to_string()],
            &[None],
            "[]",
            &[],
        )
        .expect("generate harness");

        assert!(
            harness.contains("routing::any(pickpackit_api::handlers::update_workspace)"),
            "crate Axum harness must mount the public re-export target:\n{harness}"
        );
        assert!(
            !harness.contains("routing::any(pickpackit_api::handlers::workspaces::update_workspace)"),
            "crate Axum harness must not use the private child module path:\n{harness}"
        );
    }

    #[test]
    fn cached_harness_rebuilds_when_cargo_toml_changes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).expect("src dir");
        let binary = dir.path().join("target").join("debug").join("shatter-exec-temp");
        std::fs::create_dir_all(binary.parent().expect("binary parent")).expect("target dir");
        std::fs::write(&binary, "binary").expect("binary");
        let main_rs = src.join("main.rs");
        let cargo_toml = dir.path().join("Cargo.toml");
        std::fs::write(&main_rs, "fn main() {}\n").expect("main");
        std::fs::write(&cargo_toml, "[dependencies]\nold = \"1\"\n").expect("cargo");

        assert!(cached_harness_matches(
            &binary,
            &main_rs,
            "fn main() {}\n",
            &cargo_toml,
            "[dependencies]\nold = \"1\"\n"
        ));
        assert!(
            !cached_harness_matches(
                &binary,
                &main_rs,
                "fn main() {}\n",
                &cargo_toml,
                "[dependencies]\nnew = \"1\"\n"
            ),
            "generated Cargo.toml changes must invalidate cached harness binaries"
        );
    }

    #[test]
    fn execute_axum_handler_reports_native_replay_panic_without_subprocess_exit() {
        use crate::adapters::{AxumExtractorKind, AxumExtractorMapping};

        let dir = tempfile::tempdir().expect("tempdir");
        let source_file = dir.path().join("handler.rs");
        std::fs::write(
            &source_file,
            r#"
use axum::extract::State;

pub async fn handler(State(value): State<String>) -> String {
    value
}
"#,
        )
        .expect("write source");

        let generator_file = dir.path().join("panic_gen.rs");
        std::fs::write(
            &generator_file,
            r#"
use shatter_rust::generators::GeneratorResult;

pub fn PanicString(_recipe: Option<serde_json::Value>) -> GeneratorResult {
    panic!("native replay fixture panic")
}
"#,
        )
        .expect("write generator");

        let input = serde_json::json!({
            "__shatter_native": true,
            "handle": "panic-state",
            "__shatter_replay": {
                "language": "rust",
                "file": generator_file,
                "name": "PanicString",
                "recipe": null
            }
        });
        let mappings = vec![AxumExtractorMapping {
            param_index: 0,
            kind: AxumExtractorKind::AppState,
            type_name: "State".to_string(),
        }];
        let cache: HarnessCache = Mutex::new(HashMap::new());
        let crate_cache: CrateHarnessCache = Mutex::new(HashMap::new());
        let bridge_cache: CrateBridgeHarnessCache = Mutex::new(HashMap::new());

        let result = execute_axum_handler(
            &source_file.to_string_lossy(),
            "handler",
            &[input],
            &[],
            30_000,
            &mappings,
            &cache,
            &crate_cache,
            &bridge_cache,
        );

        match result {
            Ok(result) => {
                let message = result
                    .thrown_error
                    .as_ref()
                    .and_then(|err| err.get("message"))
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default();
                assert!(
                    message.contains("native replay fixture panic"),
                    "panic should be reported as structured thrown_error, got {result:?}"
                );
            }
            Err(ExecuteError::CompilationFailed(msg)) if is_offline_compile_error_message(&msg) => {
                eprintln!(
                    "skipping execute_axum_handler_reports_native_replay_panic_without_subprocess_exit: cargo unavailable ({msg})"
                );
            }
            Err(err) => panic!("panic should not terminate harness subprocess: {err:?}"),
        }
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
        let harness = generate_axum_harness(
            source,
            "handler",
            &mappings,
            &["Json<String>".to_string()],
            &[None],
            "[]",
            &[(None, source)],
        )
        .unwrap();
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
            &[],
            false,
        )
        .unwrap();

        // Should deserialize to String (owned), not &str
        assert!(
            harness.contains("name_owned: String = match serde_json::from_value"),
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
    fn execute_for_loop_emits_loop_body_states() {
        let dir = std::env::temp_dir().join("shatter-test-loop-body-states");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("test.rs");
        std::fs::write(
            &file,
            r#"
fn sum_to(n: i32) -> i32 {
    let mut total = 0;
    for i in 0..n {
        total += i;
    }
    total
}
"#,
        )
        .unwrap();

        let cache = std::sync::Mutex::new(std::collections::HashMap::new());
        let crate_cache = std::sync::Mutex::new(std::collections::HashMap::new());
        let bridge_cache = std::sync::Mutex::new(std::collections::HashMap::new());
        let result = execute_function(
            &file.to_string_lossy(),
            "sum_to",
            &[serde_json::json!(3)],
            &[],
            5000,
            None,
            &cache,
            &crate_cache,
            &bridge_cache,
        );

        match result {
            Ok(result) => {
                assert_eq!(result.loop_body_states.len(), 3);
                for (idx, state) in result.loop_body_states.iter().enumerate() {
                    assert_eq!(state["loop_id"], serde_json::json!(0));
                    assert_eq!(state["iteration"], serde_json::json!(idx as u32));
                }
            }
            Err(ExecuteError::CompilationFailed(msg)) if is_offline_compile_error_message(&msg) => {
                eprintln!(
                    "skipping execute_for_loop_emits_loop_body_states: cargo unavailable ({msg})"
                );
            }
            Err(err) => panic!("execute failed: {err:?}"),
        }

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
    // is_harness_incompatible_attr tests (str-xt4h)
    // -------------------------------------------------------------------------

    #[test]
    fn harness_incompatible_attr_blocks_no_std() {
        let attr: syn::Attribute = syn::parse_quote!(#![no_std]);
        assert!(is_harness_incompatible_attr(&attr));
    }

    #[test]
    fn harness_incompatible_attr_blocks_panic_handler() {
        let attr: syn::Attribute = syn::parse_quote!(#[panic_handler]);
        assert!(is_harness_incompatible_attr(&attr));
    }

    #[test]
    fn harness_incompatible_attr_blocks_lang() {
        let attr: syn::Attribute = syn::parse_quote!(#[lang = "eh_personality"]);
        assert!(is_harness_incompatible_attr(&attr));
    }

    #[test]
    fn harness_incompatible_attr_allows_derive() {
        let attr: syn::Attribute = syn::parse_quote!(#[derive(Debug)]);
        assert!(!is_harness_incompatible_attr(&attr));
    }

    #[test]
    fn harness_incompatible_attr_allows_cfg() {
        let attr: syn::Attribute = syn::parse_quote!(#[cfg(test)]);
        assert!(!is_harness_incompatible_attr(&attr));
    }

    #[test]
    fn wrap_in_module_strips_no_std_attr() {
        let source = "#![no_std]\nfn add(a: i32, b: i32) -> i32 { a + b }";
        let wrapped = wrap_in_module(source).unwrap();
        assert!(
            !wrapped.contains("no_std"),
            "wrap_in_module must strip #![no_std]; got:\n{wrapped}"
        );
    }

    // -------------------------------------------------------------------------
    // is_raw_pointer_or_extern_type tests (str-xt4h)
    // -------------------------------------------------------------------------

    #[test]
    fn raw_pointer_type_detects_const_ptr() {
        assert!(is_raw_pointer_or_extern_type("*const u8"));
        assert!(is_raw_pointer_or_extern_type("* const u8"));
    }

    #[test]
    fn raw_pointer_type_detects_mut_ptr() {
        assert!(is_raw_pointer_or_extern_type("*mut T"));
        assert!(is_raw_pointer_or_extern_type("* mut T"));
    }

    #[test]
    fn raw_pointer_type_detects_fn_ptr() {
        // Top-level fn pointers are caught; fn pointers inside generics (e.g.
        // Option<fn(i32)>) are not — the check looks for ` fn(` or a leading
        // `fn(`, so `<fn(` slips through as a known limitation.
        assert!(is_raw_pointer_or_extern_type("fn(i32) -> bool"));
    }

    #[test]
    fn raw_pointer_type_detects_extern_fn() {
        assert!(is_raw_pointer_or_extern_type("extern \"C\" fn(i32)"));
    }

    #[test]
    fn raw_pointer_type_allows_plain_types() {
        assert!(!is_raw_pointer_or_extern_type("i32"));
        assert!(!is_raw_pointer_or_extern_type("String"));
        assert!(!is_raw_pointer_or_extern_type("Vec<u8>"));
        assert!(!is_raw_pointer_or_extern_type("&str"));
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
            &[],
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
        let toml = generate_cargo_toml(&runtime_path, false, false, false);
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
    fn crate_bridge_target_dir_uses_shared_cache_env() {
        let _lock = crate::ENV_LOCK.lock().unwrap();
        let cache_root = std::env::temp_dir().join("shatter-test-crate-bridge-cache");
        let cache_str = cache_root.to_string_lossy().into_owned();
        unsafe { std::env::set_var("SHATTER_HARNESS_CACHE", &cache_str) };

        let harness_dir = cache_root
            .join("rust")
            .join("crate-bridge")
            .join("0123456789abcdef");
        let target = crate_bridge_target_dir(&harness_dir);
        unsafe { std::env::set_var("SHATTER_HARNESS_CACHE", "") };

        assert_eq!(
            target,
            cache_root.join("rust").join("crate-bridge").join("target"),
            "crate-bridge harnesses should share dependency build artifacts instead of creating one target tree per harness key"
        );
        assert!(
            !target.starts_with(&harness_dir),
            "target dir {target:?} must not be nested under per-harness cache key {harness_dir:?}"
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
        let result = generate_cargo_toml_with_user_deps(user_toml, runtime_path, false, false, false);
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
        let result = generate_cargo_toml_with_user_deps(user_toml, runtime_path, false, false, false);
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
    /// path to the source file.  The crate includes serde so input structs can
    /// derive the traits needed by generated harnesses.
    fn write_test_crate(dir: &std::path::Path, source: &str) -> PathBuf {
        let src_dir = dir.join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"shatter-test\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nserde = { version = \"1\", features = [\"derive\"] }\n",
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
        pub(crate) enabled: bool,
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
    fn crate_bridge_wrapper_skips_axum_return_capture_without_extractor_params() {
        let fns = vec![CompatFn {
            name: "health".to_string(),
            param_names: vec![],
            param_types: vec![],
            return_type: Some("Json<Value>".to_string()),
            is_async: true,
        }];
        let wrapper = generate_crate_bridge_wrapper(&fns, "[]", &[]);
        assert!(wrapper.contains("shatter_wrap_health"));
        assert!(wrapper.contains("\"health\" => shatter_wrap_health(inputs)"));
        assert!(
            !wrapper.contains("serde_json::to_value(ret_val)"),
            "Axum response wrappers are not Serialize and must not be captured directly"
        );
        assert!(
            wrapper.contains("Ok(_) => { obj.insert(\"return_value\".into(), Value::Null); }"),
            "Axum response wrappers should still produce a well-formed execution result"
        );
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
        let toml = generate_crate_bridge_cargo_toml(
            "shatter-crate-bridge-exec-0123456789abcdef",
            "my-crate",
            std::path::Path::new("/some/path"),
            false,
        );
        assert!(
            toml.contains(r#"name = "shatter-crate-bridge-exec-0123456789abcdef""#),
            "driver package name must be caller-controlled so shared target dirs do not overwrite binaries"
        );
        assert!(toml.contains("shatter-crate-bridge"), "Cargo.toml must activate the shatter-crate-bridge feature");
        assert!(toml.contains("[workspace]"), "must opt out of parent workspace");
        assert!(toml.contains("debug = 0"), "dev builds should not emit throwaway debug info");
        assert!(toml.contains("incremental = false"), "dev builds should avoid per-driver incremental state");
    }

    #[test]
    fn axum_crate_driver_cargo_toml_activates_crate_bridge_feature() {
        let toml = generate_axum_crate_driver_cargo_toml(
            "shatter-exec-temp-0123456789abcdef",
            "my_crate",
            std::path::Path::new("/some/path"),
            std::path::Path::new("/runtime"),
            "[dependencies]\nserde_json = \"1\"\n",
        );

        assert!(
            toml.contains(r#"name = "shatter-exec-temp-0123456789abcdef""#),
            "Axum crate driver package name must be caller-controlled so shared target dirs do not overwrite binaries"
        );
        assert!(
            toml.contains(r#"my_crate = { path = "/some/path", features = ["shatter-crate-bridge"] }"#),
            "Axum crate driver must activate the staged crate feature so instrumented modules can see shatter-rust-runtime\n\n{toml}"
        );
        assert!(toml.contains("debug = 0"), "dev builds should not emit throwaway debug info");
        assert!(toml.contains("incremental = false"), "dev builds should avoid per-driver incremental state");
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

    #[test]
    fn crate_bridge_reports_executed_branch_lines() {
        let dir = std::env::temp_dir().join("shatter-test-bridge-coverage");
        let src_file = write_test_crate(
            &dir,
            "mod config {\n    #[derive(serde::Deserialize, serde::Serialize)]\n    pub(crate) struct Config { pub(crate) enabled: bool }\n}\n\nuse crate::config::Config;\n\nfn classify(config: Config) -> &'static str {\n    if config.enabled { \"enabled\" } else { \"disabled\" }\n}\n",
        );

        let cache: HarnessCache = Mutex::new(HashMap::new());
        let crate_cache: CrateHarnessCache = Mutex::new(HashMap::new());
        let bridge_cache: CrateBridgeHarnessCache = Mutex::new(HashMap::new());

        let result = execute_function_with_timing(
            src_file.to_str().unwrap(),
            "classify",
            &[serde_json::json!({"enabled": true})],
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
                    Some(serde_json::json!("enabled")),
                    "classify({{enabled: true}}) should return 'enabled'"
                );
                assert!(
                    r.lines_executed.iter().any(|line| *line > 0),
                    "crate_bridge should report executed source lines for instrumented branches"
                );
            }
            Err(ExecuteError::CompilationFailed(msg))
                if cargo_build_unavailable(&msg) =>
            {
                eprintln!("skipping crate_bridge_reports_executed_branch_lines: cargo unavailable ({msg})");
            }
            Err(e) => panic!("unexpected error: {e:?}"),
        }
    }

    // ─── str-31j.3: crate_bridge respects nested module context ──────────────

    /// Build a fixture crate at `dir` with a nested module containing
    /// module-local imports, one public function, and one private helper.
    /// The pickpackit-style failure mode this guards against: emitting a
    /// root-level wrapper that fails with E0425 "cannot find type `Config`"
    /// and "cannot find function `bearer_token` in module `super`".
    fn write_nested_module_fixture(dir: &std::path::Path) -> PathBuf {
        let _ = std::fs::remove_dir_all(dir);
        let src = dir.join("src");
        std::fs::create_dir_all(src.join("auth")).unwrap();
        std::fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"shatter-31j3-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nserde = { version = \"1\", features = [\"derive\"] }\n",
        ).unwrap();
        std::fs::write(
            src.join("lib.rs"),
            "pub mod auth;\n\n#[derive(serde::Serialize, serde::Deserialize, Clone)]\npub struct Config { pub token: String }\n",
        ).unwrap();
        std::fs::write(
            src.join("auth").join("mod.rs"),
            "pub mod middleware;\n",
        ).unwrap();
        let target = src.join("auth").join("middleware.rs");
        std::fs::write(
            &target,
            "use crate::Config;\n\nfn bearer_token(c: Config) -> String {\n    format!(\"Bearer {}\", c.token)\n}\n\npub fn check_auth(c: Config) -> String {\n    bearer_token(c.clone())\n}\n",
        ).unwrap();
        target
    }

    #[test]
    fn crate_bridge_nested_module_public_fn_compiles_and_executes() {
        let dir = std::env::temp_dir().join("shatter-test-31j3-pub");
        let target = write_nested_module_fixture(&dir);

        let cache: HarnessCache = Mutex::new(HashMap::new());
        let crate_cache: CrateHarnessCache = Mutex::new(HashMap::new());
        let bridge_cache: CrateBridgeHarnessCache = Mutex::new(HashMap::new());

        let result = execute_function_with_timing(
            target.to_str().unwrap(),
            "check_auth",
            &[serde_json::json!({"token": "abc"})],
            &[],
            120_000,
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
                    Some(serde_json::json!("Bearer abc")),
                    "check_auth({{token: abc}}) should return 'Bearer abc'"
                );
            }
            Err(ExecuteError::CompilationFailed(msg))
                if msg.contains("No such file") || msg.contains("spurious network error")
                    || msg.contains("download of config.json failed")
                    || msg.contains("Could not resolve host") =>
            {
                eprintln!("skipping nested_module_public: cargo unavailable ({msg})");
            }
            Err(e) => panic!(
                "expected nested-module public wrapper to compile and execute, got: {e:?}"
            ),
        }
    }

    #[test]
    fn crate_bridge_nested_module_private_helper_works_or_unsupported() {
        let dir = std::env::temp_dir().join("shatter-test-31j3-priv");
        let target = write_nested_module_fixture(&dir);

        let cache: HarnessCache = Mutex::new(HashMap::new());
        let crate_cache: CrateHarnessCache = Mutex::new(HashMap::new());
        let bridge_cache: CrateBridgeHarnessCache = Mutex::new(HashMap::new());

        let result = execute_function_with_timing(
            target.to_str().unwrap(),
            "bearer_token",
            &[serde_json::json!({"token": "xyz"})],
            &[],
            120_000,
            Some("crate_bridge"),
            None,
            &cache,
            &crate_cache,
            &bridge_cache,
        );

        let _ = std::fs::remove_dir_all(&dir);

        match result {
            Ok(r) => {
                assert_eq!(r.return_value, Some(serde_json::json!("Bearer xyz")));
            }
            Err(ExecuteError::NonExecutable(_)) => {
                // Acceptable: classified unsupported up-front (no broken codegen).
            }
            Err(ExecuteError::CompilationFailed(msg))
                if msg.contains("No such file") || msg.contains("spurious network error")
                    || msg.contains("download of config.json failed")
                    || msg.contains("Could not resolve host") =>
            {
                eprintln!("skipping nested_module_private: cargo unavailable ({msg})");
            }
            Err(e) => panic!(
                "private helper must either execute or be classified NonExecutable, got: {e:?}"
            ),
        }
    }

    // str-31j.1: BridgeSourceBackup invariants — restore must return touched
    // files byte-for-byte and remove any path that did not exist before.

    fn unique_tmp_dir(tag: &str) -> PathBuf {
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("shatter-31j1-{tag}-{pid}-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn crate_bridge_unsupported_sibling_does_not_poison_requested_function() {
        let dir = unique_tmp_dir("sibling-poison");
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"shatter-sibling-poison-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[lib]\npath = \"src/lib.rs\"\n",
        )
        .unwrap();
        std::fs::write(dir.join("src/lib.rs"), "pub mod app;\n").unwrap();
        let target = dir.join("src/app.rs");
        std::fs::write(
            &target,
            "pub struct OpaqueInput;\npub struct OpaqueOutput;\n\npub fn unsupported(_input: OpaqueInput) -> OpaqueOutput {\n    OpaqueOutput\n}\n\npub fn classify(n: i32) -> String {\n    if n > 0 { \"pos\" } else { \"nonpos\" }.to_string()\n}\n",
        )
        .unwrap();

        let cache: HarnessCache = Mutex::new(HashMap::new());
        let crate_cache: CrateHarnessCache = Mutex::new(HashMap::new());
        let bridge_cache: CrateBridgeHarnessCache = Mutex::new(HashMap::new());

        let result = execute_function_with_timing(
            target.to_str().unwrap(),
            "classify",
            &[serde_json::json!(1)],
            &[],
            120_000,
            Some("crate_bridge"),
            None,
            &cache,
            &crate_cache,
            &bridge_cache,
        );

        let unsupported_result = execute_function_with_timing(
            target.to_str().unwrap(),
            "unsupported",
            &[serde_json::json!(null)],
            &[],
            120_000,
            Some("crate_bridge"),
            None,
            &cache,
            &crate_cache,
            &bridge_cache,
        );

        match result {
            Ok(r) => assert_eq!(
                r.return_value,
                Some(serde_json::json!("pos")),
                "supported functions must execute even when the same file has unsupported siblings",
            ),
            Err(ExecuteError::CompilationFailed(msg))
                if msg.contains("No such file")
                    || msg.contains("spurious network error")
                    || msg.contains("download of config.json failed")
                    || msg.contains("Could not resolve host") =>
            {
                eprintln!("skipping sibling poison regression: cargo unavailable ({msg})");
            }
            Err(e) => panic!("expected requested function to execute despite sibling, got: {e:?}"),
        }

        match unsupported_result {
            Err(ExecuteError::NonExecutable(msg)) => assert!(
                msg.contains("JSON-harness compatible"),
                "unsupported sibling should be classified clearly, got: {msg}"
            ),
            Err(ExecuteError::CompilationFailed(msg))
                if msg.contains("No such file")
                    || msg.contains("spurious network error")
                    || msg.contains("download of config.json failed")
                    || msg.contains("Could not resolve host") =>
            {
                eprintln!("skipping unsupported classification assertion: cargo unavailable ({msg})");
            }
            other => panic!(
                "expected unsupported sibling to be classified NonExecutable, got: {other:?}"
            ),
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn backup_restores_modified_file_byte_for_byte() {
        let dir = unique_tmp_dir("modified");
        let file = dir.join("a.txt");
        let original = b"original\n contents \xff\xfe binary tail\n";
        std::fs::write(&file, original).unwrap();

        let backup = BridgeSourceBackup::snapshot(std::slice::from_ref(&file)).unwrap();
        std::fs::write(&file, b"trampled").unwrap();

        backup.restore();
        let after = std::fs::read(&file).unwrap();
        assert_eq!(after, original, "modified file must be restored byte-for-byte");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn backup_removes_files_created_after_snapshot() {
        let dir = unique_tmp_dir("created");
        let created = dir.join("__shatter.rs");
        assert!(!created.exists());

        let backup = BridgeSourceBackup::snapshot(std::slice::from_ref(&created)).unwrap();
        std::fs::write(&created, b"injected").unwrap();
        assert!(created.exists());

        backup.restore();
        assert!(!created.exists(), "files absent at snapshot must be removed on restore");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn backup_restore_is_idempotent_and_drop_safe() {
        let dir = unique_tmp_dir("idem");
        let existed = dir.join("cargo.toml");
        let created = dir.join("__shatter.rs");
        std::fs::write(&existed, b"[package]\n").unwrap();

        let backup = BridgeSourceBackup::snapshot(&[existed.clone(), created.clone()]).unwrap();
        std::fs::write(&existed, b"trampled").unwrap();
        std::fs::write(&created, b"injected").unwrap();

        backup.restore();
        assert!(backup.is_restored());
        // Re-running restore must be a no-op even after we manually mutate again.
        std::fs::write(&existed, b"mutated-after-restore").unwrap();
        backup.restore();
        assert_eq!(
            std::fs::read(&existed).unwrap(),
            b"mutated-after-restore",
            "second restore must be a no-op (already-restored guard)"
        );

        // Drop after-the-fact must not double-restore either.
        drop(backup);
        assert_eq!(std::fs::read(&existed).unwrap(), b"mutated-after-restore");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn backup_drop_restores_on_unwind() {
        let dir = unique_tmp_dir("drop");
        let file = dir.join("a.txt");
        std::fs::write(&file, b"original").unwrap();

        {
            let backup = BridgeSourceBackup::snapshot(std::slice::from_ref(&file)).unwrap();
            std::fs::write(&file, b"trampled").unwrap();
            drop(backup);
        }

        assert_eq!(
            std::fs::read(&file).unwrap(),
            b"original",
            "Drop impl must restore the snapshot",
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// str-31j.1 / str-ja70 regression: drive `execute_function_crate_bridge`
    /// against a fixture crate. Force a failure on the slow path by setting a
    /// tiny timeout so the harness build cannot complete. With the staging-copy
    /// approach (str-ja70), original files are never modified — assert every
    /// source file is byte-for-byte identical to its pre-call state and that
    /// no `src/__shatter.rs` was created in the original tree.
    #[test]
    fn crate_bridge_failure_path_leaves_target_clean() {
        let dir = unique_tmp_dir("regression-cleanup");
        std::fs::create_dir_all(dir.join("src")).unwrap();

        let cargo_toml = "[package]\nname = \"fixture-31j1\"\nversion = \"0.0.1\"\nedition = \"2021\"\n\n[lib]\npath = \"src/lib.rs\"\n";
        let lib_rs = "pub mod app;\n";
        let app_rs = "pub fn classify(n: i32) -> &'static str {\n    if n < 0 { \"neg\" } else if n == 0 { \"zero\" } else { \"pos\" }\n}\n";
        std::fs::write(dir.join("Cargo.toml"), cargo_toml).unwrap();
        std::fs::write(dir.join("src").join("lib.rs"), lib_rs).unwrap();
        let app_path = dir.join("src").join("app.rs");
        std::fs::write(&app_path, app_rs).unwrap();

        let cache: HarnessCache = Mutex::new(HashMap::new());
        let crate_cache: CrateHarnessCache = Mutex::new(HashMap::new());
        let bridge_cache: CrateBridgeHarnessCache = Mutex::new(HashMap::new());

        // Force a failure: 1ms timeout cannot complete a real cargo build, so
        // either build_and_spawn_crate_bridge_harness errors out or the dispatch
        // result is a timeout. With the staging-copy approach, original files are
        // never modified regardless of which branch is taken.
        let result = execute_function_with_timing(
            app_path.to_str().unwrap(),
            "classify",
            &[serde_json::json!(1)],
            &[],
            1,
            Some("crate_bridge"),
            None,
            &cache,
            &crate_cache,
            &bridge_cache,
        );
        drop(result);

        // Drop the cache to release any held resources (harness dir, subprocess).
        drop(bridge_cache);

        assert_eq!(
            std::fs::read(dir.join("Cargo.toml")).unwrap(),
            cargo_toml.as_bytes(),
            "Cargo.toml must be byte-for-byte unchanged after failed crate_bridge run",
        );
        assert_eq!(
            std::fs::read(dir.join("src").join("lib.rs")).unwrap(),
            lib_rs.as_bytes(),
            "lib.rs must be byte-for-byte unchanged after failed crate_bridge run",
        );
        assert_eq!(
            std::fs::read(&app_path).unwrap(),
            app_rs.as_bytes(),
            "src/app.rs must be byte-for-byte unchanged after failed crate_bridge run",
        );
        assert!(
            !dir.join("src").join("__shatter.rs").exists(),
            "src/__shatter.rs (created during injection) must be removed",
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    // str-ja70: staging-copy unit tests — verify the staging copy machinery
    // works correctly and never modifies originals.

    #[test]
    fn staging_copy_preserves_original_files() {
        let dir = unique_tmp_dir("staging-immutability");
        std::fs::create_dir_all(dir.join("src")).unwrap();

        let cargo_toml = "[package]\nname = \"fixture-ja70\"\nversion = \"0.0.1\"\nedition = \"2021\"\n\n[lib]\npath = \"src/lib.rs\"\n";
        let lib_rs = "pub mod app;\n";
        let app_rs = "pub fn add(a: i32, b: i32) -> i32 { a + b }\n";
        std::fs::write(dir.join("Cargo.toml"), cargo_toml).unwrap();
        std::fs::write(dir.join("src/lib.rs"), lib_rs).unwrap();
        std::fs::write(dir.join("src/app.rs"), app_rs).unwrap();

        // Snapshot originals before staging.
        let orig_cargo = std::fs::read(dir.join("Cargo.toml")).unwrap();
        let orig_lib = std::fs::read(dir.join("src/lib.rs")).unwrap();
        let orig_app = std::fs::read(dir.join("src/app.rs")).unwrap();

        let staging_root = unique_tmp_dir("staging-dest");
        let staging_crate = create_crate_staging_copy(&dir, &staging_root).unwrap();

        // Staging copy must exist with the expected structure.
        assert!(staging_crate.join("src/lib.rs").exists(), "staging must have src/lib.rs");
        assert!(staging_crate.join("src/app.rs").exists(), "staging must have src/app.rs");
        assert!(staging_crate.join("Cargo.toml").exists(), "staging must have Cargo.toml");

        // Mutate the staging copy to simulate instrumentation.
        std::fs::write(staging_crate.join("src/app.rs"), "// mutated by shatter\n").unwrap();
        std::fs::write(staging_crate.join("src/__shatter.rs"), "// stub\n").unwrap();

        // Original files must be byte-for-byte unchanged.
        assert_eq!(
            std::fs::read(dir.join("Cargo.toml")).unwrap(), orig_cargo,
            "Cargo.toml must be byte-for-byte unchanged after staging copy + mutation",
        );
        assert_eq!(
            std::fs::read(dir.join("src/lib.rs")).unwrap(), orig_lib,
            "lib.rs must be byte-for-byte unchanged after staging copy + mutation",
        );
        assert_eq!(
            std::fs::read(dir.join("src/app.rs")).unwrap(), orig_app,
            "src/app.rs must be byte-for-byte unchanged after staging copy + mutation",
        );
        assert!(
            !dir.join("src/__shatter.rs").exists(),
            "src/__shatter.rs must not appear in original tree",
        );

        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&staging_root);
    }

    #[test]
    fn resolve_cargo_toml_paths_makes_relative_absolute() {
        let crate_root = Path::new("/home/user/project/my-crate");
        let input = r#"[package]
name = "my-crate"
version = "0.1.0"

[dependencies]
runtime = { path = "../runtime" }
other = { path = '/abs/path' }
"#;
        let result = resolve_cargo_toml_paths(input, crate_root, None);
        // Relative path must become absolute.
        assert!(
            result.contains("/home/user/project/my-crate/../runtime"),
            "relative dep path must be resolved to absolute: {result}",
        );
        // Non-relative paths remain unchanged.
        assert!(
            result.contains("/abs/path"),
            "absolute path must be preserved: {result}",
        );
        // [workspace] injected for isolation.
        assert!(
            result.contains("[workspace]"),
            "must inject [workspace] for isolation: {result}",
        );
    }

    #[test]
    fn resolve_cargo_toml_paths_preserves_existing_workspace() {
        let crate_root = Path::new("/tmp/crate");
        let input = "[workspace]\nmembers = []\n\n[package]\nname = \"ws\"\n";
        let result = resolve_cargo_toml_paths(input, crate_root, None);
        // Must not double-inject [workspace].
        assert_eq!(
            result.matches("[workspace]").count(), 1,
            "must not duplicate [workspace]: {result}",
        );
    }

    /// str-ja70: [lib] path must not be absolutised — it is a source file path
    /// relative to the staging crate root, not a crate dependency.
    #[test]
    fn resolve_cargo_toml_paths_does_not_absolutise_lib_path() {
        let crate_root = Path::new("/home/user/project/my-crate");
        let input = r#"[package]
name = "my-crate"
version = "0.1.0"

[lib]
path = "src/lib.rs"

[dependencies]
runtime = { path = "../runtime" }
"#;
        let result = resolve_cargo_toml_paths(input, crate_root, None);
        // [lib] path must remain relative so the staging copy uses its own file.
        assert!(
            result.contains(r#"path = "src/lib.rs""#),
            "[lib] path must not be absolutised in staging copy: {result}",
        );
        // Dep path must be absolutised.
        assert!(
            result.contains("/home/user/project/my-crate/../runtime"),
            "dep path must be absolutised: {result}",
        );
    }

    #[test]
    fn has_workspace_inheritance_detects_dotted_fields() {
        assert!(has_workspace_inheritance("edition.workspace = true\n"));
        assert!(has_workspace_inheritance("[package]\nedition.workspace = true\n"));
        assert!(has_workspace_inheritance("rust-version.workspace = true\n"));
        assert!(!has_workspace_inheritance("[package]\nedition = \"2021\"\n"));
        assert!(!has_workspace_inheritance(""));
    }

    #[test]
    fn resolve_cargo_toml_paths_injects_workspace_package_fields() {
        let crate_root = Path::new("/tmp/member");
        let input = r#"[package]
name = "member"
edition.workspace = true

[dependencies]
"#;
        let ws_fields = "edition = \"2021\"\nrust-version = \"1.70\"\n";
        let result = resolve_cargo_toml_paths(input, crate_root, Some(ws_fields));
        assert!(
            result.contains("[workspace.package]"),
            "must inject [workspace.package]: {result}",
        );
        assert!(
            result.contains("edition = \"2021\""),
            "must include inherited edition: {result}",
        );
        assert!(
            result.contains("rust-version = \"1.70\""),
            "must include inherited rust-version: {result}",
        );
    }

    #[test]
    fn resolve_cargo_toml_paths_no_ws_fields_when_no_inheritance() {
        let crate_root = Path::new("/tmp/standalone");
        let input = "[package]\nname = \"standalone\"\nedition = \"2021\"\n";
        let result = resolve_cargo_toml_paths(input, crate_root, None);
        assert!(
            !result.contains("[workspace.package]"),
            "must not inject [workspace.package] without inheritance: {result}",
        );
        assert!(
            result.contains("[workspace]"),
            "must still inject [workspace] for isolation: {result}",
        );
    }

    /// str-n374: workspace member crate staging preserves workspace metadata.
    #[test]
    fn staging_copy_preserves_workspace_package_inheritance() {
        let ws_root = unique_tmp_dir("ws-root-n374");
        let member_root = ws_root.join("api");
        std::fs::create_dir_all(member_root.join("src")).unwrap();

        std::fs::write(ws_root.join("Cargo.toml"), r#"[workspace]
members = ["api"]

[workspace.package]
edition = "2021"
rust-version = "1.70"
"#).unwrap();

        std::fs::write(member_root.join("Cargo.toml"), r#"[package]
name = "api"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true

[dependencies]
"#).unwrap();

        std::fs::write(member_root.join("src/lib.rs"), "pub fn hello() {}\n").unwrap();

        let staging_root = unique_tmp_dir("staging-n374");
        let staging_crate = create_crate_staging_copy(&member_root, &staging_root).unwrap();

        let staged_toml = std::fs::read_to_string(staging_crate.join("Cargo.toml")).unwrap();
        assert!(
            staged_toml.contains("[workspace.package]"),
            "staged Cargo.toml must have [workspace.package]: {staged_toml}",
        );
        assert!(
            staged_toml.contains("edition = \"2021\""),
            "staged Cargo.toml must resolve edition from workspace: {staged_toml}",
        );
        assert!(
            staged_toml.contains("rust-version = \"1.70\""),
            "staged Cargo.toml must resolve rust-version from workspace: {staged_toml}",
        );

        // Original files must not be mutated.
        let orig_member = std::fs::read_to_string(member_root.join("Cargo.toml")).unwrap();
        assert!(
            orig_member.contains("edition.workspace = true"),
            "original Cargo.toml must not be mutated",
        );

        let _ = std::fs::remove_dir_all(&ws_root);
        let _ = std::fs::remove_dir_all(&staging_root);
    }

    /// str-n374: root-level crates without workspace inheritance still work.
    #[test]
    fn staging_copy_works_for_non_workspace_crate() {
        let crate_root = unique_tmp_dir("standalone-n374");
        std::fs::create_dir_all(crate_root.join("src")).unwrap();

        std::fs::write(crate_root.join("Cargo.toml"), r#"[package]
name = "standalone"
version = "0.1.0"
edition = "2021"
"#).unwrap();

        std::fs::write(crate_root.join("src/lib.rs"), "pub fn hello() {}\n").unwrap();

        let staging_root = unique_tmp_dir("staging-standalone-n374");
        let staging_crate = create_crate_staging_copy(&crate_root, &staging_root).unwrap();

        let staged_toml = std::fs::read_to_string(staging_crate.join("Cargo.toml")).unwrap();
        assert!(
            !staged_toml.contains("[workspace.package]"),
            "standalone crate must not have [workspace.package]: {staged_toml}",
        );
        assert!(
            staged_toml.contains("[workspace]"),
            "standalone crate must still have [workspace] for isolation: {staged_toml}",
        );

        let _ = std::fs::remove_dir_all(&crate_root);
        let _ = std::fs::remove_dir_all(&staging_root);
    }

    /// str-jx3r: writing instrumented source to a nested staging path must
    /// succeed even when intermediate directories don't exist yet.
    #[test]
    fn staging_write_creates_parent_dirs_for_nested_files() {
        let crate_root = unique_tmp_dir("staging-nested-src");
        std::fs::create_dir_all(crate_root.join("api/src")).unwrap();

        let cargo_toml = "[package]\nname = \"fixture-jx3r\"\nversion = \"0.0.1\"\nedition = \"2021\"\n";
        let lib_rs = "pub mod nested;\n";
        let nested_rs = "pub fn greet(n: i32) -> &'static str { if n > 0 { \"pos\" } else { \"non-pos\" } }\n";
        std::fs::write(crate_root.join("Cargo.toml"), cargo_toml).unwrap();
        std::fs::create_dir_all(crate_root.join("src")).unwrap();
        std::fs::write(crate_root.join("src/lib.rs"), lib_rs).unwrap();

        // Nested source lives outside `src/` — e.g. a workspace member path.
        std::fs::write(crate_root.join("api/src/suggestions.rs"), nested_rs).unwrap();

        let staging_root = unique_tmp_dir("staging-nested-dest");
        let staging_crate = create_crate_staging_copy(&crate_root, &staging_root).unwrap();

        // The staging copy has src/ but NOT api/src/.
        assert!(!staging_crate.join("api/src").exists(),
            "staging copy should not have api/src before fix");

        // Simulate the instrumented write path (post-fix): create parent dirs
        // then write.
        let rel_file = Path::new("api/src/suggestions.rs");
        let staging_file = staging_crate.join(rel_file);
        if let Some(parent) = staging_file.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&staging_file, "// instrumented\n").unwrap();

        assert!(staging_file.exists(), "nested staging file must exist after write");

        // Original must be unchanged.
        assert_eq!(
            std::fs::read_to_string(crate_root.join("api/src/suggestions.rs")).unwrap(),
            nested_rs,
            "original nested source must be byte-for-byte unchanged",
        );

        let _ = std::fs::remove_dir_all(&crate_root);
        let _ = std::fs::remove_dir_all(&staging_root);
    }

    // ─── str-gc0r: compile-time asset copying ────────────────────────────

    #[test]
    fn find_compile_time_asset_paths_basic() {
        let src = r#"
            const A: &str = include_str!("../fixtures/dev.json");
            const B: &[u8] = include_bytes!("blob.bin");
            static MIG: () = sqlx::migrate!("./migrations");
            static MIG2: () = sqlx::migrate!();
            // false positives:
            fn my_include_str() {}
            let x = "include_str!(\"not a macro\")";
        "#;
        let refs = find_compile_time_asset_paths(src);
        let paths: Vec<(&str, AssetBase)> = refs.iter().map(|r| (r.path.as_str(), r.base)).collect();
        assert!(paths.contains(&("../fixtures/dev.json", AssetBase::File)), "include_str path missing: {paths:?}");
        assert!(paths.contains(&("blob.bin", AssetBase::File)), "include_bytes path missing: {paths:?}");
        assert!(paths.contains(&("./migrations", AssetBase::Manifest)), "sqlx::migrate path missing: {paths:?}");
        assert!(paths.contains(&("migrations", AssetBase::Manifest)), "sqlx::migrate!() default missing: {paths:?}");
    }

    #[test]
    fn staging_copy_includes_compile_time_assets() {
        let crate_root = unique_tmp_dir("staging-gc0r-assets");
        std::fs::create_dir_all(crate_root.join("src")).unwrap();
        std::fs::create_dir_all(crate_root.join("fixtures")).unwrap();
        std::fs::create_dir_all(crate_root.join("migrations")).unwrap();

        std::fs::write(crate_root.join("Cargo.toml"),
            "[package]\nname = \"gc0r-fix\"\nversion = \"0.0.1\"\nedition = \"2021\"\n").unwrap();
        std::fs::write(crate_root.join("src/lib.rs"),
            "pub mod seed;\npub mod migrations;\n").unwrap();
        std::fs::write(crate_root.join("src/seed.rs"),
            "pub const DEV_FIXTURE: &str = include_str!(\"../fixtures/dev.json\");\n").unwrap();
        std::fs::write(crate_root.join("src/migrations.rs"),
            "pub fn run() { sqlx::migrate!(\"./migrations\"); }\n").unwrap();
        std::fs::write(crate_root.join("fixtures/dev.json"), "{\"k\":1}\n").unwrap();
        std::fs::write(crate_root.join("migrations/0001_init.sql"), "CREATE TABLE t (id INT);\n").unwrap();

        let staging_root = unique_tmp_dir("staging-gc0r-dest");
        let staging_crate = create_crate_staging_copy(&crate_root, &staging_root).unwrap();

        assert!(staging_crate.join("fixtures/dev.json").exists(),
            "fixtures/dev.json must be copied for include_str!");
        assert!(staging_crate.join("migrations/0001_init.sql").exists(),
            "migrations/ must be copied for sqlx::migrate!");

        // Original tree must not be mutated.
        assert!(crate_root.join("fixtures/dev.json").exists());
        assert!(crate_root.join("migrations/0001_init.sql").exists());

        let _ = std::fs::remove_dir_all(&crate_root);
        let _ = std::fs::remove_dir_all(&staging_root);
    }

    #[test]
    fn staging_copy_skips_assets_escaping_crate_root() {
        let crate_root = unique_tmp_dir("staging-gc0r-escape");
        std::fs::create_dir_all(crate_root.join("src")).unwrap();
        // Asset outside the crate root.
        let outside = crate_root.parent().unwrap().join("outside-gc0r.txt");
        std::fs::write(&outside, "external\n").unwrap();

        std::fs::write(crate_root.join("Cargo.toml"),
            "[package]\nname = \"gc0r-esc\"\nversion = \"0.0.1\"\nedition = \"2021\"\n").unwrap();
        std::fs::write(crate_root.join("src/lib.rs"),
            "pub const E: &str = include_str!(\"../outside-gc0r.txt\");\n").unwrap();

        let staging_root = unique_tmp_dir("staging-gc0r-escape-dest");
        // Must not error — escaping paths are skipped silently.
        let staging_crate = create_crate_staging_copy(&crate_root, &staging_root).unwrap();
        assert!(staging_crate.join("src/lib.rs").exists());
        assert!(!staging_crate.join("outside-gc0r.txt").exists(),
            "escaping asset must not appear in staging");

        let _ = std::fs::remove_dir_all(&crate_root);
        let _ = std::fs::remove_dir_all(&staging_root);
        let _ = std::fs::remove_file(&outside);
    }
}
