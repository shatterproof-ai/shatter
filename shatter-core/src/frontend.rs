//! Frontend subprocess manager for spawning and communicating with language frontends.
//!
//! The core engine communicates with frontends via newline-delimited JSON over stdin/stdout.
//! This module handles subprocess lifecycle (spawn, handshake, request/response, shutdown)
//! and provides a typed async API over the raw protocol.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};

use crate::protocol::{Command as ProtoCommand, Request, Response, ResponseResult, PROTOCOL_VERSION};

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

    /// The frontend process closed its stdout unexpectedly.
    #[error("frontend closed stdout unexpectedly")]
    UnexpectedEof,

    /// Failed to serialize a request to JSON.
    #[error("failed to serialize request: {0}")]
    Serialize(serde_json::Error),

    /// Failed to deserialize a response from JSON.
    #[error("failed to deserialize response: {0}")]
    Deserialize(serde_json::Error),

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
}

impl FrontendConfig {
    /// Create a config for a frontend at the given path with default settings.
    pub fn new(command: PathBuf) -> Self {
        Self {
            command,
            args: Vec::new(),
            request_timeout: Duration::from_secs(30),
            capabilities: vec![
                "analyze".into(),
                "execute".into(),
                "instrument".into(),
            ],
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
}

impl Frontend {
    /// Spawn a frontend subprocess and perform the initial handshake.
    ///
    /// Returns a ready-to-use `Frontend` after verifying protocol compatibility.
    pub async fn spawn(config: &FrontendConfig) -> Result<Self, FrontendError> {
        let mut child = Command::new(&config.command)
            .args(&config.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(FrontendError::Spawn)?;

        let stdin = child.stdin.take().expect("stdin was piped");
        let stdout = child.stdout.take().expect("stdout was piped");
        let reader = BufReader::new(stdout);

        let mut frontend = Self {
            child,
            stdin,
            reader,
            next_id: 1,
            request_timeout: config.request_timeout,
            language: None,
        };

        frontend.handshake(&config.capabilities).await?;

        Ok(frontend)
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
                ..
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
    pub async fn send(&mut self, command: ProtoCommand) -> Result<Response, FrontendError> {
        let id = self.next_id;
        self.next_id += 1;

        let request = Request::new(id, command);
        let mut json = serde_json::to_string(&request).map_err(FrontendError::Serialize)?;
        json.push('\n');

        let write_and_read = async {
            self.stdin
                .write_all(json.as_bytes())
                .await
                .map_err(FrontendError::Write)?;
            self.stdin.flush().await.map_err(FrontendError::Write)?;

            let mut line = String::new();
            let bytes_read = self
                .reader
                .read_line(&mut line)
                .await
                .map_err(FrontendError::Read)?;

            if bytes_read == 0 {
                return Err(FrontendError::UnexpectedEof);
            }

            let response: Response =
                serde_json::from_str(line.trim()).map_err(FrontendError::Deserialize)?;

            if response.id != id {
                return Err(FrontendError::IdMismatch {
                    request_id: id,
                    response_id: response.id,
                });
            }

            Ok(response)
        };

        tokio::time::timeout(self.request_timeout, write_and_read)
            .await
            .map_err(|_| FrontendError::Timeout(self.request_timeout))?
    }

    /// Request a graceful shutdown and wait for acknowledgment.
    pub async fn shutdown(mut self) -> Result<(), FrontendError> {
        let response = self.send(ProtoCommand::Shutdown).await?;

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
        let wait_result =
            tokio::time::timeout(Duration::from_secs(5), self.child.wait()).await;

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

    /// The language reported by the frontend during handshake.
    pub fn language(&self) -> Option<&str> {
        self.language.as_deref()
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
            })
            .await
            .expect("instrument failed");

        match response.result {
            ResponseResult::Instrument {
                instrumented,
                output_file,
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
            })
            .await
            .expect("request 1 failed");
        assert_eq!(r1.id, 2);

        let r2 = frontend
            .send(ProtoCommand::Analyze {
                file: "b.ts".into(),
                function: None,
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
            })
            .await
            .expect("analyze failed");

        // Instrument
        let _ = frontend
            .send(ProtoCommand::Instrument {
                file: "test.ts".into(),
                function: "fn1".into(),
                mocks: vec![],
            })
            .await
            .expect("instrument failed");

        // Execute
        let _ = frontend
            .send(ProtoCommand::Execute {
                function: "fn1".into(),
                inputs: vec![serde_json::json!(1), serde_json::json!("hello")],
                mocks: vec![],
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
    fn frontend_config_new_sets_defaults() {
        let config = FrontendConfig::new(PathBuf::from("/usr/bin/node"));
        assert_eq!(config.command, PathBuf::from("/usr/bin/node"));
        assert!(config.args.is_empty());
        assert_eq!(config.request_timeout, Duration::from_secs(30));
        assert_eq!(config.capabilities.len(), 3);
    }
}
