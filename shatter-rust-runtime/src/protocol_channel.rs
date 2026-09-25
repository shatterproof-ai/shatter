//! Protocol-channel isolation for the crate-bridge harness (str-49drv.116).
//!
//! The crate-bridge driver runs inside the user's crate, so it cannot take a
//! `libc` dependency the way the standalone and dispatch harnesses do. Before
//! this module, it wrote each JSON response with `println!` onto the same
//! stdout the target function prints to, and any printing target corrupted
//! the protocol line.
//!
//! [`ProtocolChannel::install`] takes a private duplicate of the original
//! stdout for responses, then points the process's standard output and error
//! streams at capture files for the rest of the process lifetime. After that,
//! `print!`, `println!`, `eprint!`, `eprintln!`, and `std::io::stdout()` all
//! land in the capture files, and only [`ProtocolChannel::respond`] writes to
//! the protocol pipe.
//!
//! Mechanism per platform, using std plus one OS call each:
//! - Unix: `dup2(2)` via a local `extern "C"` declaration (the symbol is in the
//!   C library std already links).
//! - Windows: `SetStdHandle` from `kernel32` (std already links it). Rust's
//!   `Stdout`/`Stderr` look up the standard handle on every write, so they
//!   follow the swap.
//!
//! The response handle is a close-on-exec (non-inheritable) clone, so child
//! processes spawned by user code cannot inherit it.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};

use serde_json::Value;

/// Maximum characters kept per captured console line, matching the
/// standalone and dispatch harnesses.
pub const MAX_CONSOLE_MESSAGE_CHARS: usize = 4096;

/// Owns the protocol response handle and the capture files that stand in for
/// stdout and stderr while the harness runs.
pub struct ProtocolChannel {
    response: File,
    original_stderr: File,
    stdout_capture: File,
    stderr_capture: File,
}

impl ProtocolChannel {
    /// Move user-visible stdout/stderr onto capture files and keep a private
    /// handle to the original stdout for protocol responses.
    pub fn install() -> io::Result<Self> {
        let _ = io::stdout().flush();
        let _ = io::stderr().flush();

        let response = sys::clone_stdout()?;
        let original_stderr = sys::clone_stderr()?;
        let stdout_capture = open_capture_file("stdout")?;
        let stderr_capture = open_capture_file("stderr")?;

        sys::redirect_stdout(&stdout_capture)?;
        if let Err(e) = sys::redirect_stderr(&stderr_capture) {
            let _ = sys::redirect_stdout(&response);
            return Err(e);
        }

        Ok(Self {
            response,
            original_stderr,
            stdout_capture,
            stderr_capture,
        })
    }

    /// Return everything user code wrote to stdout/stderr since the previous
    /// call as `console_output` side effects, and empty the capture files.
    pub fn take_console_output(&mut self) -> Vec<Value> {
        let _ = io::stdout().flush();
        let _ = io::stderr().flush();
        let stdout = drain(&mut self.stdout_capture);
        let stderr = drain(&mut self.stderr_capture);
        console_output_effects(&stdout, &stderr)
    }

    /// Write one response line to the protocol channel.
    pub fn respond(&mut self, response: &Value) -> io::Result<()> {
        let mut line = serde_json::to_string(response).map_err(io::Error::other)?;
        line.push('\n');
        self.response.write_all(line.as_bytes())?;
        self.response.flush()
    }
}

impl Drop for ProtocolChannel {
    fn drop(&mut self) {
        let _ = io::stdout().flush();
        let _ = io::stderr().flush();
        let _ = sys::redirect_stdout(&self.response);
        let _ = sys::redirect_stderr(&self.original_stderr);
    }
}

/// Build `console_output` side effects from captured text: stdout lines at
/// level `log`, then stderr lines at level `error`. Empty lines are skipped
/// and each message is truncated to [`MAX_CONSOLE_MESSAGE_CHARS`] characters.
pub fn console_output_effects(stdout: &str, stderr: &str) -> Vec<Value> {
    let lines = |text: &str, level: &'static str| {
        text.lines()
            .filter(|line| !line.is_empty())
            .map(move |line| {
                let message: String = line.chars().take(MAX_CONSOLE_MESSAGE_CHARS).collect();
                serde_json::json!({"kind": "console_output", "level": level, "message": message})
            })
            .collect::<Vec<_>>()
    };
    let mut effects = lines(stdout, "log");
    effects.extend(lines(stderr, "error"));
    effects
}

/// Prepend `console` to `result["side_effects"]`, keeping the harness-wide
/// order: console output, then thrown errors, then global state changes.
pub fn attach_console_output(result: &mut Value, console: Vec<Value>) {
    if console.is_empty() {
        return;
    }
    let Some(obj) = result.as_object_mut() else {
        return;
    };
    let side_effects = obj
        .entry("side_effects")
        .or_insert_with(|| Value::Array(Vec::new()));
    if let Some(existing) = side_effects.as_array_mut() {
        let rest = std::mem::take(existing);
        existing.extend(console);
        existing.extend(rest);
    }
}

fn drain(file: &mut File) -> String {
    let mut bytes = Vec::new();
    let _ = file.seek(SeekFrom::Start(0));
    let _ = file.read_to_end(&mut bytes);
    let _ = file.set_len(0);
    let _ = file.seek(SeekFrom::Start(0));
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Open a fresh read/write capture file that is removed without cleanup:
/// unlinked right away on Unix, delete-on-close on Windows (so it also goes
/// away when a timed-out harness is killed).
fn open_capture_file(stream: &str) -> io::Result<File> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let path = std::env::temp_dir().join(format!(
        "shatter-bridge-{}-{nanos}-{stream}",
        std::process::id()
    ));
    let mut options = std::fs::OpenOptions::new();
    options.read(true).write(true).create_new(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_ATTRIBUTE_TEMPORARY: u32 = 0x0000_0100;
        const FILE_FLAG_DELETE_ON_CLOSE: u32 = 0x0400_0000;
        options
            .share_mode(0)
            .custom_flags(FILE_ATTRIBUTE_TEMPORARY | FILE_FLAG_DELETE_ON_CLOSE);
    }
    let file = options.open(&path)?;
    #[cfg(unix)]
    let _ = std::fs::remove_file(&path);
    Ok(file)
}

#[cfg(unix)]
mod sys {
    use std::fs::File;
    use std::io;
    use std::os::raw::c_int;
    use std::os::unix::io::{AsFd, AsRawFd};

    extern "C" {
        fn dup2(src: c_int, dst: c_int) -> c_int;
    }

    pub fn clone_stdout() -> io::Result<File> {
        Ok(File::from(io::stdout().as_fd().try_clone_to_owned()?))
    }

    pub fn clone_stderr() -> io::Result<File> {
        Ok(File::from(io::stderr().as_fd().try_clone_to_owned()?))
    }

    pub fn redirect_stdout(to: &File) -> io::Result<()> {
        redirect(to, 1)
    }

    pub fn redirect_stderr(to: &File) -> io::Result<()> {
        redirect(to, 2)
    }

    fn redirect(to: &File, fd: c_int) -> io::Result<()> {
        // SAFETY: both descriptors are valid for the duration of the call;
        // dup2 atomically replaces `fd` and does not touch `to`.
        if unsafe { dup2(to.as_raw_fd(), fd) } < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

#[cfg(windows)]
mod sys {
    use std::fs::File;
    use std::io;
    use std::os::windows::io::{AsHandle, AsRawHandle, RawHandle};

    const STD_OUTPUT_HANDLE: u32 = -11i32 as u32;
    const STD_ERROR_HANDLE: u32 = -12i32 as u32;

    #[link(name = "kernel32")]
    extern "system" {
        fn SetStdHandle(n_std_handle: u32, handle: RawHandle) -> i32;
    }

    pub fn clone_stdout() -> io::Result<File> {
        Ok(File::from(io::stdout().as_handle().try_clone_to_owned()?))
    }

    pub fn clone_stderr() -> io::Result<File> {
        Ok(File::from(io::stderr().as_handle().try_clone_to_owned()?))
    }

    pub fn redirect_stdout(to: &File) -> io::Result<()> {
        redirect(to, STD_OUTPUT_HANDLE)
    }

    pub fn redirect_stderr(to: &File) -> io::Result<()> {
        redirect(to, STD_ERROR_HANDLE)
    }

    fn redirect(to: &File, which: u32) -> io::Result<()> {
        // SAFETY: `to` is an open handle owned by the ProtocolChannel, which
        // restores the original handles before dropping it.
        if unsafe { SetStdHandle(which, to.as_raw_handle()) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

#[cfg(not(any(unix, windows)))]
mod sys {
    use std::fs::File;
    use std::io;

    fn unsupported() -> io::Error {
        io::Error::new(
            io::ErrorKind::Unsupported,
            "crate-bridge protocol channel isolation is not implemented on this platform",
        )
    }

    pub fn clone_stdout() -> io::Result<File> {
        Err(unsupported())
    }

    pub fn clone_stderr() -> io::Result<File> {
        Err(unsupported())
    }

    pub fn redirect_stdout(_to: &File) -> io::Result<()> {
        Err(unsupported())
    }

    pub fn redirect_stderr(_to: &File) -> io::Result<()> {
        Err(unsupported())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn console_effects_order_stdout_before_stderr_and_skip_empty_lines() {
        let effects = console_output_effects("a\n\nb", "e1\n");
        let pairs: Vec<(&str, &str)> = effects
            .iter()
            .map(|e| (e["level"].as_str().unwrap(), e["message"].as_str().unwrap()))
            .collect();
        assert_eq!(pairs, vec![("log", "a"), ("log", "b"), ("error", "e1")]);
        assert!(effects.iter().all(|e| e["kind"] == "console_output"));
    }

    #[test]
    fn console_effects_truncate_long_lines_by_chars() {
        let long = "é".repeat(MAX_CONSOLE_MESSAGE_CHARS + 10);
        let effects = console_output_effects(&long, "");
        let message = effects[0]["message"].as_str().unwrap();
        assert_eq!(message.chars().count(), MAX_CONSOLE_MESSAGE_CHARS);
    }

    #[test]
    fn attach_prepends_console_before_existing_side_effects() {
        let mut result = serde_json::json!({"side_effects": [{"kind": "thrown_error"}]});
        attach_console_output(&mut result, console_output_effects("hi", ""));
        let kinds: Vec<&str> = result["side_effects"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["kind"].as_str().unwrap())
            .collect();
        assert_eq!(kinds, vec!["console_output", "thrown_error"]);
    }

    #[test]
    fn attach_creates_side_effects_when_missing_and_ignores_empty_console() {
        let mut result = serde_json::json!({"return_value": 1});
        attach_console_output(&mut result, Vec::new());
        assert_eq!(result, serde_json::json!({"return_value": 1}));
        attach_console_output(&mut result, console_output_effects("", "boom"));
        assert_eq!(result["side_effects"][0]["level"], "error");
    }

    #[test]
    fn capture_file_round_trips_and_drains_empty() {
        let mut file = open_capture_file("unit-test").unwrap();
        file.write_all(b"one\ntwo").unwrap();
        assert_eq!(drain(&mut file), "one\ntwo");
        assert_eq!(drain(&mut file), "");
        file.write_all(b"three").unwrap();
        assert_eq!(
            drain(&mut file),
            "three",
            "writes after a drain start at offset 0"
        );
    }
}
