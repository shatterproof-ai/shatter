//! Strace-based network dependency discovery.
//!
//! One-shot diagnostic tool that runs a command under `strace` on Linux and
//! parses the output to catalog all network-related syscalls. Useful for finding
//! external dependencies that static analysis and runtime detection both miss.
//!
//! Linux-only: uses `strace -f -e trace=network,read,write` to capture syscalls.
//! On non-Linux platforms, returns a descriptive error.

use std::collections::HashMap;
use std::fmt;
use std::path::Path;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Syscall names considered network-related for direct filtering.
const NETWORK_SYSCALLS: &[&str] = &[
    "socket",
    "connect",
    "bind",
    "listen",
    "accept",
    "accept4",
    "sendto",
    "recvfrom",
    "sendmsg",
    "recvmsg",
    "getsockname",
    "getpeername",
    "setsockopt",
    "getsockopt",
];

/// Address family prefix indicating IPv4 in strace output.
const AF_INET_PREFIX: &str = "AF_INET";

/// Address family prefix indicating IPv6 in strace output.
const AF_INET6_PREFIX: &str = "AF_INET6";

/// Address family prefix indicating Unix domain sockets.
const AF_UNIX_PREFIX: &str = "AF_UNIX";

/// Socket type indicating TCP (stream-oriented).
const SOCK_STREAM: &str = "SOCK_STREAM";

/// Socket type indicating UDP (datagram-oriented).
const SOCK_DGRAM: &str = "SOCK_DGRAM";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A network endpoint discovered via strace.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NetworkEndpoint {
    /// IP address or Unix socket path.
    pub address: String,
    /// Port number (0 for Unix sockets).
    pub port: u16,
    /// Protocol family: "ipv4", "ipv6", or "unix".
    pub family: String,
    /// Socket type: "tcp", "udp", or "unknown".
    pub socket_type: String,
}

/// A single parsed syscall event from strace output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyscallEvent {
    /// Process ID that made the syscall.
    pub pid: u32,
    /// Syscall name (e.g., "connect", "sendto").
    pub syscall: String,
    /// Raw argument string from strace.
    pub args: String,
    /// Return value from strace (e.g., "0", "-1 ECONNREFUSED").
    pub result: String,
}

/// Report of all network activity discovered by strace.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StraceReport {
    /// Command that was traced.
    pub command: Vec<String>,
    /// All parsed network syscall events.
    pub events: Vec<SyscallEvent>,
    /// Unique network endpoints discovered (deduplicated from connect/sendto calls).
    pub endpoints: Vec<NetworkEndpoint>,
    /// Summary: count of each syscall type observed.
    pub syscall_counts: HashMap<String, u32>,
}

/// Errors from strace-based discovery.
#[derive(Debug)]
pub enum StraceError {
    /// strace is not available on this platform or not installed.
    NotAvailable(String),
    /// The traced command failed.
    CommandFailed { exit_code: Option<i32>, stderr: String },
    /// Failed to parse strace output.
    ParseError(String),
    /// I/O error running strace.
    IoError(std::io::Error),
}

impl fmt::Display for StraceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StraceError::NotAvailable(msg) => write!(f, "strace not available: {msg}"),
            StraceError::CommandFailed { exit_code, stderr } => {
                write!(
                    f,
                    "traced command failed (exit code: {}): {}",
                    exit_code.map_or("unknown".to_string(), |c| c.to_string()),
                    stderr
                )
            }
            StraceError::ParseError(msg) => write!(f, "strace parse error: {msg}"),
            StraceError::IoError(e) => write!(f, "I/O error: {e}"),
        }
    }
}

impl std::error::Error for StraceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            StraceError::IoError(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for StraceError {
    fn from(e: std::io::Error) -> Self {
        StraceError::IoError(e)
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Run a command under strace and produce a network dependency report.
///
/// This is a **diagnostic-only** tool intended for one-shot discovery.
/// It spawns `strace -f -e trace=network,read,write` around the given command,
/// captures the trace output, and parses it for network-related activity.
///
/// Returns `StraceError::NotAvailable` on non-Linux platforms.
#[cfg(target_os = "linux")]
pub fn discover_network_deps(
    command: &[String],
    working_dir: Option<&Path>,
) -> Result<StraceReport, StraceError> {
    if command.is_empty() {
        return Err(StraceError::ParseError("empty command".to_string()));
    }

    // Check that strace is available.
    let strace_check = std::process::Command::new("strace")
        .arg("--version")
        .output();

    match strace_check {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(StraceError::NotAvailable(
                "strace binary not found in PATH. Install with: apt install strace".to_string(),
            ));
        }
        Err(e) => return Err(StraceError::IoError(e)),
        Ok(_) => {}
    }

    // Build strace command: strace -f -e trace=network,read,write -o /dev/stderr <cmd>
    // We capture strace output on stderr (its default) and let the traced
    // program's stdout/stderr pass through.
    let temp_dir = std::env::temp_dir();
    let trace_file = temp_dir.join(format!("shatter-strace-{}.log", std::process::id()));

    let mut strace_cmd = std::process::Command::new("strace");
    strace_cmd
        .arg("-f")
        .arg("-e")
        .arg("trace=network,read,write")
        .arg("-o")
        .arg(&trace_file)
        .args(command);

    if let Some(dir) = working_dir {
        strace_cmd.current_dir(dir);
    }

    let output = strace_cmd.output()?;

    // Read the trace file.
    let trace_output = std::fs::read_to_string(&trace_file).unwrap_or_default();
    // Clean up the trace file (best-effort).
    let _ = std::fs::remove_file(&trace_file);

    // The traced command failing is not necessarily an error for us — we still
    // want to report whatever network activity was observed. But we log a warning.
    if !output.status.success() {
        log::warn!(
            "traced command exited with code {:?}",
            output.status.code()
        );
    }

    parse_strace_output(&trace_output, command)
}

/// Non-Linux stub: always returns `NotAvailable`.
#[cfg(not(target_os = "linux"))]
pub fn discover_network_deps(
    _command: &[String],
    _working_dir: Option<&Path>,
) -> Result<StraceReport, StraceError> {
    Err(StraceError::NotAvailable(
        "strace-based discovery is only available on Linux".to_string(),
    ))
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Parse raw strace output into a structured report.
///
/// Each strace line has the format:
///   PID  syscall(args) = result
/// or for unfinished/resumed calls:
///   PID  syscall(args <unfinished ...>
///   PID  <... syscall resumed>) = result
///
/// We focus on completed calls and extract network-relevant information.
pub fn parse_strace_output(
    output: &str,
    command: &[String],
) -> Result<StraceReport, StraceError> {
    let mut events = Vec::new();
    let mut endpoint_set: HashMap<NetworkEndpoint, ()> = HashMap::new();
    let mut syscall_counts: HashMap<String, u32> = HashMap::new();
    // Track which file descriptors are sockets (from socket() calls).
    let mut socket_fds: HashMap<(u32, i32), SocketInfo> = HashMap::new();

    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() || line.contains("<unfinished") || line.contains("resumed>") {
            continue;
        }

        if let Some(parsed) = parse_strace_line(line) {
            // Track socket FDs.
            if parsed.syscall == "socket"
                && let Some(fd) = parse_return_fd(&parsed.result)
            {
                let info = parse_socket_args(&parsed.args);
                socket_fds.insert((parsed.pid, fd), info);
            }

            // Extract endpoints from connect/sendto.
            if (parsed.syscall == "connect" || parsed.syscall == "sendto")
                && let Some(endpoint) = extract_endpoint(&parsed.args, &socket_fds, parsed.pid)
            {
                endpoint_set.insert(endpoint, ());
            }

            let is_network = is_network_syscall(&parsed.syscall)
                || is_socket_io(&parsed, &socket_fds);

            if is_network {
                *syscall_counts.entry(parsed.syscall.clone()).or_insert(0) += 1;
                events.push(parsed);
            }
        }
    }

    let endpoints: Vec<NetworkEndpoint> = endpoint_set.into_keys().collect();

    Ok(StraceReport {
        command: command.to_vec(),
        events,
        endpoints,
        syscall_counts,
    })
}

/// Info about a socket FD from a socket() call.
#[derive(Debug, Clone)]
struct SocketInfo {
    family: String,
    socket_type: String,
}

/// Check if a syscall name is a known network syscall.
fn is_network_syscall(name: &str) -> bool {
    NETWORK_SYSCALLS.contains(&name)
}

/// Check if a read/write syscall is operating on a known socket FD.
fn is_socket_io(event: &SyscallEvent, socket_fds: &HashMap<(u32, i32), SocketInfo>) -> bool {
    if event.syscall != "read" && event.syscall != "write" {
        return false;
    }
    // First arg is the FD.
    if let Some(fd_str) = event.args.split(',').next()
        && let Ok(fd) = fd_str.trim().parse::<i32>()
    {
        return socket_fds.contains_key(&(event.pid, fd));
    }
    false
}

/// Parse a single strace output line into a `SyscallEvent`.
///
/// Expected format: `PID  syscall(args) = result`
fn parse_strace_line(line: &str) -> Option<SyscallEvent> {
    // PID is at the start, followed by whitespace, then the syscall.
    let mut parts = line.splitn(2, |c: char| c.is_ascii_whitespace());
    let pid_str = parts.next()?;
    let rest = parts.next()?.trim();

    let pid: u32 = pid_str.parse().ok()?;

    // Find the syscall name (everything before the first '(').
    let paren_pos = rest.find('(')?;
    let syscall = rest[..paren_pos].to_string();

    // Find matching close paren for args — need to handle nested parens.
    let after_paren = &rest[paren_pos + 1..];
    let close_pos = find_matching_close_paren(after_paren)?;
    let args = after_paren[..close_pos].to_string();

    // Result is after ") = ".
    let after_close = &after_paren[close_pos + 1..];
    let result = if let Some(eq_pos) = after_close.find('=') {
        after_close[eq_pos + 1..].trim().to_string()
    } else {
        String::new()
    };

    Some(SyscallEvent {
        pid,
        syscall,
        args,
        result,
    })
}

/// Find the position of the matching close parenthesis, handling nesting.
fn find_matching_close_paren(s: &str) -> Option<usize> {
    let mut depth = 0;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                if depth == 0 {
                    return Some(i);
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    None
}

/// Parse the return value of a syscall as a file descriptor.
fn parse_return_fd(result: &str) -> Option<i32> {
    let trimmed = result.trim();
    // Could be "3" or "3<socket:...>" — take digits.
    let fd_str: String = trimmed.chars().take_while(|c| c.is_ascii_digit() || *c == '-').collect();
    fd_str.parse().ok().filter(|fd| *fd >= 0)
}

/// Parse socket() arguments to determine family and type.
///
/// Format: `AF_INET, SOCK_STREAM, IPPROTO_TCP`
fn parse_socket_args(args: &str) -> SocketInfo {
    let parts: Vec<&str> = args.split(',').map(str::trim).collect();

    let family = if parts.first().is_some_and(|p| p.contains(AF_INET6_PREFIX)) {
        "ipv6".to_string()
    } else if parts.first().is_some_and(|p| p.contains(AF_INET_PREFIX)) {
        "ipv4".to_string()
    } else if parts.first().is_some_and(|p| p.contains(AF_UNIX_PREFIX)) {
        "unix".to_string()
    } else {
        "unknown".to_string()
    };

    let socket_type = if parts.get(1).is_some_and(|p| p.contains(SOCK_STREAM)) {
        "tcp".to_string()
    } else if parts.get(1).is_some_and(|p| p.contains(SOCK_DGRAM)) {
        "udp".to_string()
    } else {
        "unknown".to_string()
    };

    SocketInfo { family, socket_type }
}

/// Extract a network endpoint from connect() or sendto() arguments.
///
/// connect(3, {sa_family=AF_INET, sin_port=htons(443), sin_addr=inet_addr("93.184.216.34")}, 16)
fn extract_endpoint(
    args: &str,
    socket_fds: &HashMap<(u32, i32), SocketInfo>,
    pid: u32,
) -> Option<NetworkEndpoint> {
    // Get the FD from the first arg.
    let fd_str = args.split(',').next()?.trim();
    let fd: i32 = fd_str.parse().ok()?;

    let socket_info = socket_fds.get(&(pid, fd));

    // Look for sa_family.
    let family = if args.contains(AF_INET6_PREFIX) {
        "ipv6"
    } else if args.contains(AF_INET_PREFIX) {
        "ipv4"
    } else if args.contains(AF_UNIX_PREFIX) {
        "unix"
    } else if let Some(info) = socket_info {
        info.family.as_str()
    } else {
        return None;
    };

    let (address, port) = if family == "unix" {
        // Unix socket: extract path.
        let path = extract_quoted_value(args, "sun_path")
            .unwrap_or_else(|| "unknown".to_string());
        (path, 0)
    } else {
        // IP socket: extract address and port.
        let addr = extract_quoted_value(args, "inet_addr")
            .or_else(|| extract_quoted_value(args, "sin6_addr"))
            .unwrap_or_else(|| "unknown".to_string());
        let port = extract_htons_value(args).unwrap_or(0);
        (addr, port)
    };

    let socket_type = socket_info
        .map(|i| i.socket_type.clone())
        .unwrap_or_else(|| "unknown".to_string());

    Some(NetworkEndpoint {
        address,
        port,
        family: family.to_string(),
        socket_type,
    })
}

/// Extract a quoted value after a field name like `inet_addr("1.2.3.4")`.
fn extract_quoted_value(s: &str, field: &str) -> Option<String> {
    let field_pos = s.find(field)?;
    let after_field = &s[field_pos + field.len()..];
    // Find opening quote.
    let quote_start = after_field.find('"')?;
    let after_quote = &after_field[quote_start + 1..];
    let quote_end = after_quote.find('"')?;
    Some(after_quote[..quote_end].to_string())
}

/// Extract a port number from `htons(PORT)`.
fn extract_htons_value(s: &str) -> Option<u16> {
    let htons_pos = s.find("htons(")?;
    let after = &s[htons_pos + 6..];
    let end = after.find(')')?;
    after[..end].parse().ok()
}

/// Format the strace report as human-readable text.
pub fn format_report(report: &StraceReport) -> String {
    let mut out = String::new();

    out.push_str("=== Strace Network Discovery Report ===\n\n");
    out.push_str(&format!("Command: {}\n", report.command.join(" ")));
    out.push_str(&format!(
        "Total network events: {}\n",
        report.events.len()
    ));

    if !report.endpoints.is_empty() {
        out.push_str(&format!(
            "Unique endpoints: {}\n\n",
            report.endpoints.len()
        ));
        out.push_str("--- Discovered Endpoints ---\n");
        for ep in &report.endpoints {
            if ep.family == "unix" {
                out.push_str(&format!(
                    "  unix://{}  ({})\n",
                    ep.address, ep.socket_type
                ));
            } else {
                out.push_str(&format!(
                    "  {}://{}:{}  ({})\n",
                    ep.socket_type, ep.address, ep.port, ep.family
                ));
            }
        }
    } else {
        out.push_str("\nNo network endpoints discovered.\n");
    }

    if !report.syscall_counts.is_empty() {
        out.push_str("\n--- Syscall Summary ---\n");
        let mut counts: Vec<_> = report.syscall_counts.iter().collect();
        counts.sort_by(|a, b| b.1.cmp(a.1));
        for (name, count) in counts {
            out.push_str(&format!("  {name}: {count}\n"));
        }
    }

    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // Sample strace output lines for testing.
    const SAMPLE_SOCKET_LINE: &str =
        r#"12345 socket(AF_INET, SOCK_STREAM, IPPROTO_TCP) = 3"#;

    const SAMPLE_CONNECT_LINE: &str =
        r#"12345 connect(3, {sa_family=AF_INET, sin_port=htons(443), sin_addr=inet_addr("93.184.216.34")}, 16) = 0"#;

    const SAMPLE_CONNECT_IPV6_LINE: &str =
        r#"12345 connect(4, {sa_family=AF_INET6, sin6_port=htons(80), sin6_addr=inet6_addr("::1")}, 28) = 0"#;

    const SAMPLE_SENDTO_LINE: &str =
        r#"12345 sendto(5, "GET / HTTP/1.1\r\n", 16, 0, {sa_family=AF_INET, sin_port=htons(8080), sin_addr=inet_addr("10.0.0.1")}, 16) = 16"#;

    const SAMPLE_UNIX_CONNECT: &str =
        r#"12345 connect(6, {sa_family=AF_UNIX, sun_path="/var/run/docker.sock"}, 110) = 0"#;

    const SAMPLE_READ_LINE: &str =
        r#"12345 read(3, "HTTP/1.1 200 OK\r\n", 4096) = 17"#;

    const SAMPLE_WRITE_LINE: &str =
        r#"12345 write(3, "GET / HTTP/1.1\r\n", 16) = 16"#;

    const SAMPLE_BIND_LINE: &str =
        r#"12345 bind(7, {sa_family=AF_INET, sin_port=htons(3000), sin_addr=inet_addr("0.0.0.0")}, 16) = 0"#;

    #[test]
    fn parse_socket_line() {
        let event = parse_strace_line(SAMPLE_SOCKET_LINE).expect("should parse");
        assert_eq!(event.pid, 12345);
        assert_eq!(event.syscall, "socket");
        assert!(event.args.contains("AF_INET"));
        assert!(event.args.contains("SOCK_STREAM"));
        assert_eq!(event.result, "3");
    }

    #[test]
    fn parse_connect_line() {
        let event = parse_strace_line(SAMPLE_CONNECT_LINE).expect("should parse");
        assert_eq!(event.pid, 12345);
        assert_eq!(event.syscall, "connect");
        assert!(event.args.contains("sin_addr"));
        assert_eq!(event.result, "0");
    }

    #[test]
    fn parse_full_trace() {
        let trace = [
            SAMPLE_SOCKET_LINE,
            SAMPLE_CONNECT_LINE,
            SAMPLE_SENDTO_LINE,
            SAMPLE_READ_LINE,
            SAMPLE_WRITE_LINE,
        ]
        .join("\n");

        let report = parse_strace_output(&trace, &["curl".to_string(), "https://example.com".to_string()])
            .expect("should parse");

        // socket, connect, sendto, read (on socket fd 3), write (on socket fd 3)
        assert_eq!(report.events.len(), 5);
        assert!(!report.endpoints.is_empty());
    }

    #[test]
    fn extract_ipv4_endpoint() {
        let trace = [SAMPLE_SOCKET_LINE, SAMPLE_CONNECT_LINE].join("\n");
        let report = parse_strace_output(&trace, &["test".to_string()])
            .expect("should parse");

        assert_eq!(report.endpoints.len(), 1);
        let ep = &report.endpoints[0];
        assert_eq!(ep.address, "93.184.216.34");
        assert_eq!(ep.port, 443);
        assert_eq!(ep.family, "ipv4");
        assert_eq!(ep.socket_type, "tcp");
    }

    #[test]
    fn extract_ipv6_endpoint() {
        let trace = [
            r#"12345 socket(AF_INET6, SOCK_STREAM, IPPROTO_TCP) = 4"#,
            SAMPLE_CONNECT_IPV6_LINE,
        ]
        .join("\n");
        let report = parse_strace_output(&trace, &["test".to_string()])
            .expect("should parse");

        assert_eq!(report.endpoints.len(), 1);
        let ep = &report.endpoints[0];
        assert_eq!(ep.address, "::1");
        assert_eq!(ep.port, 80);
        assert_eq!(ep.family, "ipv6");
    }

    #[test]
    fn extract_unix_endpoint() {
        let trace = [
            r#"12345 socket(AF_UNIX, SOCK_STREAM, 0) = 6"#,
            SAMPLE_UNIX_CONNECT,
        ]
        .join("\n");
        let report = parse_strace_output(&trace, &["test".to_string()])
            .expect("should parse");

        assert_eq!(report.endpoints.len(), 1);
        let ep = &report.endpoints[0];
        assert_eq!(ep.address, "/var/run/docker.sock");
        assert_eq!(ep.port, 0);
        assert_eq!(ep.family, "unix");
    }

    #[test]
    fn extract_sendto_endpoint() {
        let trace = [
            r#"12345 socket(AF_INET, SOCK_DGRAM, IPPROTO_UDP) = 5"#,
            SAMPLE_SENDTO_LINE,
        ]
        .join("\n");
        let report = parse_strace_output(&trace, &["test".to_string()])
            .expect("should parse");

        assert_eq!(report.endpoints.len(), 1);
        let ep = &report.endpoints[0];
        assert_eq!(ep.address, "10.0.0.1");
        assert_eq!(ep.port, 8080);
        assert_eq!(ep.socket_type, "udp");
    }

    #[test]
    fn read_write_on_socket_fd_are_included() {
        let trace = [
            SAMPLE_SOCKET_LINE,
            SAMPLE_READ_LINE,
            SAMPLE_WRITE_LINE,
        ]
        .join("\n");
        let report = parse_strace_output(&trace, &["test".to_string()])
            .expect("should parse");

        // socket + read on fd 3 + write on fd 3
        assert_eq!(report.events.len(), 3);
        assert_eq!(*report.syscall_counts.get("read").unwrap_or(&0), 1);
        assert_eq!(*report.syscall_counts.get("write").unwrap_or(&0), 1);
    }

    #[test]
    fn read_write_on_non_socket_fd_are_excluded() {
        // fd 99 was never opened as a socket.
        let trace = r#"12345 read(99, "data", 4) = 4
12345 write(99, "data", 4) = 4"#;
        let report = parse_strace_output(trace, &["test".to_string()])
            .expect("should parse");

        assert!(report.events.is_empty());
    }

    #[test]
    fn syscall_counts_are_correct() {
        let trace = [
            SAMPLE_SOCKET_LINE,
            SAMPLE_CONNECT_LINE,
            SAMPLE_READ_LINE,
            SAMPLE_READ_LINE,
            SAMPLE_WRITE_LINE,
        ]
        .join("\n");
        let report = parse_strace_output(&trace, &["test".to_string()])
            .expect("should parse");

        assert_eq!(*report.syscall_counts.get("socket").unwrap_or(&0), 1);
        assert_eq!(*report.syscall_counts.get("connect").unwrap_or(&0), 1);
        assert_eq!(*report.syscall_counts.get("read").unwrap_or(&0), 2);
        assert_eq!(*report.syscall_counts.get("write").unwrap_or(&0), 1);
    }

    #[test]
    fn empty_trace_produces_empty_report() {
        let report = parse_strace_output("", &["test".to_string()])
            .expect("should parse");

        assert!(report.events.is_empty());
        assert!(report.endpoints.is_empty());
        assert!(report.syscall_counts.is_empty());
    }

    #[test]
    fn unfinished_and_resumed_lines_are_skipped() {
        let trace = "12345 connect(3, {sa_family=AF_INET <unfinished ...>\n\
                     12345 <... connect resumed>) = 0";
        let report = parse_strace_output(trace, &["test".to_string()])
            .expect("should parse");

        assert!(report.events.is_empty());
    }

    #[test]
    fn deduplicates_endpoints() {
        let trace = [
            SAMPLE_SOCKET_LINE,
            SAMPLE_CONNECT_LINE,
            // Same connect again.
            SAMPLE_CONNECT_LINE,
        ]
        .join("\n");
        let report = parse_strace_output(&trace, &["test".to_string()])
            .expect("should parse");

        // Two connect events but only one unique endpoint.
        assert_eq!(report.events.len(), 3); // socket + 2x connect
        assert_eq!(report.endpoints.len(), 1);
    }

    #[test]
    fn bind_event_is_captured() {
        let trace = [
            r#"12345 socket(AF_INET, SOCK_STREAM, IPPROTO_TCP) = 7"#,
            SAMPLE_BIND_LINE,
        ]
        .join("\n");
        let report = parse_strace_output(&trace, &["test".to_string()])
            .expect("should parse");

        assert_eq!(*report.syscall_counts.get("bind").unwrap_or(&0), 1);
        // bind is a network syscall, so it should be in events.
        assert!(report.events.iter().any(|e| e.syscall == "bind"));
    }

    #[test]
    fn format_report_includes_endpoints() {
        let report = StraceReport {
            command: vec!["curl".to_string(), "https://example.com".to_string()],
            events: vec![],
            endpoints: vec![NetworkEndpoint {
                address: "93.184.216.34".to_string(),
                port: 443,
                family: "ipv4".to_string(),
                socket_type: "tcp".to_string(),
            }],
            syscall_counts: HashMap::new(),
        };

        let text = format_report(&report);
        assert!(text.contains("93.184.216.34"));
        assert!(text.contains("443"));
        assert!(text.contains("tcp"));
    }

    #[test]
    fn format_report_shows_no_endpoints_message() {
        let report = StraceReport {
            command: vec!["test".to_string()],
            events: vec![],
            endpoints: vec![],
            syscall_counts: HashMap::new(),
        };

        let text = format_report(&report);
        assert!(text.contains("No network endpoints discovered"));
    }

    #[test]
    fn parse_strace_line_handles_nested_parens() {
        let line = r#"12345 connect(3, {sa_family=AF_INET, sin_port=htons(443), sin_addr=inet_addr("1.2.3.4")}, 16) = 0"#;
        let event = parse_strace_line(line).expect("should parse");
        assert_eq!(event.syscall, "connect");
        assert_eq!(event.result, "0");
    }

    #[test]
    fn parse_strace_line_returns_none_for_garbage() {
        assert!(parse_strace_line("not a valid strace line").is_none());
        assert!(parse_strace_line("").is_none());
        assert!(parse_strace_line("12345").is_none());
    }

    #[test]
    fn extract_htons_value_works() {
        assert_eq!(extract_htons_value("htons(443)"), Some(443));
        assert_eq!(extract_htons_value("htons(80)"), Some(80));
        assert_eq!(extract_htons_value("no htons here"), None);
    }

    #[test]
    fn extract_quoted_value_works() {
        assert_eq!(
            extract_quoted_value(r#"inet_addr("1.2.3.4")"#, "inet_addr"),
            Some("1.2.3.4".to_string())
        );
        assert_eq!(
            extract_quoted_value(r#"sun_path="/tmp/sock""#, "sun_path"),
            Some("/tmp/sock".to_string())
        );
        assert_eq!(extract_quoted_value("no field", "inet_addr"), None);
    }

    #[test]
    fn is_network_syscall_identifies_all_known() {
        for name in NETWORK_SYSCALLS {
            assert!(is_network_syscall(name), "{name} should be network");
        }
        assert!(!is_network_syscall("open"));
        assert!(!is_network_syscall("read"));
        assert!(!is_network_syscall("write"));
    }

    #[test]
    fn multiple_pids_tracked_separately() {
        let trace = r#"111 socket(AF_INET, SOCK_STREAM, IPPROTO_TCP) = 3
222 socket(AF_INET, SOCK_DGRAM, IPPROTO_UDP) = 3
111 connect(3, {sa_family=AF_INET, sin_port=htons(80), sin_addr=inet_addr("1.1.1.1")}, 16) = 0
222 sendto(3, "data", 4, 0, {sa_family=AF_INET, sin_port=htons(53), sin_addr=inet_addr("8.8.8.8")}, 16) = 4"#;

        let report = parse_strace_output(trace, &["test".to_string()])
            .expect("should parse");

        assert_eq!(report.endpoints.len(), 2);

        let has_tcp = report.endpoints.iter().any(|e| e.socket_type == "tcp" && e.port == 80);
        let has_udp = report.endpoints.iter().any(|e| e.socket_type == "udp" && e.port == 53);
        assert!(has_tcp, "should have TCP endpoint on port 80");
        assert!(has_udp, "should have UDP endpoint on port 53");
    }
}
