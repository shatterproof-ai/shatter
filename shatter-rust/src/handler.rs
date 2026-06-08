use std::collections::{HashMap, HashSet};
use std::io::{self, BufRead, BufReader};
use std::path::PathBuf;

use crate::generators::NativeRegistry;
use crate::protocol::{
    self, ERR_COMPILATION_ERROR, ERR_FILE_NOT_FOUND, ERR_FUNCTION_NOT_FOUND, ERR_INTERNAL_ERROR,
    ERR_INVALID_REQUEST, ERR_NOT_SUPPORTED, ERR_PARSE_ERROR, ERR_PREFLIGHT_FAILED,
    ERR_VERSION_MISMATCH, FRONTEND_LANGUAGE, FRONTEND_VERSION, PROTOCOL_VERSION, Request, Response,
};
use crate::timing::TimingCollector;
use crate::wasm_generator::WasmCache;

/// Log level for the frontend, controlled by SHATTER_LOG_LEVEL env var.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum FrontendLogLevel {
    Error = 0,
    Warn = 1,
    Info = 2,
    Debug = 3,
    Trace = 4,
}

impl FrontendLogLevel {
    fn from_env() -> Self {
        match std::env::var("SHATTER_LOG_LEVEL")
            .unwrap_or_default()
            .to_lowercase()
            .as_str()
        {
            "error" => FrontendLogLevel::Error,
            "warn" => FrontendLogLevel::Warn,
            "debug" => FrontendLogLevel::Debug,
            "trace" => FrontendLogLevel::Trace,
            _ => FrontendLogLevel::Info,
        }
    }
}

const DEFAULT_EXEC_TIMEOUT_MS: u64 = 5000;

/// Read the execution timeout from `SHATTER_EXEC_TIMEOUT` env var (seconds).
/// Returns milliseconds. Falls back to `DEFAULT_EXEC_TIMEOUT_MS` if unset, invalid, zero, or negative.
fn exec_timeout_from_env() -> u64 {
    match std::env::var("SHATTER_EXEC_TIMEOUT") {
        Ok(val) => match val.parse::<f64>() {
            Ok(secs) if secs > 0.0 => (secs * 1000.0) as u64,
            _ => DEFAULT_EXEC_TIMEOUT_MS,
        },
        Err(_) => DEFAULT_EXEC_TIMEOUT_MS,
    }
}

/// Read the harness cache directory from `SHATTER_HARNESS_CACHE` env var.
/// Returns `None` if unset or empty.
fn harness_cache_from_env() -> Option<String> {
    std::env::var("SHATTER_HARNESS_CACHE")
        .ok()
        .filter(|s| !s.is_empty())
}

/// Read the harness scratch directory from `SHATTER_HARNESS_SCRATCH` env var.
/// Returns `None` if unset or empty.
fn harness_scratch_from_env() -> Option<String> {
    std::env::var("SHATTER_HARNESS_SCRATCH")
        .ok()
        .filter(|s| !s.is_empty())
}

/// Write instrumented source to a temp directory and return the output path.
fn write_instrumented_temp(filename: &str, source: &str) -> io::Result<String> {
    let root = harness_scratch_from_env()
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join(".shatter-instrument"));
    let dir = root.join(std::process::id().to_string());
    std::fs::create_dir_all(&dir)?;
    let out_path = dir.join(filename);
    std::fs::write(&out_path, source)?;
    Ok(out_path.to_string_lossy().into_owned())
}

/// Build an `InvocationOutcome` for a successful execute path (str-hy9b.A1/A5).
///
/// Mapping:
/// - `thrown_error == None` → `Completed`, carrying `return_value`.
/// - `thrown_error.error_type == "timeout"` (set by the executor's
///   `RecvTimeoutError::Timeout` arm) → `TimedOut`.
/// - any other thrown error → `RuntimeFailed`.
fn derive_execute_outcome(
    result: &crate::executor::ExecuteResult,
) -> crate::protocol::InvocationOutcome {
    use crate::protocol::{InvocationOutcome, OutcomeStatus};
    match &result.thrown_error {
        None => InvocationOutcome {
            status: OutcomeStatus::Completed,
            short_reason: None,
            return_value: result.return_value.clone(),
            thrown_error: None,
            side_effects: Vec::new(),
        },
        Some(err) => {
            let error_type = err
                .get("error_type")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let message = err
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let status = if error_type == "timeout" {
                OutcomeStatus::TimedOut
            } else {
                OutcomeStatus::RuntimeFailed
            };
            let short_reason = if message.is_empty() {
                format!("{error_type} thrown")
            } else {
                message.to_string()
            };
            InvocationOutcome {
                status,
                short_reason: Some(short_reason),
                return_value: None,
                thrown_error: Some(err.clone()),
                side_effects: Vec::new(),
            }
        }
    }
}

/// Build an `InvocationOutcome` for an error-path execute response.
///
/// Used when the executor returns `Err(...)` and the handler emits an `error`
/// status (e.g. compilation failure → `BuildFailed`, non-executable target →
/// `Unsupported`). `short_reason` carries the executor's diagnostic.
fn error_outcome(
    status: crate::protocol::OutcomeStatus,
    message: &str,
) -> crate::protocol::InvocationOutcome {
    crate::protocol::InvocationOutcome {
        status,
        short_reason: Some(message.to_string()),
        return_value: None,
        thrown_error: None,
        side_effects: Vec::new(),
    }
}

/// Lifecycle manager for compiled Rust harness subprocesses.
///
/// Keeps one running harness process per unique (file, function, mocks) triple.
/// `close_all()` is called on shutdown to kill subprocesses and clean up harness directories.
pub struct PersistentHarnessManager {
    /// The harness subprocess cache. Interior-mutable so `handle_execute` keeps `&self`.
    pub cache: crate::executor::HarnessCache,
    /// Cache for crate-backed dispatch harnesses (one per file, keyed by source hash + mocks).
    pub crate_cache: crate::executor::CrateHarnessCache,
    /// Cache for crate-bridge harnesses (one per crate root + wrapper hash + mocks).
    pub bridge_cache: crate::executor::CrateBridgeHarnessCache,
}

impl PersistentHarnessManager {
    pub fn new() -> Self {
        Self {
            cache: std::sync::Mutex::new(std::collections::HashMap::new()),
            crate_cache: std::sync::Mutex::new(std::collections::HashMap::new()),
            bridge_cache: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// Remove cache entries whose source files no longer exist on disk.
    /// Kills subprocesses and removes standalone harness dirs (tool-owned cache).
    /// Crate-backed and bridge harness dirs are preserved (stable cache).
    /// Returns the number of entries pruned.
    pub fn prune_orphans(&self) -> usize {
        let mut pruned = 0;

        // Standalone cache: keyed by file_path.
        {
            let mut map = self.cache.lock().unwrap();
            let stale_keys: Vec<_> = map
                .keys()
                .filter(|k| !std::path::Path::new(k.file_path()).exists())
                .cloned()
                .collect();
            for key in stale_keys {
                if let Some(mut h) = map.remove(&key) {
                    let _ = h.child.kill();
                    let _ = h.child.wait();
                    let _ = std::fs::remove_dir_all(&h.harness_dir);
                }
                pruned += 1;
            }
        }

        // Crate-backed cache: keyed by file_path.
        {
            let mut map = self.crate_cache.lock().unwrap();
            let stale_keys: Vec<_> = map
                .keys()
                .filter(|k| !std::path::Path::new(k.file_path()).exists())
                .cloned()
                .collect();
            for key in stale_keys {
                if let Some(mut entry) = map.remove(&key) {
                    let _ = entry.harness.child.kill();
                    let _ = entry.harness.child.wait();
                    // Preserve harness dir (stable cache).
                }
                pruned += 1;
            }
        }

        // Bridge cache: keyed by crate_root.
        {
            let mut map = self.bridge_cache.lock().unwrap();
            let stale_keys: Vec<_> = map
                .keys()
                .filter(|k| !k.crate_root().exists())
                .cloned()
                .collect();
            for key in stale_keys {
                if let Some(mut entry) = map.remove(&key) {
                    let _ = entry.harness.child.kill();
                    let _ = entry.harness.child.wait();
                    // Preserve harness dir (stable cache).
                }
                pruned += 1;
            }
        }

        pruned
    }

    /// Remove cache entries whose harness build directories have been deleted externally.
    /// Kills subprocesses for all affected entries. Returns the number of entries pruned.
    pub fn prune_missing_artifacts(&self) -> usize {
        let mut pruned = 0;

        {
            let mut map = self.cache.lock().unwrap();
            let stale_keys: Vec<_> = map
                .iter()
                .filter(|(_, h)| !h.harness_dir.exists())
                .map(|(k, _)| k.clone())
                .collect();
            for key in stale_keys {
                if let Some(mut h) = map.remove(&key) {
                    let _ = h.child.kill();
                    let _ = h.child.wait();
                }
                pruned += 1;
            }
        }

        {
            let mut map = self.crate_cache.lock().unwrap();
            let stale_keys: Vec<_> = map
                .iter()
                .filter(|(_, entry)| !entry.harness.harness_dir.exists())
                .map(|(k, _)| k.clone())
                .collect();
            for key in stale_keys {
                if let Some(mut entry) = map.remove(&key) {
                    let _ = entry.harness.child.kill();
                    let _ = entry.harness.child.wait();
                }
                pruned += 1;
            }
        }

        {
            let mut map = self.bridge_cache.lock().unwrap();
            let stale_keys: Vec<_> = map
                .iter()
                .filter(|(_, entry)| !entry.harness.harness_dir.exists())
                .map(|(k, _)| k.clone())
                .collect();
            for key in stale_keys {
                if let Some(mut entry) = map.remove(&key) {
                    let _ = entry.harness.child.kill();
                    let _ = entry.harness.child.wait();
                }
                pruned += 1;
            }
        }

        pruned
    }

    /// Terminates all cached harness subprocesses and removes their build directories.
    pub fn close_all(&mut self) {
        let mut map = self.cache.lock().unwrap();
        for (_, mut h) in map.drain() {
            let _ = h.child.kill();
            let _ = h.child.wait();
            let _ = std::fs::remove_dir_all(&h.harness_dir);
        }
        // Crate-backed harnesses: kill subprocesses but preserve harness dirs (stable cache).
        let mut crate_map = self.crate_cache.lock().unwrap();
        for (_, mut entry) in crate_map.drain() {
            let _ = entry.harness.child.kill();
            let _ = entry.harness.child.wait();
            // Do NOT remove harness_dir — it contains the stable compiled binary.
        }
        // Crate-bridge harnesses: kill subprocesses but preserve harness dirs (stable cache).
        // With the staging-copy approach (str-ja70), original source is never modified,
        // so no restore is needed — just tear down the subprocess.
        let mut bridge_map = self.bridge_cache.lock().unwrap();
        for (_, mut entry) in bridge_map.drain() {
            let _ = entry.harness.child.kill();
            let _ = entry.harness.child.wait();
            // Do NOT remove harness_dir — it contains the stable compiled binary.
        }
    }
}

impl Default for PersistentHarnessManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Env-preflight reason: project_root has no `Cargo.toml`.
const PREFLIGHT_REASON_MISSING_CARGO_TOML: &str = "missing_cargo_toml";
/// Env-preflight reason: `cargo --version` is not available on PATH.
const PREFLIGHT_REASON_CARGO_NOT_FOUND: &str = "cargo_not_found";

/// Cached env-preflight failure. Sticky for the lifetime of the Handler.
struct PreflightFailure {
    reason: &'static str,
    detail: String,
}

pub struct Handler<R, W, L> {
    reader: BufReader<R>,
    writer: W,
    log: L,
    log_level: FrontendLogLevel,
    exec_timeout_ms: u64,
    wasm_cache: WasmCache,
    native_registry: Option<NativeRegistry>,
    /// Persistent harness subprocess manager. close_all() is called on shutdown.
    harness_manager: PersistentHarnessManager,
    /// Harness cache directory from `SHATTER_HARNESS_CACHE` (unused until execute is implemented).
    #[allow(dead_code)]
    harness_cache_dir: Option<String>,
    /// Harness scratch directory from `SHATTER_HARNESS_SCRATCH` (unused until execute is implemented).
    #[allow(dead_code)]
    harness_scratch_dir: Option<String>,
    /// Remembered from the most recent Analyze or Instrument request so Execute
    /// can fall back when the core omits the file field (which it always does).
    last_file: Option<String>,
    timing_enabled: bool,
    /// Maps prepare_id → (file_path, function_name, mocks, harness_mode).
    prepared_harnesses: HashMap<String, PreparedHarnessInfo>,
    /// Adapter registry for recognizing targets requiring adapter-owned invocation.
    adapter_registry: crate::adapters::AdapterRegistry,
    /// Cached FunctionAnalysis records keyed by "file:function_name".
    cached_analyses: HashMap<String, crate::protocol::FunctionAnalysis>,
    /// Sticky env-preflight failure. Once set, every analyze/instrument/prepare/
    /// execute/setup short-circuits with `preflight_failed`.
    preflight_failure: Option<PreflightFailure>,
    /// project_root values already checked (success or failure cached).
    preflight_checked_roots: HashSet<String>,
    /// One-shot guard for the `cargo --version` PATH probe.
    preflight_cargo_checked: bool,
    /// Latest project_root observed by any preflighted handler, so execute
    /// (whose requests do not carry project_root) can inherit it.
    last_project_root: Option<String>,
}

/// Stored info about a prepared harness, keyed by prepare_id.
struct PreparedHarnessInfo {
    file_path: String,
    function_name: String,
    mocks: Vec<serde_json::Value>,
    harness_mode: Option<String>,
}

impl<R: io::Read, W: io::Write, L: io::Write> Handler<R, W, L> {
    pub fn new(reader: R, writer: W, log: L) -> Self {
        Self {
            reader: BufReader::new(reader),
            writer,
            log,
            log_level: FrontendLogLevel::from_env(),
            exec_timeout_ms: exec_timeout_from_env(),
            harness_cache_dir: harness_cache_from_env(),
            harness_scratch_dir: harness_scratch_from_env(),
            harness_manager: PersistentHarnessManager::new(),
            wasm_cache: WasmCache::new(),
            native_registry: None,
            last_file: None,
            timing_enabled: false,
            prepared_harnesses: HashMap::new(),
            adapter_registry: crate::adapters::AdapterRegistry::with_builtins(),
            cached_analyses: HashMap::new(),
            preflight_failure: None,
            preflight_checked_roots: HashSet::new(),
            preflight_cargo_checked: false,
            last_project_root: None,
        }
    }

    /// Create a handler with a native generator registry for custom builds.
    pub fn new_with_native_registry(
        reader: R,
        writer: W,
        log: L,
        registry: NativeRegistry,
    ) -> Self {
        Self {
            reader: BufReader::new(reader),
            writer,
            log,
            log_level: FrontendLogLevel::from_env(),
            exec_timeout_ms: exec_timeout_from_env(),
            harness_cache_dir: harness_cache_from_env(),
            harness_scratch_dir: harness_scratch_from_env(),
            harness_manager: PersistentHarnessManager::new(),
            wasm_cache: WasmCache::new(),
            native_registry: Some(registry),
            last_file: None,
            timing_enabled: false,
            prepared_harnesses: HashMap::new(),
            adapter_registry: crate::adapters::AdapterRegistry::with_builtins(),
            cached_analyses: HashMap::new(),
            preflight_failure: None,
            preflight_checked_roots: HashSet::new(),
            preflight_cargo_checked: false,
            last_project_root: None,
        }
    }

    /// Create a handler with an explicit log level (for testing).
    #[cfg(test)]
    fn with_log_level(reader: R, writer: W, log: L, level: FrontendLogLevel) -> Self {
        Self {
            reader: BufReader::new(reader),
            writer,
            log,
            log_level: level,
            exec_timeout_ms: exec_timeout_from_env(),
            harness_cache_dir: harness_cache_from_env(),
            harness_scratch_dir: harness_scratch_from_env(),
            harness_manager: PersistentHarnessManager::new(),
            wasm_cache: WasmCache::new(),
            native_registry: None,
            last_file: None,
            timing_enabled: false,
            prepared_harnesses: HashMap::new(),
            adapter_registry: crate::adapters::AdapterRegistry::with_builtins(),
            cached_analyses: HashMap::new(),
            preflight_failure: None,
            preflight_checked_roots: HashSet::new(),
            preflight_cargo_checked: false,
            last_project_root: None,
        }
    }

    /// Process requests until shutdown or EOF. Returns Ok(()) on clean shutdown.
    pub fn run(mut self) -> io::Result<()> {
        self.log_at(
            FrontendLogLevel::Debug,
            &format!("Starting Rust frontend (protocol {PROTOCOL_VERSION})"),
        );

        let mut line = String::new();
        loop {
            line.clear();
            let bytes_read = self.reader.read_line(&mut line)?;
            if bytes_read == 0 {
                // EOF
                self.log_at(FrontendLogLevel::Debug, "Stdin closed, exiting");
                return Ok(());
            }

            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            self.logf(&format!("Received: {trimmed}"));

            let req: Request = match serde_json::from_str(trimmed) {
                Ok(r) => r,
                Err(e) => {
                    self.logf(&format!("Failed to parse request: {e}"));
                    let err_resp = Response {
                        id: 0,
                        status: "error".to_string(),
                        code: Some(ERR_INVALID_REQUEST.to_string()),
                        message: Some(format!("Invalid JSON: {e}")),
                        ..Response::base(0)
                    };
                    self.send(&err_resp)?;
                    continue;
                }
            };

            let (resp, shutdown) = self.dispatch(&req);
            self.send(&resp)?;

            if shutdown {
                self.log_at(FrontendLogLevel::Debug, "Shutting down");
                return Ok(());
            }
        }
    }

    fn dispatch(&mut self, req: &Request) -> (Response, bool) {
        let mut resp = Response::base(req.id);

        if !is_version_compatible(&req.protocol_version) {
            resp.status = "error".to_string();
            resp.code = Some(ERR_VERSION_MISMATCH.to_string());
            resp.message = Some(format!(
                "unsupported protocol version {:?}, expected {:?}",
                req.protocol_version,
                protocol::PROTOCOL_VERSION,
            ));
            return (resp, false);
        }

        match req.command.as_str() {
            "handshake" => (self.handle_handshake(resp, req), false),
            "analyze" => (self.handle_analyze(resp, req), false),
            "instrument" => (self.handle_instrument(resp, req), false),
            "prepare" => (self.handle_prepare(resp, req), false),
            "execute" => (self.handle_execute(resp, req), false),
            "setup" => (self.handle_setup(resp, req), false),
            "teardown" => (self.handle_teardown(resp, req), false),
            "generate" => (self.handle_generate(resp, req), false),
            "shutdown" => (self.handle_shutdown(resp), true),
            _ => {
                resp.status = "error".to_string();
                resp.code = Some(ERR_INVALID_REQUEST.to_string());
                resp.message = Some(format!("unknown command: {}", req.command));
                (resp, false)
            }
        }
    }

    fn handle_handshake(&mut self, mut resp: Response, req: &Request) -> Response {
        self.timing_enabled = req.capabilities.iter().any(|cap| cap == "timing");
        resp.status = "handshake".to_string();
        resp.frontend_version = Some(FRONTEND_VERSION.to_string());
        resp.language = Some(FRONTEND_LANGUAGE.to_string());
        resp.capabilities = Some(vec![
            "analyze".to_string(),
            "execute".to_string(),
            "generate".to_string(),
            "instrument".to_string(),
            "prepare".to_string(),
            "setup".to_string(),
            "teardown".to_string(),
        ]);
        resp
    }

    fn maybe_timing_collector(&self) -> Option<TimingCollector> {
        self.timing_enabled.then(TimingCollector::default)
    }

    /// Run a one-shot env preflight for the supplied project root.
    ///
    /// Sticky: once `preflight_failure` is set, every subsequent call returns
    /// immediately so analyze/instrument/prepare/execute/setup short-circuit
    /// with the same cached error. Idempotent per root: a successful root is
    /// remembered and not re-checked. The `cargo --version` PATH probe is
    /// itself one-shot because cargo presence is process-global.
    fn run_preflight(&mut self, project_root: Option<&str>) {
        if self.preflight_failure.is_some() {
            return;
        }
        let root = match project_root {
            Some(r) if !r.is_empty() => r,
            _ => return,
        };
        if self.preflight_checked_roots.contains(root) {
            return;
        }
        self.preflight_checked_roots.insert(root.to_string());

        let cargo_toml = std::path::Path::new(root).join("Cargo.toml");
        if !cargo_toml.exists() {
            self.preflight_failure = Some(PreflightFailure {
                reason: PREFLIGHT_REASON_MISSING_CARGO_TOML,
                detail: cargo_toml.display().to_string(),
            });
            return;
        }

        if !self.preflight_cargo_checked {
            self.preflight_cargo_checked = true;
            let cargo_ok = std::process::Command::new("cargo")
                .arg("--version")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if !cargo_ok {
                self.preflight_failure = Some(PreflightFailure {
                    reason: PREFLIGHT_REASON_CARGO_NOT_FOUND,
                    detail: "cargo".to_string(),
                });
            }
        }
    }

    /// Build the canonical error response for the cached preflight failure.
    /// Wire-level code is `preflight_failed`; message keeps the structured
    /// `preflight_failed: <reason>: <detail>` prefix used by TS so log
    /// scrapers and run reports stay compatible across frontends.
    fn preflight_error_response(&self, id: u64) -> Response {
        let f = self
            .preflight_failure
            .as_ref()
            .expect("preflight_error_response called without a cached failure");
        let mut resp = Response::base(id);
        resp.status = "error".to_string();
        resp.code = Some(ERR_PREFLIGHT_FAILED.to_string());
        resp.message = Some(format!("preflight_failed: {}: {}", f.reason, f.detail));
        resp
    }

    /// Remember the most recent `project_root` so execute (whose requests do
    /// not carry project_root) inherits it for its own preflight call.
    fn remember_project_root(&mut self, project_root: Option<&str>) {
        if let Some(r) = project_root.filter(|s| !s.is_empty()) {
            self.last_project_root = Some(r.to_string());
        }
    }

    fn finalize_response(
        &self,
        mut resp: Response,
        timing: Option<&mut TimingCollector>,
    ) -> Response {
        if let Some(timing) = timing {
            timing.record("serialize.response", |_| ());
            resp.timing = timing.summary();
        }
        resp
    }

    fn handle_analyze(&mut self, mut resp: Response, req: &Request) -> Response {
        let mut timing = self.maybe_timing_collector();
        let file_path = match &req.file {
            Some(f) => f,
            None => {
                resp.status = "error".to_string();
                resp.code = Some(ERR_INVALID_REQUEST.to_string());
                resp.message = Some("analyze command requires a file path".to_string());
                return resp;
            }
        };

        self.remember_project_root(req.project_root.as_deref());
        self.run_preflight(req.project_root.as_deref());
        if self.preflight_failure.is_some() {
            return self.preflight_error_response(req.id);
        }

        self.last_file = Some(file_path.clone());

        let path = std::path::Path::new(file_path);
        if !path.exists() {
            resp.status = "error".to_string();
            resp.code = Some(ERR_FILE_NOT_FOUND.to_string());
            resp.message = Some(format!("file not found: {file_path}"));
            return resp;
        }

        let analysis = if let Some(timing) = timing.as_mut() {
            timing.record("analyze.total", |timing| {
                crate::analyzer::analyze_file_with_context_and_timing(
                    path,
                    req.function.as_deref(),
                    Some(timing),
                )
            })
        } else {
            crate::analyzer::analyze_file_with_context(path, req.function.as_deref())
        };

        match analysis {
            Ok((mut functions, file_ctx)) => {
                // Run adapter recognizers and derive invocation models.
                let resolved = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
                for func in &mut functions {
                    let hints = self.adapter_registry.recognize_all(func, &file_ctx);
                    func.invocation_model = crate::adapters::derive_invocation_model(&hints);
                    func.adapter_hints = hints;
                    let key = format!("{}:{}", resolved.display(), func.name);
                    self.cached_analyses.insert(key, func.clone());
                }
                resp.status = "analyze".to_string();
                resp.functions = Some(functions);
                self.finalize_response(resp, timing.as_mut())
            }
            Err(e) => {
                resp.status = "error".to_string();
                resp.code = Some(
                    match &e {
                        crate::analyzer::AnalyzeError::FileNotFound(_) => ERR_FILE_NOT_FOUND,
                        crate::analyzer::AnalyzeError::ReadError(_) => ERR_INTERNAL_ERROR,
                        crate::analyzer::AnalyzeError::ParseError(_) => ERR_PARSE_ERROR,
                        crate::analyzer::AnalyzeError::FunctionNotFound(_) => {
                            ERR_FUNCTION_NOT_FOUND
                        }
                    }
                    .to_string(),
                );
                resp.message = Some(e.to_string());
                self.finalize_response(resp, timing.as_mut())
            }
        }
    }

    fn handle_instrument(&mut self, mut resp: Response, req: &Request) -> Response {
        let mut timing = self.maybe_timing_collector();
        let file_path = match &req.file {
            Some(f) => f,
            None => {
                resp.status = "error".to_string();
                resp.code = Some(ERR_INVALID_REQUEST.to_string());
                resp.message = Some("instrument command requires a file path".to_string());
                return resp;
            }
        };

        self.remember_project_root(req.project_root.as_deref());
        self.run_preflight(req.project_root.as_deref());
        if self.preflight_failure.is_some() {
            return self.preflight_error_response(req.id);
        }

        self.last_file = Some(file_path.clone());

        let path = std::path::Path::new(file_path);
        if !path.exists() {
            resp.status = "error".to_string();
            resp.code = Some(ERR_FILE_NOT_FOUND.to_string());
            resp.message = Some(format!("file not found: {file_path}"));
            return resp;
        }

        let instrumented = if let Some(timing) = timing.as_mut() {
            timing.record("instrument.total", |timing| {
                crate::instrument::instrument_file_with_timing(
                    path,
                    req.function.as_deref(),
                    Some(timing),
                )
            })
        } else {
            crate::instrument::instrument_file(path, req.function.as_deref())
        };

        match instrumented {
            Ok(result) => {
                // Write instrumented source to a temp file
                let source_name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("instrumented.rs");
                match write_instrumented_temp(source_name, &result.source) {
                    Ok(output_path) => {
                        resp.status = "instrument".to_string();
                        resp.instrumented = Some(true);
                        resp.output_file = Some(output_path);
                        resp.message = Some(format!(
                            "instrumented {} branch points",
                            result.branch_count
                        ));
                        self.finalize_response(resp, timing.as_mut())
                    }
                    Err(e) => {
                        resp.status = "error".to_string();
                        resp.code = Some(ERR_INTERNAL_ERROR.to_string());
                        resp.message = Some(format!("failed to write instrumented output: {e}"));
                        self.finalize_response(resp, timing.as_mut())
                    }
                }
            }
            Err(e) => {
                resp.status = "error".to_string();
                resp.code = Some(
                    match &e {
                        crate::instrument::InstrumentError::FileNotFound(_) => ERR_FILE_NOT_FOUND,
                        crate::instrument::InstrumentError::ReadError(_) => ERR_INTERNAL_ERROR,
                        crate::instrument::InstrumentError::ParseError(_) => ERR_PARSE_ERROR,
                    }
                    .to_string(),
                );
                resp.message = Some(e.to_string());
                self.finalize_response(resp, timing.as_mut())
            }
        }
    }

    /// Pre-build a harness for a (file, function, mocks) triple.
    /// Returns a prepare_id that can be passed to subsequent execute calls.
    fn handle_prepare(&mut self, mut resp: Response, req: &Request) -> Response {
        let mut timing = self.maybe_timing_collector();

        self.remember_project_root(req.project_root.as_deref());
        self.run_preflight(req.project_root.as_deref());
        if self.preflight_failure.is_some() {
            return self.preflight_error_response(req.id);
        }

        let file_path = match req.file.as_ref().or(self.last_file.as_ref()) {
            Some(f) => f.clone(),
            None => {
                resp.status = "error".to_string();
                resp.code = Some(ERR_INVALID_REQUEST.to_string());
                resp.message = Some("prepare requires a file path".to_string());
                return resp;
            }
        };
        let function_name = match &req.function {
            Some(f) => f.clone(),
            None => {
                resp.status = "error".to_string();
                resp.code = Some(ERR_INVALID_REQUEST.to_string());
                resp.message = Some("prepare requires a function name".to_string());
                return resp;
            }
        };

        if !std::path::Path::new(&file_path).exists() {
            resp.status = "error".to_string();
            resp.code = Some(ERR_FILE_NOT_FOUND.to_string());
            resp.message = Some(format!("file not found: {file_path}"));
            return resp;
        }

        let prepare_id =
            crate::executor::compute_prepare_id(&file_path, &function_name, &req.mocks);

        // Idempotent: return immediately if already prepared with same id.
        if self.prepared_harnesses.contains_key(&prepare_id) {
            resp.status = "prepare".to_string();
            resp.prepare_id = Some(prepare_id);
            return self.finalize_response(resp, timing.as_mut());
        }

        let harness_mode = req.harness_mode.as_deref();
        let build_result = if let Some(timing) = timing.as_mut() {
            timing.record("prepare.total", |_| {
                crate::executor::prepare_harness(
                    &file_path,
                    &function_name,
                    &req.mocks,
                    self.exec_timeout_ms,
                    harness_mode,
                    &self.harness_manager.cache,
                    &self.harness_manager.crate_cache,
                    &self.harness_manager.bridge_cache,
                )
            })
        } else {
            crate::executor::prepare_harness(
                &file_path,
                &function_name,
                &req.mocks,
                self.exec_timeout_ms,
                harness_mode,
                &self.harness_manager.cache,
                &self.harness_manager.crate_cache,
                &self.harness_manager.bridge_cache,
            )
        };

        match build_result {
            Ok(()) => {
                self.prepared_harnesses.insert(
                    prepare_id.clone(),
                    PreparedHarnessInfo {
                        file_path,
                        function_name,
                        mocks: req.mocks.clone(),
                        harness_mode: req.harness_mode.clone(),
                    },
                );
                resp.status = "prepare".to_string();
                resp.prepare_id = Some(prepare_id);
                self.finalize_response(resp, timing.as_mut())
            }
            Err(crate::executor::ExecuteError::NonExecutable(msg)) => {
                resp.status = "error".to_string();
                resp.code = Some(ERR_NOT_SUPPORTED.to_string());
                resp.message = Some(msg);
                self.finalize_response(resp, timing.as_mut())
            }
            Err(crate::executor::ExecuteError::CompilationFailed(msg)) => {
                resp.status = "error".to_string();
                resp.code = Some(ERR_COMPILATION_ERROR.to_string());
                resp.message = Some(msg);
                self.finalize_response(resp, timing.as_mut())
            }
            Err(e) => {
                resp.status = "error".to_string();
                resp.code = Some(ERR_INTERNAL_ERROR.to_string());
                resp.message = Some(e.to_string());
                self.finalize_response(resp, timing.as_mut())
            }
        }
    }

    fn handle_execute(&mut self, mut resp: Response, req: &Request) -> Response {
        let mut timing = self.maybe_timing_collector();

        self.remember_project_root(req.project_root.as_deref());
        let preflight_root = req
            .project_root
            .clone()
            .or_else(|| self.last_project_root.clone());
        self.run_preflight(preflight_root.as_deref());
        if self.preflight_failure.is_some() {
            return self.preflight_error_response(req.id);
        }

        // If a prepare_id is provided, look up the prepared harness info and use
        // its file/function/mocks. The harness is already compiled and cached.
        let (eff_file, eff_function, eff_mocks, eff_harness_mode);
        if let Some(ref pid) = req.prepare_id {
            if let Some(info) = self.prepared_harnesses.get(pid) {
                eff_file = info.file_path.clone();
                eff_function = info.function_name.clone();
                eff_mocks = info.mocks.clone();
                eff_harness_mode = info.harness_mode.clone();
            } else {
                resp.status = "error".to_string();
                resp.code = Some(ERR_INVALID_REQUEST.to_string());
                resp.message = Some(format!("unknown prepare_id: {pid}"));
                return resp;
            }
        } else {
            eff_file = match req.file.as_ref().or(self.last_file.as_ref()) {
                Some(f) => f.clone(),
                None => {
                    resp.status = "error".to_string();
                    resp.code = Some(ERR_INVALID_REQUEST.to_string());
                    resp.message = Some("execute command requires a file path (none provided and no prior analyze/instrument)".to_string());
                    return resp;
                }
            };
            eff_function = match &req.function {
                Some(f) => f.clone(),
                None => {
                    resp.status = "error".to_string();
                    resp.code = Some(ERR_INVALID_REQUEST.to_string());
                    resp.message = Some("execute command requires a function name".to_string());
                    return resp;
                }
            };
            eff_mocks = req.mocks.clone();
            eff_harness_mode = req.harness_mode.clone();
        }

        let file_path = &eff_file;
        let function_name = &eff_function;

        if self.log_level >= FrontendLogLevel::Debug {
            eprintln!(
                "[shatter-rust] Executing {function_name} in {file_path} with {} inputs",
                req.inputs.len()
            );
        }

        // Check cached analysis for adapter-owned invocation model.
        let resolved = std::path::Path::new(file_path.as_str())
            .canonicalize()
            .unwrap_or_else(|_| std::path::PathBuf::from(file_path.as_str()));
        let cache_key = format!("{}:{}", resolved.display(), function_name);
        let mut analysis_for_execute = self.cached_analyses.get(&cache_key).cloned();
        if analysis_for_execute.is_none()
            && let Ok((mut functions, file_ctx)) = crate::analyzer::analyze_file_with_context(
                std::path::Path::new(file_path),
                Some(function_name),
            )
            && let Some(mut func) = functions.drain(..).find(|func| func.name == *function_name)
        {
            let hints = self.adapter_registry.recognize_all(&func, &file_ctx);
            func.invocation_model = crate::adapters::derive_invocation_model(&hints);
            func.adapter_hints = hints;
            self.cached_analyses.insert(cache_key.clone(), func.clone());
            analysis_for_execute = Some(func);
        }
        let invocation_model = analysis_for_execute
            .as_ref()
            .map(|a| &a.invocation_model)
            .cloned()
            .unwrap_or_default();

        let strategy = crate::adapters::choose_invocation_strategy(&invocation_model);

        let harness_mode = eff_harness_mode.as_deref();
        let execution = match strategy {
            crate::adapters::InvocationStrategy::AdapterOwned { ref adapter_id } => {
                crate::adapters::execute_adapter_owned(
                    adapter_id,
                    file_path,
                    function_name,
                    &req.inputs,
                    &eff_mocks,
                    self.exec_timeout_ms,
                    analysis_for_execute.as_ref(),
                    &self.harness_manager.cache,
                    &self.harness_manager.crate_cache,
                    &self.harness_manager.bridge_cache,
                )
            }
            crate::adapters::InvocationStrategy::Unsupported { ref adapter_id } => {
                Err(crate::executor::ExecuteError::NonExecutable(format!(
                    "adapter not supported by this frontend: {adapter_id}"
                )))
            }
            crate::adapters::InvocationStrategy::Direct => {
                if let Some(timing) = timing.as_mut() {
                    timing.record("execute.total", |timing| {
                        crate::executor::execute_function_with_timing(
                            file_path,
                            function_name,
                            &req.inputs,
                            &eff_mocks,
                            self.exec_timeout_ms,
                            harness_mode,
                            Some(timing),
                            &self.harness_manager.cache,
                            &self.harness_manager.crate_cache,
                            &self.harness_manager.bridge_cache,
                        )
                    })
                } else {
                    crate::executor::execute_function(
                        file_path,
                        function_name,
                        &req.inputs,
                        &eff_mocks,
                        self.exec_timeout_ms,
                        harness_mode,
                        &self.harness_manager.cache,
                        &self.harness_manager.crate_cache,
                        &self.harness_manager.bridge_cache,
                    )
                }
            }
        };

        match execution {
            Ok(result) => {
                resp.status = "execute".to_string();
                resp.outcome = Some(derive_execute_outcome(&result));
                resp.return_value = result.return_value;
                resp.thrown_error = result.thrown_error;
                resp.branch_path = Some(result.branch_path);
                resp.lines_executed = Some(result.lines_executed);
                resp.calls_to_external = Some(result.calls_to_external);
                resp.path_constraints = Some(result.path_constraints);
                resp.side_effects = Some(result.side_effects);
                resp.loop_body_states = Some(result.loop_body_states);
                resp.performance = Some(result.performance);
                self.finalize_response(resp, timing.as_mut())
            }
            Err(crate::executor::ExecuteError::FileError(msg)) => {
                resp.status = "error".to_string();
                resp.code = Some(ERR_FILE_NOT_FOUND.to_string());
                resp.outcome = Some(error_outcome(
                    crate::protocol::OutcomeStatus::RuntimeFailed,
                    &msg,
                ));
                resp.message = Some(msg);
                self.finalize_response(resp, timing.as_mut())
            }
            Err(crate::executor::ExecuteError::CompilationFailed(msg)) => {
                resp.status = "error".to_string();
                resp.code = Some(ERR_COMPILATION_ERROR.to_string());
                resp.outcome = Some(error_outcome(
                    crate::protocol::OutcomeStatus::BuildFailed,
                    &msg,
                ));
                resp.message = Some(msg);
                self.finalize_response(resp, timing.as_mut())
            }
            Err(crate::executor::ExecuteError::NonExecutable(msg)) => {
                resp.status = "error".to_string();
                resp.code = Some(ERR_NOT_SUPPORTED.to_string());
                resp.outcome = Some(error_outcome(
                    crate::protocol::OutcomeStatus::Unsupported,
                    &msg,
                ));
                resp.message = Some(msg);
                self.finalize_response(resp, timing.as_mut())
            }
            Err(e) => {
                resp.status = "error".to_string();
                resp.code = Some(ERR_INTERNAL_ERROR.to_string());
                let msg = e.to_string();
                resp.outcome = Some(error_outcome(
                    crate::protocol::OutcomeStatus::RuntimeFailed,
                    &msg,
                ));
                resp.message = Some(msg);
                self.finalize_response(resp, timing.as_mut())
            }
        }
    }

    fn handle_setup(&mut self, mut resp: Response, req: &Request) -> Response {
        let mut timing = self.maybe_timing_collector();

        self.remember_project_root(req.project_root.as_deref());
        self.run_preflight(req.project_root.as_deref());
        if self.preflight_failure.is_some() {
            return self.preflight_error_response(req.id);
        }

        let file_path = match &req.file {
            Some(f) => f,
            None => {
                resp.status = "error".to_string();
                resp.code = Some(ERR_INVALID_REQUEST.to_string());
                resp.message = Some("setup command requires a file path".to_string());
                return resp;
            }
        };
        let level = match &req.level {
            Some(l) => *l,
            None => {
                resp.status = "error".to_string();
                resp.code = Some(ERR_INVALID_REQUEST.to_string());
                resp.message = Some("setup command requires a level".to_string());
                return resp;
            }
        };
        let scope = match &req.scope {
            Some(s) => s.as_str(),
            None => {
                resp.status = "error".to_string();
                resp.code = Some(ERR_INVALID_REQUEST.to_string());
                resp.message = Some("setup command requires a scope".to_string());
                return resp;
            }
        };

        let path = std::path::Path::new(file_path);
        let setup_result = if let Some(timing) = timing.as_mut() {
            timing.record("setup.total", |_| {
                crate::setup::run_setup(path, level, scope, req.parent_context.as_ref())
            })
        } else {
            crate::setup::run_setup(path, level, scope, req.parent_context.as_ref())
        };

        match setup_result {
            Ok(result) => {
                resp.status = "setup".to_string();
                resp.setup_context = Some(result.context);
                self.finalize_response(resp, timing.as_mut())
            }
            Err(crate::setup::SetupError::FileNotFound(_)) => {
                resp.status = "error".to_string();
                resp.code = Some(ERR_FILE_NOT_FOUND.to_string());
                resp.message = Some(format!("setup file not found: {file_path}"));
                self.finalize_response(resp, timing.as_mut())
            }
            Err(crate::setup::SetupError::CompilationFailed(msg)) => {
                resp.status = "error".to_string();
                resp.code = Some(ERR_COMPILATION_ERROR.to_string());
                resp.message = Some(msg);
                self.finalize_response(resp, timing.as_mut())
            }
            Err(e) => {
                resp.status = "error".to_string();
                resp.code = Some(ERR_INTERNAL_ERROR.to_string());
                resp.message = Some(e.to_string());
                self.finalize_response(resp, timing.as_mut())
            }
        }
    }

    fn handle_teardown(&mut self, mut resp: Response, req: &Request) -> Response {
        let mut timing = self.maybe_timing_collector();
        let level = match &req.level {
            Some(l) => *l,
            None => {
                resp.status = "error".to_string();
                resp.code = Some(ERR_INVALID_REQUEST.to_string());
                resp.message = Some("teardown command requires a level".to_string());
                return resp;
            }
        };
        let scope = match &req.scope {
            Some(s) => s.as_str(),
            None => {
                resp.status = "error".to_string();
                resp.code = Some(ERR_INVALID_REQUEST.to_string());
                resp.message = Some("teardown command requires a scope".to_string());
                return resp;
            }
        };

        let teardown_result = if let Some(timing) = timing.as_mut() {
            timing.record("teardown.total", |_| {
                crate::setup::run_teardown(level, scope)
            })
        } else {
            crate::setup::run_teardown(level, scope)
        };

        // Prune orphaned harness entries at function-level teardown.
        if level == crate::protocol::SetupLevel::Function {
            self.prepared_harnesses.clear();
            self.cached_analyses.clear();
            self.harness_manager.prune_orphans();
            self.harness_manager.prune_missing_artifacts();
        }

        match teardown_result {
            Ok(()) => {
                resp.status = "teardown_ack".to_string();
                self.finalize_response(resp, timing.as_mut())
            }
            Err(e) => {
                resp.status = "error".to_string();
                resp.code = Some(ERR_INTERNAL_ERROR.to_string());
                resp.message = Some(e.to_string());
                self.finalize_response(resp, timing.as_mut())
            }
        }
    }

    fn handle_generate(&mut self, mut resp: Response, req: &Request) -> Response {
        let mut timing = self.maybe_timing_collector();
        let file_path = match &req.file {
            Some(f) => f,
            None => {
                resp.status = "error".to_string();
                resp.code = Some(ERR_INVALID_REQUEST.to_string());
                resp.message = Some("generate command requires a file path".to_string());
                return resp;
            }
        };
        let func_name = match &req.name {
            Some(n) => n.clone(),
            None => {
                resp.status = "error".to_string();
                resp.code = Some(ERR_INVALID_REQUEST.to_string());
                resp.message = Some("generate command requires a name".to_string());
                return resp;
            }
        };

        let path = std::path::Path::new(file_path);
        let ext = path.extension().and_then(|e| e.to_str());

        let generated = if let Some(timing) = timing.as_mut() {
            timing.record("generate.total", |_| {
                self.generate_response(resp, ext, path, &func_name, file_path, req)
            })
        } else {
            self.generate_response(resp, ext, path, &func_name, file_path, req)
        };

        self.finalize_response(generated, timing.as_mut())
    }

    fn generate_response(
        &mut self,
        mut resp: Response,
        ext: Option<&str>,
        path: &std::path::Path,
        func_name: &str,
        file_path: &str,
        req: &Request,
    ) -> Response {
        match ext {
            Some("wasm") => {
                match self
                    .wasm_cache
                    .generate(path, func_name, req.recipe.as_ref())
                {
                    Ok((value, generator_id, recipe)) => {
                        resp.status = "generate".to_string();
                        resp.value = Some(value);
                        resp.generator_id = Some(generator_id);
                        resp.recipe = recipe;
                        resp
                    }
                    Err(e) => {
                        resp.status = "error".to_string();
                        resp.code = Some(ERR_INTERNAL_ERROR.to_string());
                        resp.message = Some(e);
                        resp
                    }
                }
            }
            Some("rs") => {
                if let Some(ref registry) = self.native_registry {
                    match registry.generate_for_replay(
                        Some(file_path),
                        func_name,
                        req.recipe.clone(),
                    ) {
                        Ok((value, generator_id, recipe)) => {
                            resp.status = "generate".to_string();
                            resp.value = Some(attach_native_replay_metadata(
                                value,
                                file_path,
                                func_name,
                                Some(recipe.clone()),
                            ));
                            resp.generator_id = Some(generator_id);
                            resp.recipe = Some(recipe);
                            resp
                        }
                        Err(e) => {
                            resp.status = "error".to_string();
                            resp.code = Some(ERR_INTERNAL_ERROR.to_string());
                            resp.message = Some(e);
                            resp
                        }
                    }
                } else {
                    resp.status = "error".to_string();
                    resp.code = Some(ERR_INTERNAL_ERROR.to_string());
                    resp.message = Some(format!(
                        "native generator {func_name:?} requires a custom build (run `shatter build-frontend rust`)"
                    ));
                    resp
                }
            }
            _ => {
                resp.status = "error".to_string();
                resp.code = Some(ERR_INVALID_REQUEST.to_string());
                resp.message = Some(format!("unsupported generator file type: {}", file_path));
                resp
            }
        }
    }

    fn handle_shutdown(&mut self, mut resp: Response) -> Response {
        self.prepared_harnesses.clear();
        self.cached_analyses.clear();
        self.harness_manager.prune_orphans();
        self.harness_manager.prune_missing_artifacts();
        self.harness_manager.close_all();
        resp.status = "shutdown_ack".to_string();
        resp
    }

    fn send(&mut self, resp: &Response) -> io::Result<()> {
        let data = serde_json::to_string(resp).map_err(io::Error::other)?;
        self.logf(&format!("Sent: {data}"));
        writeln!(self.writer, "{data}")?;
        self.writer.flush()
    }

    /// Log at TRACE level (protocol messages).
    fn logf(&mut self, msg: &str) {
        self.log_at(FrontendLogLevel::Trace, msg);
    }

    /// Log at a specific level.
    fn log_at(&mut self, level: FrontendLogLevel, msg: &str) {
        if self.log_level >= level {
            let _ = writeln!(self.log, "[shatter-rust] {msg}");
        }
    }
}

/// Parse major and minor components from a semver string.
fn parse_major_minor(version: &str) -> Option<(u32, u32)> {
    let mut parts = version.splitn(3, '.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    Some((major, minor))
}

/// Check major.minor compatibility, ignoring patch version.
/// Matches the Go/TypeScript frontends' semver-compatible behavior.
fn is_version_compatible(version: &str) -> bool {
    let req = parse_major_minor(version);
    let ours = parse_major_minor(protocol::PROTOCOL_VERSION);
    match (req, ours) {
        (Some((rmaj, rmin)), Some((omaj, omin))) => rmaj == omaj && rmin == omin,
        _ => false,
    }
}

fn attach_native_replay_metadata(
    mut value: serde_json::Value,
    file_path: &str,
    func_name: &str,
    recipe: Option<serde_json::Value>,
) -> serde_json::Value {
    if let Some(obj) = value.as_object_mut()
        && obj
            .get("__shatter_native")
            .and_then(serde_json::Value::as_bool)
            == Some(true)
    {
        obj.insert(
            "__shatter_replay".to_string(),
            serde_json::json!({
                "language": "rust",
                "file": file_path,
                "name": func_name,
                "recipe": recipe.unwrap_or(serde_json::Value::Null),
            }),
        );
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Send a single request and read the response.
    fn send_recv(req_json: &str) -> Response {
        let input = format!("{req_json}\n");
        let mut output = Vec::new();
        let handler = Handler::new(input.as_bytes(), &mut output, io::sink());
        handler.run().expect("handler.run");
        let output_str = String::from_utf8(output).expect("valid utf8");
        let first_line = output_str
            .lines()
            .next()
            .expect("at least one response line");
        serde_json::from_str(first_line).expect("valid JSON response")
    }

    /// Send multiple requests and return all responses.
    fn conversation(requests: &[&str]) -> Vec<Response> {
        let input = requests.join("\n") + "\n";
        let mut output = Vec::new();
        let handler = Handler::new(input.as_bytes(), &mut output, io::sink());
        handler.run().expect("handler.run");
        let output_str = String::from_utf8(output).expect("valid utf8");
        output_str
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| serde_json::from_str(l).expect("valid JSON response"))
            .collect()
    }

    fn timing_phase_names(resp: &Response) -> std::collections::BTreeSet<String> {
        resp.timing
            .as_ref()
            .map(|summary| {
                summary
                    .phases
                    .iter()
                    .map(|phase| phase.phase_path.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn is_offline_compile_error(resp: &Response) -> bool {
        resp.status == "error"
            && resp.message.as_deref().is_some_and(|msg| {
                msg.contains("spurious network error")
                    || msg.contains("download of config.json failed")
                    || msg.contains("Could not resolve host")
                    || msg.contains("Could not resolve hostname")
            })
    }

    #[test]
    fn preflight_returns_preflight_failed_when_cargo_toml_missing() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().display().to_string();
        let req = format!(
            r#"{{"protocol_version":"0.1.0","id":1,"command":"analyze","file":"{root}/missing.rs","project_root":"{root}"}}"#,
            root = root
        );
        let resp = send_recv(&req);
        assert_eq!(resp.status, "error");
        assert_eq!(resp.code.as_deref(), Some(ERR_PREFLIGHT_FAILED));
        let msg = resp.message.unwrap_or_default();
        assert!(
            msg.starts_with("preflight_failed: missing_cargo_toml: "),
            "unexpected message: {msg}"
        );
        assert!(
            msg.contains("Cargo.toml"),
            "message missing Cargo.toml: {msg}"
        );
    }

    #[test]
    fn preflight_is_sticky_across_subsequent_requests() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().display().to_string();
        let req1 = format!(
            r#"{{"protocol_version":"0.1.0","id":1,"command":"analyze","file":"{root}/missing.rs","project_root":"{root}"}}"#,
            root = root
        );
        // Second request points at a *different* valid-looking project_root; the
        // sticky failure must still short-circuit it.
        let other = tempfile::tempdir().unwrap();
        std::fs::write(other.path().join("Cargo.toml"), "[package]\nname=\"x\"\n").unwrap();
        let req2 = format!(
            r#"{{"protocol_version":"0.1.0","id":2,"command":"analyze","file":"{}/missing.rs","project_root":"{}"}}"#,
            other.path().display(),
            other.path().display()
        );
        let responses = conversation(&[&req1, &req2]);
        assert_eq!(responses.len(), 2);
        for r in &responses {
            assert_eq!(r.status, "error");
            assert_eq!(r.code.as_deref(), Some(ERR_PREFLIGHT_FAILED));
        }
        // Both responses carry the *first* root's detail because the failure is sticky.
        let detail = format!("{}/Cargo.toml", root);
        assert!(
            responses[1]
                .message
                .as_deref()
                .unwrap_or("")
                .contains(&detail)
        );
    }

    #[test]
    fn preflight_short_circuits_execute_via_last_project_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().display().to_string();
        let analyze = format!(
            r#"{{"protocol_version":"0.1.0","id":1,"command":"analyze","file":"{root}/missing.rs","project_root":"{root}"}}"#,
            root = root
        );
        // Execute carries no project_root; it must inherit the sticky failure
        // (and would inherit last_project_root anyway).
        let execute =
            r#"{"protocol_version":"0.1.0","id":2,"command":"execute","function":"f","inputs":[]}"#;
        let responses = conversation(&[&analyze, execute]);
        assert_eq!(responses.len(), 2);
        assert_eq!(responses[0].code.as_deref(), Some(ERR_PREFLIGHT_FAILED));
        assert_eq!(responses[1].code.as_deref(), Some(ERR_PREFLIGHT_FAILED));
    }

    #[test]
    fn preflight_skips_when_no_project_root() {
        // No project_root and no prior project_root means preflight does
        // nothing; analyze proceeds to its own file-not-found check.
        let req = r#"{"protocol_version":"0.1.0","id":1,"command":"analyze","file":"/nonexistent/path/missing.rs"}"#;
        let resp = send_recv(req);
        assert_eq!(resp.status, "error");
        assert_eq!(resp.code.as_deref(), Some(ERR_FILE_NOT_FOUND));
    }

    #[test]
    fn preflight_succeeds_on_valid_cargo_root() {
        // Drive run_preflight directly so success caches the root without
        // depending on a full analyze flow.
        let input: &[u8] = b"";
        let mut output = Vec::new();
        let mut handler = Handler::new(input, &mut output, io::sink());
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]\nname=\"x\"\n").unwrap();
        let root = dir.path().display().to_string();
        handler.run_preflight(Some(&root));
        assert!(handler.preflight_failure.is_none());
        assert!(handler.preflight_checked_roots.contains(&root));
    }

    #[test]
    fn handshake_returns_rust_language() {
        let resp = send_recv(
            r#"{"protocol_version":"0.1.0","id":1,"command":"handshake","capabilities":["analyze"]}"#,
        );
        assert_eq!(resp.status, "handshake");
        assert_eq!(resp.language.as_deref(), Some("rust"));
        assert_eq!(
            resp.frontend_version.as_deref(),
            Some(crate::protocol::FRONTEND_VERSION)
        );
    }

    #[test]
    fn executor_result_preserves_loop_body_states() {
        let raw = serde_json::json!({
            "return_value": 6,
            "branch_path": [],
            "lines_executed": [],
            "calls_to_external": [],
            "path_constraints": [],
            "side_effects": [],
            "loop_body_states": [
                {"loop_id": 3, "iteration": 0, "locals": {}},
                {"loop_id": 3, "iteration": 1, "locals": {}}
            ],
            "performance": {
                "wall_time_ms": 0.0,
                "cpu_time_us": 0,
                "heap_used_bytes": 0,
                "heap_allocated_bytes": 0
            }
        });

        let result: crate::executor::ExecuteResult =
            serde_json::from_value(raw).expect("execute result");

        assert_eq!(result.loop_body_states.len(), 2);
        assert_eq!(result.loop_body_states[0]["loop_id"], 3);
        assert_eq!(result.loop_body_states[1]["iteration"], 1);
    }

    #[test]
    fn response_serializes_loop_body_states() {
        let mut resp = Response::base(7);
        resp.status = "execute".to_string();
        resp.loop_body_states = Some(vec![serde_json::json!({
            "loop_id": 4,
            "iteration": 0,
            "locals": {}
        })]);

        let encoded = serde_json::to_value(&resp).expect("response JSON");

        assert_eq!(
            encoded["loop_body_states"][0]["loop_id"],
            serde_json::json!(4)
        );
        assert_eq!(
            encoded["loop_body_states"][0]["iteration"],
            serde_json::json!(0)
        );
    }

    #[test]
    fn handshake_returns_all_capabilities() {
        let resp = send_recv(
            r#"{"protocol_version":"0.1.0","id":1,"command":"handshake","capabilities":["analyze"]}"#,
        );
        let caps = resp.capabilities.expect("capabilities present");
        assert!(caps.contains(&"analyze".to_string()));
        assert!(caps.contains(&"execute".to_string()));
        assert!(caps.contains(&"generate".to_string()));
        assert!(caps.contains(&"instrument".to_string()));
        assert!(
            caps.contains(&"prepare".to_string()),
            "prepare must be advertised"
        );
        assert!(
            caps.contains(&"setup".to_string()),
            "setup must be advertised"
        );
        assert!(
            caps.contains(&"teardown".to_string()),
            "teardown must be advertised"
        );
    }

    #[test]
    fn prepare_without_file_returns_error() {
        let resp = send_recv(r#"{"protocol_version":"0.1.0","id":1,"command":"prepare"}"#);
        assert_eq!(resp.status, "error");
        assert_eq!(resp.code.as_deref(), Some("invalid_request"));
    }

    #[test]
    fn prepare_without_function_returns_error() {
        let resp = send_recv(
            r#"{"protocol_version":"0.1.0","id":1,"command":"prepare","file":"/tmp/test.rs"}"#,
        );
        assert_eq!(resp.status, "error");
        assert_eq!(resp.code.as_deref(), Some("invalid_request"));
    }

    #[test]
    fn prepare_with_missing_file_returns_file_not_found() {
        let resp = send_recv(
            r#"{"protocol_version":"0.1.0","id":1,"command":"prepare","file":"/tmp/nonexistent_shatter_test.rs","function":"add"}"#,
        );
        assert_eq!(resp.status, "error");
        assert_eq!(resp.code.as_deref(), Some("file_not_found"));
    }

    #[test]
    fn handshake_with_timing_capability_does_not_emit_timing() {
        let resp = send_recv(
            r#"{"protocol_version":"0.1.0","id":1,"command":"handshake","capabilities":["analyze","timing"]}"#,
        );
        assert!(resp.timing.is_none());
    }

    #[test]
    fn handshake_echoes_request_id() {
        let resp = send_recv(
            r#"{"protocol_version":"0.1.0","id":42,"command":"handshake","capabilities":[]}"#,
        );
        assert_eq!(resp.id, 42);
    }

    #[test]
    fn handshake_includes_protocol_version() {
        let resp = send_recv(
            r#"{"protocol_version":"0.1.0","id":1,"command":"handshake","capabilities":[]}"#,
        );
        assert_eq!(resp.protocol_version, crate::protocol::PROTOCOL_VERSION);
    }

    #[test]
    fn shutdown_returns_ack_and_stops() {
        let resp = send_recv(r#"{"protocol_version":"0.1.0","id":5,"command":"shutdown"}"#);
        assert_eq!(resp.status, "shutdown_ack");
        assert_eq!(resp.id, 5);
    }

    #[test]
    fn version_mismatch_returns_error() {
        let resp = send_recv(
            r#"{"protocol_version":"99.0.0","id":1,"command":"handshake","capabilities":[]}"#,
        );
        assert_eq!(resp.status, "error");
        assert_eq!(resp.code.as_deref(), Some(ERR_VERSION_MISMATCH));
    }

    #[test]
    fn compatible_patch_version_is_accepted() {
        // major.minor match with different patch should succeed (parity with TS/Go)
        let resp = send_recv(
            r#"{"protocol_version":"0.1.99","id":1,"command":"handshake","capabilities":[]}"#,
        );
        assert_eq!(resp.status, "handshake");
    }

    #[test]
    fn malformed_version_is_rejected() {
        let resp = send_recv(
            r#"{"protocol_version":"abc","id":1,"command":"handshake","capabilities":[]}"#,
        );
        assert_eq!(resp.status, "error");
        assert_eq!(resp.code.as_deref(), Some(ERR_VERSION_MISMATCH));
    }

    #[test]
    fn unknown_command_returns_error() {
        let resp = send_recv(r#"{"protocol_version":"0.1.0","id":1,"command":"foobar"}"#);
        assert_eq!(resp.status, "error");
        assert_eq!(resp.code.as_deref(), Some(ERR_INVALID_REQUEST));
    }

    #[test]
    fn malformed_json_returns_invalid_request() {
        let resp = send_recv("not valid json");
        assert_eq!(resp.status, "error");
        assert_eq!(resp.code.as_deref(), Some(ERR_INVALID_REQUEST));
        assert_eq!(resp.id, 0);
        assert!(
            resp.message
                .as_deref()
                .unwrap_or("")
                .contains("Invalid JSON")
        );
    }

    #[test]
    fn malformed_json_does_not_abort_handler() {
        let responses = conversation(&[
            "not valid json",
            r#"{"protocol_version":"0.1.0","id":99,"command":"shutdown"}"#,
        ]);
        assert_eq!(responses.len(), 2);
        assert_eq!(responses[0].status, "error");
        assert_eq!(responses[0].code.as_deref(), Some(ERR_INVALID_REQUEST));
        assert_eq!(responses[0].id, 0);
        assert_eq!(responses[1].status, "shutdown_ack");
        assert_eq!(responses[1].id, 99);
    }

    #[test]
    fn analyze_without_file_returns_error() {
        let resp = send_recv(r#"{"protocol_version":"0.1.0","id":2,"command":"analyze"}"#);
        assert_eq!(resp.status, "error");
        assert_eq!(resp.code.as_deref(), Some(ERR_INVALID_REQUEST));
    }

    #[test]
    fn analyze_emits_timing_when_requested() {
        let dir = std::env::temp_dir().join("shatter-test-rust-timing-analyze");
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("analyze.rs");
        std::fs::write(&file, "fn add(a: i32, b: i32) -> i32 { a + b }\n").expect("write file");

        let responses = conversation(&[
            r#"{"protocol_version":"0.1.0","id":1,"command":"handshake","capabilities":["analyze","timing"]}"#,
            &format!(
                r#"{{"protocol_version":"0.1.0","id":2,"command":"analyze","file":"{}","function":"add"}}"#,
                file.display()
            ),
        ]);
        let phases = timing_phase_names(&responses[1]);
        for expected in [
            "analyze.total",
            "analyze.read",
            "analyze.parse",
            "analyze.walk",
            "serialize.response",
        ] {
            assert!(phases.contains(expected), "missing timing phase {expected}");
        }
    }

    #[test]
    fn analyze_with_nonexistent_file_returns_file_not_found() {
        let resp = send_recv(
            r#"{"protocol_version":"0.1.0","id":2,"command":"analyze","file":"nonexistent.rs"}"#,
        );
        assert_eq!(resp.status, "error");
        assert_eq!(resp.code.as_deref(), Some(ERR_FILE_NOT_FOUND));
    }

    #[test]
    fn instrument_without_file_returns_error() {
        let resp = send_recv(r#"{"protocol_version":"0.1.0","id":3,"command":"instrument"}"#);
        assert_eq!(resp.status, "error");
        assert_eq!(resp.code.as_deref(), Some(ERR_INVALID_REQUEST));
    }

    #[test]
    fn instrument_with_nonexistent_file_returns_file_not_found() {
        let resp = send_recv(
            r#"{"protocol_version":"0.1.0","id":3,"command":"instrument","file":"nonexistent.rs"}"#,
        );
        assert_eq!(resp.status, "error");
        assert_eq!(resp.code.as_deref(), Some(ERR_FILE_NOT_FOUND));
    }

    #[test]
    fn instrument_with_valid_file_returns_success() {
        // Create a temp file with valid Rust source
        let dir = std::env::temp_dir().join("shatter-test-instrument");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("sample.rs");
        std::fs::write(
            &file,
            "fn foo(x: i32) -> bool { if x > 0 { true } else { false } }",
        )
        .unwrap();

        let file_path = file.to_string_lossy();
        let req = format!(
            r#"{{"protocol_version":"0.1.0","id":3,"command":"instrument","file":"{}"}}"#,
            file_path.replace('\\', "\\\\")
        );
        let resp = send_recv(&req);
        assert_eq!(
            resp.status, "instrument",
            "expected instrument status, got: {:?}",
            resp.message
        );
        assert_eq!(resp.instrumented, Some(true));
        assert!(resp.output_file.is_some());

        // Clean up
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn instrument_temp_file_avoids_direct_shatter_tmp_parent() {
        let output = write_instrumented_temp(
            "sample.rs",
            "fn sample(x: i32) -> i32 { if x > 0 { x } else { -x } }",
        )
        .expect("write instrumented temp");
        let output_path = std::path::PathBuf::from(&output);
        let parent = output_path
            .parent()
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .expect("instrumented output has named parent");

        assert!(
            !parent.starts_with("shatter-instrument-"),
            "instrumentation temp files must not use a direct shatter-* parent under TMPDIR: {output}"
        );

        if let Some(parent) = output_path.parent() {
            let _ = std::fs::remove_dir_all(parent);
        }
    }

    #[test]
    fn execute_without_file_returns_error() {
        let resp = send_recv(
            r#"{"protocol_version":"0.1.0","id":4,"command":"execute","function":"F","inputs":[],"mocks":[]}"#,
        );
        assert_eq!(resp.status, "error");
        assert_eq!(resp.code.as_deref(), Some(ERR_INVALID_REQUEST));
        assert!(resp.message.as_deref().unwrap_or("").contains("file"));
    }

    #[test]
    fn execute_emits_timing_when_requested() {
        let dir = std::env::temp_dir().join("shatter-test-rust-timing-execute");
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("execute.rs");
        std::fs::write(&file, "fn add(a: i32, b: i32) -> i32 { a + b }\n").expect("write file");

        let responses = conversation(&[
            r#"{"protocol_version":"0.1.0","id":1,"command":"handshake","capabilities":["execute","timing"]}"#,
            &format!(
                r#"{{"protocol_version":"0.1.0","id":2,"command":"execute","file":"{}","function":"add","inputs":[1,2],"mocks":[]}}"#,
                file.display()
            ),
        ]);
        if is_offline_compile_error(&responses[1]) {
            eprintln!(
                "skipping execute_emits_timing_when_requested: cargo unavailable ({})",
                responses[1].message.as_deref().unwrap_or("unknown error")
            );
            return;
        }
        assert_eq!(responses[1].status, "execute");
        let phases = timing_phase_names(&responses[1]);
        for expected in [
            "execute.total",
            "execute.read_source",
            "execute.extract_signature",
            "execute.instrument",
            "execute.generate_harness",
            "execute.build",
            "serialize.response",
        ] {
            assert!(phases.contains(expected), "missing timing phase {expected}");
        }
    }

    #[test]
    fn execute_without_function_returns_error() {
        let resp = send_recv(
            r#"{"protocol_version":"0.1.0","id":4,"command":"execute","file":"test.rs","inputs":[],"mocks":[]}"#,
        );
        assert_eq!(resp.status, "error");
        assert_eq!(resp.code.as_deref(), Some(ERR_INVALID_REQUEST));
        assert!(resp.message.as_deref().unwrap_or("").contains("function"));
    }

    #[test]
    fn execute_with_nonexistent_file_returns_error() {
        let resp = send_recv(
            r#"{"protocol_version":"0.1.0","id":4,"command":"execute","file":"/nonexistent/file.rs","function":"f","inputs":[],"mocks":[]}"#,
        );
        assert_eq!(resp.status, "error");
        assert_eq!(resp.code.as_deref(), Some(ERR_FILE_NOT_FOUND));
    }

    #[test]
    fn multiple_commands_in_sequence() {
        let responses = conversation(&[
            r#"{"protocol_version":"0.1.0","id":1,"command":"handshake","capabilities":["analyze"]}"#,
            r#"{"protocol_version":"0.1.0","id":2,"command":"shutdown"}"#,
        ]);
        assert_eq!(responses.len(), 2);
        assert_eq!(responses[0].status, "handshake");
        assert_eq!(responses[1].status, "shutdown_ack");
    }

    #[test]
    fn shutdown_stops_processing_further_commands() {
        let responses = conversation(&[
            r#"{"protocol_version":"0.1.0","id":1,"command":"shutdown"}"#,
            r#"{"protocol_version":"0.1.0","id":2,"command":"handshake","capabilities":[]}"#,
        ]);
        assert_eq!(responses.len(), 1, "shutdown should stop processing");
    }

    #[test]
    fn empty_lines_are_skipped() {
        let input = "\n\n{\"protocol_version\":\"0.1.0\",\"id\":1,\"command\":\"shutdown\"}\n\n";
        let mut output = Vec::new();
        let handler = Handler::new(input.as_bytes(), &mut output, io::sink());
        handler.run().expect("handler.run");
        let output_str = String::from_utf8(output).expect("valid utf8");
        let lines: Vec<&str> = output_str.lines().filter(|l| !l.is_empty()).collect();
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn debug_output_goes_to_log() {
        let input =
            r#"{"protocol_version":"0.1.0","id":1,"command":"shutdown"}"#.to_string() + "\n";
        let mut output = Vec::new();
        let mut log = Vec::new();
        let handler = Handler::with_log_level(
            input.as_bytes(),
            &mut output,
            &mut log,
            FrontendLogLevel::Trace,
        );
        handler.run().expect("handler.run");
        let log_str = String::from_utf8(log).expect("valid utf8");
        assert!(
            log_str.contains("[shatter-rust]"),
            "log missing prefix: {log_str}"
        );
        assert!(
            log_str.contains("Shutting down"),
            "log missing shutdown message: {log_str}"
        );
    }

    #[test]
    fn log_level_filtering_suppresses_trace_at_info() {
        let input =
            r#"{"protocol_version":"0.1.0","id":1,"command":"shutdown"}"#.to_string() + "\n";
        let mut output = Vec::new();
        let mut log = Vec::new();
        let handler = Handler::with_log_level(
            input.as_bytes(),
            &mut output,
            &mut log,
            FrontendLogLevel::Info,
        );
        handler.run().expect("handler.run");
        let log_str = String::from_utf8(log).expect("valid utf8");
        // At INFO level, protocol messages (Received/Sent/Shutting down) should be suppressed
        assert!(
            !log_str.contains("Received:"),
            "trace messages should be suppressed at info: {log_str}"
        );
        assert!(
            !log_str.contains("Sent:"),
            "trace messages should be suppressed at info: {log_str}"
        );
    }

    #[test]
    fn log_level_filtering_shows_debug_at_debug() {
        let input =
            r#"{"protocol_version":"0.1.0","id":1,"command":"shutdown"}"#.to_string() + "\n";
        let mut output = Vec::new();
        let mut log = Vec::new();
        let handler = Handler::with_log_level(
            input.as_bytes(),
            &mut output,
            &mut log,
            FrontendLogLevel::Debug,
        );
        handler.run().expect("handler.run");
        let log_str = String::from_utf8(log).expect("valid utf8");
        // At DEBUG level, lifecycle messages should appear but not protocol details
        assert!(
            log_str.contains("Starting Rust frontend"),
            "debug messages should appear at debug: {log_str}"
        );
        assert!(
            log_str.contains("Shutting down"),
            "debug messages should appear at debug: {log_str}"
        );
        assert!(
            !log_str.contains("Received:"),
            "trace messages should be suppressed at debug: {log_str}"
        );
    }

    // -- Setup command tests --

    #[test]
    fn setup_without_file_returns_error() {
        let resp = send_recv(
            r#"{"protocol_version":"0.1.0","id":10,"command":"setup","scope":"myFunc","level":"function"}"#,
        );
        assert_eq!(resp.status, "error");
        assert_eq!(resp.code.as_deref(), Some(ERR_INVALID_REQUEST));
        assert!(resp.message.as_deref().unwrap_or("").contains("file"));
    }

    #[test]
    fn setup_without_level_returns_error() {
        let resp = send_recv(
            r#"{"protocol_version":"0.1.0","id":10,"command":"setup","file":"./setup.rs","scope":"myFunc"}"#,
        );
        assert_eq!(resp.status, "error");
        assert_eq!(resp.code.as_deref(), Some(ERR_INVALID_REQUEST));
        assert!(resp.message.as_deref().unwrap_or("").contains("level"));
    }

    #[test]
    fn setup_without_scope_returns_error() {
        let resp = send_recv(
            r#"{"protocol_version":"0.1.0","id":10,"command":"setup","file":"./setup.rs","level":"function"}"#,
        );
        assert_eq!(resp.status, "error");
        assert_eq!(resp.code.as_deref(), Some(ERR_INVALID_REQUEST));
        assert!(resp.message.as_deref().unwrap_or("").contains("scope"));
    }

    #[test]
    fn setup_with_nonexistent_file_returns_file_not_found() {
        let resp = send_recv(
            r#"{"protocol_version":"0.1.0","id":10,"command":"setup","file":"./nonexistent_setup.rs","scope":"myFunc","level":"function"}"#,
        );
        assert_eq!(resp.status, "error");
        assert_eq!(resp.code.as_deref(), Some(ERR_FILE_NOT_FOUND));
    }

    // -- Teardown command tests --

    #[test]
    fn teardown_without_level_returns_error() {
        let resp = send_recv(
            r#"{"protocol_version":"0.1.0","id":11,"command":"teardown","scope":"myFunc"}"#,
        );
        assert_eq!(resp.status, "error");
        assert_eq!(resp.code.as_deref(), Some(ERR_INVALID_REQUEST));
        assert!(resp.message.as_deref().unwrap_or("").contains("level"));
    }

    #[test]
    fn teardown_without_scope_returns_error() {
        let resp = send_recv(
            r#"{"protocol_version":"0.1.0","id":11,"command":"teardown","level":"function"}"#,
        );
        assert_eq!(resp.status, "error");
        assert_eq!(resp.code.as_deref(), Some(ERR_INVALID_REQUEST));
        assert!(resp.message.as_deref().unwrap_or("").contains("scope"));
    }

    #[test]
    fn teardown_with_valid_fields_returns_success() {
        let resp = send_recv(
            r#"{"protocol_version":"0.1.0","id":11,"command":"teardown","scope":"myFunc","level":"function"}"#,
        );
        assert_eq!(resp.status, "teardown_ack");
        assert_eq!(resp.id, 11);
    }

    #[test]
    fn teardown_ack_matches_protocol_spec() {
        let resp = send_recv(
            r#"{"protocol_version":"0.1.0","id":50,"command":"teardown","scope":"cleanup","level":"session"}"#,
        );
        assert_eq!(
            resp.status, "teardown_ack",
            "teardown must return teardown_ack per protocol spec"
        );
    }

    // -- Generate command tests --

    #[test]
    fn generate_without_file_returns_error() {
        let resp = send_recv(
            r#"{"protocol_version":"0.1.0","id":13,"command":"generate","name":"User","kind":"type_name"}"#,
        );
        assert_eq!(resp.status, "error");
        assert_eq!(resp.code.as_deref(), Some(ERR_INVALID_REQUEST));
        assert!(resp.message.as_deref().unwrap_or("").contains("file"));
    }

    #[test]
    fn generate_without_name_returns_error() {
        let resp = send_recv(
            r#"{"protocol_version":"0.1.0","id":13,"command":"generate","file":"./gen.ts","kind":"type_name"}"#,
        );
        assert_eq!(resp.status, "error");
        assert_eq!(resp.code.as_deref(), Some(ERR_INVALID_REQUEST));
        assert!(resp.message.as_deref().unwrap_or("").contains("name"));
    }

    #[test]
    fn generate_non_wasm_file_returns_unsupported() {
        let resp = send_recv(
            r#"{"protocol_version":"0.1.0","id":14,"command":"generate","file":"./gen.ts","name":"User","kind":"type_name"}"#,
        );
        assert_eq!(resp.status, "error");
        assert_eq!(resp.code.as_deref(), Some(ERR_INVALID_REQUEST));
        assert!(
            resp.message
                .as_deref()
                .unwrap_or("")
                .contains("unsupported generator file type")
        );
    }

    #[test]
    fn native_replay_metadata_is_embedded_in_native_sentinel() {
        let value = serde_json::json!({
            "__shatter_native": true,
            "handle": "h_0001"
        });
        let value = attach_native_replay_metadata(
            value,
            "/tmp/generators.rs",
            "GeneratedString",
            Some(serde_json::json!({"value": "abc"})),
        );

        assert_eq!(value["__shatter_replay"]["language"], "rust");
        assert_eq!(value["__shatter_replay"]["file"], "/tmp/generators.rs");
        assert_eq!(value["__shatter_replay"]["name"], "GeneratedString");
        assert_eq!(
            value["__shatter_replay"]["recipe"],
            serde_json::json!({"value": "abc"})
        );
    }

    #[test]
    fn native_rs_generate_drops_replayable_live_value() {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };

        use crate::generators::GeneratorResult;

        struct DropProbe(Arc<AtomicUsize>);

        impl Drop for DropProbe {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }

        let drops = Arc::new(AtomicUsize::new(0));
        let generator_file = "/tmp/shatter-replayable-generator.rs";
        let generator_drops = Arc::clone(&drops);
        let mut registry = NativeRegistry::new();
        registry.register_for_file(
            generator_file,
            "State",
            Box::new(move |_recipe| GeneratorResult {
                id: "replayable-state".to_string(),
                value: Box::new(DropProbe(Arc::clone(&generator_drops))),
                recipe: serde_json::json!({"seed": 1}),
            }),
        );

        let req: Request = serde_json::from_value(serde_json::json!({
            "protocol_version": "0.1.0",
            "id": 41,
            "command": "generate",
            "file": generator_file,
            "name": "State",
            "kind": "type_name"
        }))
        .expect("request should decode");
        let mut handler =
            Handler::new_with_native_registry(io::empty(), Vec::new(), io::sink(), registry);

        let (resp, shutdown) = handler.dispatch(&req);

        assert!(!shutdown);
        assert_eq!(resp.status, "generate");
        let value = resp.value.expect("generate response should include value");
        assert_eq!(value["__shatter_native"], true);
        assert!(value.get("__shatter_replay").is_some());
        let registry = handler
            .native_registry
            .as_ref()
            .expect("native registry should remain attached");
        assert!(
            registry.handles.is_empty(),
            "replayable .rs generator output should not retain live handles"
        );
        assert_eq!(
            drops.load(Ordering::SeqCst),
            1,
            "live replayable generator value should be dropped after recipe capture"
        );
    }

    #[test]
    fn native_rs_generate_uses_project_root_as_cwd_and_restores_caller_cwd() {
        use crate::generators::GeneratorResult;

        let caller_cwd = std::env::current_dir().expect("read caller cwd");
        let project_dir = tempfile::tempdir().expect("project tempdir");
        let generator_file = "/tmp/shatter-project-root-generator.rs";
        let mut registry = NativeRegistry::new();
        registry.register_for_file(
            generator_file,
            "State",
            Box::new(|_recipe| {
                let cwd = std::env::current_dir()
                    .expect("read generator cwd")
                    .display()
                    .to_string();
                GeneratorResult {
                    id: "project-root-state".to_string(),
                    value: Box::new(()),
                    recipe: serde_json::json!({ "cwd": cwd }),
                }
            }),
        );

        let req: Request = serde_json::from_value(serde_json::json!({
            "protocol_version": "0.1.0",
            "id": 42,
            "command": "generate",
            "file": generator_file,
            "name": "State",
            "kind": "type_name",
            "project_root": project_dir.path()
        }))
        .expect("request should decode");
        let mut handler =
            Handler::new_with_native_registry(io::empty(), Vec::new(), io::sink(), registry);

        let (resp, shutdown) = handler.dispatch(&req);

        assert!(!shutdown);
        assert_eq!(resp.status, "generate");
        assert_eq!(
            resp.recipe.expect("recipe")["cwd"],
            project_dir.path().display().to_string()
        );
        assert_eq!(
            std::env::current_dir().expect("read restored cwd"),
            caller_cwd
        );
    }

    #[test]
    fn generate_wasm_missing_file_returns_error() {
        let resp = send_recv(
            r#"{"protocol_version":"0.1.0","id":15,"command":"generate","file":"/nonexistent/gen.wasm","name":"gen","kind":"type_name"}"#,
        );
        assert_eq!(resp.status, "error");
        assert_eq!(resp.code.as_deref(), Some(ERR_INTERNAL_ERROR));
        assert!(resp.message.as_deref().unwrap_or("").contains("not found"));
    }

    // -- Integration: new commands in conversation --

    #[test]
    fn setup_teardown_generate_in_conversation() {
        let responses = conversation(&[
            r#"{"protocol_version":"0.1.0","id":1,"command":"handshake","capabilities":["analyze"]}"#,
            r#"{"protocol_version":"0.1.0","id":2,"command":"setup","file":"./nonexistent_setup.rs","scope":"fn1","level":"function"}"#,
            r#"{"protocol_version":"0.1.0","id":3,"command":"teardown","scope":"fn1","level":"function"}"#,
            r#"{"protocol_version":"0.1.0","id":4,"command":"generate","file":"./gen.ts","name":"User","kind":"type_name"}"#,
            r#"{"protocol_version":"0.1.0","id":5,"command":"shutdown"}"#,
        ]);
        assert_eq!(responses.len(), 5);
        assert_eq!(responses[0].status, "handshake");
        assert_eq!(responses[0].id, 1);
        // setup with nonexistent file returns file_not_found error
        assert_eq!(responses[1].status, "error");
        assert_eq!(responses[1].code.as_deref(), Some(ERR_FILE_NOT_FOUND));
        // teardown succeeds (no-op for now)
        assert_eq!(responses[2].status, "teardown_ack");
        assert_eq!(responses[2].id, 3);
        // generate with non-wasm file returns "unsupported generator file type"
        assert_eq!(responses[3].status, "error");
        assert_eq!(responses[3].code.as_deref(), Some(ERR_INVALID_REQUEST));
        assert_eq!(responses[4].status, "shutdown_ack");
        assert_eq!(responses[4].id, 5);
    }

    #[test]
    fn new_commands_echo_correct_request_ids() {
        let responses = conversation(&[
            r#"{"protocol_version":"0.1.0","id":100,"command":"setup","file":"./nonexistent.rs","scope":"f","level":"function"}"#,
            r#"{"protocol_version":"0.1.0","id":200,"command":"teardown","scope":"f","level":"function"}"#,
            r#"{"protocol_version":"0.1.0","id":300,"command":"generate","file":"./g.ts","name":"T","kind":"type_name"}"#,
        ]);
        assert_eq!(responses[0].id, 100);
        assert_eq!(responses[1].id, 200);
        assert_eq!(responses[2].id, 300);
    }

    // -- Execute file fallback tests (str-rv0k) --

    #[test]
    fn execute_without_file_falls_back_to_last_analyzed_file() {
        // The core never sends `file` in Execute — the frontend must remember it
        // from the prior Analyze request.
        let dir = std::env::temp_dir().join("shatter-test-rv0k-analyze");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("simple.rs");
        std::fs::write(&file, "pub fn add(a: i32, b: i32) -> i32 { a + b }").unwrap();

        let file_path = file.to_string_lossy().replace('\\', "\\\\");
        let responses = conversation(&[
            // 1. Analyze sets last_file
            &format!(
                r#"{{"protocol_version":"0.1.0","id":1,"command":"analyze","file":"{file_path}"}}"#
            ),
            // 2. Execute without file field — should use last_file, not return invalid_request
            r#"{"protocol_version":"0.1.0","id":2,"command":"execute","function":"add","inputs":[1,2],"mocks":[]}"#,
        ]);

        assert_eq!(
            responses[0].status, "analyze",
            "analyze failed: {:?}",
            responses[0].message
        );
        // The execute should NOT fail with "requires a file path"
        assert_ne!(
            responses[1].code.as_deref(),
            Some(ERR_INVALID_REQUEST),
            "execute should fall back to last_file from analyze, got: {:?}",
            responses[1].message,
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn execute_without_file_falls_back_to_last_instrumented_file() {
        let dir = std::env::temp_dir().join("shatter-test-rv0k-instrument");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("branchy.rs");
        std::fs::write(
            &file,
            "pub fn check(x: i32) -> bool { if x > 0 { true } else { false } }",
        )
        .unwrap();

        let file_path = file.to_string_lossy().replace('\\', "\\\\");
        let responses = conversation(&[
            &format!(
                r#"{{"protocol_version":"0.1.0","id":1,"command":"instrument","file":"{file_path}"}}"#
            ),
            r#"{"protocol_version":"0.1.0","id":2,"command":"execute","function":"check","inputs":[42],"mocks":[]}"#,
        ]);

        assert_eq!(
            responses[0].status, "instrument",
            "instrument failed: {:?}",
            responses[0].message
        );
        assert_ne!(
            responses[1].code.as_deref(),
            Some(ERR_INVALID_REQUEST),
            "execute should fall back to last_file from instrument, got: {:?}",
            responses[1].message,
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn execute_without_file_and_no_prior_context_still_errors() {
        // With no prior analyze/instrument, execute without file should still error
        let resp = send_recv(
            r#"{"protocol_version":"0.1.0","id":1,"command":"execute","function":"f","inputs":[],"mocks":[]}"#,
        );
        assert_eq!(resp.status, "error");
        assert_eq!(resp.code.as_deref(), Some(ERR_INVALID_REQUEST));
        assert!(resp.message.as_deref().unwrap_or("").contains("file"));
    }

    // -- Exec timeout tests --
    // SAFETY: env var mutations in tests are inherently racy but acceptable for unit tests.
    // These tests exercise exec_timeout_from_env() which only reads SHATTER_EXEC_TIMEOUT.

    #[test]
    fn exec_timeout_default_is_5000ms() {
        let _lock = crate::ENV_LOCK.lock().unwrap();
        unsafe { std::env::remove_var("SHATTER_EXEC_TIMEOUT") };
        assert_eq!(exec_timeout_from_env(), DEFAULT_EXEC_TIMEOUT_MS);
    }

    #[test]
    fn exec_timeout_reads_env_var() {
        let _lock = crate::ENV_LOCK.lock().unwrap();
        unsafe { std::env::set_var("SHATTER_EXEC_TIMEOUT", "3") };
        let result = exec_timeout_from_env();
        unsafe { std::env::remove_var("SHATTER_EXEC_TIMEOUT") };
        assert_eq!(result, 3000);
    }

    #[test]
    fn exec_timeout_reads_fractional_seconds() {
        let _lock = crate::ENV_LOCK.lock().unwrap();
        unsafe { std::env::set_var("SHATTER_EXEC_TIMEOUT", "2.5") };
        let result = exec_timeout_from_env();
        unsafe { std::env::remove_var("SHATTER_EXEC_TIMEOUT") };
        assert_eq!(result, 2500);
    }

    #[test]
    fn exec_timeout_ignores_invalid() {
        let _lock = crate::ENV_LOCK.lock().unwrap();
        unsafe { std::env::set_var("SHATTER_EXEC_TIMEOUT", "not-a-number") };
        let result = exec_timeout_from_env();
        unsafe { std::env::remove_var("SHATTER_EXEC_TIMEOUT") };
        assert_eq!(result, DEFAULT_EXEC_TIMEOUT_MS);
    }

    #[test]
    fn exec_timeout_ignores_zero() {
        let _lock = crate::ENV_LOCK.lock().unwrap();
        unsafe { std::env::set_var("SHATTER_EXEC_TIMEOUT", "0") };
        let result = exec_timeout_from_env();
        unsafe { std::env::remove_var("SHATTER_EXEC_TIMEOUT") };
        assert_eq!(result, DEFAULT_EXEC_TIMEOUT_MS);
    }

    #[test]
    fn harness_cache_from_env_reads_var() {
        let _lock = crate::ENV_LOCK.lock().unwrap();
        unsafe { std::env::set_var("SHATTER_HARNESS_CACHE", "/tmp/cache") };
        let result = harness_cache_from_env();
        unsafe { std::env::remove_var("SHATTER_HARNESS_CACHE") };
        assert_eq!(result, Some("/tmp/cache".to_string()));
    }

    #[test]
    fn harness_cache_from_env_empty_returns_none() {
        let _lock = crate::ENV_LOCK.lock().unwrap();
        unsafe { std::env::set_var("SHATTER_HARNESS_CACHE", "") };
        let result = harness_cache_from_env();
        unsafe { std::env::remove_var("SHATTER_HARNESS_CACHE") };
        assert_eq!(result, None);
    }

    #[test]
    fn harness_scratch_from_env_reads_var() {
        let _lock = crate::ENV_LOCK.lock().unwrap();
        unsafe { std::env::set_var("SHATTER_HARNESS_SCRATCH", "/tmp/scratch") };
        let result = harness_scratch_from_env();
        unsafe { std::env::remove_var("SHATTER_HARNESS_SCRATCH") };
        assert_eq!(result, Some("/tmp/scratch".to_string()));
    }

    #[test]
    fn harness_scratch_from_env_unset_returns_none() {
        let _lock = crate::ENV_LOCK.lock().unwrap();
        unsafe { std::env::remove_var("SHATTER_HARNESS_SCRATCH") };
        let result = harness_scratch_from_env();
        assert_eq!(result, None);
    }

    #[test]
    fn error_code_parity_with_registry() {
        use crate::protocol::ALL_ERROR_CODES;
        assert_eq!(
            ALL_ERROR_CODES.len(),
            12,
            "ALL_ERROR_CODES must have 12 entries matching registry.yaml"
        );
        // Each code must be a non-empty snake_case string.
        for code in ALL_ERROR_CODES {
            assert!(!code.is_empty(), "error code must not be empty");
            assert!(
                code.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                "error code {code:?} must be snake_case"
            );
        }
    }

    #[test]
    fn prune_orphans_removes_stale_standalone_entries() {
        let manager = PersistentHarnessManager::new();
        let harness_dir = tempfile::tempdir().unwrap();
        let harness_dir_path = harness_dir.path().to_path_buf();

        let key = crate::executor::HarnessKey::new_test("/tmp/nonexistent-source.rs", "my_func");
        let harness = crate::executor::PersistentHarness::new_dummy(harness_dir_path.clone());
        manager.cache.lock().unwrap().insert(key, harness);

        let pruned = manager.prune_orphans();
        assert_eq!(pruned, 1, "should prune 1 entry with nonexistent source");
        assert!(
            manager.cache.lock().unwrap().is_empty(),
            "cache should be empty after pruning"
        );
    }

    #[test]
    fn prune_orphans_keeps_valid_entries() {
        let manager = PersistentHarnessManager::new();
        let harness_dir = tempfile::tempdir().unwrap();

        // Use the test's own Cargo.toml as an existing file.
        let existing_file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("Cargo.toml")
            .to_string_lossy()
            .to_string();

        let key = crate::executor::HarnessKey::new_test(&existing_file, "my_func");
        let harness =
            crate::executor::PersistentHarness::new_dummy(harness_dir.path().to_path_buf());
        manager.cache.lock().unwrap().insert(key, harness);

        let pruned = manager.prune_orphans();
        assert_eq!(pruned, 0, "should not prune entries with existing source");
        assert_eq!(manager.cache.lock().unwrap().len(), 1);
    }

    #[test]
    fn prune_orphans_is_idempotent() {
        let manager = PersistentHarnessManager::new();
        let harness_dir = tempfile::tempdir().unwrap();

        let key = crate::executor::HarnessKey::new_test("/tmp/gone.rs", "foo");
        let harness =
            crate::executor::PersistentHarness::new_dummy(harness_dir.path().to_path_buf());
        manager.cache.lock().unwrap().insert(key, harness);

        let first = manager.prune_orphans();
        let second = manager.prune_orphans();
        assert_eq!(first, 1);
        assert_eq!(second, 0, "second prune should find nothing");
    }

    #[test]
    fn prune_missing_artifacts_removes_deleted_dirs() {
        let manager = PersistentHarnessManager::new();
        let harness_dir = tempfile::tempdir().unwrap();
        let harness_dir_path = harness_dir.path().to_path_buf();

        // Use an existing file as source so prune_orphans wouldn't remove it.
        let existing_file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("Cargo.toml")
            .to_string_lossy()
            .to_string();

        let key = crate::executor::HarnessKey::new_test(&existing_file, "my_func");
        let harness = crate::executor::PersistentHarness::new_dummy(harness_dir_path.clone());
        manager.cache.lock().unwrap().insert(key, harness);

        // Delete the harness dir to simulate external cleanup.
        drop(harness_dir);
        std::fs::remove_dir_all(&harness_dir_path).ok();

        let pruned = manager.prune_missing_artifacts();
        assert_eq!(pruned, 1, "should prune 1 entry with missing artifacts");
        assert!(manager.cache.lock().unwrap().is_empty());
    }

    #[test]
    fn shutdown_after_source_deleted_returns_ack() {
        // Create a temp file, reference it in a request, then delete it before shutdown.
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let source_path = tmp.path().to_string_lossy().to_string();
        std::fs::write(&source_path, "fn example() {}").unwrap();

        let analyze = format!(
            r#"{{"protocol_version":"0.1.0","id":1,"command":"analyze","file":"{source_path}"}}"#
        );
        let shutdown = r#"{"protocol_version":"0.1.0","id":2,"command":"shutdown"}"#;

        // Delete the source before shutdown.
        std::fs::remove_file(&source_path).ok();

        let responses = conversation(&[&analyze, shutdown]);
        // The analyze may fail (file deleted between request crafting and handling) — that's fine.
        // The shutdown must always succeed.
        let last = responses.last().expect("at least one response");
        assert_eq!(
            last.status, "shutdown_ack",
            "shutdown should succeed even after source deletion"
        );
    }
}
