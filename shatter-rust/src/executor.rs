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

/// Read the harness cache root from `SHATTER_HARNESS_CACHE`.
/// Returns `None` if unset or empty.
fn harness_cache_root() -> Option<PathBuf> {
    std::env::var("SHATTER_HARNESS_CACHE")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
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

            return Ok(FnContext {
                sig: FnSignature {
                    param_names,
                    param_types,
                    return_type,
                    has_generics,
                    generic_names,
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
fn check_bin_only_compatibility(
    function_name: &str,
    ctx: &FnContext,
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
fn generate_harness(
    instrumented_source: &str,
    function_name: &str,
    param_names: &[String],
    param_types: &[String],
    return_type: Option<&str>,
    mocks_json: &str,
    static_mut_names: &[String],
) -> Result<String, ExecuteError> {
    let module_block = wrap_in_module(instrumented_source)?;
    let mut h = String::with_capacity(8192);

    h.push_str("#![allow(unused_imports)]\n");
    h.push_str("use std::io::BufRead;\n");
    h.push_str("use serde_json::Value;\n\n");
    h.push_str(&module_block);
    h.push_str("\n\nfn main() {\n");

    // Register mocks once at startup (baked in, same for all calls to this harness).
    h.push_str(&format!("    let mocks_json = r#\"{}\"#;\n", mocks_json));
    h.push_str(
        "    let mocks: Vec<Value> = serde_json::from_str(mocks_json).unwrap_or_default();\n",
    );
    h.push_str("    for mock in &mocks {\n");
    h.push_str("        if let (Some(symbol), Some(return_values)) = (\n");
    h.push_str("            mock.get(\"symbol\").and_then(|s| s.as_str()),\n");
    h.push_str("            mock.get(\"return_values\").and_then(|v| v.as_array()),\n");
    h.push_str("        ) {\n");
    h.push_str("            shatter_rust_runtime::register_mock(symbol, return_values.clone());\n");
    h.push_str("        }\n");
    h.push_str("    }\n\n");

    // Main request loop: read one JSON line per call, write one JSON line response.
    h.push_str("    let stdin = std::io::stdin();\n");
    h.push_str("    let mut reader = std::io::BufReader::new(stdin.lock());\n");
    h.push_str("    loop {\n");
    h.push_str("        let mut line = String::new();\n");
    h.push_str("        match reader.read_line(&mut line) {\n");
    h.push_str("            Ok(0) | Err(_) => break,\n");
    h.push_str("            Ok(_) => {}\n");
    h.push_str("        }\n");
    h.push_str("        let line = line.trim();\n");
    h.push_str("        if line.is_empty() { continue; }\n");
    h.push_str("        let req: Value = serde_json::from_str(line).unwrap_or_default();\n");
    h.push_str("        let inputs = req[\"inputs\"].as_array().cloned().unwrap_or_default();\n\n");

    // Reset runtime state (branch recorder, etc.) — keeps mock registrations.
    h.push_str("        shatter_rust_runtime::reset();\n\n");

    // Snapshot mutable globals before execution.
    // Each `static mut` is read with `unsafe` and serialized to JSON.
    // Variables whose type does not implement Serialize produce `None` and are skipped.
    if !static_mut_names.is_empty() {
        h.push_str("        // Snapshot mutable static variables before execution\n");
        for name in static_mut_names {
            h.push_str(&format!(
                "        let __before_{name} = unsafe {{ serde_json::to_value(&user_code::{name}).ok() }};\n"
            ));
        }
        h.push('\n');
    }

    // Deserialize each input parameter from the request's inputs array.
    // Reference types like `&str` can't be deserialized directly — deserialize
    // to the owned type (e.g. `String`) and borrow in the function call.
    // Slice references like `&[&str]` need a two-step conversion:
    // deserialize to `Vec<String>`, then create a `Vec<&str>` of borrows.
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

    // Call the function inside catch_unwind, measuring time
    h.push_str("        let start = std::time::Instant::now();\n");
    h.push_str("        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {\n");
    h.push_str(&format!("            user_code::{function_name}({args})\n"));
    h.push_str("        }));\n");
    h.push_str("        let wall_time_ms = start.elapsed().as_secs_f64() * 1000.0;\n\n");

    // Flush runtime results
    h.push_str("        let runtime_json = shatter_rust_runtime::flush_results();\n");
    h.push_str(
        "        let mut exec_result: Value = serde_json::from_str(&runtime_json).unwrap_or(Value::Object(Default::default()));\n",
    );
    h.push_str("        let obj = exec_result.as_object_mut().unwrap();\n\n");

    // Set return_value or thrown_error
    h.push_str("        match result {\n");
    if return_type.is_some() {
        h.push_str("            Ok(ret_val) => {\n");
        h.push_str(
            "                obj.insert(\"return_value\".into(), serde_json::to_value(&ret_val).unwrap_or(Value::Null));\n",
        );
        h.push_str("            }\n");
    } else {
        h.push_str("            Ok(()) => {\n");
        h.push_str("                obj.insert(\"return_value\".into(), Value::Null);\n");
        h.push_str("            }\n");
    }
    h.push_str("            Err(panic_info) => {\n");
    h.push_str("                let msg = if let Some(s) = panic_info.downcast_ref::<&str>() {\n");
    h.push_str("                    s.to_string()\n");
    h.push_str("                } else if let Some(s) = panic_info.downcast_ref::<String>() {\n");
    h.push_str("                    s.clone()\n");
    h.push_str("                } else {\n");
    h.push_str("                    format!(\"{:?}\", panic_info)\n");
    h.push_str("                };\n");
    h.push_str("                obj.insert(\"thrown_error\".into(), serde_json::json!({\n");
    h.push_str("                    \"error_type\": \"runtime_error\",\n");
    h.push_str("                    \"message\": msg,\n");
    h.push_str("                }));\n");
    h.push_str("            }\n");
    h.push_str("        }\n\n");

    // Set performance metrics
    h.push_str("        obj.insert(\"performance\".into(), serde_json::json!({\n");
    h.push_str("            \"wall_time_ms\": wall_time_ms,\n");
    h.push_str("            \"cpu_time_us\": 0,\n");
    h.push_str("            \"heap_used_bytes\": 0,\n");
    h.push_str("            \"heap_allocated_bytes\": 0,\n");
    h.push_str("        }));\n\n");

    // Detect global state changes by comparing before/after snapshots of mutable statics.
    // Changes are appended to the side_effects array in the execution result.
    if !static_mut_names.is_empty() {
        h.push_str("        // Detect mutable static changes and emit global_state_change side effects\n");
        h.push_str("        let mut __global_side_effects: Vec<serde_json::Value> = Vec::new();\n");
        for name in static_mut_names {
            h.push_str(&format!(
                "        let __after_{name} = unsafe {{ serde_json::to_value(&user_code::{name}).ok() }};\n"
            ));
            h.push_str(&format!(
                "        if let (Some(__b), Some(__a)) = (__before_{name}, __after_{name}) {{\n"
            ));
            h.push_str("            if __b != __a {\n");
            h.push_str(&format!(
                "                __global_side_effects.push(serde_json::json!({{\"kind\":\"global_state_change\",\"variable\":\"{name}\",\"before\":__b,\"after\":__a}}));\n"
            ));
            h.push_str("            }\n");
            h.push_str("        }\n");
        }
        h.push_str(
            "        let __se = obj.entry(\"side_effects\").or_insert(serde_json::json!([]));\n",
        );
        h.push_str(
            "        if let Some(__arr) = __se.as_array_mut() { __arr.extend(__global_side_effects); }\n\n",
        );
    } else {
        h.push_str("        obj.entry(\"side_effects\").or_insert(serde_json::json!([]));\n\n");
    }

    // Write result to stdout and flush
    h.push_str("        println!(\"{}\", serde_json::to_string(&exec_result).unwrap());\n");
    h.push_str("        let _ = std::io::Write::flush(&mut std::io::stdout());\n");
    h.push_str("    } // end loop\n");
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
    timing: Option<&mut TimingCollector>,
) -> Result<PersistentHarness, ExecuteError> {
    let src_dir = harness_dir.join("src");
    std::fs::create_dir_all(&src_dir)?;

    let cargo_toml = generate_cargo_toml(runtime_path);
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

/// Execute an instrumented Rust function via a persistent harness subprocess.
///
/// On the first call for a given (file, function, mocks) triple, compiles the
/// harness and spawns the subprocess. Subsequent calls reuse the cached process.
pub fn execute_function(
    file_path: &str,
    function_name: &str,
    inputs: &[Value],
    mocks: &[Value],
    timeout_ms: u64,
    cache: &HarnessCache,
) -> Result<ExecuteResult, ExecuteError> {
    execute_function_with_timing(file_path, function_name, inputs, mocks, timeout_ms, None, cache)
}

pub fn execute_function_with_timing(
    file_path: &str,
    function_name: &str,
    inputs: &[Value],
    mocks: &[Value],
    timeout_ms: u64,
    mut timing: Option<&mut TimingCollector>,
    cache: &HarnessCache,
) -> Result<ExecuteResult, ExecuteError> {
    let path = Path::new(file_path);
    if !path.exists() {
        return Err(ExecuteError::FileError(format!("file not found: {file_path}")));
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
    check_bin_only_compatibility(function_name, &ctx)?;

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
        )?
    };

    let harness_dir = make_harness_dir();
    std::fs::create_dir_all(&harness_dir)?;

    let mut harness = if let Some(timing) = timing {
        timing.record("execute.build", |timing| {
            build_and_spawn_harness(&harness_source, &harness_dir, &runtime_path, Some(timing))
        })?
    } else {
        build_and_spawn_harness(&harness_source, &harness_dir, &runtime_path, None)?
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
            "[]",
            &[],
        )
        .unwrap();
        assert!(harness.contains("mod user_code"));
        assert!(harness.contains("user_code::classify_number(n)"));
        assert!(harness.contains("catch_unwind"));
        assert!(harness.contains("flush_results"));
        assert!(harness.contains("shatter_rust_runtime::reset()"));
        assert!(harness.contains("loop"));
        assert!(harness.contains("read_line"));
    }

    #[test]
    fn generate_harness_void_function() {
        let harness = generate_harness("fn noop() {}", "noop", &[], &[], None, "[]", &[]).unwrap();
        assert!(harness.contains("user_code::noop()"));
        assert!(harness.contains("Ok(())"));
        assert!(harness.contains("loop"));
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
        let result = execute_function("/nonexistent/file.rs", "f", &[], &[], 5000, &cache);
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
        let result = execute_function(
            &file.to_string_lossy(),
            "add",
            &[serde_json::json!(1)], // only 1 input, needs 2
            &[],
            5000,
            &cache,
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
        let result = execute_function(
            &file.to_string_lossy(),
            "query",
            &[serde_json::json!(null)],
            &[],
            5000,
            &cache,
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
        let err = check_bin_only_compatibility("identity", &ctx).unwrap_err();
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
        let err = check_bin_only_compatibility("query", &ctx).unwrap_err();
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
        let err = check_bin_only_compatibility("process", &ctx).unwrap_err();
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
        let err = check_bin_only_compatibility("init", &ctx).unwrap_err();
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
        let err = check_bin_only_compatibility("dispatch", &ctx).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("generic type parameters"), "should list generics: {msg}");
        assert!(msg.contains("trait object"), "should list trait object: {msg}");
    }

    #[test]
    fn compat_primitives_pass() {
        let source = "fn add(a: i32, b: i32) -> i32 { a + b }";
        let ctx = extract_fn_context(source, "add").unwrap();
        assert!(check_bin_only_compatibility("add", &ctx).is_ok());
    }

    #[test]
    fn compat_std_types_pass() {
        let source = "use std::collections::HashMap;\nfn f(v: Vec<String>, m: HashMap<String, i32>) -> Option<bool> { None }";
        let ctx = extract_fn_context(source, "f").unwrap();
        assert!(check_bin_only_compatibility("f", &ctx).is_ok());
    }

    #[test]
    fn compat_local_struct_passes() {
        let source = "struct Point { x: f64, y: f64 }\nfn origin() -> Point { Point { x: 0.0, y: 0.0 } }";
        let ctx = extract_fn_context(source, "origin").unwrap();
        assert!(check_bin_only_compatibility("origin", &ctx).is_ok());
    }

    #[test]
    fn compat_local_struct_param_passes() {
        let source = "struct Config { debug: bool }\nfn setup(c: Config) -> bool { c.debug }";
        let ctx = extract_fn_context(source, "setup").unwrap();
        assert!(check_bin_only_compatibility("setup", &ctx).is_ok());
    }

    #[test]
    fn compat_ref_params_pass() {
        let source = "fn greet(name: &str) -> String { format!(\"hi {name}\") }";
        let ctx = extract_fn_context(source, "greet").unwrap();
        assert!(check_bin_only_compatibility("greet", &ctx).is_ok());
    }

    #[test]
    fn execute_generic_fn_returns_non_executable() {
        let dir = std::env::temp_dir().join("shatter-test-exec-generic");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("test.rs");
        std::fs::write(&file, "fn identity<T: Clone>(x: T) -> T { x.clone() }").unwrap();

        let cache = std::sync::Mutex::new(std::collections::HashMap::new());
        let result = execute_function(
            &file.to_string_lossy(),
            "identity",
            &[serde_json::json!(42)],
            &[],
            5000,
            &cache,
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
        let result = execute_function(
            &file.to_string_lossy(),
            "process",
            &[serde_json::json!(null)],
            &[],
            5000,
            &cache,
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
        // side_effects must still be present (via or_insert)
        assert!(
            harness.contains("side_effects"),
            "side_effects entry must still be present\n\nharness:\n{harness}"
        );
    }

    // --- Standalone harness fallback tests ---

    #[test]
    fn standalone_target_dir_uses_cache_env() {
        // When SHATTER_HARNESS_CACHE is set, standalone_target_dir returns a path inside it.
        let cache_root = std::env::temp_dir().join("shatter-test-cache-root");
        let cache_str = cache_root.to_string_lossy().into_owned();
        unsafe { std::env::set_var("SHATTER_HARNESS_CACHE", &cache_str) };

        let target = standalone_target_dir();
        assert!(target.is_some(), "expected Some when SHATTER_HARNESS_CACHE is set");
        let target = target.unwrap();
        assert!(
            target.starts_with(&cache_root),
            "target dir {target:?} should be under cache root {cache_str}"
        );
        assert!(
            target.ends_with("rust/standalone/target"),
            "target dir should end with rust/standalone/target, got {target:?}"
        );

        // Restore: set to empty so harness_cache_root() returns None
        unsafe { std::env::set_var("SHATTER_HARNESS_CACHE", "") };
    }

    #[test]
    fn standalone_target_dir_none_when_unset() {
        unsafe { std::env::set_var("SHATTER_HARNESS_CACHE", "") };
        assert!(standalone_target_dir().is_none());
    }

    #[test]
    fn make_request_scratch_uses_scratch_env() {
        // When SHATTER_HARNESS_SCRATCH is set, make_request_scratch returns a path inside it.
        let scratch_root = std::env::temp_dir().join("shatter-test-scratch-root");
        let scratch_str = scratch_root.to_string_lossy().into_owned();
        unsafe { std::env::set_var("SHATTER_HARNESS_SCRATCH", &scratch_str) };

        let scratch = make_request_scratch();
        assert!(
            scratch.starts_with(&scratch_root),
            "scratch dir {scratch:?} should be under scratch root {scratch_str}"
        );

        unsafe { std::env::set_var("SHATTER_HARNESS_SCRATCH", "") };
    }

    #[test]
    fn make_request_scratch_fallback_to_temp() {
        // When SHATTER_HARNESS_SCRATCH is empty/unset, make_request_scratch returns a temp-based path.
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
        unsafe { std::env::set_var("SHATTER_HARNESS_SCRATCH", "") };
        let a = make_request_scratch();
        let b = make_request_scratch();
        assert_ne!(a, b, "each call should produce a distinct scratch path");
    }
}
