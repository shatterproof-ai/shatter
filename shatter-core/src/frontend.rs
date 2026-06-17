//! Frontend subprocess manager for spawning and communicating with language frontends.
//!
//! The core engine communicates with frontends via newline-delimited JSON over stdin/stdout.
//! This module handles subprocess lifecycle (spawn, handshake, request/response, shutdown)
//! and provides a typed async API over the raw protocol.

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tracing::Instrument;

/// Maximum bytes of subprocess stderr retained for diagnostic surfacing
/// when a frontend exits before producing a response. 16 KiB is enough
/// to capture stack traces and module-init exceptions while bounding
/// memory if the subprocess floods stderr.
const STDERR_CAPTURE_LIMIT: usize = 16 * 1024;

/// Grace period to wait for the child to exit after stdout EOF so we
/// can collect its exit status for the diagnostic message.
const POST_EOF_WAIT: Duration = Duration::from_secs(2);

use crate::protocol::{
    Command as ProtoCommand, PROTOCOL_VERSION, Request, Response, ResponseResult,
};

/// Default timeout for individual frontend requests (30 seconds).
pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Grace period to wait for the frontend process to exit during shutdown
/// before force-killing it.
const SHUTDOWN_GRACE_PERIOD: Duration = Duration::from_secs(5);

/// Errors that can occur when communicating with a frontend subprocess.
#[derive(Debug, thiserror::Error)]
pub enum FrontendError {
    /// Failed to spawn the frontend process.
    #[error("failed to spawn frontend: {0}")]
    Spawn(std::io::Error),

    /// Failed to write a request to the frontend's stdin.
    #[error("failed to write to frontend stdin: {0}")]
    Write(std::io::Error),

    /// Failed to read a response from the frontend's stdout.
    #[error("failed to read from frontend stdout: {0}")]
    Read(std::io::Error),

    /// The frontend process closed its stdout before responding.
    ///
    /// Carries the captured stderr tail and exit status so callers can
    /// classify the failure (e.g., module-init throw vs. clean exit)
    /// rather than seeing only an opaque "closed stdout unexpectedly".
    #[error("frontend subprocess exited before responding: {binary} ({exit_status}){stderr_section}",
        binary = .binary.display(),
        exit_status = exit_status_display(.exit_status.as_ref()),
        stderr_section = stderr_section(.stderr_tail))]
    SubprocessExited {
        binary: PathBuf,
        exit_status: Option<std::process::ExitStatus>,
        stderr_tail: String,
    },

    /// Failed to serialize a request to JSON.
    #[error("failed to serialize request: {0}")]
    Serialize(serde_json::Error),

    /// Failed to deserialize a response from JSON.
    #[error(
        "failed to deserialize frontend response ({response_bytes} bytes): {source}\n  hint: {hint}"
    )]
    Deserialize {
        source: serde_json::Error,
        response_bytes: usize,
        hint: String,
    },

    /// The response ID did not match the request ID.
    #[error("response id {response_id} does not match request id {request_id}")]
    IdMismatch { request_id: u64, response_id: u64 },

    /// The frontend returned a protocol error response.
    #[error("frontend error ({code:?}): {message}")]
    Protocol {
        code: crate::protocol::ErrorCode,
        message: String,
        details: Option<serde_json::Value>,
    },

    /// A request timed out waiting for a response.
    #[error("request timed out after {0:?}")]
    Timeout(Duration),

    /// The frontend process exited unexpectedly.
    #[error("frontend process exited with status: {0}")]
    ProcessExited(std::process::ExitStatus),

    /// Protocol version mismatch during handshake.
    #[error("protocol version mismatch: core={core}, frontend={frontend}")]
    VersionMismatch { core: String, frontend: String },
}

/// Configuration for spawning a frontend subprocess.
#[derive(Debug, Clone)]
pub struct FrontendConfig {
    /// Path to the frontend binary or script.
    pub command: PathBuf,
    /// Arguments to pass to the frontend.
    pub args: Vec<String>,
    /// Timeout for individual requests.
    pub request_timeout: Duration,
    /// Capabilities to advertise during handshake.
    pub capabilities: Vec<String>,
    /// Environment variables to set on the subprocess.
    pub env_vars: Vec<(String, String)>,
}

impl FrontendConfig {
    /// Create a config for a frontend at the given path with default settings.
    pub fn new(command: PathBuf) -> Self {
        Self {
            command,
            args: Vec::new(),
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
            capabilities: vec!["analyze".into(), "execute".into(), "instrument".into()],
            env_vars: Vec::new(),
        }
    }
}

/// A running frontend subprocess.
///
/// Manages the lifecycle of a single frontend process and provides typed
/// request/response communication over the JSON-over-stdio protocol.
pub struct Frontend {
    child: Child,
    stdin: tokio::process::ChildStdin,
    reader: BufReader<tokio::process::ChildStdout>,
    next_id: u64,
    request_timeout: Duration,
    language: Option<String>,
    capabilities: Vec<String>,
    binary: PathBuf,
    stderr_buf: Arc<Mutex<Vec<u8>>>,
    stderr_task: Option<tokio::task::JoinHandle<()>>,
    /// Number of timed-out responses still pending in the subprocess stdout
    /// pipe. When a request times out, the subprocess may still be processing
    /// it and will eventually write a response. Before sending the next
    /// request, we must drain these stale responses to keep request/response
    /// ID pairing consistent.
    pending_drain: u64,
    /// Set to `true` after any request timeout.  Once tainted, all subsequent
    /// `send()` / `send_raw()` calls return `FrontendError::Timeout`
    /// immediately without touching the subprocess.  This prevents the
    /// request/response ID sequence from becoming permanently misaligned: once
    /// we miss a response the pipe is out-of-sync and no amount of draining
    /// can safely recover it while new work keeps arriving.  Callers that need
    /// to continue must drop this frontend and spawn a fresh one.
    tainted: bool,
}

impl Frontend {
    /// Spawn a frontend subprocess and perform the initial handshake.
    ///
    /// Returns a ready-to-use `Frontend` after verifying protocol compatibility.
    pub async fn spawn(config: &FrontendConfig) -> Result<Self, FrontendError> {
        let mut frontend = Self::launch(config)?;

        frontend
            .handshake(&config.capabilities)
            .instrument(tracing::info_span!("frontend.handshake"))
            .await?;

        Ok(frontend)
    }

    /// Launch the frontend subprocess and wire up its stdio, but do **not**
    /// perform the handshake.
    ///
    /// Split out from [`Frontend::spawn`] so the handshake can be driven
    /// separately. This is what lets a test deterministically reproduce a
    /// pre-handshake crash: launch the process, wait for it to exit, then
    /// handshake — guaranteeing the handshake write hits an already-closed
    /// pipe rather than racing the subprocess's exit.
    fn launch(config: &FrontendConfig) -> Result<Self, FrontendError> {
        let mut child = {
            let _spawn_span = tracing::info_span!("frontend.spawn").entered();
            let mut cmd = Command::new(&config.command);
            cmd.args(&config.args)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true);
            for (key, value) in &config.env_vars {
                cmd.env(key, value);
            }
            cmd.spawn().map_err(FrontendError::Spawn)?
        };

        let stdin = child.stdin.take().expect("stdin was piped");
        let stdout = child.stdout.take().expect("stdout was piped");
        let stderr = child.stderr.take().expect("stderr was piped");
        let reader = BufReader::new(stdout);

        let stderr_buf: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
        let stderr_task = spawn_stderr_capture(stderr, Arc::clone(&stderr_buf));

        Ok(Self {
            child,
            stdin,
            reader,
            next_id: 1,
            request_timeout: config.request_timeout,
            language: None,
            capabilities: Vec::new(),
            binary: config.command.clone(),
            stderr_buf,
            stderr_task: Some(stderr_task),
            pending_drain: 0,
            tainted: false,
        })
    }

    /// Perform the protocol handshake with the frontend.
    async fn handshake(&mut self, capabilities: &[String]) -> Result<(), FrontendError> {
        let response = self
            .send(ProtoCommand::Handshake {
                capabilities: capabilities.to_vec(),
            })
            .await?;

        match response.result {
            ResponseResult::Handshake {
                frontend_version,
                language,
                capabilities: frontend_caps,
            } => {
                let core_major_minor = major_minor(PROTOCOL_VERSION);
                let frontend_major_minor = major_minor(&frontend_version);
                if core_major_minor != frontend_major_minor {
                    return Err(FrontendError::VersionMismatch {
                        core: PROTOCOL_VERSION.to_string(),
                        frontend: frontend_version,
                    });
                }
                self.language = Some(language);
                self.capabilities = frontend_caps;
                Ok(())
            }
            ResponseResult::Error {
                code,
                message,
                details,
            } => Err(FrontendError::Protocol {
                code,
                message,
                details,
            }),
            other => Err(FrontendError::Protocol {
                code: crate::protocol::ErrorCode::InvalidRequest,
                message: format!("unexpected handshake response: {other:?}"),
                details: None,
            }),
        }
    }

    /// Send a command to the frontend and wait for the response.
    ///
    /// Automatically assigns a request ID and enforces the configured timeout.
    /// If previous requests timed out, their stale responses are drained from
    /// the pipe before reading the response for the current request — this
    /// prevents ID mismatch errors when the subprocess finishes processing a
    /// timed-out request after the caller has moved on.
    pub async fn send(&mut self, command: ProtoCommand) -> Result<Response, FrontendError> {
        self.send_with_timeout(command, self.request_timeout).await
    }

    /// Send a command with an explicit timeout for this request.
    ///
    /// This is used for request classes that need a stricter local deadline
    /// than the frontend's general request timeout. A timeout still taints the
    /// frontend because the response stream can no longer be trusted.
    pub async fn send_with_timeout(
        &mut self,
        command: ProtoCommand,
        request_timeout: Duration,
    ) -> Result<Response, FrontendError> {
        let span = request_span(&command);

        // A tainted frontend has a misaligned request/response pipe — refuse
        // further sends immediately so callers get a clean Timeout instead of
        // an IdMismatch from a stale buffered response.
        if self.tainted {
            return Err(FrontendError::Timeout(request_timeout));
        }

        let id = self.next_id;
        self.next_id += 1;

        let request = Request::new(id, command);
        let mut json = serde_json::to_string(&request).map_err(FrontendError::Serialize)?;
        json.push('\n');

        // Phase 1: drain stale responses from previously timed-out requests.
        // Each timed-out send() left one unread response in the pipe. We read
        // and discard them before writing this request, using a generous
        // per-response timeout (equal to request_timeout) since the subprocess
        // may still be processing.
        if self.pending_drain > 0 {
            self.drain_stale_responses().await?;
        }

        // Phase 2: send the new request and read its response.
        // Copy request_timeout before the async block to avoid holding a
        // shared borrow on `self` while the block captures a mutable borrow.
        let write_and_read = async {
            self.stdin
                .write_all(json.as_bytes())
                .await
                .map_err(FrontendError::Write)?;
            self.stdin.flush().await.map_err(FrontendError::Write)?;

            self.read_response(id).await
        };

        let outcome = tokio::time::timeout(request_timeout, write_and_read)
            .instrument(span)
            .await;

        match outcome {
            Ok(inner) => self.enrich_subprocess_exit(inner).await,
            Err(_) => {
                // Timeout fired. The request was likely written to stdin but
                // the response was not read. Mark the frontend tainted so
                // subsequent calls fail fast with Timeout rather than risking
                // an IdMismatch from the stale buffered response.
                self.tainted = true;
                self.pending_drain += 1;
                Err(FrontendError::Timeout(request_timeout))
            }
        }
    }

    /// Read and discard stale responses left by timed-out requests.
    ///
    /// Uses `request_timeout` as the deadline for each stale response. If a
    /// stale response doesn't arrive within that window the subprocess is
    /// considered stuck and we return a timeout error.
    async fn drain_stale_responses(&mut self) -> Result<(), FrontendError> {
        let count = self.pending_drain;
        for i in 0..count {
            let drain_one = async {
                let mut stale_line = String::new();
                let stale_bytes = self
                    .reader
                    .read_line(&mut stale_line)
                    .await
                    .map_err(FrontendError::Read)?;

                if stale_bytes == 0 {
                    return Err(FrontendError::SubprocessExited {
                        binary: self.binary.clone(),
                        exit_status: None,
                        stderr_tail: String::new(),
                    });
                }
                tracing::debug!(
                    drain_index = i,
                    total_drain = count,
                    line_len = stale_line.len(),
                    "drained stale response from timed-out request"
                );
                Ok(())
            };

            tokio::time::timeout(self.request_timeout, drain_one)
                .await
                .map_err(|_| FrontendError::Timeout(self.request_timeout))??;

            // Successfully drained one response — decrement so that if a
            // later drain or the main read times out, pending_drain is
            // accurate.
            self.pending_drain -= 1;
        }
        Ok(())
    }

    /// Read a single response line and validate its ID.
    async fn read_response(&mut self, expected_id: u64) -> Result<Response, FrontendError> {
        let mut line = String::new();
        let bytes_read = self
            .reader
            .read_line(&mut line)
            .await
            .map_err(FrontendError::Read)?;

        if bytes_read == 0 {
            return Err(FrontendError::SubprocessExited {
                binary: PathBuf::new(),
                exit_status: None,
                stderr_tail: String::new(),
            });
        }

        let response: Response =
            serde_json::from_str(line.trim()).map_err(|e| deserialize_error(e, &line))?;

        if response.id != expected_id {
            return Err(FrontendError::IdMismatch {
                request_id: expected_id,
                response_id: response.id,
            });
        }

        if let Some(timing) = &response.timing {
            crate::timing::record_protocol_timing(timing);
        }

        Ok(response)
    }

    /// Send a raw JSON value as a request, auto-assigning an ID.
    ///
    /// Unlike `send()`, this accepts an arbitrary JSON object, allowing extra
    /// fields (e.g., `file` for the Rust frontend's Execute command) that the
    /// typed `Command` enum does not include. The caller must ensure the JSON
    /// is a valid protocol request except for the `id` field, which is overwritten.
    pub async fn send_raw(
        &mut self,
        mut request: serde_json::Value,
    ) -> Result<Response, FrontendError> {
        let span = tracing::info_span!("frontend.request.raw");
        let id = self.next_id;
        self.next_id += 1;

        request["id"] = serde_json::Value::from(id);

        let mut json = serde_json::to_string(&request).map_err(FrontendError::Serialize)?;
        json.push('\n');

        // A tainted frontend must not be used again — return Timeout so the
        // caller handles it as a clean termination rather than an IdMismatch.
        if self.tainted {
            return Err(FrontendError::Timeout(self.request_timeout));
        }

        // Drain stale responses from previously timed-out requests.
        if self.pending_drain > 0 {
            self.drain_stale_responses().await?;
        }

        // Copy request_timeout before the async block to avoid a shared/mutable
        // borrow conflict on `self`.
        let request_timeout = self.request_timeout;
        let write_and_read = async {
            self.stdin
                .write_all(json.as_bytes())
                .await
                .map_err(FrontendError::Write)?;
            self.stdin.flush().await.map_err(FrontendError::Write)?;

            self.read_response(id).await
        };

        let outcome = tokio::time::timeout(request_timeout, write_and_read)
            .instrument(span)
            .await;

        match outcome {
            Ok(inner) => self.enrich_subprocess_exit(inner).await,
            Err(_) => {
                self.tainted = true;
                self.pending_drain += 1;
                Err(FrontendError::Timeout(self.request_timeout))
            }
        }
    }

    /// If `outcome` indicates the subprocess has gone away, wait for the child
    /// to terminate and attach the exit status plus captured stderr tail.
    /// Other errors and `Ok` pass through.
    ///
    /// Two conditions mean the subprocess is gone:
    ///
    /// * `SubprocessExited` — `read_response` saw EOF on stdout.
    /// * `Write(BrokenPipe)` — the request write failed because the subprocess
    ///   closed its stdin read end, i.e. it has exited or is exiting.
    ///
    /// Both are surfaced as the same `SubprocessExited` diagnostic. A
    /// write-side broken pipe is **never** returned to the caller as-is:
    /// doing so was a race (str-9wu7) where a pre-handshake crash leaked a
    /// low-level `BrokenPipe`, discarding the stderr tail and exit status the
    /// diagnostic is meant to carry.
    async fn enrich_subprocess_exit(
        &mut self,
        outcome: Result<Response, FrontendError>,
    ) -> Result<Response, FrontendError> {
        match &outcome {
            Err(FrontendError::SubprocessExited { .. }) => {}
            Err(FrontendError::Write(error))
                if error.kind() == std::io::ErrorKind::BrokenPipe => {}
            _ => return outcome,
        }

        let exit_status = self.reap_for_diagnostic().await;
        self.subprocess_exited_error(exit_status).await
    }

    /// Wait for the (exiting) subprocess to terminate so its exit status can be
    /// attached to the diagnostic.
    ///
    /// The subprocess has already closed its stdio, so `wait()` normally
    /// returns immediately with the real exit status. The bounded wait guards
    /// against a pathological child that closes its pipes without exiting; if
    /// the grace period elapses we force-kill it so `wait()` can complete.
    /// This keeps the diagnostic path deterministic — it always resolves to a
    /// `SubprocessExited` error and never hangs or leaks a low-level pipe error
    /// because a timer beat the reap under load.
    async fn reap_for_diagnostic(&mut self) -> Option<std::process::ExitStatus> {
        if let Ok(Ok(status)) = tokio::time::timeout(POST_EOF_WAIT, self.child.wait()).await {
            return Some(status);
        }
        // Grace elapsed (or wait errored): the child is wedged with its pipes
        // closed. Force termination so the follow-up wait is guaranteed to
        // complete, then record whatever status we can.
        let _ = self.child.start_kill();
        match tokio::time::timeout(POST_EOF_WAIT, self.child.wait()).await {
            Ok(Ok(status)) => Some(status),
            _ => None,
        }
    }

    async fn subprocess_exited_error(
        &mut self,
        exit_status: Option<std::process::ExitStatus>,
    ) -> Result<Response, FrontendError> {
        // The stderr capture task only finishes once the child closes stderr,
        // which `wait()` above guarantees when an exit status is present.
        // Awaiting the task ensures any buffered bytes have been copied into
        // `stderr_buf` before we drain.
        if let Some(handle) = self.stderr_task.take() {
            let _ = tokio::time::timeout(POST_EOF_WAIT, handle).await;
        }
        let stderr_tail = drain_stderr(&self.stderr_buf);
        Err(FrontendError::SubprocessExited {
            binary: self.binary.clone(),
            exit_status,
            stderr_tail,
        })
    }

    /// Request a graceful shutdown and wait for acknowledgment.
    ///
    /// If the frontend is tainted (a previous request timed out), the
    /// subprocess is killed directly rather than attempting a graceful
    /// shutdown over the corrupted pipe.
    pub async fn shutdown(mut self) -> Result<(), FrontendError> {
        if self.tainted {
            // Pipe is in an unknown state — skip the protocol exchange and
            // force-kill so the subprocess does not linger.
            let _ = self.child.kill().await;
            return Ok(());
        }

        let response = match self.send(ProtoCommand::Shutdown).await {
            Ok(response) => response,
            Err(error) => {
                // The child may be busy handling a timed-out request and never
                // read the shutdown command. Since this method consumes the
                // frontend, force-kill here instead of relying on drop.
                let _ = self.child.kill().await;
                return Err(error);
            }
        };

        match response.result {
            ResponseResult::ShutdownAck => {}
            ResponseResult::Error {
                code,
                message,
                details,
            } => {
                return Err(FrontendError::Protocol {
                    code,
                    message,
                    details,
                });
            }
            _ => {}
        }

        // Wait briefly for the process to exit, then kill if still running.
        let wait_result = tokio::time::timeout(SHUTDOWN_GRACE_PERIOD, self.child.wait()).await;

        match wait_result {
            Ok(Ok(_status)) => Ok(()),
            Ok(Err(e)) => Err(FrontendError::Read(e)),
            Err(_) => {
                // Process didn't exit in time; force kill.
                let _ = self.child.kill().await;
                Ok(())
            }
        }
    }

    /// Check whether the frontend subprocess is still running.
    ///
    /// Returns `false` if the child has exited or `try_wait()` fails.
    pub fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Returns `true` if a previous request timed out and this frontend can
    /// no longer be used safely.  Callers should drop this frontend and spawn
    /// a fresh one rather than trying to send additional requests.
    pub fn is_tainted(&self) -> bool {
        self.tainted
    }

    /// The language reported by the frontend during handshake.
    pub fn language(&self) -> Option<&str> {
        self.language.as_deref()
    }

    /// Capabilities advertised by the frontend during handshake.
    pub fn capabilities(&self) -> &[String] {
        &self.capabilities
    }
}

fn spawn_stderr_capture(
    mut stderr: tokio::process::ChildStderr,
    buf: Arc<Mutex<Vec<u8>>>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut chunk = [0u8; 4096];
        loop {
            match stderr.read(&mut chunk).await {
                Ok(0) | Err(_) => return,
                Ok(n) => {
                    let mut guard = buf.lock().expect("stderr buffer mutex poisoned");
                    let remaining = STDERR_CAPTURE_LIMIT.saturating_sub(guard.len());
                    if remaining == 0 {
                        continue;
                    }
                    let take = n.min(remaining);
                    guard.extend_from_slice(&chunk[..take]);
                }
            }
        }
    })
}

fn drain_stderr(buf: &Arc<Mutex<Vec<u8>>>) -> String {
    let mut guard = buf.lock().expect("stderr buffer mutex poisoned");
    let bytes = std::mem::take(&mut *guard);
    String::from_utf8_lossy(&bytes).trim_end().to_string()
}

fn exit_status_display(status: Option<&std::process::ExitStatus>) -> String {
    match status {
        Some(s) => s.to_string(),
        None => "exit status unknown".to_string(),
    }
}

fn stderr_section(tail: &str) -> String {
    if tail.is_empty() {
        String::new()
    } else {
        format!("\nstderr: {tail}")
    }
}

fn request_span(command: &ProtoCommand) -> tracing::Span {
    match command {
        ProtoCommand::Handshake { .. } => tracing::info_span!("frontend.request.handshake"),
        ProtoCommand::Analyze { .. } => tracing::info_span!("frontend.request.analyze"),
        ProtoCommand::Instrument { .. } => tracing::info_span!("frontend.request.instrument"),
        ProtoCommand::Prepare { .. } => tracing::info_span!("frontend.request.prepare"),
        ProtoCommand::Execute { .. } => tracing::info_span!("frontend.request.execute"),
        ProtoCommand::Setup { .. } => tracing::info_span!("frontend.request.setup"),
        ProtoCommand::Teardown { .. } => tracing::info_span!("frontend.request.teardown"),
        ProtoCommand::Generate { .. } => tracing::info_span!("frontend.request.generate"),
        ProtoCommand::GetInvocationPlan { .. } => {
            tracing::info_span!("frontend.request.get_invocation_plan")
        }
        ProtoCommand::Shutdown => tracing::info_span!("frontend.request.shutdown"),
    }
}

/// Build a descriptive deserialization error that helps diagnose whether the
/// failure is from truncation, malformed payload, or a missing/unexpected field.
fn deserialize_error(source: serde_json::Error, raw_line: &str) -> FrontendError {
    let response_bytes = raw_line.len();
    let err_msg = source.to_string();
    let hint = if !raw_line.trim_end().ends_with('}') {
        "response appears truncated (does not end with '}')".into()
    } else if err_msg.contains("missing field") {
        format!(
            "frontend response has unexpected shape — a required field is absent; \
             first 200 chars: {}",
            &raw_line[..raw_line.len().min(200)]
        )
    } else {
        format!(
            "invalid JSON payload; first 200 chars: {}",
            &raw_line[..raw_line.len().min(200)]
        )
    };
    FrontendError::Deserialize {
        source,
        response_bytes,
        hint,
    }
}

/// Extract "major.minor" from a semver string for compatibility comparison.
fn major_minor(version: &str) -> &str {
    match version.rfind('.') {
        Some(pos) => &version[..pos],
        None => version,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn noop_frontend_path() -> PathBuf {
        // Walk up from the shatter-core crate to the workspace root.
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        manifest_dir.join("../protocol/noop-frontend.sh")
    }

    fn noop_config() -> FrontendConfig {
        let mut config = FrontendConfig::new(PathBuf::from("bash"));
        config.args = vec![noop_frontend_path().to_string_lossy().into_owned()];
        config.request_timeout = Duration::from_secs(5);
        config
    }

    #[tokio::test]
    async fn spawn_and_handshake_with_noop_frontend() {
        let config = noop_config();
        let frontend = Frontend::spawn(&config).await.expect("spawn failed");
        assert_eq!(frontend.language(), Some("noop"));
        frontend.shutdown().await.expect("shutdown failed");
    }

    #[tokio::test]
    async fn analyze_returns_stub_function() {
        let config = noop_config();
        let mut frontend = Frontend::spawn(&config).await.expect("spawn failed");

        let response = frontend
            .send(ProtoCommand::Analyze {
                file: "test.ts".into(),
                function: Some("myFunc".into()),
                project_root: None,
                execution_profile: None,
            })
            .await
            .expect("analyze failed");

        match response.result {
            ResponseResult::Analyze { functions } => {
                assert_eq!(functions.len(), 1);
                assert_eq!(functions[0].name, "stub");
            }
            other => panic!("expected Analyze response, got: {other:?}"),
        }

        frontend.shutdown().await.expect("shutdown failed");
    }

    #[tokio::test]
    async fn instrument_returns_success() {
        let config = noop_config();
        let mut frontend = Frontend::spawn(&config).await.expect("spawn failed");

        let response = frontend
            .send(ProtoCommand::Instrument {
                file: "test.ts".into(),
                function: "myFunc".into(),
                mocks: vec![],
                project_root: None,
                execution_profile: None,
            })
            .await
            .expect("instrument failed");

        match response.result {
            ResponseResult::Instrument {
                instrumented,
                output_file,
                ..
            } => {
                assert!(instrumented);
                assert!(output_file.is_none());
            }
            other => panic!("expected Instrument response, got: {other:?}"),
        }

        frontend.shutdown().await.expect("shutdown failed");
    }

    #[tokio::test]
    async fn execute_returns_empty_result() {
        let config = noop_config();
        let mut frontend = Frontend::spawn(&config).await.expect("spawn failed");

        let response = frontend
            .send(ProtoCommand::Execute {
                function: "myFunc".into(),
                inputs: vec![serde_json::json!(42)],
                mocks: vec![],
                setup_context: None,
                capture: true,
                prepare_id: None,
                execution_profile: None,
                plan: None,
            })
            .await
            .expect("execute failed");

        match response.result {
            ResponseResult::Execute(result) => {
                assert!(result.branch_path.is_empty());
                assert!(result.side_effects.is_empty());
            }
            other => panic!("expected Execute response, got: {other:?}"),
        }

        frontend.shutdown().await.expect("shutdown failed");
    }

    #[tokio::test]
    async fn multiple_requests_use_sequential_ids() {
        let config = noop_config();
        let mut frontend = Frontend::spawn(&config).await.expect("spawn failed");

        // Handshake already consumed id=1, so next should be id=2.
        let r1 = frontend
            .send(ProtoCommand::Analyze {
                file: "a.ts".into(),
                function: None,
                project_root: None,
                execution_profile: None,
            })
            .await
            .expect("request 1 failed");
        assert_eq!(r1.id, 2);

        let r2 = frontend
            .send(ProtoCommand::Analyze {
                file: "b.ts".into(),
                function: None,
                project_root: None,
                execution_profile: None,
            })
            .await
            .expect("request 2 failed");
        assert_eq!(r2.id, 3);

        frontend.shutdown().await.expect("shutdown failed");
    }

    #[tokio::test]
    async fn spawn_nonexistent_binary_returns_spawn_error() {
        let config = FrontendConfig::new(PathBuf::from("/nonexistent/binary"));
        let result = Frontend::spawn(&config).await;
        assert!(matches!(result, Err(FrontendError::Spawn(_))));
    }

    #[tokio::test]
    async fn full_lifecycle_handshake_through_shutdown() {
        let config = noop_config();
        let mut frontend = Frontend::spawn(&config).await.expect("spawn failed");

        // Analyze
        let _ = frontend
            .send(ProtoCommand::Analyze {
                file: "test.ts".into(),
                function: Some("fn1".into()),
                project_root: None,
                execution_profile: None,
            })
            .await
            .expect("analyze failed");

        // Instrument
        let _ = frontend
            .send(ProtoCommand::Instrument {
                file: "test.ts".into(),
                function: "fn1".into(),
                mocks: vec![],
                project_root: None,
                execution_profile: None,
            })
            .await
            .expect("instrument failed");

        // Execute
        let _ = frontend
            .send(ProtoCommand::Execute {
                function: "fn1".into(),
                inputs: vec![serde_json::json!(1), serde_json::json!("hello")],
                mocks: vec![],
                setup_context: None,
                capture: true,
                prepare_id: None,
                execution_profile: None,
                plan: None,
            })
            .await
            .expect("execute failed");

        // Shutdown
        frontend.shutdown().await.expect("shutdown failed");
    }

    #[test]
    fn major_minor_extracts_correctly() {
        assert_eq!(super::major_minor("0.1.0"), "0.1");
        assert_eq!(super::major_minor("1.2.3"), "1.2");
        assert_eq!(super::major_minor("10.20.30"), "10.20");
    }

    #[test]
    fn major_minor_handles_no_dots() {
        assert_eq!(super::major_minor("1"), "1");
    }

    #[test]
    fn request_spans_flow_into_expected_phase_names() {
        let handle = crate::timing::TimingHandle::default();
        let dispatch = handle.dispatch();

        tracing::dispatcher::with_default(&dispatch, || {
            {
                let _handshake = super::request_span(&ProtoCommand::Handshake {
                    capabilities: vec!["analyze".into()],
                })
                .entered();
            }
            {
                let _execute = super::request_span(&ProtoCommand::Execute {
                    function: "f".into(),
                    inputs: vec![],
                    mocks: vec![],
                    setup_context: None,
                    capture: true,
                    prepare_id: None,
                    execution_profile: None,
                    plan: None,
                })
                .entered();
            }
        });

        let phase_paths: Vec<String> = handle
            .snapshot()
            .into_iter()
            .map(|summary| summary.phase_path)
            .collect();

        assert!(
            phase_paths
                .iter()
                .any(|phase| phase == "frontend.request.handshake")
        );
        assert!(
            phase_paths
                .iter()
                .any(|phase| phase == "frontend.request.execute")
        );
    }

    #[tokio::test]
    async fn is_alive_returns_true_for_running_frontend() {
        let config = noop_config();
        let mut frontend = Frontend::spawn(&config).await.expect("spawn failed");
        assert!(frontend.is_alive());
        frontend.shutdown().await.expect("shutdown failed");
    }

    #[tokio::test]
    async fn is_alive_returns_false_after_shutdown() {
        let config = noop_config();
        let mut frontend = Frontend::spawn(&config).await.expect("spawn failed");
        // Send shutdown and wait for process exit
        let _ = frontend.send(crate::protocol::Command::Shutdown).await;
        // Give the process a moment to exit
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(!frontend.is_alive());
    }

    #[test]
    fn frontend_config_new_sets_defaults() {
        let config = FrontendConfig::new(PathBuf::from("/usr/bin/node"));
        assert_eq!(config.command, PathBuf::from("/usr/bin/node"));
        assert!(config.args.is_empty());
        assert_eq!(config.request_timeout, DEFAULT_REQUEST_TIMEOUT);
        assert_eq!(config.capabilities.len(), 3);
    }

    fn slow_frontend_config() -> FrontendConfig {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let slow_path = manifest_dir.join("../protocol/slow-frontend.sh");
        let mut config = FrontendConfig::new(PathBuf::from("bash"));
        config.args = vec![slow_path.to_string_lossy().into_owned()];
        config.request_timeout = Duration::from_millis(100);
        config
    }

    fn crashing_frontend_config(script_body: &str) -> (FrontendConfig, tempfile::NamedTempFile) {
        use std::io::Write as _;
        let mut script = tempfile::Builder::new()
            .prefix("shatter-crashing-frontend-")
            .suffix(".sh")
            .tempfile()
            .expect("create temp script");
        write!(script, "#!/usr/bin/env bash\n{script_body}").expect("write script");
        let mut config = FrontendConfig::new(PathBuf::from("bash"));
        config.args = vec![script.path().to_string_lossy().into_owned()];
        config.request_timeout = Duration::from_secs(5);
        (config, script)
    }

    #[tokio::test]
    async fn handshake_subprocess_crash_surfaces_stderr_and_exit_status() {
        // Simulate a frontend that throws during module initialization:
        // emit the error to stderr and exit with code 1 before any stdout
        // is written. The handshake send() should return a SubprocessExited
        // diagnostic containing both signals — not an opaque "closed stdout".
        let (config, _keep) = crashing_frontend_config(
            "echo 'TypeError: Cannot read properties of undefined (reading \"foo\")' >&2\nexit 1\n",
        );
        match Frontend::spawn(&config).await {
            Err(FrontendError::SubprocessExited {
                exit_status,
                stderr_tail,
                binary,
            }) => {
                assert!(
                    stderr_tail.contains("TypeError"),
                    "stderr tail should preserve the thrown message, got: {stderr_tail:?}"
                );
                let status = exit_status.expect("exit status should be captured");
                assert_eq!(status.code(), Some(1));
                assert_eq!(binary, PathBuf::from("bash"));
            }
            Err(other) => panic!("expected SubprocessExited, got: {other:?}"),
            Ok(_) => panic!("expected SubprocessExited, got Ok(Frontend)"),
        }
    }

    #[tokio::test]
    async fn handshake_after_confirmed_child_exit_surfaces_subprocess_diagnostic() {
        // Deterministic version of the pre-handshake crash (str-9wu7). The
        // `handshake_subprocess_crash_surfaces_stderr_and_exit_status` test
        // races the subprocess's exit against the handshake write, so it
        // exercises *either* the EOF path *or* the broken-pipe path depending
        // on scheduler timing. Here we launch the frontend, wait until the
        // child has fully exited, and only then handshake. With the child's
        // stdin read end already closed, the handshake write deterministically
        // fails with BrokenPipe — pinning down the write-race path. It must
        // still surface the SubprocessExited diagnostic with the stderr tail
        // and exit status, never a bare Write(BrokenPipe).
        let (config, _keep) = crashing_frontend_config(
            "echo 'TypeError: Cannot read properties of undefined (reading \"foo\")' >&2\nexit 1\n",
        );

        let mut frontend = Frontend::launch(&config).expect("launch failed");

        // Wait until the child has exited so the next write hits a closed pipe.
        let mut waited = Duration::ZERO;
        while frontend.is_alive() {
            assert!(
                waited < Duration::from_secs(5),
                "crashing frontend should have exited promptly"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
            waited += Duration::from_millis(10);
        }

        match frontend.handshake(&config.capabilities).await {
            Err(FrontendError::SubprocessExited {
                exit_status,
                stderr_tail,
                binary,
            }) => {
                assert!(
                    stderr_tail.contains("TypeError"),
                    "stderr tail should preserve the thrown message, got: {stderr_tail:?}"
                );
                let status = exit_status.expect("exit status should be captured");
                assert_eq!(status.code(), Some(1));
                assert_eq!(binary, PathBuf::from("bash"));
            }
            Err(other) => panic!("expected SubprocessExited, got: {other:?}"),
            Ok(()) => panic!("expected SubprocessExited, got Ok(())"),
        }
    }

    #[tokio::test]
    async fn subprocess_crash_after_handshake_surfaces_stderr() {
        // Frontend completes handshake, then on the next request writes a
        // panic-like trace to stderr and exits. The follow-up send() should
        // surface the stderr tail instead of a bare EOF error.
        let (config, _keep) = crashing_frontend_config(
            "read -r line\n\
             echo '{\"protocol_version\":\"0.1.0\",\"id\":1,\"status\":\"handshake\",\"frontend_version\":\"0.1.0\",\"language\":\"crash-ts\",\"capabilities\":[]}'\n\
             read -r line\n\
             echo 'fatal: out of memory during module init' >&2\n\
             exit 137\n",
        );
        let mut frontend = Frontend::spawn(&config)
            .await
            .expect("handshake should succeed");
        let result = frontend
            .send(ProtoCommand::Analyze {
                file: "x.ts".into(),
                function: None,
                project_root: None,
                execution_profile: None,
            })
            .await;
        match result {
            Err(FrontendError::SubprocessExited {
                exit_status,
                stderr_tail,
                ..
            }) => {
                assert!(
                    stderr_tail.contains("fatal: out of memory"),
                    "stderr tail should include the crash message, got: {stderr_tail:?}"
                );
                let status = exit_status.expect("exit status should be captured");
                assert_eq!(status.code(), Some(137));
            }
            other => panic!("expected SubprocessExited, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn request_broken_pipe_after_child_exit_surfaces_subprocess_diagnostic() {
        let (config, _keep) = crashing_frontend_config(
            "read -r line\n\
             echo '{\"protocol_version\":\"0.1.0\",\"id\":1,\"status\":\"handshake\",\"frontend_version\":\"0.1.0\",\"language\":\"crash-ts\",\"capabilities\":[]}'\n\
             echo 'fatal: child exited before request write' >&2\n\
             exit 23\n",
        );
        let mut frontend = Frontend::spawn(&config)
            .await
            .expect("handshake should succeed");
        for _ in 0..20 {
            if !frontend.is_alive() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }

        let result = frontend
            .send(ProtoCommand::Analyze {
                file: "x.ts".into(),
                function: None,
                project_root: None,
                execution_profile: None,
            })
            .await;

        match result {
            Err(FrontendError::SubprocessExited {
                exit_status,
                stderr_tail,
                ..
            }) => {
                assert!(
                    stderr_tail.contains("fatal: child exited before request write"),
                    "stderr tail should include the crash message, got: {stderr_tail:?}"
                );
                let status = exit_status.expect("exit status should be captured");
                assert_eq!(status.code(), Some(23));
            }
            other => panic!("expected SubprocessExited, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn shutdown_timeout_kills_unresponsive_frontend() {
        let (mut config, _keep) = crashing_frontend_config(
            "read -r line\n\
             echo '{\"protocol_version\":\"0.1.0\",\"id\":1,\"status\":\"handshake\",\"frontend_version\":\"0.1.0\",\"language\":\"stubborn\",\"capabilities\":[]}'\n\
             read -r line\n\
             sleep 5\n",
        );
        config.request_timeout = Duration::from_millis(100);

        let frontend = Frontend::spawn(&config)
            .await
            .expect("handshake should succeed");
        let child_pid = frontend.child.id().expect("child pid should be known");

        let result = frontend.shutdown().await;
        assert!(
            matches!(result, Err(FrontendError::Timeout(_))),
            "unresponsive shutdown should surface a timeout, got: {result:?}"
        );

        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(
            !Path::new(&format!("/proc/{child_pid}")).exists(),
            "frontend child process {child_pid} should be killed after shutdown timeout"
        );
    }

    #[tokio::test]
    async fn spawn_timeout_returns_timeout_error() {
        let config = slow_frontend_config();
        match Frontend::spawn(&config).await {
            Err(FrontendError::Timeout(_)) => {} // expected
            Err(other) => panic!("expected FrontendError::Timeout, got: {other:?}"),
            Ok(_) => panic!("expected timeout error from slow frontend"),
        }
    }

    fn slow_once_frontend_config() -> FrontendConfig {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let script_path = manifest_dir.join("../protocol/slow-once-frontend.sh");
        let mut config = FrontendConfig::new(PathBuf::from("bash"));
        config.args = vec![script_path.to_string_lossy().into_owned()];
        // Short timeout so the slow analyze request triggers a timeout quickly.
        config.request_timeout = Duration::from_millis(300);
        config
    }

    /// After a request times out, the frontend is tainted and subsequent sends
    /// must return Timeout immediately rather than attempting to drain the stale
    /// response and risking an IdMismatch.
    #[tokio::test]
    async fn timeout_taints_frontend_and_subsequent_send_returns_timeout() {
        // slow-once-frontend.sh completes handshake (fast), then sleeps 2s on
        // the first analyze, triggering our 300 ms request timeout.
        let mut config = slow_once_frontend_config();
        // Handshake must succeed — use a long timeout just for that step, then
        // override to a short one for the subsequent slow request.
        config.request_timeout = Duration::from_secs(5);
        let mut frontend = Frontend::spawn(&config)
            .await
            .expect("spawn/handshake failed");

        // Now shorten the timeout so the slow analyze triggers it.
        frontend.request_timeout = Duration::from_millis(300);

        // First send — the slow analyze should time out.
        let first_result = frontend
            .send(ProtoCommand::Analyze {
                file: "test.ts".into(),
                function: None,
                project_root: None,
                execution_profile: None,
            })
            .await;
        assert!(
            matches!(first_result, Err(FrontendError::Timeout(_))),
            "expected Timeout on slow analyze, got: {first_result:?}"
        );
        assert!(
            frontend.is_tainted(),
            "frontend should be tainted after a timeout"
        );

        // Second send — must return Timeout (from tainted guard), NOT IdMismatch.
        let second_result = frontend
            .send(ProtoCommand::Analyze {
                file: "test.ts".into(),
                function: None,
                project_root: None,
                execution_profile: None,
            })
            .await;
        assert!(
            matches!(second_result, Err(FrontendError::Timeout(_))),
            "second send after timeout should return Timeout, not IdMismatch; got: {second_result:?}"
        );
        assert!(
            !matches!(second_result, Err(FrontendError::IdMismatch { .. })),
            "IdMismatch must not appear after a timeout — tainted guard failed"
        );
    }

    /// `is_tainted()` returns false on a fresh frontend and true after a timeout.
    #[tokio::test]
    async fn is_tainted_reflects_timeout_state() {
        let mut config = slow_once_frontend_config();
        // Start with long timeout for handshake.
        config.request_timeout = Duration::from_secs(5);
        let mut frontend = Frontend::spawn(&config)
            .await
            .expect("spawn/handshake failed");
        assert!(
            !frontend.is_tainted(),
            "fresh frontend should not be tainted"
        );

        // Trigger a timeout.
        frontend.request_timeout = Duration::from_millis(300);
        let _ = frontend
            .send(ProtoCommand::Analyze {
                file: "test.ts".into(),
                function: None,
                project_root: None,
                execution_profile: None,
            })
            .await;

        assert!(
            frontend.is_tainted(),
            "frontend should be tainted after a timeout"
        );
    }
}
