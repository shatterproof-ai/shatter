use std::collections::HashMap;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use shatter_core::cache::BehaviorMapCache;
use shatter_core::discovery::Language as DiscoveryLanguage;
use shatter_core::frontend::{Frontend, FrontendConfig};
use shatter_core::log_level::LogLevel;

use crate::args::Language;

/// Lower bound on resolved parallelism. Avoids sub-saturating small machines and
/// keeps benchmarks comparable across hosts.
pub(crate) const PARALLELISM_FLOOR: usize = 4;

/// Upper bound on resolved parallelism. Each frontend may shell out to a
/// multi-process toolchain (e.g. `go build` with `GOMAXPROCS=nproc`); without a
/// ceiling, large hosts fork-bomb themselves into OOM. See str-eam2.
pub(crate) const PARALLELISM_CEILING: usize = 16;

/// Default observer-pool size when neither the CLI nor `shatter.config.json`
/// supplies a value (str-frc.6). `1` keeps the legacy single-process random
/// exploration path so behavior is identical to pre-str-frc.3 runs.
pub(crate) const DEFAULT_OBSERVER_POOL_SIZE: usize = 1;

/// Resolve the observer-pool size for an explore run with CLI > config >
/// built-in default precedence. Returns at least `1` so the random explorer
/// never sees a zero-sized pool. Matches the resolution pattern used for
/// `parallelism` / `parallelism_min` / `parallelism_max` in the scan command.
pub(crate) fn resolve_observer_pool(
    cli_value: Option<usize>,
    config_value: Option<usize>,
) -> usize {
    cli_value
        .or(config_value)
        .unwrap_or(DEFAULT_OBSERVER_POOL_SIZE)
        .max(1)
}

/// Resolve the candidate-queue capacity override with CLI > config >
/// built-in default precedence. Returns `None` when neither side supplies a
/// value so the explorer falls back to its auto-derived default (str-frc.5).
pub(crate) fn resolve_candidate_queue_capacity(
    cli_value: Option<usize>,
    config_value: Option<usize>,
) -> Option<usize> {
    cli_value.or(config_value)
}

/// Cap injected as `GOMAXPROCS` into the Go frontend's environment. The Go
/// frontend invokes `go build` to compile the wrapper; that toolchain run
/// defaults to `GOMAXPROCS=nproc`, so N concurrent Go frontends each spawn
/// their own `nproc`-wide toolchain and exhaust CPU/memory on large hosts.
/// Capping at 2 keeps each toolchain compact without changing Shatter-level
/// parallelism (which is governed by `PARALLELISM_FLOOR`/`_CEILING`). See
/// str-ovs6 for the kapow-scan blowup that motivated this cap.
pub(crate) const GO_FRONTEND_GOMAXPROCS: &str = "2";

/// Effective floor/ceiling for the parallelism clamp.
///
/// Defaults to `[PARALLELISM_FLOOR, PARALLELISM_CEILING]` (str-eam2). Users on
/// tiny CI runners or large dedicated machines can widen the range via
/// `--parallelism-min` / `--parallelism-max` flags or matching
/// `parallelism_min` / `parallelism_max` keys in `shatter.config.json`
/// (str-v01r).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ParallelismBounds {
    pub(crate) floor: usize,
    pub(crate) ceiling: usize,
}

impl ParallelismBounds {
    /// Built-in defaults from str-eam2: `[PARALLELISM_FLOOR, PARALLELISM_CEILING]`.
    #[allow(dead_code)] // used by tests and the str-eam2 fallback path
    pub(crate) const fn defaults() -> Self {
        Self {
            floor: PARALLELISM_FLOOR,
            ceiling: PARALLELISM_CEILING,
        }
    }

    /// Resolve effective bounds from optional user overrides. `None` falls
    /// back to the built-in default for that side.
    ///
    /// Returns an error when the resolved range is empty (`min > max`) or
    /// non-positive (`min == 0` or `max == 0`).
    pub(crate) fn from_overrides(
        min_override: Option<usize>,
        max_override: Option<usize>,
    ) -> Result<Self, String> {
        let floor = min_override.unwrap_or(PARALLELISM_FLOOR);
        let ceiling = max_override.unwrap_or(PARALLELISM_CEILING);
        if floor == 0 {
            return Err(
                "parallelism floor must be at least 1 (got --parallelism-min 0)".to_string(),
            );
        }
        if ceiling == 0 {
            return Err(
                "parallelism ceiling must be at least 1 (got --parallelism-max 0)".to_string(),
            );
        }
        if floor > ceiling {
            return Err(format!(
                "parallelism floor ({floor}) cannot exceed ceiling ({ceiling}); \
                 check --parallelism-min / --parallelism-max"
            ));
        }
        Ok(Self { floor, ceiling })
    }
}

/// Resolve the effective parallelism using the built-in default bounds.
/// Thin wrapper over [`resolve_parallelism_with_bounds`] for callers that do
/// not honor the str-v01r override flags.
#[allow(dead_code)] // retained as a default-bounds shorthand and used by tests
pub(crate) fn resolve_parallelism(requested: usize) -> usize {
    resolve_parallelism_with_bounds(requested, ParallelismBounds::defaults())
}

/// Resolve effective parallelism using caller-supplied bounds (str-v01r).
///
/// `requested == 0` means "auto-detect": query `available_parallelism()` and
/// clamp into `[bounds.floor, bounds.ceiling]` so default behavior remains
/// predictable across hosts.
///
/// An explicit non-zero `requested` honors the user's intent — the floor is
/// NOT applied (the floor exists to keep auto-detected defaults comparable
/// across hosts, not to override an explicit request). The ceiling still
/// applies because it guards against fork-bombing the host via per-worker
/// toolchain subprocesses (see `PARALLELISM_CEILING`). A warning is logged
/// only when the ceiling actually clamps the value (str-p2rz).
pub(crate) fn resolve_parallelism_with_bounds(
    requested: usize,
    bounds: ParallelismBounds,
) -> usize {
    if requested == 0 {
        let detected = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        return detected.clamp(bounds.floor, bounds.ceiling);
    }
    if requested > bounds.ceiling {
        log::warn!(
            "--parallelism {requested} clamped to {ceiling} (ceiling [{ceiling}] caps per-worker \
             toolchain fan-out; raise with --parallelism-max)",
            ceiling = bounds.ceiling,
        );
        return bounds.ceiling;
    }
    requested
}

/// Per-language ceiling on auto-detected parallelism. Multi-process toolchains
/// (Go's `go build`, Rust's `cargo`) consume substantially more host resources
/// per worker than a single-process Node frontend, so the auto-detected
/// default must be capped tighter when these languages participate.
///
/// Explicit `--parallelism N` requests are NOT capped by this table — only the
/// auto-detect path is — so power users can opt past the per-language default
/// up to the global ceiling.
pub(crate) const TS_AUTODETECT_CAP: usize = usize::MAX;
pub(crate) const GO_AUTODETECT_CAP: usize = 8;
pub(crate) const RUST_AUTODETECT_CAP: usize = 8;

/// Per-language autodetect cap for a single language. See `TS_AUTODETECT_CAP`
/// et al. for rationale.
fn language_autodetect_cap(lang: DiscoveryLanguage) -> usize {
    match lang {
        DiscoveryLanguage::TypeScript => TS_AUTODETECT_CAP,
        DiscoveryLanguage::Go => GO_AUTODETECT_CAP,
        DiscoveryLanguage::Rust => RUST_AUTODETECT_CAP,
    }
}

/// Worst-case (minimum) per-language autodetect cap across `needed_langs`.
/// An empty set yields `usize::MAX` (no per-language cap), preserving the
/// pre-str-qp31 behavior when language detection has not yet happened.
pub(crate) fn per_language_autodetect_cap<'a, I>(needed_langs: I) -> usize
where
    I: IntoIterator<Item = &'a DiscoveryLanguage>,
{
    needed_langs
        .into_iter()
        .map(|l| language_autodetect_cap(*l))
        .min()
        .unwrap_or(usize::MAX)
}

/// Resolve effective parallelism for a scan, taking the participating
/// languages into account (str-qp31) and honoring user-supplied bounds
/// (str-v01r).
///
/// For `requested == 0` (auto-detect): take `available_parallelism()`, apply
/// the per-language cap (worst-case-wins for mixed-language scans), then apply
/// the global `[bounds.floor, bounds.ceiling]` clamp.
///
/// For an explicit non-zero `requested`: only the global clamp applies — the
/// per-language table governs the *default*, not user-supplied values.
pub(crate) fn resolve_parallelism_for_langs<'a, I>(
    requested: usize,
    needed_langs: I,
    bounds: ParallelismBounds,
) -> usize
where
    I: IntoIterator<Item = &'a DiscoveryLanguage>,
{
    if requested == 0 {
        let detected = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        let lang_cap = per_language_autodetect_cap(needed_langs);
        let after_lang_cap = detected.min(lang_cap);
        return after_lang_cap.clamp(bounds.floor, bounds.ceiling);
    }
    resolve_parallelism_with_bounds(requested, bounds)
}

/// Resolve the project root: explicit `project_dir` wins, otherwise auto-detect from `reference_path`.
pub(crate) fn resolve_project_root(
    project_dir: Option<&Path>,
    reference_path: &Path,
) -> Option<String> {
    if let Some(dir) = project_dir {
        Some(dir.to_string_lossy().into_owned())
    } else {
        shatter_core::project::detect_project_root(reference_path)
            .map(|r| r.path.to_string_lossy().into_owned())
    }
}

/// Strip `root` prefix from `path` to produce a relative path string.
/// Falls back to the full path if stripping fails.
pub(crate) fn relativize_path(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

/// Terminal color support.
pub(crate) struct Colors {
    pub(crate) bold: &'static str,
    pub(crate) dim: &'static str,
    pub(crate) reset: &'static str,
}

impl Colors {
    pub(crate) fn new(use_color: bool) -> Self {
        if use_color {
            Colors {
                bold: "\x1b[1m",
                dim: "\x1b[2m",
                reset: "\x1b[0m",
            }
        } else {
            Colors {
                bold: "",
                dim: "",
                reset: "",
            }
        }
    }
}

/// Maximum number of `WouldBlock` retries before a stdout write is treated
/// as fatal. Each retry yields briefly to let the consumer drain. This
/// covers the common case where a PTY or `tee` pipe is momentarily full;
/// a stuck consumer is bounded by `STDOUT_WRITE_RETRY_BUDGET` × the
/// backoff sleep below.
const STDOUT_WRITE_RETRY_BUDGET: usize = 1024;

/// Write `buf` to `w` in full, retrying on `WouldBlock` and `Interrupted`.
///
/// Returns `Ok(())` once every byte is accepted, or an `io::Error` for any
/// other failure (including the retry budget being exhausted, which
/// surfaces as the last `WouldBlock` error).
///
/// Unlike `Write::write_all`, this tolerates a nonblocking stdout — the
/// default `std::io::stdout()` lock panics on `WouldBlock`, which is the
/// root cause of the EAGAIN panic during large report rendering when the
/// CLI is run under a PTY or piped through `tee` (str-jeen.62).
pub(crate) fn write_all_resilient<W: Write>(w: &mut W, mut buf: &[u8]) -> io::Result<()> {
    let mut retries: usize = 0;
    while !buf.is_empty() {
        match w.write(buf) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "failed to write whole buffer",
                ));
            }
            Ok(n) => {
                buf = &buf[n..];
                retries = 0;
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                if retries >= STDOUT_WRITE_RETRY_BUDGET {
                    return Err(e);
                }
                retries += 1;
                // Brief yield: lets the kernel drain the pipe / the PTY
                // consumer catch up without busy-spinning.
                thread::sleep(Duration::from_micros(100));
            }
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// Write `s` to stdout with EAGAIN tolerance and clean BrokenPipe exit.
///
/// `BrokenPipe` exits the process with status 0 — the consumer is gone
/// and we have nothing useful left to do, which matches the behavior of
/// standard Unix CLIs like `head`/`less`. Any other I/O error is reported
/// to stderr and the process exits with status 1. `WouldBlock` is
/// retried via [`write_all_resilient`].
pub(crate) fn print_stdout(s: &str) {
    let stdout = io::stdout();
    let mut lock = stdout.lock();
    match write_all_resilient(&mut lock, s.as_bytes()) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => {
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("Error writing to stdout: {e}");
            std::process::exit(1);
        }
    }
}

/// Print Markdown to stdout, rendered with termimad formatting when `use_color` is true.
///
/// Both branches route through [`print_stdout`] so a nonblocking or closed
/// stdout surfaces as a controlled CLI exit rather than a Rust panic
/// inside the `print!` / `termimad::print_text` macros.
pub(crate) fn print_markdown(md: &str, use_color: bool) {
    if use_color {
        // Render to a String via the FmtText `Display` impl so we control
        // the write path. `termimad::print_text` writes to stdout
        // internally and would panic on EAGAIN.
        let rendered = format!("{}", termimad::term_text(md));
        print_stdout(&rendered);
    } else {
        print_stdout(md);
    }
}

/// Check for a custom-built frontend binary at `.shatter-cache/bin/shatter-{lang}-custom`.
///
/// Also checks project-local `.shatter-cache/bin/` and legacy `.shatter/bin/`
/// when a project `.shatter` directory is provided.
pub(crate) fn find_custom_binary(shatter_dir: Option<&Path>, lang: &str) -> Option<PathBuf> {
    let binary_name = format!("shatter-{lang}-custom");
    let cache_bin = PathBuf::from(".shatter-cache")
        .join("bin")
        .join(&binary_name);
    if cache_bin.is_file() {
        return Some(cache_bin);
    }

    if let Some(shatter_dir) = shatter_dir {
        if let Some(project_root) = shatter_dir.parent() {
            let project_cache_bin = project_root
                .join(".shatter-cache")
                .join("bin")
                .join(&binary_name);
            if project_cache_bin.is_file() {
                return Some(project_cache_bin);
            }
        }

        let legacy_bin = shatter_dir.join("bin").join(&binary_name);
        if legacy_bin.is_file() {
            return Some(legacy_bin);
        }
    }

    None
}

/// Search PATH for a binary by name, returning the first match.
pub(crate) fn find_on_path(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    std::env::split_paths(&path_var)
        .map(|dir| dir.join(name))
        .find(|p| p.is_file())
}

/// Install hint surfaced when the Rust frontend cannot be found at any of the
/// search locations. Kept in one place so discovery-time precheck and
/// spawn-time errors emit the same actionable message (str-bnsw, str-difh).
///
/// The message frames this as the EXPECTED state after a workspace-root
/// `cargo build --release --bin shatter` (which intentionally only builds the
/// main CLI) so users do not read it as a broken install, and gives the exact
/// commands and binary placement needed to enable Rust scans.
pub(crate) const RUST_FRONTEND_INSTALL_HINT: &str =
    "this is the expected state after `cargo build --release --bin shatter` from the \
     workspace root, which only builds the main CLI. To enable Rust scans, build the Rust \
     frontend with `cargo build --manifest-path shatter-rust/Cargo.toml` (add --release for \
     an optimized binary) and either run shatter from the workspace root (so \
     shatter-rust/target/debug/shatter-rust or shatter-rust/target/release/shatter-rust is \
     auto-discovered) or install it on PATH with `cargo install --path shatter-rust`. See \
     README.md `Build from source` for full setup details";

/// Per-target status code emitted when a target file is skipped because its
/// language frontend is unavailable on this host (str-jeen.13).
///
/// Surfaced as a `STATUS skipped_by_unavailable_frontend ...` stderr line so
/// broad-run wrappers (Kapow re-run, etc.) can classify the row as
/// environmental rather than as a hard target failure. The line shape is
/// intentionally machine-parseable: space-separated `key=value` pairs with no
/// quoting. Values do not contain spaces (paths are absolutized; the install
/// hint is collapsed into a quote-free phrase).
pub(crate) const STATUS_SKIPPED_BY_UNAVAILABLE_FRONTEND: &str = "skipped_by_unavailable_frontend";

/// Emit one structured per-target status line for a target that was skipped
/// because its language frontend is unavailable. Goes to stderr so that piped
/// stdout reports remain clean.
pub(crate) fn emit_skipped_unavailable_frontend(
    file: &Path,
    language: Language,
    install_hint: &str,
) {
    eprintln!(
        "STATUS {status} language={lang} file={file} hint={hint}",
        status = STATUS_SKIPPED_BY_UNAVAILABLE_FRONTEND,
        lang = language.label(),
        file = file.display(),
        hint = install_hint,
    );
}

/// Whether a language frontend can be located on this host.
///
/// Computed during target discovery (str-bnsw) so that mixed-language scans
/// can skip files for unavailable languages with a clear status, and
/// single-language runs can fail fast before walking the source tree.
#[derive(Debug, Clone)]
pub(crate) enum FrontendAvailability {
    Available,
    Unavailable {
        language: Language,
        install_hint: &'static str,
    },
}

impl FrontendAvailability {
    #[cfg(test)]
    pub(crate) fn is_available(&self) -> bool {
        matches!(self, FrontendAvailability::Available)
    }

    /// User-facing one-line message for the unavailable case. Returns `None`
    /// when the frontend is available.
    pub(crate) fn unavailable_message(&self) -> Option<String> {
        match self {
            FrontendAvailability::Available => None,
            FrontendAvailability::Unavailable {
                language,
                install_hint,
            } => Some(format!(
                "shatter-{} frontend not found: {install_hint}",
                language.label()
            )),
        }
    }
}

/// Check whether the named language frontend is reachable on this host.
///
/// TypeScript and Go ship embedded in the CLI binary, so they are always
/// available. Rust is sourced externally — checked in this order:
/// custom binary (`.shatter-cache/bin/`), `$PATH`, then the conventional
/// `./shatter-rust/target/debug/` and `./target/debug/` build outputs.
pub(crate) fn check_frontend_availability(
    language: Language,
    shatter_dir: Option<&Path>,
) -> FrontendAvailability {
    match language {
        Language::TypeScript | Language::Go => FrontendAvailability::Available,
        Language::Rust => {
            if find_custom_binary(shatter_dir, "rust").is_some()
                || find_on_path("shatter-rust").is_some()
            {
                return FrontendAvailability::Available;
            }
            let candidates = [
                PathBuf::from("./shatter-rust/target/debug/shatter-rust"),
                PathBuf::from("./target/debug/shatter-rust"),
            ];
            if candidates.iter().any(|p| p.is_file()) {
                FrontendAvailability::Available
            } else {
                FrontendAvailability::Unavailable {
                    language: Language::Rust,
                    install_hint: RUST_FRONTEND_INSTALL_HINT,
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn frontend_config(
    language: Language,
    timeout: Duration,
    log_level: LogLevel,
    exec_timeout: u64,
    build_timeout: u64,
    memory_limit: Option<u64>,
    shatter_dir: Option<&Path>,
    timing_enabled: bool,
    release: bool,
) -> Result<FrontendConfig, String> {
    let (command, mut args) = match language {
        Language::TypeScript => {
            let bundle_path = crate::embedded_frontend::ensure_extracted()?;
            (
                PathBuf::from("node"),
                vec![
                    "--no-warnings".to_string(),
                    bundle_path.to_string_lossy().into_owned(),
                ],
            )
        }
        Language::Go => {
            if let Some(custom) = find_custom_binary(shatter_dir, "go") {
                (custom, vec![])
            } else {
                let binary_path = crate::embedded_go_frontend::ensure_extracted()?;
                (binary_path, vec![])
            }
        }
        Language::Rust => {
            if let Some(custom) = find_custom_binary(shatter_dir, "rust") {
                (custom, vec![])
            } else if let Some(path) = find_on_path("shatter-rust") {
                (path, vec![])
            } else {
                // shatter-rust is outside the workspace, so check both locations
                let candidates = [
                    PathBuf::from("./shatter-rust/target/debug/shatter-rust"),
                    PathBuf::from("./target/debug/shatter-rust"),
                ];
                if let Some(path) = candidates.iter().find(|p| p.is_file()) {
                    (path.clone(), vec![])
                } else {
                    return Err(format!(
                        "shatter-rust frontend not found: {RUST_FRONTEND_INSTALL_HINT}"
                    ));
                }
            }
        }
    };

    // Apply memory limit: for TS, --max-old-space-size must come before the script
    if let Some(mb) = memory_limit {
        match language {
            Language::TypeScript => {
                args.insert(0, format!("--max-old-space-size={mb}"));
            }
            Language::Go | Language::Rust => {
                // Go: GOMEMLIMIT is set via env_vars below
                // Rust: no memory limit mechanism yet
            }
        }
    }

    let mut config = FrontendConfig::new(command);
    config.args = args;
    config.request_timeout = timeout;
    apply_frontend_env(&mut config, log_level, exec_timeout, build_timeout, release);
    if timing_enabled {
        config.capabilities.push("timing".to_string());
    }

    if let Some(mb) = memory_limit
        && language == Language::Go
    {
        let bytes = mb * 1024 * 1024;
        config
            .env_vars
            .push(("GOMEMLIMIT".to_string(), format!("{bytes}B")));
    }

    // Cap the inner `go build` toolchain's parallelism so N concurrent Go
    // frontends don't fork-bomb large hosts. See str-ovs6.
    if language == Language::Go {
        config
            .env_vars
            .push(("GOMAXPROCS".to_string(), GO_FRONTEND_GOMAXPROCS.to_string()));
    }

    // str-gg9v: host-write isolation is applied at *target execution* time, not
    // to the frontend process as a whole. Redirecting the frontend's own cwd
    // would break relative source-path resolution during analysis (the frontend
    // resolves the target file against its cwd). Instead the CLI exports
    // `SHATTER_HOST_WRITE_DIR` (see `host_writes::IsolationGuard`), and each
    // frontend redirects only the cwd of the subprocess/context that actually
    // runs the target: the Go launcher (`prepared_launcher.go`), the Rust
    // harness binary (`executor.rs`). The frontend inherits the exported env var
    // automatically, so nothing is threaded through `FrontendConfig` here.

    Ok(config)
}

/// Apply harness storage environment variables to a frontend config.
pub(crate) fn apply_storage_env(
    config: &mut FrontendConfig,
    storage: &shatter_core::harness_storage::HarnessStorage,
) {
    for (key, value) in storage.env_vars() {
        config.env_vars.push((key, value));
    }
}

/// Apply project-scoped harness storage env vars to a frontend config.
///
/// When `project_root` is `Some`, creates a [`HarnessStorage`] with
/// project-scoped cache and artifact directories.  When `None`, the storage
/// roots fall back to temp-based paths (no durable cache).
pub(crate) fn apply_project_storage(config: &mut FrontendConfig, project_root: Option<&str>) {
    if let Some(root) = project_root {
        let storage = project_storage_for_env(Path::new(root));
        apply_storage_env(config, &storage);
    }
}

fn project_storage_for_env(project_root: &Path) -> shatter_core::harness_storage::HarnessStorage {
    use shatter_core::harness_storage::{ENV_HARNESS_CACHE, HarnessStorage};

    let default = HarnessStorage::for_project(project_root);
    let cache_root = std::env::var_os(ENV_HARNESS_CACHE)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| default.cache_root().to_path_buf());

    HarnessStorage::new(
        cache_root,
        default.scratch_root().to_path_buf(),
        default.artifact_root().to_path_buf(),
    )
}

/// Apply harness storage env vars rooted under the OS temp dir, so a
/// frontend session writes its harness cache, scratch, and artifact outputs
/// outside the project tree.
///
/// Used by the scan command when the caller asked for clean external-audit
/// behavior (explicit external `-o` outputs together with `--no-cache
/// --no-seeds`, str-1wcl). The directories are created under
/// the OS tempdir and inherit its normal cleanup policy. Harness cache is
/// stable per current working directory so analysis/execution frontends and
/// repeated external-audit runs can reuse expensive build artifacts without
/// writing to the audited project tree. Scratch and artifact roots remain
/// per-session.
///
/// Also points the Go frontend's workspace root
/// (`SHATTER_GO_WORKSPACE_ROOT`) at a sibling tempdir so the Go frontend's
/// per-package analysis cache and generated harness outputs do not land in
/// `<project>/.shatter-cache/go-workspace/`.
pub(crate) fn apply_external_audit_storage(config: &mut FrontendConfig) {
    use shatter_core::harness_storage::{ENV_HARNESS_CACHE, HarnessStorage};
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let session_id = format!(
        "shatter-audit-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed),
    );
    let base = std::env::temp_dir().join(session_id);
    let cache_root = std::env::var_os(ENV_HARNESS_CACHE)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(external_audit_cache_root);
    let scratch_root = base.join("scratch");
    let artifact_root = base.join("artifacts");
    let go_workspace_root = base.join("go-workspace");

    let storage = HarnessStorage::new(cache_root, scratch_root, artifact_root);
    apply_storage_env(config, &storage);
    config.env_vars.push((
        "SHATTER_GO_WORKSPACE_ROOT".to_string(),
        go_workspace_root.to_string_lossy().into_owned(),
    ));
}

fn external_audit_cache_root() -> PathBuf {
    use std::hash::{Hash, Hasher};

    let identity = std::env::current_dir().unwrap_or_else(|_| std::env::temp_dir());
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    identity.hash(&mut hasher);
    std::env::temp_dir()
        .join("shatter-audit-cache")
        .join(format!("{:016x}", hasher.finish()))
        .join("harness")
}

/// Disable the Go frontend's per-package discovery / analysis cache for
/// this session.
///
/// Mirrored from `--no-cache`: the CLI's help text now claims that flag
/// disables every on-disk cache the scan command touches, including the Go
/// frontend's analysis cache. Setting `SHATTER_DISABLE_ANALYSIS_CACHE=1` on
/// the spawned frontend is what makes that promise true (str-1wcl).
pub(crate) fn disable_frontend_analysis_cache(config: &mut FrontendConfig) {
    config.env_vars.push((
        "SHATTER_DISABLE_ANALYSIS_CACHE".to_string(),
        "1".to_string(),
    ));
}

/// Apply standard environment variables to a frontend config.
pub(crate) fn apply_frontend_env(
    config: &mut FrontendConfig,
    log_level: LogLevel,
    exec_timeout: u64,
    build_timeout: u64,
    release: bool,
) {
    config.env_vars.push((
        LogLevel::ENV_VAR.to_string(),
        log_level.as_str().to_string(),
    ));
    config
        .env_vars
        .push(("SHATTER_EXEC_TIMEOUT".to_string(), exec_timeout.to_string()));
    config.env_vars.push((
        "SHATTER_BUILD_TIMEOUT".to_string(),
        build_timeout.to_string(),
    ));
    if release {
        config
            .env_vars
            .push(("SHATTER_HARNESS_RELEASE".to_string(), "1".to_string()));
    }
}

/// For each function's external dependencies that are NOT in the current file,
/// attempts to load the callee's cached behavior map and extract its stored
/// fingerprint. Returns a map from callee name to deep fingerprint.
pub(crate) fn load_external_fingerprints(
    functions: &[shatter_core::protocol::FunctionAnalysis],
    cache: Option<&BehaviorMapCache>,
) -> std::collections::HashMap<String, String> {
    let mut external_fps = std::collections::HashMap::new();
    let cache = match cache {
        Some(c) => c,
        None => return external_fps,
    };

    let local_names: std::collections::HashSet<&str> =
        functions.iter().map(|f| f.name.as_str()).collect();

    for func in functions {
        for dep in &func.dependencies {
            if local_names.contains(dep.symbol.as_str()) {
                continue;
            }
            if external_fps.contains_key(&dep.symbol) {
                continue;
            }
            if let Ok(Some(cached_map)) = cache.load(&dep.symbol)
                && let Some(fp) = cached_map.fingerprint
            {
                external_fps.insert(dep.symbol.clone(), fp);
            }
        }
    }

    external_fps
}

/// Build a [`MetaConfig`] from CLI flags, applying overrides on top of defaults.
pub(crate) fn build_meta_config(
    no_adaptive: bool,
    score_window: Option<usize>,
    cold_start: Option<u64>,
    strategy_floor: Option<f64>,
    strategy_weights: Option<&str>,
) -> Result<shatter_core::strategy::MetaConfig, Box<dyn std::error::Error>> {
    let mut config = shatter_core::config::ExplorationConfig::default();
    if no_adaptive {
        config.adaptive = false;
    }
    if let Some(w) = score_window {
        config.score_window = w;
    }
    if let Some(c) = cold_start {
        config.cold_start = c;
    }
    if let Some(f) = strategy_floor {
        config.strategy_floor = f;
    }
    if let Some(weights_str) = strategy_weights {
        config.strategy_weights =
            Some(shatter_core::config::ExplorationConfig::parse_strategy_weights(weights_str)?);
    }
    Ok(config.to_meta_config())
}

/// Map discovery Language to CLI Language for frontend_config.
pub(crate) fn discovery_lang_to_cli_lang(lang: DiscoveryLanguage) -> Option<Language> {
    match lang {
        DiscoveryLanguage::TypeScript => Some(Language::TypeScript),
        DiscoveryLanguage::Go => Some(Language::Go),
        DiscoveryLanguage::Rust => Some(Language::Rust),
    }
}

/// Shutdown all frontends in a map.
pub(crate) async fn shutdown_all_frontends(frontends: HashMap<DiscoveryLanguage, Frontend>) {
    for (_, frontend) in frontends {
        if let Err(e) = frontend.shutdown().await {
            log::warn!("frontend shutdown error: {e}");
        }
    }
}

pub(crate) async fn shutdown_frontend(frontend: Frontend) {
    if let Err(e) = frontend.shutdown().await {
        log::warn!("frontend shutdown error: {e}");
    }
}

/// Default max-iterations when the user does not provide `--max-iterations`.
pub(crate) const DEFAULT_MAX_ITERATIONS: u32 = 100;
/// Default total-timeout (seconds) when the user does not provide `--timeout`.
pub(crate) const DEFAULT_TIMEOUT: u64 = 60;

/// Resolved exploration budget, accounting for MC/DC multipliers.
pub(crate) struct ResolvedBudgets {
    /// Effective max-iterations. `None` means unbounded (run until timeout/interrupt).
    pub max_iterations: Option<u32>,
    /// Effective wall-clock timeout in seconds (user value or MC/DC-scaled default).
    pub timeout: u64,
    /// Effective per-query solver timeout in seconds (user value, or 10s under MC/DC, or None).
    pub solver_timeout: Option<u64>,
}

/// Resolve exploration budgets from optional user-provided values, applying MC/DC
/// multipliers to any parameter the user did not explicitly set.
///
/// When `mcdc` is true and a parameter is `None` (not user-provided), the MC/DC
/// default is used (5× for iterations, 5× for timeout, 10 s for solver timeout).
/// When a parameter is `Some`, the user-provided value is used unchanged.
/// When `mcdc` is false and `max_iterations` is `None`, returns
/// `Some(DEFAULT_MAX_ITERATIONS)` (bounded by default).
///
/// Pass `--max-iterations 0` to opt into unbounded exploration.
pub(crate) fn resolve_mcdc_budgets(
    max_iterations: Option<u32>,
    timeout: Option<u64>,
    solver_timeout: Option<u64>,
    mcdc: bool,
) -> ResolvedBudgets {
    ResolvedBudgets {
        max_iterations: match max_iterations {
            Some(0) => None, // explicit opt-in to unbounded
            Some(n) => Some(n),
            None if mcdc => Some(DEFAULT_MAX_ITERATIONS * 5),
            None => Some(DEFAULT_MAX_ITERATIONS),
        },
        timeout: timeout.unwrap_or(if mcdc {
            DEFAULT_TIMEOUT * 5
        } else {
            DEFAULT_TIMEOUT
        }),
        solver_timeout: if mcdc && solver_timeout.is_none() {
            Some(10)
        } else {
            solver_timeout
        },
    }
}

#[cfg(test)]
mod stdout_write_tests {
    //! Regression coverage for str-jeen.62: large scan report rendering
    //! must not panic when stdout returns `WouldBlock` (PTY / `tee` /
    //! nonblocking pipe) and must not panic on `BrokenPipe`.
    use super::*;
    use std::io::{self, Write};

    /// A `Write` that returns `WouldBlock` for the first `n` calls, then
    /// accepts writes normally into `sink`.
    struct FlakyWriter {
        remaining_wouldblock: usize,
        sink: Vec<u8>,
    }

    impl Write for FlakyWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            if self.remaining_wouldblock > 0 {
                self.remaining_wouldblock -= 1;
                return Err(io::Error::new(io::ErrorKind::WouldBlock, "would block"));
            }
            self.sink.extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// A `Write` that always returns `BrokenPipe`.
    struct BrokenPipeWriter;

    impl Write for BrokenPipeWriter {
        fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(io::ErrorKind::BrokenPipe, "pipe closed"))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn write_all_resilient_retries_on_wouldblock() {
        let payload = b"large report payload";
        let mut w = FlakyWriter {
            remaining_wouldblock: 5,
            sink: Vec::new(),
        };
        write_all_resilient(&mut w, payload).expect("should succeed after retries");
        assert_eq!(w.sink, payload, "full payload must reach the sink");
    }

    #[test]
    fn write_all_resilient_surfaces_broken_pipe() {
        let mut w = BrokenPipeWriter;
        let err = write_all_resilient(&mut w, b"x")
            .expect_err("BrokenPipe must surface as Err so print_stdout can exit cleanly");
        assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);
    }

    #[test]
    fn write_all_resilient_handles_large_chunked_payload() {
        // Simulate a multi-KB report where every other write attempt
        // returns WouldBlock — the realistic PTY-pressure case.
        struct AlternatingWriter {
            block_next: bool,
            sink: Vec<u8>,
        }
        impl Write for AlternatingWriter {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                if self.block_next {
                    self.block_next = false;
                    return Err(io::Error::new(io::ErrorKind::WouldBlock, "block"));
                }
                self.block_next = true;
                // Only accept a small chunk at a time, like a real pipe.
                let take = buf.len().min(64);
                self.sink.extend_from_slice(&buf[..take]);
                Ok(take)
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let payload: Vec<u8> = (0..4096).map(|i| (i % 251) as u8).collect();
        let mut w = AlternatingWriter {
            block_next: true,
            sink: Vec::new(),
        };
        write_all_resilient(&mut w, &payload).expect("chunked + WouldBlock must succeed");
        assert_eq!(w.sink, payload);
    }
}

#[cfg(test)]
mod mcdc_budget_tests {
    use super::*;

    #[test]
    fn mcdc_default_budgets_are_scaled() {
        let b = resolve_mcdc_budgets(None, None, None, true);
        assert_eq!(
            b.max_iterations,
            Some(DEFAULT_MAX_ITERATIONS * 5),
            "max_iterations should be 5x"
        );
        assert_eq!(b.timeout, DEFAULT_TIMEOUT * 5, "timeout should be 5x");
        assert_eq!(
            b.solver_timeout,
            Some(10),
            "solver_timeout should default to 10s under mcdc"
        );
    }

    #[test]
    fn non_mcdc_default_budgets_are_bounded() {
        let b = resolve_mcdc_budgets(None, None, None, false);
        assert_eq!(
            b.max_iterations,
            Some(DEFAULT_MAX_ITERATIONS),
            "no --max-iterations defaults to DEFAULT_MAX_ITERATIONS"
        );
        assert_eq!(b.timeout, DEFAULT_TIMEOUT);
        assert_eq!(b.solver_timeout, None);
    }

    #[test]
    fn zero_max_iterations_means_unbounded() {
        let b = resolve_mcdc_budgets(Some(0), None, None, false);
        assert_eq!(b.max_iterations, None, "--max-iterations 0 means unbounded");
    }

    #[test]
    fn user_provided_values_override_mcdc_defaults() {
        let b = resolve_mcdc_budgets(Some(42), Some(30), Some(5), true);
        assert_eq!(
            b.max_iterations,
            Some(42),
            "user-provided max_iterations must not be multiplied"
        );
        assert_eq!(
            b.timeout, 30,
            "user-provided timeout must not be multiplied"
        );
        assert_eq!(
            b.solver_timeout,
            Some(5),
            "user-provided solver_timeout must not be changed"
        );
    }

    #[test]
    fn partial_user_override_with_mcdc() {
        // User provides max_iterations but not timeout or solver_timeout
        let b = resolve_mcdc_budgets(Some(200), None, None, true);
        assert_eq!(b.max_iterations, Some(200), "user value wins");
        assert_eq!(
            b.timeout,
            DEFAULT_TIMEOUT * 5,
            "unspecified timeout gets mcdc scaling"
        );
        assert_eq!(
            b.solver_timeout,
            Some(10),
            "unspecified solver_timeout gets mcdc default"
        );
    }
}

#[cfg(test)]
mod cli_parity_tests {
    use clap::Parser;

    use super::*;
    use crate::args::{Cli, CliCommand, ExploreArgs, ScanArgs};

    /// CLI parity contract: the canonical list of environment variables the CLI must
    /// set for every frontend invocation, with their expected default values when the
    /// user does not provide the corresponding flag.
    ///
    /// Governed commands: `explore`, `scan`, and other frontend-spawning subcommands
    /// that do not have intentionally elevated defaults (e.g. `observe` uses 30s/60s
    /// because it executes many inputs in a single session — that divergence is
    /// documented in PARITY.md).
    const GOVERNED_ENV_VARS: &[&str] = &[
        "SHATTER_LOG_LEVEL",
        "SHATTER_EXEC_TIMEOUT",
        "SHATTER_BUILD_TIMEOUT",
    ];
    /// Canonical CLI default for `--exec-timeout` (seconds) across governed commands.
    const CLI_EXEC_TIMEOUT_DEFAULT_SECS: u64 = 10;
    /// Canonical CLI default for `--build-timeout` (seconds) across governed commands.
    const CLI_BUILD_TIMEOUT_DEFAULT_SECS: u64 = 30;
    /// Canonical CLI default for `--log-level`.
    const CLI_LOG_LEVEL_DEFAULT: &str = "info";
    static HARNESS_CACHE_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    static CWD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Every governed env var must appear in the env_vars vector produced by
    /// `apply_frontend_env`. This is the minimal contract: if a var is missing,
    /// the frontend never receives it regardless of what the CLI flag says.
    #[test]
    fn apply_frontend_env_sets_all_governed_vars() {
        let mut config = FrontendConfig::new(std::path::PathBuf::from("dummy"));
        apply_frontend_env(
            &mut config,
            LogLevel::Info,
            CLI_EXEC_TIMEOUT_DEFAULT_SECS,
            CLI_BUILD_TIMEOUT_DEFAULT_SECS,
            false,
        );
        let keys: std::collections::HashSet<&str> =
            config.env_vars.iter().map(|(k, _)| k.as_str()).collect();
        for var in GOVERNED_ENV_VARS {
            assert!(
                keys.contains(var),
                "apply_frontend_env must set governed env var {var} — \
                 add it to apply_frontend_env() in helpers.rs"
            );
        }
    }

    /// The governed env vars must carry the correct values matching the contract
    /// constants, not arbitrary defaults.
    #[test]
    fn apply_frontend_env_values_match_contract_defaults() {
        let mut config = FrontendConfig::new(std::path::PathBuf::from("dummy"));
        apply_frontend_env(
            &mut config,
            LogLevel::Info,
            CLI_EXEC_TIMEOUT_DEFAULT_SECS,
            CLI_BUILD_TIMEOUT_DEFAULT_SECS,
            false,
        );
        let env_map: std::collections::HashMap<&str, &str> = config
            .env_vars
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();

        assert_eq!(
            env_map.get("SHATTER_LOG_LEVEL").copied(),
            Some(CLI_LOG_LEVEL_DEFAULT),
            "SHATTER_LOG_LEVEL default must be {CLI_LOG_LEVEL_DEFAULT}"
        );
        assert_eq!(
            env_map.get("SHATTER_EXEC_TIMEOUT").copied(),
            Some(CLI_EXEC_TIMEOUT_DEFAULT_SECS.to_string().as_str()),
            "SHATTER_EXEC_TIMEOUT default must be {CLI_EXEC_TIMEOUT_DEFAULT_SECS}"
        );
        assert_eq!(
            env_map.get("SHATTER_BUILD_TIMEOUT").copied(),
            Some(CLI_BUILD_TIMEOUT_DEFAULT_SECS.to_string().as_str()),
            "SHATTER_BUILD_TIMEOUT default must be {CLI_BUILD_TIMEOUT_DEFAULT_SECS}"
        );
    }

    /// The `explore` subcommand must expose `--exec-timeout` and `--build-timeout`
    /// with the governed defaults. If a future edit changes the default_value_t,
    /// this test fails and forces a PARITY.md update.
    #[test]
    fn explore_defaults_match_parity_contract() {
        let cli = Cli::parse_from(["shatter", "explore", "dummy.ts"]);
        match cli.command {
            CliCommand::Explore(__args) => {
            let ExploreArgs {
                exec_timeout,
                build_timeout,
                ..
            } = *__args;

                assert_eq!(
                    exec_timeout, CLI_EXEC_TIMEOUT_DEFAULT_SECS,
                    "`explore --exec-timeout` default ({exec_timeout}s) diverges from \
                     parity contract ({CLI_EXEC_TIMEOUT_DEFAULT_SECS}s); \
                     update the contract constant or restore the arg default"
                );
                assert_eq!(
                    build_timeout, CLI_BUILD_TIMEOUT_DEFAULT_SECS,
                    "`explore --build-timeout` default ({build_timeout}s) diverges from \
                     parity contract ({CLI_BUILD_TIMEOUT_DEFAULT_SECS}s); \
                     update the contract constant or restore the arg default"
                );
            }
            _ => panic!("expected Explore command"),
        }
    }

    /// The `scan` subcommand must expose the same governed defaults as `explore`.
    /// exec_timeout is Option<u64> (resolved via project config chain); when
    /// no flag is passed, the built-in default in shatter-core matches the
    /// parity contract.
    #[test]
    fn scan_defaults_match_parity_contract() {
        let cli = Cli::parse_from(["shatter", "scan", "src/"]);
        match cli.command {
            CliCommand::Scan(__args) => {
            let ScanArgs {
                exec_timeout,
                build_timeout,
                ..
            } = *__args;

                // exec_timeout is None when not explicitly passed; the built-in
                // default (resolved at runtime) matches the parity contract.
                assert_eq!(
                    exec_timeout.unwrap_or(shatter_core::config::DEFAULT_SCAN_EXEC_TIMEOUT),
                    CLI_EXEC_TIMEOUT_DEFAULT_SECS,
                    "`scan --exec-timeout` resolved default diverges from \
                     parity contract ({CLI_EXEC_TIMEOUT_DEFAULT_SECS}s)"
                );
                assert_eq!(
                    build_timeout, CLI_BUILD_TIMEOUT_DEFAULT_SECS,
                    "`scan --build-timeout` default ({build_timeout}s) diverges from \
                     parity contract ({CLI_BUILD_TIMEOUT_DEFAULT_SECS}s)"
                );
            }
            _ => panic!("expected Scan command"),
        }
    }

    /// Every language frontend config must include all governed env vars.
    /// Tests TypeScript and Go (Rust frontend requires the binary on PATH so is
    /// skipped here; its env-var handling is tested in shatter-rust unit tests).
    #[test]
    fn frontend_config_propagates_all_governed_vars() {
        for lang in [Language::TypeScript, Language::Go] {
            let config = frontend_config(
                lang,
                shatter_core::frontend::DEFAULT_REQUEST_TIMEOUT,
                LogLevel::Info,
                CLI_EXEC_TIMEOUT_DEFAULT_SECS,
                CLI_BUILD_TIMEOUT_DEFAULT_SECS,
                None,
                None,
                false,
                false,
            )
            .unwrap_or_else(|e| panic!("frontend_config({lang:?}) failed: {e}"));

            let keys: std::collections::HashSet<&str> =
                config.env_vars.iter().map(|(k, _)| k.as_str()).collect();
            for var in GOVERNED_ENV_VARS {
                assert!(
                    keys.contains(var),
                    "frontend_config({lang:?}) must propagate governed env var {var}"
                );
            }
        }
    }

    #[test]
    fn find_custom_binary_prefers_cwd_cache() {
        let _guard = CWD_LOCK.lock().unwrap();
        let original_cwd = std::env::current_dir().unwrap();
        let temp_dir = tempfile::tempdir().unwrap();
        let cache_dir = temp_dir.path().join(".shatter-cache").join("bin");
        std::fs::create_dir_all(&cache_dir).unwrap();
        let expected = PathBuf::from(".shatter-cache/bin/shatter-rust-custom");
        std::fs::write(cache_dir.join("shatter-rust-custom"), b"").unwrap();

        std::env::set_current_dir(temp_dir.path()).unwrap();
        let actual = find_custom_binary(None, "rust");
        std::env::set_current_dir(original_cwd).unwrap();

        assert_eq!(actual, Some(expected));
    }

    #[test]
    fn find_custom_binary_uses_project_cache_from_non_project_cwd() {
        let _guard = CWD_LOCK.lock().unwrap();
        let original_cwd = std::env::current_dir().unwrap();
        let project_dir = tempfile::tempdir().unwrap();
        let other_dir = tempfile::tempdir().unwrap();
        let shatter_dir = project_dir.path().join(".shatter");
        let cache_dir = project_dir.path().join(".shatter-cache").join("bin");
        std::fs::create_dir_all(&shatter_dir).unwrap();
        std::fs::create_dir_all(&cache_dir).unwrap();
        let expected = cache_dir.join("shatter-rust-custom");
        std::fs::write(&expected, b"").unwrap();

        std::env::set_current_dir(other_dir.path()).unwrap();
        let actual = find_custom_binary(Some(&shatter_dir), "rust");
        std::env::set_current_dir(original_cwd).unwrap();

        assert_eq!(actual, Some(expected));
    }

    #[test]
    fn find_custom_binary_uses_legacy_shatter_bin() {
        let project_dir = tempfile::tempdir().unwrap();
        let shatter_bin_dir = project_dir.path().join(".shatter").join("bin");
        std::fs::create_dir_all(&shatter_bin_dir).unwrap();
        let expected = shatter_bin_dir.join("shatter-rust-custom");
        std::fs::write(&expected, b"").unwrap();

        let shatter_dir = project_dir.path().join(".shatter");
        assert_eq!(
            find_custom_binary(Some(&shatter_dir), "rust"),
            Some(expected)
        );
    }

    #[test]
    fn frontend_config_rust_uses_project_custom_binary_from_non_project_cwd() {
        let _guard = CWD_LOCK.lock().unwrap();
        let original_cwd = std::env::current_dir().unwrap();
        let project_dir = tempfile::tempdir().unwrap();
        let other_dir = tempfile::tempdir().unwrap();
        let shatter_dir = project_dir.path().join(".shatter");
        let cache_dir = project_dir.path().join(".shatter-cache").join("bin");
        std::fs::create_dir_all(&shatter_dir).unwrap();
        std::fs::create_dir_all(&cache_dir).unwrap();
        let expected = cache_dir.join("shatter-rust-custom");
        std::fs::write(&expected, b"").unwrap();

        std::env::set_current_dir(other_dir.path()).unwrap();
        let config = frontend_config(
            Language::Rust,
            shatter_core::frontend::DEFAULT_REQUEST_TIMEOUT,
            LogLevel::Info,
            10,
            30,
            None,
            Some(&shatter_dir),
            false,
            false,
        )
        .unwrap();
        std::env::set_current_dir(original_cwd).unwrap();

        assert_eq!(config.command, expected);
    }

    /// `apply_storage_env` must set all three storage env vars.
    #[test]
    fn apply_storage_env_sets_all_storage_vars() {
        use shatter_core::harness_storage::{
            ENV_ARTIFACT_DIR, ENV_HARNESS_CACHE, ENV_HARNESS_SCRATCH, HarnessStorage,
        };
        let storage = HarnessStorage::for_project(Path::new("/tmp/test"));
        let mut config = FrontendConfig::new(PathBuf::from("dummy"));
        apply_storage_env(&mut config, &storage);
        let keys: std::collections::HashSet<&str> =
            config.env_vars.iter().map(|(k, _)| k.as_str()).collect();
        for var in [ENV_HARNESS_CACHE, ENV_HARNESS_SCRATCH, ENV_ARTIFACT_DIR] {
            assert!(keys.contains(var), "apply_storage_env must set {var}");
        }
    }

    /// `apply_project_storage` sets storage vars when a project root is provided.
    #[test]
    fn apply_project_storage_with_root() {
        use shatter_core::harness_storage::{
            ENV_ARTIFACT_DIR, ENV_HARNESS_CACHE, ENV_HARNESS_SCRATCH,
        };
        let mut config = FrontendConfig::new(PathBuf::from("dummy"));
        apply_project_storage(&mut config, Some("/tmp/project"));
        let keys: std::collections::HashSet<&str> =
            config.env_vars.iter().map(|(k, _)| k.as_str()).collect();
        for var in [ENV_HARNESS_CACHE, ENV_HARNESS_SCRATCH, ENV_ARTIFACT_DIR] {
            assert!(keys.contains(var), "apply_project_storage must set {var}");
        }
    }

    #[test]
    fn apply_project_storage_honors_harness_cache_env_override() {
        use shatter_core::harness_storage::ENV_HARNESS_CACHE;

        let _guard = HARNESS_CACHE_ENV_LOCK.lock().unwrap();
        let previous = std::env::var_os(ENV_HARNESS_CACHE);
        unsafe {
            std::env::set_var(ENV_HARNESS_CACHE, "/tmp/shatter-harness-override");
        }

        let mut config = FrontendConfig::new(PathBuf::from("dummy"));
        apply_project_storage(&mut config, Some("/tmp/project"));

        unsafe {
            match previous {
                Some(value) => std::env::set_var(ENV_HARNESS_CACHE, value),
                None => std::env::remove_var(ENV_HARNESS_CACHE),
            }
        }

        let value = config
            .env_vars
            .iter()
            .rev()
            .find_map(|(key, value)| (key == ENV_HARNESS_CACHE).then_some(value));
        assert_eq!(
            value,
            Some(&"/tmp/shatter-harness-override".to_string()),
            "project storage must not overwrite caller-provided SHATTER_HARNESS_CACHE"
        );
    }

    #[test]
    fn apply_project_storage_uses_project_harness_cache_by_default() {
        use shatter_core::harness_storage::ENV_HARNESS_CACHE;

        let _guard = HARNESS_CACHE_ENV_LOCK.lock().unwrap();
        let previous = std::env::var_os(ENV_HARNESS_CACHE);
        unsafe {
            std::env::remove_var(ENV_HARNESS_CACHE);
        }

        let mut config = FrontendConfig::new(PathBuf::from("dummy"));
        apply_project_storage(&mut config, Some("/tmp/project"));

        unsafe {
            match previous {
                Some(value) => std::env::set_var(ENV_HARNESS_CACHE, value),
                None => std::env::remove_var(ENV_HARNESS_CACHE),
            }
        }

        let value = config
            .env_vars
            .iter()
            .rev()
            .find_map(|(key, value)| (key == ENV_HARNESS_CACHE).then_some(value));
        assert_eq!(
            value,
            Some(&"/tmp/project/.shatter/cache/harness".to_string()),
            "project storage should keep the project-local harness cache default when no override is set"
        );
    }

    #[test]
    fn apply_external_audit_storage_reuses_cache_across_frontends() {
        use shatter_core::harness_storage::{
            ENV_ARTIFACT_DIR, ENV_HARNESS_CACHE, ENV_HARNESS_SCRATCH,
        };

        let _guard = HARNESS_CACHE_ENV_LOCK.lock().unwrap();
        let previous = std::env::var_os(ENV_HARNESS_CACHE);
        unsafe {
            std::env::remove_var(ENV_HARNESS_CACHE);
        }

        let mut first = FrontendConfig::new(PathBuf::from("dummy"));
        let mut second = FrontendConfig::new(PathBuf::from("dummy"));
        apply_external_audit_storage(&mut first);
        apply_external_audit_storage(&mut second);

        unsafe {
            match previous {
                Some(value) => std::env::set_var(ENV_HARNESS_CACHE, value),
                None => std::env::remove_var(ENV_HARNESS_CACHE),
            }
        }

        let env_value = |config: &FrontendConfig, key: &str| -> String {
            config
                .env_vars
                .iter()
                .rev()
                .find_map(|(env_key, value)| (env_key == key).then_some(value.clone()))
                .expect("expected env var")
        };

        assert_eq!(
            env_value(&first, ENV_HARNESS_CACHE),
            env_value(&second, ENV_HARNESS_CACHE),
            "external-audit analysis and execution frontends should share harness build cache"
        );
        assert_ne!(
            env_value(&first, ENV_HARNESS_SCRATCH),
            env_value(&second, ENV_HARNESS_SCRATCH),
            "external-audit scratch dirs should stay per frontend session"
        );
        assert_ne!(
            env_value(&first, ENV_ARTIFACT_DIR),
            env_value(&second, ENV_ARTIFACT_DIR),
            "external-audit artifact dirs should stay per frontend session"
        );
    }

    // ---- str-bnsw: frontend availability precheck ----

    /// TypeScript and Go ship embedded — always available.
    #[test]
    fn frontend_availability_ts_and_go_are_always_available() {
        assert!(check_frontend_availability(Language::TypeScript, None).is_available());
        assert!(check_frontend_availability(Language::Go, None).is_available());
    }

    /// When no `shatter-rust` binary is reachable from any of the search
    /// locations, the precheck reports `Unavailable` with the install hint —
    /// no spawn attempt and no generic failure.
    #[test]
    fn frontend_availability_rust_unavailable_returns_install_hint() {
        let _guard = CWD_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().expect("tempdir");
        let prev_path = std::env::var_os("PATH");
        let prev_cwd = std::env::current_dir().expect("cwd");
        // Isolate: empty PATH (no shatter-rust on PATH) and cwd in an empty
        // tempdir (so the ./shatter-rust/target/debug and ./target/debug
        // candidates miss).
        // SAFETY: tests in this crate are run single-threaded in the
        // env-mutating subset; we restore both before returning.
        unsafe {
            std::env::set_var("PATH", "");
        }
        std::env::set_current_dir(tmp.path()).expect("chdir tmp");

        let availability = check_frontend_availability(Language::Rust, None);

        // Restore environment before any assertion can panic.
        std::env::set_current_dir(&prev_cwd).expect("restore cwd");
        unsafe {
            match prev_path {
                Some(v) => std::env::set_var("PATH", v),
                None => std::env::remove_var("PATH"),
            }
        }

        assert!(
            !availability.is_available(),
            "expected Unavailable, got {availability:?}"
        );
        let msg = availability
            .unavailable_message()
            .expect("unavailable variant should produce a message");
        assert!(
            msg.contains("shatter-rust frontend not found"),
            "message should name the missing frontend: {msg}"
        );
        assert!(
            msg.contains("cargo build --manifest-path shatter-rust/Cargo.toml"),
            "message should include the build instructions: {msg}"
        );
        // str-difh: clarify that this is expected after a workspace-root
        // build, and point at the README for full setup.
        assert!(
            msg.contains("expected") && msg.contains("main CLI"),
            "message should frame the missing frontend as expected and \
             clarify the main CLI is working: {msg}"
        );
        assert!(
            msg.contains("cargo install --path shatter-rust"),
            "message should include the `cargo install` alternative: {msg}"
        );
        assert!(
            msg.contains("README.md"),
            "message should point at README.md for setup details: {msg}"
        );
    }

    /// `apply_project_storage` is a no-op when project root is None.
    #[test]
    fn apply_project_storage_without_root() {
        let mut config = FrontendConfig::new(PathBuf::from("dummy"));
        let before = config.env_vars.len();
        apply_project_storage(&mut config, None);
        assert_eq!(
            config.env_vars.len(),
            before,
            "no vars should be added when project_root is None"
        );
    }
}

/// Resolve the LLM config that applies to an explore run.
///
/// The oracle bundle is run-scoped, so this uses the explicit `--config` when
/// present, otherwise the hierarchical `.shatter/config.yaml` stack nearest to
/// the first expanded target file. `--set` overrides are inserted as the
/// highest-priority YAML layer.
pub(crate) fn resolve_llm_config(
    config_path: Option<&Path>,
    first_target_dir: Option<&Path>,
    set_overrides: &[String],
) -> Result<Option<shatter_core::config::LlmConfig>, shatter_core::config::ConfigError> {
    let mut configs = if let Some(path) = config_path {
        vec![shatter_core::config::parse_config(path)?]
    } else if let Some(start_dir) = first_target_dir {
        shatter_core::config::discover_configs(start_dir)?
    } else {
        Vec::new()
    };

    if !set_overrides.is_empty() {
        let set_config = shatter_core::config::parse_set_overrides(set_overrides)?;
        configs.insert(0, set_config);
    }

    Ok(shatter_core::config::merge_configs(&configs).llm)
}

/// Construct an LLM adapter from config, wrapped with rate-limiting.
///
/// Factored out of [`build_oracle_bundle`] so the match-on-adapter
/// block compiles cleanly in both test and non-test builds without
/// triggering unreachable-code warnings.
///
/// Recognized adapters (via [`shatter_llm::build_oracle`]):
/// - `"anthropic"` — Anthropic (Claude) API
/// - `"openai"`    — OpenAI (GPT) API or compatible proxy
/// - `"google"`    — Google Gemini API
/// - `"custom"`    — Generic HTTP POST endpoint (llm.custom config required)
/// - `"local"`     — OpenAI-compatible local server (llm.local config required)
/// - `"mock"`      — Scripted mock for tests (cfg(test) only)
fn build_oracle_adapter(
    config: &shatter_core::config::LlmConfig,
) -> Result<std::sync::Arc<dyn shatter_core::oracle::SeedOracle>, Box<dyn std::error::Error>> {
    match config.adapter.as_str() {
        #[cfg(test)]
        "mock" => {
            let adapter = shatter_llm::MockSeedOracle::scripted(vec![]);
            let wrapped = shatter_llm::RateLimitedOracle::new(adapter, config.max_retries);
            Ok(std::sync::Arc::new(wrapped))
        }
        #[cfg(not(test))]
        "mock" => Err("adapter 'mock' is only available in test builds".into()),
        // All other adapters (anthropic, openai, google, custom, local) are
        // implemented in the shatter-llm crate and dispatched via its registry.
        _other => shatter_llm::build_oracle(config).map_err(|e| e.into()),
    }
}

/// Build an [`shatter_core::oracle::OracleBundle`] from CLI overrides.
///
/// Returns `Ok(Some(bundle))` when the oracle is enabled, `Ok(None)` when
/// disabled, or `Err` when the requested adapter is unavailable.
pub(crate) fn build_oracle_bundle(
    overrides: &crate::args::LlmOverrides,
    config_file: Option<&shatter_core::config::LlmConfig>,
) -> Result<Option<shatter_core::oracle::OracleBundle>, Box<dyn std::error::Error>> {
    use std::sync::Arc;

    use shatter_core::config::LlmConfig;
    use shatter_core::oracle::OracleBundle;

    let mut config = config_file.cloned().unwrap_or_else(LlmConfig::default);
    if overrides.llm {
        config.enabled = true;
    }
    if let Some(ref adapter) = overrides.llm_adapter {
        config.adapter = adapter.clone();
    }
    if let Some(budget) = overrides.llm_token_budget {
        config.max_token_budget = budget;
    }
    if !config.enabled {
        return Ok(None);
    }

    // Select adapter by name. build_oracle_adapter delegates to
    // shatter_llm::build_oracle for provider adapters (anthropic, openai,
    // google, custom, local) and handles the cfg(test) "mock" adapter inline.
    let oracle: Arc<dyn shatter_core::oracle::SeedOracle> = build_oracle_adapter(&config)?;

    let runtime = shared_oracle_runtime();

    Ok(Some(OracleBundle {
        oracle,
        config,
        runtime,
    }))
}

fn shared_oracle_runtime() -> std::sync::Arc<tokio::runtime::Runtime> {
    static ORACLE_RUNTIME: std::sync::OnceLock<std::sync::Arc<tokio::runtime::Runtime>> =
        std::sync::OnceLock::new();

    std::sync::Arc::clone(ORACLE_RUNTIME.get_or_init(|| {
        std::sync::Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .thread_name("shatter-llm-oracle")
                .enable_all()
                .build()
                .expect("failed to build oracle runtime"),
        )
    }))
}

/// str-7v73: Escalate a secondary timeout (build or request) to at least
/// `timeout_per_fn` when the secondary is at its clap default. The Prepare
/// command and individual `frontend.send()` calls each carry their own
/// timeouts; if they fire before the per-function exploration budget the
/// user requested, the per-fn flag appears silently ignored. Returns the
/// secondary value unchanged when it was explicitly set (i.e. != default).
pub(crate) fn escalate_timeout(secondary: u64, secondary_default: u64, timeout_per_fn: u64) -> u64 {
    if secondary == secondary_default && timeout_per_fn > secondary {
        timeout_per_fn
    } else {
        secondary
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// str-frc.6: the resolver picks the CLI value when set, falls back to
    /// the project-config value, and lands on the built-in default when both
    /// sides are unset. Default is `1` so behavior is identical to a run
    /// without these knobs.
    #[test]
    fn resolve_observer_pool_precedence_cli_over_config_over_default() {
        assert_eq!(
            resolve_observer_pool(None, None),
            DEFAULT_OBSERVER_POOL_SIZE
        );
        assert_eq!(resolve_observer_pool(None, Some(6)), 6);
        assert_eq!(resolve_observer_pool(Some(3), Some(6)), 3);
        // Zero is clamped up so the explorer never sees an empty pool.
        assert_eq!(resolve_observer_pool(Some(0), None), 1);
    }

    /// str-frc.6: the queue-capacity resolver mirrors the precedence chain.
    /// The default is `None` (auto-derive) so default-preserving behavior
    /// matches pre-str-frc.5 runs.
    #[test]
    fn resolve_candidate_queue_capacity_precedence_cli_over_config_over_default() {
        assert_eq!(resolve_candidate_queue_capacity(None, None), None);
        assert_eq!(resolve_candidate_queue_capacity(None, Some(16)), Some(16));
        assert_eq!(resolve_candidate_queue_capacity(Some(8), Some(16)), Some(8));
    }

    #[test]
    fn resolve_llm_config_reads_explicit_config_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config_path = dir.path().join("config.yaml");
        std::fs::write(
            &config_path,
            r#"
llm:
  enabled: true
  adapter: local
  local:
    command: ["ollama", "serve"]
    model: llama3.2
    port: 11434
    startup_timeout_seconds: 60
"#,
        )
        .expect("write config");

        let llm = resolve_llm_config(Some(&config_path), None, &[])
            .expect("resolve llm config")
            .expect("llm config");

        assert!(llm.enabled);
        assert_eq!(llm.adapter, "local");
        let local = llm.local.expect("local adapter config");
        assert_eq!(
            local.command,
            vec!["ollama".to_string(), "serve".to_string()]
        );
        assert_eq!(local.model, "llama3.2");
    }

    #[test]
    fn resolve_parallelism_clamps_explicit_above_ceiling() {
        assert_eq!(resolve_parallelism(32), PARALLELISM_CEILING);
        assert_eq!(
            resolve_parallelism(PARALLELISM_CEILING + 1),
            PARALLELISM_CEILING
        );
    }

    #[test]
    fn resolve_parallelism_honors_explicit_below_floor() {
        // str-p2rz: explicit values below the auto-detect floor are honored
        // as-is so users can intentionally run with low parallelism for
        // debugging, small CI runners, or small targets.
        assert_eq!(resolve_parallelism(1), 1);
        assert_eq!(resolve_parallelism(PARALLELISM_FLOOR - 1), PARALLELISM_FLOOR - 1);
    }

    #[test]
    fn resolve_parallelism_passes_explicit_in_range() {
        let mid = (PARALLELISM_FLOOR + PARALLELISM_CEILING) / 2;
        assert_eq!(resolve_parallelism(mid), mid);
        assert_eq!(resolve_parallelism(PARALLELISM_FLOOR), PARALLELISM_FLOOR);
        assert_eq!(
            resolve_parallelism(PARALLELISM_CEILING),
            PARALLELISM_CEILING
        );
    }

    #[test]
    fn resolve_parallelism_autodetect_is_in_range() {
        let v = resolve_parallelism(0);
        assert!(
            (PARALLELISM_FLOOR..=PARALLELISM_CEILING).contains(&v),
            "auto-detected parallelism {v} outside [{PARALLELISM_FLOOR}, {PARALLELISM_CEILING}]"
        );
    }

    // ---- str-qp31: per-language parallelism defaults ----

    #[test]
    fn per_lang_cap_ts_only_is_unbounded() {
        let langs = [DiscoveryLanguage::TypeScript];
        assert_eq!(per_language_autodetect_cap(&langs), TS_AUTODETECT_CAP);
        assert_eq!(TS_AUTODETECT_CAP, usize::MAX);
    }

    #[test]
    fn per_lang_cap_go_only_is_eight() {
        let langs = [DiscoveryLanguage::Go];
        assert_eq!(per_language_autodetect_cap(&langs), GO_AUTODETECT_CAP);
        assert_eq!(GO_AUTODETECT_CAP, 8);
    }

    #[test]
    fn per_lang_cap_rust_only_is_eight() {
        let langs = [DiscoveryLanguage::Rust];
        assert_eq!(per_language_autodetect_cap(&langs), RUST_AUTODETECT_CAP);
        assert_eq!(RUST_AUTODETECT_CAP, 8);
    }

    #[test]
    fn per_lang_cap_mixed_takes_worst_case() {
        // TS is unbounded, Go is 8 → mixed must be 8 (the tighter cap wins).
        let mixed_ts_go = [DiscoveryLanguage::TypeScript, DiscoveryLanguage::Go];
        assert_eq!(per_language_autodetect_cap(&mixed_ts_go), 8);

        let mixed_ts_rust = [DiscoveryLanguage::TypeScript, DiscoveryLanguage::Rust];
        assert_eq!(per_language_autodetect_cap(&mixed_ts_rust), 8);

        let mixed_go_rust = [DiscoveryLanguage::Go, DiscoveryLanguage::Rust];
        assert_eq!(per_language_autodetect_cap(&mixed_go_rust), 8);

        let all_three = [
            DiscoveryLanguage::TypeScript,
            DiscoveryLanguage::Go,
            DiscoveryLanguage::Rust,
        ];
        assert_eq!(per_language_autodetect_cap(&all_three), 8);
    }

    #[test]
    fn per_lang_cap_empty_is_unbounded() {
        let langs: [DiscoveryLanguage; 0] = [];
        assert_eq!(per_language_autodetect_cap(&langs), usize::MAX);
    }

    #[test]
    fn resolve_parallelism_for_langs_autodetect_ts_only_uses_global_clamp() {
        // TS-only autodetect: capped only by [FLOOR, CEILING].
        let langs = [DiscoveryLanguage::TypeScript];
        let v = resolve_parallelism_for_langs(0, &langs, ParallelismBounds::defaults());
        assert!(
            (PARALLELISM_FLOOR..=PARALLELISM_CEILING).contains(&v),
            "TS-only autodetect {v} outside [{PARALLELISM_FLOOR}, {PARALLELISM_CEILING}]"
        );
    }

    #[test]
    fn resolve_parallelism_for_langs_autodetect_go_only_capped_at_eight() {
        let langs = [DiscoveryLanguage::Go];
        let v = resolve_parallelism_for_langs(0, &langs, ParallelismBounds::defaults());
        assert!(
            (PARALLELISM_FLOOR..=GO_AUTODETECT_CAP).contains(&v),
            "Go-only autodetect {v} outside [{PARALLELISM_FLOOR}, {GO_AUTODETECT_CAP}]"
        );
        // The global ceiling is still 16, but the Go cap is tighter — we must
        // never exceed 8 in this branch even on a 32-core host.
        assert!(v <= GO_AUTODETECT_CAP);
    }

    #[test]
    fn resolve_parallelism_for_langs_autodetect_rust_only_capped_at_eight() {
        let langs = [DiscoveryLanguage::Rust];
        let v = resolve_parallelism_for_langs(0, &langs, ParallelismBounds::defaults());
        assert!(
            (PARALLELISM_FLOOR..=RUST_AUTODETECT_CAP).contains(&v),
            "Rust-only autodetect {v} outside [{PARALLELISM_FLOOR}, {RUST_AUTODETECT_CAP}]"
        );
        assert!(v <= RUST_AUTODETECT_CAP);
    }

    #[test]
    fn resolve_parallelism_for_langs_autodetect_mixed_takes_worst_case() {
        let langs = [DiscoveryLanguage::TypeScript, DiscoveryLanguage::Go];
        let v = resolve_parallelism_for_langs(0, &langs, ParallelismBounds::defaults());
        // Mixed TS+Go: worst case (Go=8) wins.
        assert!(v <= GO_AUTODETECT_CAP);
        assert!(v >= PARALLELISM_FLOOR);
    }

    #[test]
    fn resolve_parallelism_for_langs_explicit_value_ignores_lang_cap() {
        // Per spec: per-language table governs the *default*, not explicit
        // user requests. An explicit --parallelism 12 with Go should still
        // produce 12 (clamped only by the global [4, 16] range).
        let langs = [DiscoveryLanguage::Go];
        assert_eq!(
            resolve_parallelism_for_langs(12, &langs, ParallelismBounds::defaults()),
            12
        );
        assert_eq!(
            resolve_parallelism_for_langs(16, &langs, ParallelismBounds::defaults()),
            16
        );
    }

    #[test]
    fn resolve_parallelism_for_langs_explicit_value_still_capped_by_ceiling() {
        // Explicit values exceeding the ceiling are clamped down (fork-bomb
        // guard), but values below the floor are honored as-is (str-p2rz:
        // the floor only governs auto-detected defaults).
        let langs = [DiscoveryLanguage::Go];
        assert_eq!(
            resolve_parallelism_for_langs(32, &langs, ParallelismBounds::defaults()),
            PARALLELISM_CEILING
        );
        assert_eq!(
            resolve_parallelism_for_langs(1, &langs, ParallelismBounds::defaults()),
            1,
            "explicit --parallelism 1 must be honored (str-p2rz)"
        );
        assert_eq!(
            resolve_parallelism_for_langs(2, &langs, ParallelismBounds::defaults()),
            2,
            "explicit --parallelism 2 must be honored (str-p2rz)"
        );
    }

    #[test]
    fn resolve_parallelism_for_langs_autodetect_empty_set_matches_legacy() {
        // An empty needed_langs set must behave like the language-agnostic
        // resolve_parallelism: clamped only by the global range.
        let langs: [DiscoveryLanguage; 0] = [];
        let v = resolve_parallelism_for_langs(0, &langs, ParallelismBounds::defaults());
        assert!(
            (PARALLELISM_FLOOR..=PARALLELISM_CEILING).contains(&v),
            "empty-langs autodetect {v} outside [{PARALLELISM_FLOOR}, {PARALLELISM_CEILING}]"
        );
    }

    // ---- str-v01r: parallelism floor/ceiling overrides ----

    #[test]
    fn parallelism_bounds_defaults_match_global_constants() {
        let b = ParallelismBounds::defaults();
        assert_eq!(b.floor, PARALLELISM_FLOOR);
        assert_eq!(b.ceiling, PARALLELISM_CEILING);
    }

    #[test]
    fn parallelism_bounds_from_overrides_neither_set_uses_defaults() {
        let b = ParallelismBounds::from_overrides(None, None).unwrap();
        assert_eq!(b, ParallelismBounds::defaults());
    }

    #[test]
    fn parallelism_bounds_from_overrides_only_min_raises_floor() {
        // Only --parallelism-min: floor moves, ceiling keeps the default.
        let b = ParallelismBounds::from_overrides(Some(2), None).unwrap();
        assert_eq!(b.floor, 2);
        assert_eq!(b.ceiling, PARALLELISM_CEILING);
    }

    #[test]
    fn parallelism_bounds_from_overrides_only_max_lowers_ceiling() {
        // Only --parallelism-max: ceiling moves, floor keeps the default.
        let b = ParallelismBounds::from_overrides(None, Some(32)).unwrap();
        assert_eq!(b.floor, PARALLELISM_FLOOR);
        assert_eq!(b.ceiling, 32);
    }

    #[test]
    fn parallelism_bounds_from_overrides_min_equals_max_pins_value() {
        let b = ParallelismBounds::from_overrides(Some(6), Some(6)).unwrap();
        assert_eq!(b.floor, 6);
        assert_eq!(b.ceiling, 6);
    }

    #[test]
    fn parallelism_bounds_from_overrides_min_greater_than_max_errors() {
        let err =
            ParallelismBounds::from_overrides(Some(10), Some(5)).expect_err("min > max must error");
        assert!(
            err.contains("floor (10)") && err.contains("ceiling (5)"),
            "error should name both bounds: {err}"
        );
    }

    #[test]
    fn parallelism_bounds_from_overrides_zero_min_errors() {
        let err =
            ParallelismBounds::from_overrides(Some(0), None).expect_err("min == 0 must error");
        assert!(err.contains("at least 1"), "error should say >= 1: {err}");
    }

    #[test]
    fn parallelism_bounds_from_overrides_zero_max_errors() {
        let err =
            ParallelismBounds::from_overrides(None, Some(0)).expect_err("max == 0 must error");
        assert!(err.contains("at least 1"), "error should say >= 1: {err}");
    }

    #[test]
    fn parallelism_bounds_from_overrides_implicit_default_floor_below_explicit_max() {
        // Pathological: if a user sets --parallelism-max 2, the implicit
        // default floor (4) would exceed it. We must error rather than
        // silently produce an empty range.
        let err = ParallelismBounds::from_overrides(None, Some(2))
            .expect_err("default floor (4) above explicit max (2) must error");
        assert!(
            err.contains("floor (4)") && err.contains("ceiling (2)"),
            "error should name both bounds: {err}"
        );
    }

    #[test]
    fn resolve_parallelism_with_bounds_explicit_capped_by_ceiling_only() {
        // Explicit user-supplied values are honored as long as they fit
        // under the ceiling. The floor governs the auto-detect path only
        // (str-p2rz).
        let bounds = ParallelismBounds::from_overrides(Some(2), Some(32)).unwrap();
        // Above custom ceiling: clamped down.
        assert_eq!(resolve_parallelism_with_bounds(64, bounds), 32);
        // Below custom floor: still honored as-is.
        assert_eq!(resolve_parallelism_with_bounds(1, bounds), 1);
        // In range: passes through.
        assert_eq!(resolve_parallelism_with_bounds(20, bounds), 20);
    }

    #[test]
    fn resolve_parallelism_with_bounds_pinned_floor_only_applies_to_autodetect() {
        // min == max still applies on the auto-detect path, but an explicit
        // request is honored even if it lies outside the pinned range.
        let bounds = ParallelismBounds::from_overrides(Some(7), Some(7)).unwrap();
        // Auto-detect: pinned.
        assert_eq!(resolve_parallelism_with_bounds(0, bounds), 7);
        // Explicit below the pinned floor: honored.
        assert_eq!(resolve_parallelism_with_bounds(1, bounds), 1);
        // Explicit above the pinned ceiling: clamped to ceiling.
        assert_eq!(resolve_parallelism_with_bounds(100, bounds), 7);
    }

    #[test]
    fn resolve_parallelism_with_bounds_explicit_low_value_honored_by_default_bounds() {
        // Direct regression for str-p2rz: --parallelism 1 / 2 with the
        // built-in [4, 16] bounds must produce 1 / 2, not 4.
        let bounds = ParallelismBounds::defaults();
        assert_eq!(resolve_parallelism_with_bounds(1, bounds), 1);
        assert_eq!(resolve_parallelism_with_bounds(2, bounds), 2);
        assert_eq!(resolve_parallelism_with_bounds(3, bounds), 3);
        // At and above the floor: unchanged.
        assert_eq!(resolve_parallelism_with_bounds(4, bounds), 4);
        assert_eq!(resolve_parallelism_with_bounds(8, bounds), 8);
        // Above the ceiling: clamped to ceiling.
        assert_eq!(resolve_parallelism_with_bounds(64, bounds), PARALLELISM_CEILING);
    }

    #[test]
    fn resolve_parallelism_with_bounds_autodetect_in_custom_range() {
        let bounds = ParallelismBounds::from_overrides(Some(2), Some(32)).unwrap();
        let v = resolve_parallelism_with_bounds(0, bounds);
        assert!(
            (bounds.floor..=bounds.ceiling).contains(&v),
            "autodetect {v} outside [{}, {}]",
            bounds.floor,
            bounds.ceiling
        );
    }

    #[test]
    fn resolve_parallelism_for_langs_with_custom_bounds_lower_ceiling_wins() {
        // Go's per-language cap is 8. With --parallelism-max 32, the lang cap
        // (8) is still tighter and should win on the autodetect path.
        let bounds = ParallelismBounds::from_overrides(Some(2), Some(32)).unwrap();
        let langs = [DiscoveryLanguage::Go];
        let v = resolve_parallelism_for_langs(0, &langs, bounds);
        assert!(v <= GO_AUTODETECT_CAP);
        assert!(v >= bounds.floor);
    }

    #[test]
    fn resolve_parallelism_for_langs_with_custom_bounds_explicit_uses_custom_ceiling() {
        // Explicit value bypasses the per-language cap but is clamped by the
        // user-supplied ceiling. --parallelism-max 32 with --parallelism 64
        // should yield 32, not the default ceiling (16) and not the lang cap
        // (8).
        let bounds = ParallelismBounds::from_overrides(Some(2), Some(32)).unwrap();
        let langs = [DiscoveryLanguage::Go];
        assert_eq!(resolve_parallelism_for_langs(64, &langs, bounds), 32);
        assert_eq!(resolve_parallelism_for_langs(20, &langs, bounds), 20);
        // Explicit value below the custom floor is honored as-is (str-p2rz).
        assert_eq!(resolve_parallelism_for_langs(1, &langs, bounds), 1);
    }

    #[test]
    fn resolve_parallelism_for_langs_lowered_ceiling_below_lang_cap_wins() {
        // User explicitly tightens the ceiling below the per-language cap:
        // override wins. --parallelism-max 4 with Go (lang cap 8) → at most 4.
        let bounds = ParallelismBounds::from_overrides(None, Some(4)).unwrap();
        let langs = [DiscoveryLanguage::Go];
        let v = resolve_parallelism_for_langs(0, &langs, bounds);
        assert!(v <= 4);
        assert_eq!(v, 4); // floor is default 4 too; the range is pinned
    }

    #[test]
    fn frontend_config_passes_timeout_env_vars() {
        let mut config = FrontendConfig::new(PathBuf::from("dummy"));
        apply_frontend_env(&mut config, LogLevel::Info, 20, 45, false);
        let env_map: std::collections::HashMap<_, _> = config.env_vars.iter().cloned().collect();
        assert_eq!(
            env_map.get("SHATTER_EXEC_TIMEOUT").map(|s| s.as_str()),
            Some("20")
        );
        assert_eq!(
            env_map.get("SHATTER_BUILD_TIMEOUT").map(|s| s.as_str()),
            Some("45")
        );
    }

    #[test]
    fn frontend_config_typescript_uses_embedded_bundle() {
        let config = frontend_config(
            Language::TypeScript,
            shatter_core::frontend::DEFAULT_REQUEST_TIMEOUT,
            LogLevel::Info,
            10,
            30,
            None,
            None,
            false,
            false,
        )
        .unwrap();
        assert_eq!(config.command, PathBuf::from("node"));
        assert_eq!(
            config.request_timeout,
            shatter_core::frontend::DEFAULT_REQUEST_TIMEOUT
        );
        // First arg suppresses Node warnings, second is the extracted bundle
        assert_eq!(config.args.len(), 2);
        assert_eq!(config.args[0], "--no-warnings");
        assert!(
            config.args[1].contains("frontend-"),
            "expected embedded bundle path, got: {}",
            config.args[1]
        );
    }

    #[test]
    fn frontend_config_go_uses_embedded_binary() {
        let config = frontend_config(
            Language::Go,
            Duration::from_secs(45),
            LogLevel::Info,
            10,
            30,
            None,
            None,
            false,
            false,
        )
        .unwrap();
        assert_eq!(config.request_timeout, Duration::from_secs(45));
        assert!(config.args.is_empty());
        // The command should point to the extracted binary, not a relative dev path
        let cmd_str = config.command.to_string_lossy();
        assert!(
            cmd_str.contains("go-frontend-"),
            "expected embedded binary path, got: {cmd_str}",
        );
    }

    /// The Go frontend shells out to `go build` for wrapper compilation, which by
    /// default uses `GOMAXPROCS=nproc`. When N frontends each run their own
    /// toolchain, large hosts fork-bomb themselves. The Go branch of
    /// `frontend_config` must inject `GOMAXPROCS=GO_FRONTEND_GOMAXPROCS` to cap
    /// the inner toolchain. See str-ovs6.
    #[test]
    fn frontend_config_go_caps_gomaxprocs() {
        let config = frontend_config(
            Language::Go,
            shatter_core::frontend::DEFAULT_REQUEST_TIMEOUT,
            LogLevel::Info,
            10,
            30,
            None,
            None,
            false,
            false,
        )
        .unwrap();
        let env_map: std::collections::HashMap<&str, &str> = config
            .env_vars
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        assert_eq!(
            env_map.get("GOMAXPROCS").copied(),
            Some(GO_FRONTEND_GOMAXPROCS),
            "Go frontend must inject GOMAXPROCS={GO_FRONTEND_GOMAXPROCS} to cap \
             inner `go build` toolchain (str-ovs6); got env={:?}",
            config.env_vars,
        );
    }

    /// Non-Go frontends should not receive `GOMAXPROCS` — it's a Go-toolchain
    /// knob and leaking it into TS would be confusing noise.
    #[test]
    fn frontend_config_non_go_omits_gomaxprocs() {
        let config = frontend_config(
            Language::TypeScript,
            shatter_core::frontend::DEFAULT_REQUEST_TIMEOUT,
            LogLevel::Info,
            10,
            30,
            None,
            None,
            false,
            false,
        )
        .unwrap();
        let keys: std::collections::HashSet<&str> =
            config.env_vars.iter().map(|(k, _)| k.as_str()).collect();
        assert!(
            !keys.contains("GOMAXPROCS"),
            "non-Go frontends must not carry GOMAXPROCS; got env={:?}",
            config.env_vars,
        );
    }

    #[test]
    fn frontend_config_adds_timing_capability_when_enabled() {
        let config = frontend_config(
            Language::TypeScript,
            shatter_core::frontend::DEFAULT_REQUEST_TIMEOUT,
            LogLevel::Info,
            10,
            30,
            None,
            None,
            true,
            false,
        )
        .unwrap();
        assert!(config.capabilities.iter().any(|cap| cap == "timing"));
    }

    #[test]
    fn build_meta_config_defaults() {
        let config = build_meta_config(false, None, None, None, None).unwrap();
        assert!(config.adaptive);
        assert_eq!(
            config.window_size,
            shatter_core::config::DEFAULT_EXPLORATION_SCORE_WINDOW
        );
        assert_eq!(
            config.cold_start_threshold,
            shatter_core::config::DEFAULT_EXPLORATION_COLD_START
        );
        assert!(
            (config.floor - shatter_core::config::DEFAULT_EXPLORATION_STRATEGY_FLOOR).abs()
                < f64::EPSILON
        );
        assert!(config.static_weights.is_none());
    }

    #[test]
    fn build_meta_config_with_overrides() {
        let config = build_meta_config(
            true,
            Some(50),
            Some(10),
            Some(0.05),
            Some("random=0.8,literals=0.2"),
        )
        .unwrap();
        assert!(!config.adaptive);
        assert_eq!(config.window_size, 50);
        assert_eq!(config.cold_start_threshold, 10);
        assert!((config.floor - 0.05).abs() < f64::EPSILON);
        let weights = config.static_weights.unwrap();
        assert_eq!(weights.len(), 2);
    }

    #[test]
    fn build_meta_config_invalid_weights() {
        let result = build_meta_config(false, None, None, None, Some("bad"));
        assert!(result.is_err());
    }

    #[test]
    fn discovery_lang_to_cli_lang_maps_correctly() {
        assert_eq!(
            discovery_lang_to_cli_lang(DiscoveryLanguage::TypeScript),
            Some(Language::TypeScript)
        );
        assert_eq!(
            discovery_lang_to_cli_lang(DiscoveryLanguage::Go),
            Some(Language::Go)
        );
        assert_eq!(
            discovery_lang_to_cli_lang(DiscoveryLanguage::Rust),
            Some(Language::Rust)
        );
    }

    /// str-7v73: escalate_timeout raises secondary timeouts when they are at
    /// their default and timeout_per_fn exceeds them.
    #[test]
    fn escalate_timeout_raises_default_to_per_fn() {
        // Default 30s with per-fn 180s → escalated to 180s
        assert_eq!(escalate_timeout(30, 30, 180), 180);
        // Explicitly set to 60s (not default) → unchanged
        assert_eq!(escalate_timeout(60, 30, 180), 60);
        // Default 30s with per-fn 30s → unchanged (no escalation needed)
        assert_eq!(escalate_timeout(30, 30, 30), 30);
        // Default 30s with per-fn 10s → unchanged (per-fn below secondary)
        assert_eq!(escalate_timeout(30, 30, 10), 30);
    }

    // ── str-g5b: oracle adapter dispatch ──────────────────────────────────────

    #[test]
    fn build_oracle_bundle_disabled_returns_none() {
        let overrides = crate::args::LlmOverrides {
            llm: false,
            llm_adapter: None,
            llm_token_budget: None,
        };
        let result = build_oracle_bundle(&overrides, None);
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn build_oracle_bundle_uses_enabled_config_adapter() {
        use shatter_core::config::{CustomAdapterConfig, CustomAuthMode, LlmConfig};
        let overrides = crate::args::LlmOverrides {
            llm: false,
            llm_adapter: None,
            llm_token_budget: None,
        };
        let config = LlmConfig {
            enabled: true,
            adapter: "custom".into(),
            custom: Some(CustomAdapterConfig {
                url: "http://localhost:8080/v1/chat".into(),
                headers: Default::default(),
                auth: CustomAuthMode::None,
                request_path: "$.prompt".into(),
                response_path: "$.text".into(),
            }),
            ..LlmConfig::default()
        };

        let result = build_oracle_bundle(&overrides, Some(&config));

        assert!(result.is_ok());
        let bundle = result.unwrap().expect("enabled config should build oracle");
        assert_eq!(bundle.config.adapter, "custom");
        assert_eq!(bundle.config.max_token_budget, config.max_token_budget);
    }

    #[test]
    fn build_oracle_adapter_mock_succeeds_in_test_build() {
        let config = shatter_core::config::LlmConfig {
            adapter: "mock".into(),
            ..shatter_core::config::LlmConfig::default()
        };
        // In cfg(test), "mock" should succeed.
        let result = build_oracle_adapter(&config);
        if let Err(ref e) = result {
            panic!("mock adapter should succeed in test builds: {e}");
        }
    }

    #[tokio::test]
    async fn build_oracle_bundle_drops_safely_in_async_context() {
        let overrides = crate::args::LlmOverrides {
            llm: false,
            llm_adapter: None,
            llm_token_budget: None,
        };
        let config = shatter_core::config::LlmConfig {
            enabled: true,
            adapter: "mock".into(),
            ..shatter_core::config::LlmConfig::default()
        };

        let bundle = build_oracle_bundle(&overrides, Some(&config))
            .expect("mock oracle bundle should build")
            .expect("enabled config should produce a bundle");

        drop(bundle);
    }

    #[test]
    fn build_oracle_adapter_custom_requires_config() {
        let config = shatter_core::config::LlmConfig {
            adapter: "custom".into(),
            ..shatter_core::config::LlmConfig::default()
        };
        match build_oracle_adapter(&config) {
            Err(e) => assert!(
                e.to_string().contains("llm.custom config required"),
                "expected missing-config error, got: {e}"
            ),
            Ok(_) => panic!("expected error without llm.custom config"),
        }
    }

    #[test]
    fn build_oracle_adapter_custom_with_config_succeeds() {
        use shatter_core::config::{CustomAdapterConfig, CustomAuthMode};
        let config = shatter_core::config::LlmConfig {
            adapter: "custom".into(),
            custom: Some(CustomAdapterConfig {
                url: "http://localhost:8080/v1/chat".into(),
                headers: Default::default(),
                auth: CustomAuthMode::None,
                request_path: "$.prompt".into(),
                response_path: "$.text".into(),
            }),
            ..shatter_core::config::LlmConfig::default()
        };
        let result = build_oracle_adapter(&config);
        if let Err(ref e) = result {
            panic!("custom adapter with config should succeed: {e}");
        }
    }

    #[test]
    fn build_oracle_adapter_local_requires_config() {
        let config = shatter_core::config::LlmConfig {
            adapter: "local".into(),
            ..shatter_core::config::LlmConfig::default()
        };
        match build_oracle_adapter(&config) {
            Err(e) => assert!(
                e.to_string().contains("llm.local config required"),
                "expected missing-config error, got: {e}"
            ),
            Ok(_) => panic!("expected error without llm.local config"),
        }
    }

    #[test]
    fn build_oracle_adapter_local_with_config_succeeds() {
        use shatter_core::config::LocalAdapterConfig;
        let config = shatter_core::config::LlmConfig {
            adapter: "local".into(),
            local: Some(LocalAdapterConfig {
                command: vec!["echo".into()],
                model: "test-model".into(),
                port: 11434,
                startup_timeout_seconds: 5,
            }),
            ..shatter_core::config::LlmConfig::default()
        };
        let result = build_oracle_adapter(&config);
        if let Err(ref e) = result {
            panic!("local adapter with config should succeed: {e}");
        }
    }

    #[test]
    fn build_oracle_adapter_unknown_adapter_errors() {
        let config = shatter_core::config::LlmConfig {
            adapter: "magic-llm".into(),
            ..shatter_core::config::LlmConfig::default()
        };
        match build_oracle_adapter(&config) {
            Err(e) => assert!(
                e.to_string().contains("unknown LLM adapter"),
                "expected unknown-adapter error, got: {e}"
            ),
            Ok(_) => panic!("expected error for unknown adapter"),
        }
    }
}
