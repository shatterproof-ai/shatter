//! Default-deny policy for executing target functions on the host filesystem
//! (str-gg9v).
//!
//! Shatter explores target functions by *running* them with mined and generated
//! inputs. A target that opens, creates, renames, or deletes a file by relative
//! path will mutate the invoking repository: literal-mined and generated string
//! inputs become stray files in the repo root. This was observed in the wild —
//! direct `shatter scan` runs (outside a Docker-sandboxed wrapper) left ~20
//! zero-byte files named after mutation prefixes and generated string values.
//!
//! Two independent controls address this:
//!
//! 1. **Default-deny** ([`ensure_execution_permitted`]): when no OS sandbox
//!    backend is configured, refuse to execute targets unless the operator opts
//!    in with `--allow-host-writes` (or `SHATTER_ALLOW_HOST_WRITES=1`).
//! 2. **Throwaway working directory** ([`IsolationGuard`]): whenever we do
//!    execute without an OS sandbox, point each frontend at a fresh temp
//!    directory so relative-path writes land there and are cleaned with the run.
//!
//! The sandbox backend itself lives in the Go frontend
//! (`SHATTER_SANDBOX_BACKEND=none|bwrap|docker`, see `shatter-go/sandbox`); this
//! module only detects whether one is active and, if not, applies the CLI-side
//! guard and isolation.

use crate::args::CliCommand;

/// Env var an operator (or a wrapper script / CI job) can set to opt into
/// unsandboxed host execution without passing `--allow-host-writes` on every
/// invocation. Truthy values: `1`, `true`, `yes` (case-insensitive).
pub(crate) const ALLOW_HOST_WRITES_ENV: &str = "SHATTER_ALLOW_HOST_WRITES";

/// Env var selecting the Go frontend's OS sandbox backend
/// (`none`/`bwrap`/`docker`). Any value other than empty/`none` counts as a
/// configured sandbox and satisfies the default-deny gate.
pub(crate) const SANDBOX_BACKEND_ENV: &str = "SHATTER_SANDBOX_BACKEND";

/// Env var carrying the absolute path of the throwaway host-write isolation
/// directory. Set on every execution-command frontend when no OS sandbox is
/// active; the Go frontend reads it to run its launcher there instead of in the
/// target module directory (and to capture files created there as side effects).
pub(crate) const ISOLATION_DIR_ENV: &str = "SHATTER_HOST_WRITE_DIR";

/// Returns whether the given value denotes a truthy opt-in.
fn is_truthy(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

/// Whether an OS-level sandbox backend is configured via `SHATTER_SANDBOX_BACKEND`.
///
/// A configured sandbox already contains the target's filesystem writes, so the
/// default-deny gate does not apply.
pub(crate) fn sandbox_backend_configured() -> bool {
    std::env::var(SANDBOX_BACKEND_ENV)
        .ok()
        .map(|v| {
            let v = v.trim().to_ascii_lowercase();
            !v.is_empty() && v != "none"
        })
        .unwrap_or(false)
}

/// Whether the operator opted into host writes via the environment.
fn env_opt_in() -> bool {
    std::env::var(ALLOW_HOST_WRITES_ENV)
        .ok()
        .map(|v| is_truthy(&v))
        .unwrap_or(false)
}

/// Whether target execution is permitted given the `--allow-host-writes` flag.
///
/// Permitted when any of: an OS sandbox backend is configured, the flag is set,
/// or the env opt-in is truthy.
pub(crate) fn execution_permitted(allow_flag: bool) -> bool {
    allow_flag || env_opt_in() || sandbox_backend_configured()
}

/// Whether a subcommand runs (or replays) target functions and is therefore
/// subject to the default-deny gate. Analysis-only commands (`analyze`,
/// `stale`) never execute a target and are exempt.
pub(crate) fn command_executes_targets(command: &CliCommand) -> bool {
    matches!(
        command,
        CliCommand::Explore(_)
            | CliCommand::Scan(_)
            | CliCommand::Run { .. }
            | CliCommand::Observe { .. }
            | CliCommand::Bench { .. }
            | CliCommand::Properties { .. }
            | CliCommand::Revalidate { .. }
    )
}

/// The instructive refusal message printed when execution is denied.
pub(crate) fn refusal_message() -> String {
    format!(
        "refusing to execute target functions without a sandbox.\n\n\
         Shatter runs your target functions with mined and generated inputs. A \
         target that opens, creates, renames, or deletes a file by relative path \
         will mutate this repository (stray files named after generated inputs, \
         overwritten or deleted files, etc.).\n\n\
         Choose one:\n  \
         • Configure an OS sandbox (recommended):\n      \
         export {backend}=docker    # or: bwrap\n  \
         • Opt into unsandboxed execution (targets still run in a throwaway \
         working directory):\n      \
         shatter … --allow-host-writes\n      \
         # or, once per shell/CI job:\n      \
         export {allow}=1\n",
        backend = SANDBOX_BACKEND_ENV,
        allow = ALLOW_HOST_WRITES_ENV,
    )
}

/// Enforce the default-deny gate. Returns an instructive error when execution is
/// not permitted.
pub(crate) fn ensure_execution_permitted(allow_flag: bool) -> Result<(), String> {
    if execution_permitted(allow_flag) {
        Ok(())
    } else {
        Err(refusal_message())
    }
}

/// Enforce the default-deny gate for `command` and, when execution is permitted
/// but no OS sandbox is active, create the throwaway isolation directory.
///
/// Returns the guard to keep alive for the command's duration, or `None` for
/// analysis-only commands and for runs already contained by an OS sandbox.
/// Returns the instructive refusal message when execution is denied.
pub(crate) fn setup(
    command: &CliCommand,
    allow_flag: bool,
) -> Result<Option<IsolationGuard>, String> {
    if !command_executes_targets(command) {
        return Ok(None);
    }
    ensure_execution_permitted(allow_flag)?;
    if sandbox_backend_configured() {
        // The OS sandbox already contains the target's writes.
        return Ok(None);
    }
    let guard = IsolationGuard::new()
        .map_err(|e| format!("failed to create host-write isolation directory: {e}"))?;
    Ok(Some(guard))
}

/// Owns the throwaway host-write isolation directory for one command invocation.
///
/// While alive, `SHATTER_HOST_WRITE_DIR` is exported into the process
/// environment so spawned frontends inherit it, and [`Self::path`] is applied as
/// each frontend's working directory. Dropping the guard removes the directory.
pub(crate) struct IsolationGuard {
    /// The throwaway directory. Held only for its `Drop`, which removes the
    /// directory when the command finishes; never read directly (the path
    /// crosses into `frontend_config` via `SHATTER_HOST_WRITE_DIR`, not this
    /// field).
    _dir: tempfile::TempDir,
}

impl IsolationGuard {
    /// Create the isolation directory and export its path via
    /// `SHATTER_HOST_WRITE_DIR` so child frontends inherit it.
    ///
    /// # Safety
    ///
    /// Mutates the process environment via `std::env::set_var`. Called once from
    /// `main` before any frontend subprocess is spawned and before any
    /// concurrent environment reads, which is the safe window for this on the
    /// 2024 edition.
    pub(crate) fn new() -> std::io::Result<Self> {
        let dir = tempfile::Builder::new()
            .prefix(&format!("shatter-hostwrite-{}-", std::process::id()))
            .tempdir()?;
        // SAFETY: set before spawning any frontend / worker that reads the env.
        unsafe {
            std::env::set_var(ISOLATION_DIR_ENV, dir.path());
        }
        Ok(Self { _dir: dir })
    }
}

impl Drop for IsolationGuard {
    fn drop(&mut self) {
        // Clear the exported path so nothing observes a stale isolation dir
        // after the throwaway directory is removed. The directory itself is
        // removed by `TempDir`'s own `Drop` immediately after this runs.
        //
        // SAFETY: runs during single-threaded command teardown in `main`, after
        // every frontend subprocess has been joined; no concurrent env readers.
        unsafe {
            std::env::remove_var(ISOLATION_DIR_ENV);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Serialize env-var mutation across these tests; the process environment is
    /// global shared state.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    struct EnvReset {
        backend: Option<String>,
        allow: Option<String>,
    }
    impl EnvReset {
        fn capture() -> Self {
            Self {
                backend: std::env::var(SANDBOX_BACKEND_ENV).ok(),
                allow: std::env::var(ALLOW_HOST_WRITES_ENV).ok(),
            }
        }
    }
    impl Drop for EnvReset {
        fn drop(&mut self) {
            // SAFETY: test-only; guarded by ENV_LOCK, no concurrent readers.
            unsafe {
                match &self.backend {
                    Some(v) => std::env::set_var(SANDBOX_BACKEND_ENV, v),
                    None => std::env::remove_var(SANDBOX_BACKEND_ENV),
                }
                match &self.allow {
                    Some(v) => std::env::set_var(ALLOW_HOST_WRITES_ENV, v),
                    None => std::env::remove_var(ALLOW_HOST_WRITES_ENV),
                }
            }
        }
    }

    fn set(key: &str, value: Option<&str>) {
        // SAFETY: test-only; guarded by ENV_LOCK, no concurrent readers.
        unsafe {
            match value {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
    }

    #[test]
    fn denies_without_flag_or_sandbox() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _reset = EnvReset::capture();
        set(SANDBOX_BACKEND_ENV, None);
        set(ALLOW_HOST_WRITES_ENV, None);
        assert!(!execution_permitted(false));
        assert!(ensure_execution_permitted(false).is_err());
    }

    #[test]
    fn flag_permits() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _reset = EnvReset::capture();
        set(SANDBOX_BACKEND_ENV, None);
        set(ALLOW_HOST_WRITES_ENV, None);
        assert!(execution_permitted(true));
        assert!(ensure_execution_permitted(true).is_ok());
    }

    #[test]
    fn env_opt_in_permits() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _reset = EnvReset::capture();
        set(SANDBOX_BACKEND_ENV, None);
        for truthy in ["1", "true", "YES", "on"] {
            set(ALLOW_HOST_WRITES_ENV, Some(truthy));
            assert!(execution_permitted(false), "{truthy} should permit");
        }
        set(ALLOW_HOST_WRITES_ENV, Some("0"));
        assert!(!execution_permitted(false));
        set(ALLOW_HOST_WRITES_ENV, Some(""));
        assert!(!execution_permitted(false));
    }

    #[test]
    fn configured_sandbox_permits() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _reset = EnvReset::capture();
        set(ALLOW_HOST_WRITES_ENV, None);
        set(SANDBOX_BACKEND_ENV, Some("docker"));
        assert!(sandbox_backend_configured());
        assert!(execution_permitted(false));
        set(SANDBOX_BACKEND_ENV, Some("bwrap"));
        assert!(execution_permitted(false));
        // "none"/empty do not count as a configured sandbox.
        set(SANDBOX_BACKEND_ENV, Some("none"));
        assert!(!sandbox_backend_configured());
        assert!(!execution_permitted(false));
    }

    #[test]
    fn refusal_message_names_remedies() {
        let msg = refusal_message();
        assert!(msg.contains(SANDBOX_BACKEND_ENV));
        assert!(msg.contains("--allow-host-writes"));
        assert!(msg.contains(ALLOW_HOST_WRITES_ENV));
    }
}
