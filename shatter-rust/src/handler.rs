use std::io::{self, BufRead, BufReader};

use crate::generators::NativeRegistry;
use crate::protocol::{
    self, Request, Response, FRONTEND_LANGUAGE, FRONTEND_VERSION, PROTOCOL_VERSION,
};
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

/// Write instrumented source to a temp directory and return the output path.
fn write_instrumented_temp(filename: &str, source: &str) -> io::Result<String> {
    let dir = std::env::temp_dir().join(format!(
        "shatter-instrument-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir)?;
    let out_path = dir.join(filename);
    std::fs::write(&out_path, source)?;
    Ok(out_path.to_string_lossy().into_owned())
}

/// Processes protocol requests from stdin and writes responses to stdout.
pub struct Handler<R, W, L> {
    reader: BufReader<R>,
    writer: W,
    log: L,
    log_level: FrontendLogLevel,
    #[allow(dead_code)] // will be used when execute is implemented
    exec_timeout_ms: u64,
    wasm_cache: WasmCache,
    native_registry: Option<NativeRegistry>,
}

impl<R: io::Read, W: io::Write, L: io::Write> Handler<R, W, L> {
    pub fn new(reader: R, writer: W, log: L) -> Self {
        Self {
            reader: BufReader::new(reader),
            writer,
            log,
            log_level: FrontendLogLevel::from_env(),
            exec_timeout_ms: exec_timeout_from_env(),
            wasm_cache: WasmCache::new(),
            native_registry: None,
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
            wasm_cache: WasmCache::new(),
            native_registry: Some(registry),
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
            wasm_cache: WasmCache::new(),
            native_registry: None,
        }
    }

    /// Process requests until shutdown or EOF. Returns Ok(()) on clean shutdown.
    pub fn run(mut self) -> io::Result<()> {
        self.log_at(FrontendLogLevel::Debug, &format!(
            "Starting Rust frontend (protocol {PROTOCOL_VERSION})"
        ));

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

        if req.protocol_version != protocol::PROTOCOL_VERSION {
            resp.status = "error".to_string();
            resp.code = Some("version_mismatch".to_string());
            resp.message = Some(format!(
                "unsupported protocol version {:?}, expected {:?}",
                req.protocol_version,
                protocol::PROTOCOL_VERSION,
            ));
            return (resp, false);
        }

        match req.command.as_str() {
            "handshake" => (self.handle_handshake(resp), false),
            "analyze" => (self.handle_analyze(resp, req), false),
            "instrument" => (self.handle_instrument(resp, req), false),
            "execute" => (self.handle_execute(resp), false),
            "setup" => (self.handle_setup(resp, req), false),
            "teardown" => (self.handle_teardown(resp), false),
            "generate" => (self.handle_generate(resp, req), false),
            "shutdown" => (self.handle_shutdown(resp), true),
            _ => {
                resp.status = "error".to_string();
                resp.code = Some("invalid_request".to_string());
                resp.message = Some(format!("unknown command: {}", req.command));
                (resp, false)
            }
        }
    }

    fn handle_handshake(&self, mut resp: Response) -> Response {
        resp.status = "handshake".to_string();
        resp.frontend_version = Some(FRONTEND_VERSION.to_string());
        resp.language = Some(FRONTEND_LANGUAGE.to_string());
        resp.capabilities = Some(vec![
            "analyze".to_string(),
            "execute".to_string(),
            "generate".to_string(),
            "instrument".to_string(),
        ]);
        resp
    }

    fn handle_analyze(&self, mut resp: Response, req: &Request) -> Response {
        let file_path = match &req.file {
            Some(f) => f,
            None => {
                resp.status = "error".to_string();
                resp.code = Some("invalid_request".to_string());
                resp.message = Some("analyze command requires a file path".to_string());
                return resp;
            }
        };

        let path = std::path::Path::new(file_path);
        if !path.exists() {
            resp.status = "error".to_string();
            resp.code = Some("file_not_found".to_string());
            resp.message = Some(format!("file not found: {file_path}"));
            return resp;
        }

        match crate::analyzer::analyze_file(path, req.function.as_deref()) {
            Ok(functions) => {
                resp.status = "analyze".to_string();
                resp.functions = Some(functions);
                resp
            }
            Err(e) => {
                resp.status = "error".to_string();
                resp.code = Some(
                    match &e {
                        crate::analyzer::AnalyzeError::FileNotFound(_) => "file_not_found",
                        crate::analyzer::AnalyzeError::ReadError(_) => "internal_error",
                        crate::analyzer::AnalyzeError::ParseError(_) => "parse_error",
                        crate::analyzer::AnalyzeError::FunctionNotFound(_) => "function_not_found",
                    }
                    .to_string(),
                );
                resp.message = Some(e.to_string());
                resp
            }
        }
    }

    fn handle_instrument(&self, mut resp: Response, req: &Request) -> Response {
        let file_path = match &req.file {
            Some(f) => f,
            None => {
                resp.status = "error".to_string();
                resp.code = Some("invalid_request".to_string());
                resp.message = Some("instrument command requires a file path".to_string());
                return resp;
            }
        };

        let path = std::path::Path::new(file_path);
        if !path.exists() {
            resp.status = "error".to_string();
            resp.code = Some("file_not_found".to_string());
            resp.message = Some(format!("file not found: {file_path}"));
            return resp;
        }

        match crate::instrument::instrument_file(path, req.function.as_deref()) {
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
                        resp
                    }
                    Err(e) => {
                        resp.status = "error".to_string();
                        resp.code = Some("internal_error".to_string());
                        resp.message = Some(format!("failed to write instrumented output: {e}"));
                        resp
                    }
                }
            }
            Err(e) => {
                resp.status = "error".to_string();
                resp.code = Some(match &e {
                    crate::instrument::InstrumentError::FileNotFound(_) => "file_not_found",
                    crate::instrument::InstrumentError::ReadError(_) => "internal_error",
                    crate::instrument::InstrumentError::ParseError(_) => "parse_error",
                }.to_string());
                resp.message = Some(e.to_string());
                resp
            }
        }
    }

    fn handle_execute(&self, mut resp: Response) -> Response {
        resp.status = "error".to_string();
        resp.code = Some("internal_error".to_string());
        resp.message = Some("execute command not yet implemented".to_string());
        resp
    }

    fn handle_setup(&self, mut resp: Response, req: &Request) -> Response {
        if req.file.is_none() {
            resp.status = "error".to_string();
            resp.code = Some("invalid_request".to_string());
            resp.message = Some("setup command requires a file path".to_string());
            return resp;
        }
        if req.function.is_none() {
            resp.status = "error".to_string();
            resp.code = Some("invalid_request".to_string());
            resp.message = Some("setup command requires a function name".to_string());
            return resp;
        }

        resp.status = "error".to_string();
        resp.code = Some("internal_error".to_string());
        resp.message = Some("setup command not yet implemented".to_string());
        resp
    }

    fn handle_teardown(&self, mut resp: Response) -> Response {
        resp.status = "error".to_string();
        resp.code = Some("internal_error".to_string());
        resp.message = Some("teardown command not yet implemented".to_string());
        resp
    }

    fn handle_generate(&mut self, mut resp: Response, req: &Request) -> Response {
        let file_path = match &req.file {
            Some(f) => f,
            None => {
                resp.status = "error".to_string();
                resp.code = Some("invalid_request".to_string());
                resp.message = Some("generate command requires a file path".to_string());
                return resp;
            }
        };
        let func_name = match &req.name {
            Some(n) => n.clone(),
            None => {
                resp.status = "error".to_string();
                resp.code = Some("invalid_request".to_string());
                resp.message = Some("generate command requires a name".to_string());
                return resp;
            }
        };

        let path = std::path::Path::new(file_path);
        let ext = path.extension().and_then(|e| e.to_str());

        match ext {
            Some("wasm") => {
                match self.wasm_cache.generate(path, &func_name, req.recipe.as_ref()) {
                    Ok((value, generator_id, recipe)) => {
                        resp.status = "generate".to_string();
                        resp.value = Some(value);
                        resp.generator_id = Some(generator_id);
                        resp.recipe = recipe;
                        resp
                    }
                    Err(e) => {
                        resp.status = "error".to_string();
                        resp.code = Some("internal_error".to_string());
                        resp.message = Some(e);
                        resp
                    }
                }
            }
            Some("rs") => {
                if let Some(ref registry) = self.native_registry {
                    match registry.generate(&func_name, req.recipe.clone()) {
                        Ok((value, generator_id, recipe)) => {
                            resp.status = "generate".to_string();
                            resp.value = Some(value);
                            resp.generator_id = Some(generator_id);
                            resp.recipe = Some(recipe);
                            resp
                        }
                        Err(e) => {
                            resp.status = "error".to_string();
                            resp.code = Some("internal_error".to_string());
                            resp.message = Some(e);
                            resp
                        }
                    }
                } else {
                    resp.status = "error".to_string();
                    resp.code = Some("internal_error".to_string());
                    resp.message = Some(format!(
                        "native generator {func_name:?} requires a custom build (run `shatter build-frontend rust`)"
                    ));
                    resp
                }
            }
            _ => {
                resp.status = "error".to_string();
                resp.code = Some("invalid_request".to_string());
                resp.message = Some(format!(
                    "unsupported generator file type: {}",
                    file_path
                ));
                resp
            }
        }
    }

    fn handle_shutdown(&self, mut resp: Response) -> Response {
        resp.status = "shutdown_ack".to_string();
        resp
    }

    fn send(&mut self, resp: &Response) -> io::Result<()> {
        let data = serde_json::to_string(resp)
            .map_err(io::Error::other)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Send a single request and read the response.
    fn send_recv(req_json: &str) -> Response {
        let input = format!("{req_json}\n");
        let mut output = Vec::new();
        let handler = Handler::new(
            input.as_bytes(),
            &mut output,
            io::sink(),
        );
        handler.run().expect("handler.run");
        let output_str = String::from_utf8(output).expect("valid utf8");
        let first_line = output_str.lines().next().expect("at least one response line");
        serde_json::from_str(first_line).expect("valid JSON response")
    }

    /// Send multiple requests and return all responses.
    fn conversation(requests: &[&str]) -> Vec<Response> {
        let input = requests.join("\n") + "\n";
        let mut output = Vec::new();
        let handler = Handler::new(
            input.as_bytes(),
            &mut output,
            io::sink(),
        );
        handler.run().expect("handler.run");
        let output_str = String::from_utf8(output).expect("valid utf8");
        output_str
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| serde_json::from_str(l).expect("valid JSON response"))
            .collect()
    }

    #[test]
    fn handshake_returns_rust_language() {
        let resp = send_recv(
            r#"{"protocol_version":"0.1.0","id":1,"command":"handshake","capabilities":["analyze"]}"#,
        );
        assert_eq!(resp.status, "handshake");
        assert_eq!(resp.language.as_deref(), Some("rust"));
        assert_eq!(resp.frontend_version.as_deref(), Some(crate::protocol::FRONTEND_VERSION));
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
        let resp = send_recv(
            r#"{"protocol_version":"0.1.0","id":5,"command":"shutdown"}"#,
        );
        assert_eq!(resp.status, "shutdown_ack");
        assert_eq!(resp.id, 5);
    }

    #[test]
    fn version_mismatch_returns_error() {
        let resp = send_recv(
            r#"{"protocol_version":"99.0.0","id":1,"command":"handshake","capabilities":[]}"#,
        );
        assert_eq!(resp.status, "error");
        assert_eq!(resp.code.as_deref(), Some("version_mismatch"));
    }

    #[test]
    fn unknown_command_returns_error() {
        let resp = send_recv(
            r#"{"protocol_version":"0.1.0","id":1,"command":"foobar"}"#,
        );
        assert_eq!(resp.status, "error");
        assert_eq!(resp.code.as_deref(), Some("invalid_request"));
    }

    #[test]
    fn analyze_without_file_returns_error() {
        let resp = send_recv(
            r#"{"protocol_version":"0.1.0","id":2,"command":"analyze"}"#,
        );
        assert_eq!(resp.status, "error");
        assert_eq!(resp.code.as_deref(), Some("invalid_request"));
    }

    #[test]
    fn analyze_with_nonexistent_file_returns_file_not_found() {
        let resp = send_recv(
            r#"{"protocol_version":"0.1.0","id":2,"command":"analyze","file":"nonexistent.rs"}"#,
        );
        assert_eq!(resp.status, "error");
        assert_eq!(resp.code.as_deref(), Some("file_not_found"));
    }

    #[test]
    fn instrument_without_file_returns_error() {
        let resp = send_recv(
            r#"{"protocol_version":"0.1.0","id":3,"command":"instrument"}"#,
        );
        assert_eq!(resp.status, "error");
        assert_eq!(resp.code.as_deref(), Some("invalid_request"));
    }

    #[test]
    fn instrument_with_nonexistent_file_returns_file_not_found() {
        let resp = send_recv(
            r#"{"protocol_version":"0.1.0","id":3,"command":"instrument","file":"nonexistent.rs"}"#,
        );
        assert_eq!(resp.status, "error");
        assert_eq!(resp.code.as_deref(), Some("file_not_found"));
    }

    #[test]
    fn instrument_with_valid_file_returns_success() {
        // Create a temp file with valid Rust source
        let dir = std::env::temp_dir().join("shatter-test-instrument");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("sample.rs");
        std::fs::write(&file, "fn foo(x: i32) -> bool { if x > 0 { true } else { false } }").unwrap();

        let file_path = file.to_string_lossy();
        let req = format!(
            r#"{{"protocol_version":"0.1.0","id":3,"command":"instrument","file":"{}"}}"#,
            file_path.replace('\\', "\\\\")
        );
        let resp = send_recv(&req);
        assert_eq!(resp.status, "instrument", "expected instrument status, got: {:?}", resp.message);
        assert_eq!(resp.instrumented, Some(true));
        assert!(resp.output_file.is_some());

        // Clean up
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn execute_returns_not_implemented() {
        let resp = send_recv(
            r#"{"protocol_version":"0.1.0","id":4,"command":"execute","function":"F","inputs":[],"mocks":[]}"#,
        );
        assert_eq!(resp.status, "error");
        assert_eq!(resp.code.as_deref(), Some("internal_error"));
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
        let handler = Handler::new(
            input.as_bytes(),
            &mut output,
            io::sink(),
        );
        handler.run().expect("handler.run");
        let output_str = String::from_utf8(output).expect("valid utf8");
        let lines: Vec<&str> = output_str.lines().filter(|l| !l.is_empty()).collect();
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn response_is_valid_ndjson() {
        let input = concat!(
            r#"{"protocol_version":"0.1.0","id":1,"command":"handshake","capabilities":[]}"#,
            "\n",
            r#"{"protocol_version":"0.1.0","id":2,"command":"shutdown"}"#,
            "\n",
        );
        let mut output = Vec::new();
        let handler = Handler::new(
            input.as_bytes(),
            &mut output,
            io::sink(),
        );
        handler.run().expect("handler.run");
        let output_str = String::from_utf8(output).expect("valid utf8");
        for line in output_str.lines() {
            if line.is_empty() {
                continue;
            }
            let parsed: Result<serde_json::Value, _> = serde_json::from_str(line);
            assert!(parsed.is_ok(), "not valid JSON: {line}");
        }
    }

    #[test]
    fn debug_output_goes_to_log() {
        let input = r#"{"protocol_version":"0.1.0","id":1,"command":"shutdown"}"#.to_string() + "\n";
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
        assert!(log_str.contains("[shatter-rust]"), "log missing prefix: {log_str}");
        assert!(log_str.contains("Shutting down"), "log missing shutdown message: {log_str}");
    }

    #[test]
    fn log_level_filtering_suppresses_trace_at_info() {
        let input = r#"{"protocol_version":"0.1.0","id":1,"command":"shutdown"}"#.to_string() + "\n";
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
        assert!(!log_str.contains("Received:"), "trace messages should be suppressed at info: {log_str}");
        assert!(!log_str.contains("Sent:"), "trace messages should be suppressed at info: {log_str}");
    }

    #[test]
    fn log_level_filtering_shows_debug_at_debug() {
        let input = r#"{"protocol_version":"0.1.0","id":1,"command":"shutdown"}"#.to_string() + "\n";
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
        assert!(log_str.contains("Starting Rust frontend"), "debug messages should appear at debug: {log_str}");
        assert!(log_str.contains("Shutting down"), "debug messages should appear at debug: {log_str}");
        assert!(!log_str.contains("Received:"), "trace messages should be suppressed at debug: {log_str}");
    }

    // -- Setup command tests --

    #[test]
    fn setup_without_file_returns_error() {
        let resp = send_recv(
            r#"{"protocol_version":"0.1.0","id":10,"command":"setup","function":"myFunc","mode":"per_function"}"#,
        );
        assert_eq!(resp.status, "error");
        assert_eq!(resp.code.as_deref(), Some("invalid_request"));
        assert!(resp.message.as_deref().unwrap_or("").contains("file"));
    }

    #[test]
    fn setup_without_function_returns_error() {
        let resp = send_recv(
            r#"{"protocol_version":"0.1.0","id":10,"command":"setup","file":"./setup.ts","mode":"per_function"}"#,
        );
        assert_eq!(resp.status, "error");
        assert_eq!(resp.code.as_deref(), Some("invalid_request"));
        assert!(resp.message.as_deref().unwrap_or("").contains("function"));
    }

    #[test]
    fn setup_with_valid_fields_returns_not_implemented() {
        let resp = send_recv(
            r#"{"protocol_version":"0.1.0","id":10,"command":"setup","file":"./setup.ts","function":"myFunc","mode":"per_function"}"#,
        );
        assert_eq!(resp.status, "error");
        assert_eq!(resp.code.as_deref(), Some("internal_error"));
        assert!(resp.message.as_deref().unwrap_or("").contains("not yet implemented"));
    }

    #[test]
    fn setup_per_execution_mode_returns_not_implemented() {
        let resp = send_recv(
            r#"{"protocol_version":"0.1.0","id":11,"command":"setup","file":"./setup.ts","function":"auth","mode":"per_execution"}"#,
        );
        assert_eq!(resp.status, "error");
        assert_eq!(resp.code.as_deref(), Some("internal_error"));
    }

    // -- Teardown command tests --

    #[test]
    fn teardown_returns_not_implemented() {
        let resp = send_recv(
            r#"{"protocol_version":"0.1.0","id":12,"command":"teardown","function":"myFunc"}"#,
        );
        assert_eq!(resp.status, "error");
        assert_eq!(resp.code.as_deref(), Some("internal_error"));
        assert!(resp.message.as_deref().unwrap_or("").contains("not yet implemented"));
    }

    // -- Generate command tests --

    #[test]
    fn generate_without_file_returns_error() {
        let resp = send_recv(
            r#"{"protocol_version":"0.1.0","id":13,"command":"generate","name":"User","kind":"type_name"}"#,
        );
        assert_eq!(resp.status, "error");
        assert_eq!(resp.code.as_deref(), Some("invalid_request"));
        assert!(resp.message.as_deref().unwrap_or("").contains("file"));
    }

    #[test]
    fn generate_without_name_returns_error() {
        let resp = send_recv(
            r#"{"protocol_version":"0.1.0","id":13,"command":"generate","file":"./gen.ts","kind":"type_name"}"#,
        );
        assert_eq!(resp.status, "error");
        assert_eq!(resp.code.as_deref(), Some("invalid_request"));
        assert!(resp.message.as_deref().unwrap_or("").contains("name"));
    }

    #[test]
    fn generate_non_wasm_file_returns_unsupported() {
        let resp = send_recv(
            r#"{"protocol_version":"0.1.0","id":14,"command":"generate","file":"./gen.ts","name":"User","kind":"type_name"}"#,
        );
        assert_eq!(resp.status, "error");
        assert_eq!(resp.code.as_deref(), Some("invalid_request"));
        assert!(resp.message.as_deref().unwrap_or("").contains("unsupported generator file type"));
    }

    #[test]
    fn generate_wasm_missing_file_returns_error() {
        let resp = send_recv(
            r#"{"protocol_version":"0.1.0","id":15,"command":"generate","file":"/nonexistent/gen.wasm","name":"gen","kind":"type_name"}"#,
        );
        assert_eq!(resp.status, "error");
        assert_eq!(resp.code.as_deref(), Some("internal_error"));
        assert!(resp.message.as_deref().unwrap_or("").contains("not found"));
    }

    // -- Integration: new commands in conversation --

    #[test]
    fn setup_teardown_generate_in_conversation() {
        let responses = conversation(&[
            r#"{"protocol_version":"0.1.0","id":1,"command":"handshake","capabilities":["analyze"]}"#,
            r#"{"protocol_version":"0.1.0","id":2,"command":"setup","file":"./setup.ts","function":"fn1","mode":"per_function"}"#,
            r#"{"protocol_version":"0.1.0","id":3,"command":"teardown","function":"fn1"}"#,
            r#"{"protocol_version":"0.1.0","id":4,"command":"generate","file":"./gen.ts","name":"User","kind":"type_name"}"#,
            r#"{"protocol_version":"0.1.0","id":5,"command":"shutdown"}"#,
        ]);
        assert_eq!(responses.len(), 5);
        assert_eq!(responses[0].status, "handshake");
        assert_eq!(responses[0].id, 1);
        // setup and teardown return "not yet implemented" errors
        for i in 1..=2 {
            assert_eq!(responses[i].status, "error");
            assert_eq!(responses[i].code.as_deref(), Some("internal_error"));
        }
        // generate with non-wasm file returns "unsupported generator file type"
        assert_eq!(responses[3].status, "error");
        assert_eq!(responses[3].code.as_deref(), Some("invalid_request"));
        assert_eq!(responses[4].status, "shutdown_ack");
        assert_eq!(responses[4].id, 5);
    }

    #[test]
    fn new_commands_echo_correct_request_ids() {
        let responses = conversation(&[
            r#"{"protocol_version":"0.1.0","id":100,"command":"setup","file":"./s.ts","function":"f","mode":"per_function"}"#,
            r#"{"protocol_version":"0.1.0","id":200,"command":"teardown","function":"f"}"#,
            r#"{"protocol_version":"0.1.0","id":300,"command":"generate","file":"./g.ts","name":"T","kind":"type_name"}"#,
        ]);
        assert_eq!(responses[0].id, 100);
        assert_eq!(responses[1].id, 200);
        assert_eq!(responses[2].id, 300);
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
    fn exec_timeout_ignores_negative() {
        unsafe { std::env::set_var("SHATTER_EXEC_TIMEOUT", "-5") };
        let result = exec_timeout_from_env();
        unsafe { std::env::remove_var("SHATTER_EXEC_TIMEOUT") };
        assert_eq!(result, DEFAULT_EXEC_TIMEOUT_MS);
    }
}
