use std::io::{self, BufRead, BufReader};

use crate::generators::NativeRegistry;
use crate::protocol::{
    self, ERR_COMPILATION_ERROR, ERR_FILE_NOT_FOUND, ERR_FUNCTION_NOT_FOUND, ERR_INTERNAL_ERROR,
    ERR_INVALID_REQUEST, ERR_NOT_SUPPORTED, ERR_PARSE_ERROR, ERR_VERSION_MISMATCH,
    FRONTEND_LANGUAGE, FRONTEND_VERSION, PROTOCOL_VERSION, Request, Response,
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
#[cfg(test)]
fn harness_cache_from_env() -> Option<String> {
    std::env::var("SHATTER_HARNESS_CACHE")
        .ok()
        .filter(|s| !s.is_empty())
}

/// Read the harness scratch directory from `SHATTER_HARNESS_SCRATCH` env var.
/// Returns `None` if unset or empty.
#[cfg(test)]
fn harness_scratch_from_env() -> Option<String> {
    std::env::var("SHATTER_HARNESS_SCRATCH")
        .ok()
        .filter(|s| !s.is_empty())
}

/// Write instrumented source to a temp directory and return the output path.
fn write_instrumented_temp(filename: &str, source: &str) -> io::Result<String> {
    let dir = std::env::temp_dir().join(format!("shatter-instrument-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let out_path = dir.join(filename);
    std::fs::write(&out_path, source)?;
    Ok(out_path.to_string_lossy().into_owned())
}

/// Processes protocol requests from stdin and writes responses to stdout.
/// Lifecycle manager for persistent harness subprocesses.
///
/// Keeps compiled harness processes alive across multiple execute calls so they
/// can be reused without recompilation. Currently a skeleton — actual harness
/// compilation and execution are implemented once the Rust execute handler is
/// complete. The struct is wired into `Handler` and its `close_all()` method is
/// called on shutdown so resources are released correctly when that work lands.
pub struct PersistentHarnessManager {
    /// Placeholder for the process map (source_path + func_name → subprocess).
    /// Will hold `HashMap<HarnessKey, ChildProcess>` once execute is implemented.
    _placeholder: (),
}

impl PersistentHarnessManager {
    pub fn new() -> Self {
        Self { _placeholder: () }
    }

    /// Terminates all cached harness subprocesses and frees their resources.
    /// Called from the shutdown handler to ensure clean process teardown.
    pub fn close_all(&mut self) {
        // No-op until execute is implemented and harness processes are spawned.
    }
}

impl Default for PersistentHarnessManager {
    fn default() -> Self {
        Self::new()
    }
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
            "setup".to_string(),
            "teardown".to_string(),
        ]);
        resp
    }

    fn maybe_timing_collector(&self) -> Option<TimingCollector> {
        self.timing_enabled.then(TimingCollector::default)
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
                crate::analyzer::analyze_file_with_timing(
                    path,
                    req.function.as_deref(),
                    Some(timing),
                )
            })
        } else {
            crate::analyzer::analyze_file(path, req.function.as_deref())
        };

        match analysis {
            Ok(functions) => {
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

    fn handle_execute(&self, mut resp: Response, req: &Request) -> Response {
        let mut timing = self.maybe_timing_collector();
        let file_path = match req.file.as_ref().or(self.last_file.as_ref()) {
            Some(f) => f,
            None => {
                resp.status = "error".to_string();
                resp.code = Some(ERR_INVALID_REQUEST.to_string());
                resp.message = Some("execute command requires a file path (none provided and no prior analyze/instrument)".to_string());
                return resp;
            }
        };
        let function_name = match &req.function {
            Some(f) => f,
            None => {
                resp.status = "error".to_string();
                resp.code = Some(ERR_INVALID_REQUEST.to_string());
                resp.message = Some("execute command requires a function name".to_string());
                return resp;
            }
        };

        if self.log_level >= FrontendLogLevel::Debug {
            eprintln!(
                "[shatter-rust] Executing {function_name} in {file_path} with {} inputs",
                req.inputs.len()
            );
        }

        let execution = if let Some(timing) = timing.as_mut() {
            timing.record("execute.total", |timing| {
                crate::executor::execute_function_with_timing(
                    file_path,
                    function_name,
                    &req.inputs,
                    &req.mocks,
                    self.exec_timeout_ms,
                    Some(timing),
                )
            })
        } else {
            crate::executor::execute_function(
                file_path,
                function_name,
                &req.inputs,
                &req.mocks,
                self.exec_timeout_ms,
            )
        };

        match execution {
            Ok(result) => {
                resp.status = "execute".to_string();
                resp.return_value = result.return_value;
                resp.thrown_error = result.thrown_error;
                resp.branch_path = Some(result.branch_path);
                resp.lines_executed = Some(result.lines_executed);
                resp.calls_to_external = Some(result.calls_to_external);
                resp.path_constraints = Some(result.path_constraints);
                resp.side_effects = Some(result.side_effects);
                resp.performance = Some(result.performance);
                self.finalize_response(resp, timing.as_mut())
            }
            Err(crate::executor::ExecuteError::FileError(msg)) => {
                resp.status = "error".to_string();
                resp.code = Some(ERR_FILE_NOT_FOUND.to_string());
                resp.message = Some(msg);
                self.finalize_response(resp, timing.as_mut())
            }
            Err(crate::executor::ExecuteError::CompilationFailed(msg)) => {
                resp.status = "error".to_string();
                resp.code = Some(ERR_COMPILATION_ERROR.to_string());
                resp.message = Some(msg);
                self.finalize_response(resp, timing.as_mut())
            }
            Err(crate::executor::ExecuteError::NonExecutable(msg)) => {
                resp.status = "error".to_string();
                resp.code = Some(ERR_NOT_SUPPORTED.to_string());
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

    fn handle_setup(&self, mut resp: Response, req: &Request) -> Response {
        let mut timing = self.maybe_timing_collector();
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

    fn handle_teardown(&self, mut resp: Response, req: &Request) -> Response {
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
                    match registry.generate(func_name, req.recipe.clone()) {
                        Ok((value, generator_id, recipe)) => {
                            resp.status = "generate".to_string();
                            resp.value = Some(value);
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
            caps.contains(&"setup".to_string()),
            "setup must be advertised"
        );
        assert!(
            caps.contains(&"teardown".to_string()),
            "teardown must be advertised"
        );
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
        assert_eq!(responses[1].status, "execute");
        let phases = timing_phase_names(&responses[1]);
        for expected in [
            "execute.total",
            "execute.read_source",
            "execute.extract_signature",
            "execute.instrument",
            "execute.build",
            "execute.run",
            "execute.parse_result",
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
        unsafe { std::env::remove_var("SHATTER_EXEC_TIMEOUT") };
        assert_eq!(exec_timeout_from_env(), DEFAULT_EXEC_TIMEOUT_MS);
    }

    #[test]
    fn exec_timeout_reads_env_var() {
        unsafe { std::env::set_var("SHATTER_EXEC_TIMEOUT", "3") };
        let result = exec_timeout_from_env();
        unsafe { std::env::remove_var("SHATTER_EXEC_TIMEOUT") };
        assert_eq!(result, 3000);
    }

    #[test]
    fn exec_timeout_reads_fractional_seconds() {
        unsafe { std::env::set_var("SHATTER_EXEC_TIMEOUT", "2.5") };
        let result = exec_timeout_from_env();
        unsafe { std::env::remove_var("SHATTER_EXEC_TIMEOUT") };
        assert_eq!(result, 2500);
    }

    #[test]
    fn exec_timeout_ignores_invalid() {
        unsafe { std::env::set_var("SHATTER_EXEC_TIMEOUT", "not-a-number") };
        let result = exec_timeout_from_env();
        unsafe { std::env::remove_var("SHATTER_EXEC_TIMEOUT") };
        assert_eq!(result, DEFAULT_EXEC_TIMEOUT_MS);
    }

    #[test]
    fn exec_timeout_ignores_zero() {
        unsafe { std::env::set_var("SHATTER_EXEC_TIMEOUT", "0") };
        let result = exec_timeout_from_env();
        unsafe { std::env::remove_var("SHATTER_EXEC_TIMEOUT") };
        assert_eq!(result, DEFAULT_EXEC_TIMEOUT_MS);
    }

    #[test]
    fn harness_cache_from_env_reads_var() {
        unsafe { std::env::set_var("SHATTER_HARNESS_CACHE", "/tmp/cache") };
        let result = harness_cache_from_env();
        unsafe { std::env::remove_var("SHATTER_HARNESS_CACHE") };
        assert_eq!(result, Some("/tmp/cache".to_string()));
    }

    #[test]
    fn harness_cache_from_env_empty_returns_none() {
        unsafe { std::env::set_var("SHATTER_HARNESS_CACHE", "") };
        let result = harness_cache_from_env();
        unsafe { std::env::remove_var("SHATTER_HARNESS_CACHE") };
        assert_eq!(result, None);
    }

    #[test]
    fn harness_scratch_from_env_reads_var() {
        unsafe { std::env::set_var("SHATTER_HARNESS_SCRATCH", "/tmp/scratch") };
        let result = harness_scratch_from_env();
        unsafe { std::env::remove_var("SHATTER_HARNESS_SCRATCH") };
        assert_eq!(result, Some("/tmp/scratch".to_string()));
    }

    #[test]
    fn harness_scratch_from_env_unset_returns_none() {
        unsafe { std::env::remove_var("SHATTER_HARNESS_SCRATCH") };
        let result = harness_scratch_from_env();
        assert_eq!(result, None);
    }

    #[test]
    fn error_code_parity_with_registry() {
        use crate::protocol::ALL_ERROR_CODES;
        assert_eq!(
            ALL_ERROR_CODES.len(),
            11,
            "ALL_ERROR_CODES must have 11 entries matching registry.yaml"
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
}
