use std::io::{self, BufRead, BufReader};

use crate::protocol::{
    self, Request, Response, FRONTEND_LANGUAGE, FRONTEND_VERSION, PROTOCOL_VERSION,
};

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

/// Processes protocol requests from stdin and writes responses to stdout.
pub struct Handler<R, W, L> {
    reader: BufReader<R>,
    writer: W,
    log: L,
    log_level: FrontendLogLevel,
}

impl<R: io::Read, W: io::Write, L: io::Write> Handler<R, W, L> {
    pub fn new(reader: R, writer: W, log: L) -> Self {
        Self {
            reader: BufReader::new(reader),
            writer,
            log,
            log_level: FrontendLogLevel::from_env(),
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

    fn dispatch(&self, req: &Request) -> (Response, bool) {
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
            "instrument".to_string(),
        ]);
        resp
    }

    fn handle_analyze(&self, mut resp: Response, req: &Request) -> Response {
        if req.file.is_none() {
            resp.status = "error".to_string();
            resp.code = Some("invalid_request".to_string());
            resp.message = Some("analyze command requires a file path".to_string());
            return resp;
        }

        resp.status = "error".to_string();
        resp.code = Some("internal_error".to_string());
        resp.message = Some("analyze command not yet implemented".to_string());
        resp
    }

    fn handle_instrument(&self, mut resp: Response, req: &Request) -> Response {
        if req.file.is_none() {
            resp.status = "error".to_string();
            resp.code = Some("invalid_request".to_string());
            resp.message = Some("instrument command requires a file path".to_string());
            return resp;
        }

        resp.status = "error".to_string();
        resp.code = Some("internal_error".to_string());
        resp.message = Some("instrument command not yet implemented".to_string());
        resp
    }

    fn handle_execute(&self, mut resp: Response) -> Response {
        resp.status = "error".to_string();
        resp.code = Some("internal_error".to_string());
        resp.message = Some("execute command not yet implemented".to_string());
        resp
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
        assert_eq!(resp.frontend_version.as_deref(), Some("0.1.0"));
    }

    #[test]
    fn handshake_returns_all_capabilities() {
        let resp = send_recv(
            r#"{"protocol_version":"0.1.0","id":1,"command":"handshake","capabilities":["analyze"]}"#,
        );
        let caps = resp.capabilities.expect("capabilities present");
        assert!(caps.contains(&"analyze".to_string()));
        assert!(caps.contains(&"execute".to_string()));
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
        assert_eq!(resp.protocol_version, "0.1.0");
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
    fn analyze_with_file_returns_not_implemented() {
        let resp = send_recv(
            r#"{"protocol_version":"0.1.0","id":2,"command":"analyze","file":"test.rs"}"#,
        );
        assert_eq!(resp.status, "error");
        assert_eq!(resp.code.as_deref(), Some("internal_error"));
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
    fn instrument_with_file_returns_not_implemented() {
        let resp = send_recv(
            r#"{"protocol_version":"0.1.0","id":3,"command":"instrument","file":"test.rs"}"#,
        );
        assert_eq!(resp.status, "error");
        assert_eq!(resp.code.as_deref(), Some("internal_error"));
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
}
