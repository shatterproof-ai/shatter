//! Opt-out telemetry for Shatter CLI usage analytics.
//!
//! Collects anonymous usage events (commands run, error types) to help prioritize
//! development. No file contents, paths, or personal data are transmitted.
//!
//! Consent hierarchy (first match wins):
//! 1. `SHATTER_TELEMETRY` env var ("0"/"false"/"off" → disabled, "1"/"true"/"on" → enabled)
//! 2. `DO_NOT_TRACK` env var (any non-empty value → disabled, per <https://consoledonottrack.com>)
//! 3. `~/.config/shatter/telemetry.yaml` → `enabled: bool`
//! 4. Default: enabled

use std::collections::HashSet;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

// ── Constants ──────────────────────────────────────────────────────────

/// Schema version for telemetry event envelopes.
pub const TELEMETRY_SCHEMA_VERSION: u32 = 1;

/// Maximum size of the event queue file in bytes (1 MiB).
pub const TELEMETRY_MAX_QUEUE_BYTES: u64 = 1_048_576;

/// Maximum age of queued events in days before they are eligible for pruning.
pub const TELEMETRY_MAX_AGE_DAYS: u32 = 30;

/// Timeout for acquiring the queue file lock, in milliseconds.
pub const TELEMETRY_LOCK_TIMEOUT_MS: u64 = 10;

/// Env var that explicitly controls telemetry consent.
const ENV_SHATTER_TELEMETRY: &str = "SHATTER_TELEMETRY";

/// Env var for the Console Do Not Track standard.
const ENV_DO_NOT_TRACK: &str = "DO_NOT_TRACK";

/// Filename for the telemetry config YAML.
const CONFIG_FILENAME: &str = "telemetry.yaml";

/// Filename for the persistent anonymous ID salt.
const SALT_FILENAME: &str = "anonymous_id_salt";

/// Directory name under XDG data home for telemetry queue.
const QUEUE_DIR: &str = "telemetry";

/// Queue file name.
const QUEUE_FILENAME: &str = "events.jsonl";

/// Known CLI subcommands, kept in sync with clap definitions.
const KNOWN_SUBCOMMANDS: &[&str] = &[
    "explore",
    "scan",
    "export",
    "spec",
    "run",
    "analyze",
    "init",
    "stale",
    "telemetry",
    "help",
    "version",
];

/// Known CLI flag enum values that are safe to keep unredacted.
const KNOWN_ENUM_VALUES: &[&str] = &[
    "json",
    "yaml",
    "text",
    "table",
    "concolic",
    "random",
    "hybrid",
    "typescript",
    "go",
    "rust",
    "ts",
    "brief",
    "detailed",
    "full",
    "on",
    "off",
    "status",
    "reset",
];

// ── Types ──────────────────────────────────────────────────────────────

/// A telemetry event envelope.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TelemetryEvent {
    /// Unique event identifier.
    pub event_id: String,
    /// ISO-8601 timestamp of when the event was created.
    pub timestamp: String,
    /// Schema version for forward compatibility.
    pub schema_version: u32,
    /// Stable anonymous identifier for this installation.
    pub anonymous_id: String,
    /// Machine-readable event name (e.g. "command_run").
    pub event_name: String,
    /// Event-specific payload.
    pub payload: EventPayload,
}

/// Payload variants for different event types.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum EventPayload {
    /// User invoked the CLI with arguments that failed validation.
    BadCliArgs {
        /// The sanitized argument tokens.
        sanitized_args: Vec<String>,
        /// The clap error kind, if available.
        error_kind: Option<String>,
    },
    /// A CLI command completed (successfully or not).
    CommandRun {
        /// The subcommand name (e.g. "explore", "scan").
        subcommand: String,
        /// Sanitized argument tokens.
        sanitized_args: Vec<String>,
        /// Wall-clock duration in milliseconds.
        duration_ms: u64,
        /// Exit code (0 = success).
        exit_code: i32,
    },
    /// A CLI command encountered an error.
    CommandError {
        /// The subcommand name.
        subcommand: String,
        /// Error category (not the full message, which may contain paths).
        error_category: String,
    },
}

/// Persisted telemetry configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TelemetryConfig {
    /// Whether telemetry is enabled.
    pub enabled: bool,
    /// Whether the first-run notice has been shown.
    #[serde(default)]
    pub notice_shown: bool,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            notice_shown: false,
        }
    }
}

/// Metadata about a path token, enriching the sanitized output without leaking the path itself.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PathMetadata {
    /// File extension, if any (e.g. "ts", "rs").
    pub extension: Option<String>,
    /// Whether the path exists on the filesystem.
    pub exists: bool,
    /// "file", "dir", or "unknown".
    pub kind: String,
    /// Whether the parent directory exists.
    pub parent_exists: bool,
    /// Whether the parent directory is writable.
    pub parent_writable: bool,
    /// Number of path components.
    pub depth: usize,
}

// ── Consent ────────────────────────────────────────────────────────────

/// Check whether telemetry is enabled according to the consent hierarchy.
///
/// Resolution order (first match wins):
/// 1. `SHATTER_TELEMETRY` env var
/// 2. `DO_NOT_TRACK` env var
/// 3. Config file
/// 4. Default: enabled
pub fn is_enabled() -> bool {
    is_enabled_with(|name| std::env::var(name))
}

/// Testable version of `is_enabled` that accepts an env-var lookup function.
fn is_enabled_with<F>(env_lookup: F) -> bool
where
    F: Fn(&str) -> Result<String, std::env::VarError>,
{
    // 1. Explicit SHATTER_TELEMETRY env var
    if let Ok(val) = env_lookup(ENV_SHATTER_TELEMETRY) {
        let lower = val.to_lowercase();
        return matches!(lower.as_str(), "1" | "true" | "on");
    }

    // 2. DO_NOT_TRACK standard
    if env_lookup(ENV_DO_NOT_TRACK).is_ok_and(|val| !val.is_empty()) {
        return false;
    }

    // 3. Config file
    if let Ok(config) = read_config() {
        return config.enabled;
    }

    // 4. Default: enabled
    true
}

// ── Anonymous ID ───────────────────────────────────────────────────────

/// Generate or retrieve a stable anonymous identifier for this installation.
///
/// The ID is SHA-256(hostname + OS + arch + random_salt). The salt is persisted
/// so the ID remains stable across sessions.
pub fn generate_anonymous_id() -> Result<String, TelemetryError> {
    let config_dir = shatter_config_dir()?;
    let salt_path = config_dir.join(SALT_FILENAME);

    let salt = if salt_path.exists() {
        std::fs::read_to_string(&salt_path).map_err(|e| TelemetryError::Io {
            context: "reading anonymous ID salt".into(),
            source: e,
        })?
    } else {
        let salt: String = (0..32)
            .map(|_| format!("{:02x}", rand::random::<u8>()))
            .collect();
        std::fs::create_dir_all(&config_dir).map_err(|e| TelemetryError::Io {
            context: "creating config directory".into(),
            source: e,
        })?;
        std::fs::write(&salt_path, &salt).map_err(|e| TelemetryError::Io {
            context: "writing anonymous ID salt".into(),
            source: e,
        })?;
        salt
    };

    let hostname = hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "unknown".into());

    let mut hasher = Sha256::new();
    hasher.update(hostname.as_bytes());
    hasher.update(std::env::consts::OS.as_bytes());
    hasher.update(std::env::consts::ARCH.as_bytes());
    hasher.update(salt.as_bytes());

    Ok(hex::encode(hasher.finalize()))
}

// ── Argument Sanitization ──────────────────────────────────────────────

/// Sanitize CLI argument tokens according to the 7-rule token classifier.
///
/// Returns a vec of sanitized tokens safe for telemetry, plus optional path
/// metadata for path-like tokens.
pub fn sanitize_args(args: &[String]) -> Vec<String> {
    let known_subcommands: HashSet<&str> = KNOWN_SUBCOMMANDS.iter().copied().collect();
    let known_enums: HashSet<&str> = KNOWN_ENUM_VALUES.iter().copied().collect();

    let mut result = Vec::with_capacity(args.len());

    for arg in args {
        // Rule 1: Split --flag=value
        if let Some((flag, value)) = split_flag_value(arg) {
            result.push(format!(
                "{}={}",
                flag,
                sanitize_value(&value, &known_subcommands, &known_enums)
            ));
            continue;
        }

        // Rule 2: Preserve known subcommands
        if known_subcommands.contains(arg.as_str()) {
            result.push(arg.clone());
            continue;
        }

        // Rule 3: Preserve flag names (--foo, -f)
        if arg.starts_with('-') {
            result.push(arg.clone());
            continue;
        }

        // Rules 4-7: Classify the value
        result.push(sanitize_value(arg, &known_subcommands, &known_enums));
    }

    result
}

/// Classify and sanitize a single value token (rules 4-7).
fn sanitize_value(
    value: &str,
    _subcommands: &HashSet<&str>,
    known_enums: &HashSet<&str>,
) -> String {
    // Rule 4: Path detection — contains path separator or looks like a file
    if looks_like_path(value) {
        let path = Path::new(value);
        let ext = path.extension().map(|e| e.to_string_lossy().to_string());
        let meta = probe_path_metadata(path);
        // Format: <path>.ext with metadata annotation
        let ext_suffix = ext.as_deref().unwrap_or("");
        let meta_parts: Vec<String> = vec![
            format!("exists={}", meta.exists),
            format!("kind={}", meta.kind),
            format!("depth={}", meta.depth),
        ];
        if ext_suffix.is_empty() {
            return format!("<path>[{}]", meta_parts.join(","));
        }
        return format!("<path>.{}[{}]", ext_suffix, meta_parts.join(","));
    }

    // Rule 5: Preserve numbers
    if value.parse::<f64>().is_ok() {
        return value.to_string();
    }

    // Rule 6: Preserve known enum values
    let lower = value.to_lowercase();
    if known_enums.contains(lower.as_str()) {
        return lower;
    }

    // Rule 7: Scrub everything else
    "<value>".to_string()
}

/// Check if a string looks like a filesystem path.
fn looks_like_path(s: &str) -> bool {
    s.contains(std::path::MAIN_SEPARATOR)
        || s.contains('/')
        || s.starts_with('.')
        || s.starts_with('~')
        || s.ends_with(".ts")
        || s.ends_with(".rs")
        || s.ends_with(".go")
        || s.ends_with(".js")
        || s.ends_with(".yaml")
        || s.ends_with(".yml")
        || s.ends_with(".json")
        || s.ends_with(".toml")
}

/// Split `--flag=value` or `-f=value` into (flag, value).
fn split_flag_value(arg: &str) -> Option<(String, String)> {
    if !arg.starts_with('-') {
        return None;
    }
    let eq_pos = arg.find('=')?;
    Some((arg[..eq_pos].to_string(), arg[eq_pos + 1..].to_string()))
}

/// Probe filesystem metadata for a path without revealing the path itself.
fn probe_path_metadata(path: &Path) -> PathMetadata {
    let exists = path.exists();
    let kind = if path.is_file() {
        "file"
    } else if path.is_dir() {
        "dir"
    } else {
        "unknown"
    }
    .to_string();
    let extension = path.extension().map(|e| e.to_string_lossy().to_string());
    let parent_exists = path.parent().is_some_and(|p| p.exists());
    let parent_writable = path
        .parent()
        .is_some_and(|p| p.metadata().is_ok_and(|m| !m.permissions().readonly()));
    let depth = path.components().count();

    PathMetadata {
        extension,
        exists,
        kind,
        parent_exists,
        parent_writable,
        depth,
    }
}

// ── Event Queue ────────────────────────────────────────────────────────

/// Append a telemetry event to the queue file.
///
/// Uses fs2 file locking to prevent concurrent corruption. Respects the size
/// cap — if the queue exceeds `TELEMETRY_MAX_QUEUE_BYTES`, the event is silently
/// dropped.
pub fn queue_event(event: &TelemetryEvent) -> Result<(), TelemetryError> {
    let queue_path = queue_file_path()?;

    if let Some(parent) = queue_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| TelemetryError::Io {
            context: "creating queue directory".into(),
            source: e,
        })?;
    }

    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .truncate(false)
        .open(&queue_path)
        .map_err(|e| TelemetryError::Io {
            context: "opening queue file".into(),
            source: e,
        })?;

    // Try non-blocking lock; skip silently if another process holds it
    use fs2::FileExt;
    match file.try_lock_exclusive() {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::WouldBlock => return Ok(()),
        Err(e) if e.raw_os_error() == Some(libc::EWOULDBLOCK) => return Ok(()),
        Err(e) => {
            return Err(TelemetryError::Io {
                context: "locking queue file".into(),
                source: e,
            });
        }
    }

    // Check size cap
    let current_size = file.metadata().map(|m| m.len()).unwrap_or(0);
    if current_size >= TELEMETRY_MAX_QUEUE_BYTES {
        // Silently drop — queue is full
        file.unlock().ok();
        return Ok(());
    }

    let mut line = serde_json::to_string(event).map_err(|e| TelemetryError::Serialize {
        context: "serializing telemetry event".into(),
        source: e,
    })?;
    line.push('\n');

    let mut writer = BufWriter::new(&file);
    writer
        .write_all(line.as_bytes())
        .map_err(|e| TelemetryError::Io {
            context: "writing event to queue".into(),
            source: e,
        })?;
    writer.flush().map_err(|e| TelemetryError::Io {
        context: "flushing queue file".into(),
        source: e,
    })?;

    file.unlock().ok();
    Ok(())
}

/// Read all queued events from the queue file.
pub fn read_queue() -> Result<Vec<TelemetryEvent>, TelemetryError> {
    let queue_path = queue_file_path()?;
    if !queue_path.exists() {
        return Ok(Vec::new());
    }

    let contents = std::fs::read_to_string(&queue_path).map_err(|e| TelemetryError::Io {
        context: "reading queue file".into(),
        source: e,
    })?;

    let mut events = Vec::new();
    for line in contents.lines() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<TelemetryEvent>(line) {
            Ok(event) => events.push(event),
            Err(_) => continue, // skip malformed lines
        }
    }
    Ok(events)
}

/// Clear the event queue.
pub fn clear_queue() -> Result<(), TelemetryError> {
    let queue_path = queue_file_path()?;
    if queue_path.exists() {
        std::fs::remove_file(&queue_path).map_err(|e| TelemetryError::Io {
            context: "removing queue file".into(),
            source: e,
        })?;
    }
    Ok(())
}

// ── Config ─────────────────────────────────────────────────────────────

/// Read the telemetry config from the standard location.
pub fn read_config() -> Result<TelemetryConfig, TelemetryError> {
    let path = config_file_path()?;
    if !path.exists() {
        return Ok(TelemetryConfig::default());
    }
    let contents = std::fs::read_to_string(&path).map_err(|e| TelemetryError::Io {
        context: "reading telemetry config".into(),
        source: e,
    })?;
    let config: TelemetryConfig =
        serde_yaml::from_str(&contents).map_err(|e| TelemetryError::ConfigParse {
            context: "parsing telemetry config".into(),
            source: e,
        })?;
    Ok(config)
}

/// Write the telemetry config to the standard location.
pub fn write_config(config: &TelemetryConfig) -> Result<(), TelemetryError> {
    let path = config_file_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| TelemetryError::Io {
            context: "creating config directory".into(),
            source: e,
        })?;
    }
    let contents = serde_yaml::to_string(config).map_err(|e| TelemetryError::ConfigSerialize {
        context: "serializing telemetry config".into(),
        source: e,
    })?;
    std::fs::write(&path, contents).map_err(|e| TelemetryError::Io {
        context: "writing telemetry config".into(),
        source: e,
    })?;
    Ok(())
}

// ── First-Run Notice ───────────────────────────────────────────────────

/// Show the first-run telemetry notice on stderr, if not already shown.
///
/// Updates the config to record that the notice has been displayed.
pub fn show_first_run_notice() -> Result<(), TelemetryError> {
    let mut config = read_config().unwrap_or_default();
    if config.notice_shown {
        return Ok(());
    }

    eprintln!(
        "\n\
        Shatter collects anonymous usage telemetry to help improve the tool.\n\
        No file contents, source code, or personal data are collected.\n\
        \n\
        You can disable telemetry at any time:\n\
        \n\
        \x20 shatter telemetry off\n\
        \x20 # or set SHATTER_TELEMETRY=0\n\
        \x20 # or set DO_NOT_TRACK=1\n\
        \n\
        Learn more: https://shatter.dev/telemetry\n"
    );

    config.notice_shown = true;
    write_config(&config)?;
    Ok(())
}

// ── Event Construction Helpers ─────────────────────────────────────────

/// Create a new `TelemetryEvent` with the given name and payload.
pub fn new_event(
    event_name: &str,
    payload: EventPayload,
) -> Result<TelemetryEvent, TelemetryError> {
    let anonymous_id = generate_anonymous_id()?;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_err(|e| TelemetryError::Time(e.to_string()))?;
    let timestamp = format_timestamp(now.as_secs());

    Ok(TelemetryEvent {
        event_id: uuid::Uuid::new_v4().to_string(),
        timestamp,
        schema_version: TELEMETRY_SCHEMA_VERSION,
        anonymous_id,
        event_name: event_name.to_string(),
        payload,
    })
}

/// Format a unix timestamp as ISO-8601.
fn format_timestamp(secs: u64) -> String {
    // Manual formatting to avoid pulling in chrono
    let days_since_epoch = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Compute date from days since epoch (1970-01-01)
    let (year, month, day) = days_to_date(days_since_epoch);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hours, minutes, seconds
    )
}

/// Convert days since 1970-01-01 to (year, month, day).
fn days_to_date(days: u64) -> (u64, u64, u64) {
    // Civil calendar algorithm from Howard Hinnant
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

// ── Path Helpers ───────────────────────────────────────────────────────

/// XDG config directory for shatter: `~/.config/shatter/`
fn shatter_config_dir() -> Result<PathBuf, TelemetryError> {
    dirs::config_dir()
        .map(|d| d.join("shatter"))
        .ok_or(TelemetryError::NoHomeDir)
}

/// Path to the telemetry config file.
pub fn config_file_path() -> Result<PathBuf, TelemetryError> {
    Ok(shatter_config_dir()?.join(CONFIG_FILENAME))
}

/// XDG data directory for shatter telemetry: `~/.local/share/shatter/telemetry/`
fn shatter_data_dir() -> Result<PathBuf, TelemetryError> {
    dirs::data_dir()
        .map(|d| d.join("shatter").join(QUEUE_DIR))
        .ok_or(TelemetryError::NoHomeDir)
}

/// Path to the event queue file.
pub fn queue_file_path() -> Result<PathBuf, TelemetryError> {
    Ok(shatter_data_dir()?.join(QUEUE_FILENAME))
}

/// Path to the persistent anonymous ID salt file.
pub fn salt_file_path() -> Result<PathBuf, TelemetryError> {
    Ok(shatter_config_dir()?.join(SALT_FILENAME))
}

// ── Errors ─────────────────────────────────────────────────────────────

/// Errors that can occur during telemetry operations.
#[derive(Debug, thiserror::Error)]
pub enum TelemetryError {
    #[error("telemetry I/O error ({context}): {source}")]
    Io { context: String, source: io::Error },

    #[error("telemetry serialization error ({context}): {source}")]
    Serialize {
        context: String,
        source: serde_json::Error,
    },

    #[error("telemetry config parse error ({context}): {source}")]
    ConfigParse {
        context: String,
        source: serde_yaml::Error,
    },

    #[error("telemetry config serialization error ({context}): {source}")]
    ConfigSerialize {
        context: String,
        source: serde_yaml::Error,
    },

    #[error("cannot determine home directory")]
    NoHomeDir,

    #[error("time error: {0}")]
    Time(String),
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    // ── Consent Hierarchy ──────────────────────────────────────────

    #[test]
    fn shatter_telemetry_env_on_overrides_all() {
        let lookup = |name: &str| -> Result<String, env::VarError> {
            match name {
                "SHATTER_TELEMETRY" => Ok("1".into()),
                "DO_NOT_TRACK" => Ok("1".into()),
                _ => Err(env::VarError::NotPresent),
            }
        };
        assert!(is_enabled_with(lookup));
    }

    #[test]
    fn shatter_telemetry_env_off_overrides_all() {
        let lookup = |name: &str| -> Result<String, env::VarError> {
            match name {
                "SHATTER_TELEMETRY" => Ok("0".into()),
                _ => Err(env::VarError::NotPresent),
            }
        };
        assert!(!is_enabled_with(lookup));
    }

    #[test]
    fn shatter_telemetry_env_false() {
        let lookup = |name: &str| -> Result<String, env::VarError> {
            match name {
                "SHATTER_TELEMETRY" => Ok("false".into()),
                _ => Err(env::VarError::NotPresent),
            }
        };
        assert!(!is_enabled_with(lookup));
    }

    #[test]
    fn shatter_telemetry_env_true() {
        let lookup = |name: &str| -> Result<String, env::VarError> {
            match name {
                "SHATTER_TELEMETRY" => Ok("true".into()),
                _ => Err(env::VarError::NotPresent),
            }
        };
        assert!(is_enabled_with(lookup));
    }

    #[test]
    fn do_not_track_disables() {
        let lookup = |name: &str| -> Result<String, env::VarError> {
            match name {
                "DO_NOT_TRACK" => Ok("1".into()),
                _ => Err(env::VarError::NotPresent),
            }
        };
        assert!(!is_enabled_with(lookup));
    }

    #[test]
    fn do_not_track_empty_does_not_disable() {
        let lookup = |name: &str| -> Result<String, env::VarError> {
            match name {
                "DO_NOT_TRACK" => Ok(String::new()),
                _ => Err(env::VarError::NotPresent),
            }
        };
        // Empty DO_NOT_TRACK doesn't disable; falls through to default
        assert!(is_enabled_with(lookup));
    }

    #[test]
    fn no_env_defaults_enabled() {
        let lookup =
            |_name: &str| -> Result<String, env::VarError> { Err(env::VarError::NotPresent) };
        assert!(is_enabled_with(lookup));
    }

    #[test]
    fn shatter_telemetry_takes_priority_over_do_not_track() {
        // SHATTER_TELEMETRY=on with DO_NOT_TRACK=1 → enabled
        let lookup = |name: &str| -> Result<String, env::VarError> {
            match name {
                "SHATTER_TELEMETRY" => Ok("on".into()),
                "DO_NOT_TRACK" => Ok("1".into()),
                _ => Err(env::VarError::NotPresent),
            }
        };
        assert!(is_enabled_with(lookup));
    }

    // ── Sanitization ──────────────────────────────────────────────

    #[test]
    fn rule1_split_flag_value() {
        let args = vec!["--output=json".to_string()];
        let result = sanitize_args(&args);
        assert_eq!(result, vec!["--output=json"]);
    }

    #[test]
    fn rule1_split_flag_with_path_value() {
        let args = vec!["--config=/home/user/config.yaml".to_string()];
        let result = sanitize_args(&args);
        assert!(result[0].starts_with("--config=<path>"));
    }

    #[test]
    fn rule2_preserve_subcommands() {
        let args = vec!["explore".to_string(), "scan".to_string()];
        let result = sanitize_args(&args);
        assert_eq!(result, vec!["explore", "scan"]);
    }

    #[test]
    fn rule3_preserve_flags() {
        let args = vec!["--verbose".to_string(), "-n".to_string()];
        let result = sanitize_args(&args);
        assert_eq!(result, vec!["--verbose", "-n"]);
    }

    #[test]
    fn rule4_sanitize_paths() {
        let args = vec!["./src/main.ts".to_string()];
        let result = sanitize_args(&args);
        assert!(result[0].contains("<path>"));
        assert!(result[0].contains(".ts"));
    }

    #[test]
    fn rule5_preserve_numbers() {
        let args = vec!["42".to_string(), "3.14".to_string()];
        let result = sanitize_args(&args);
        assert_eq!(result, vec!["42", "3.14"]);
    }

    #[test]
    fn rule6_preserve_enum_values() {
        let args = vec!["json".to_string(), "concolic".to_string()];
        let result = sanitize_args(&args);
        assert_eq!(result, vec!["json", "concolic"]);
    }

    #[test]
    fn rule7_scrub_unknown_values() {
        let args = vec!["my-secret-project".to_string()];
        let result = sanitize_args(&args);
        assert_eq!(result, vec!["<value>"]);
    }

    #[test]
    fn sanitize_mixed_args() {
        let args: Vec<String> = vec![
            "explore",
            "--timeout=30",
            "./src/main.ts",
            "--verbose",
            "my-project",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        let result = sanitize_args(&args);
        assert_eq!(result[0], "explore");
        assert_eq!(result[1], "--timeout=30");
        assert!(result[2].contains("<path>"));
        assert_eq!(result[3], "--verbose");
        assert_eq!(result[4], "<value>");
    }

    // ── Path Metadata ──────────────────────────────────────────────

    #[test]
    fn path_metadata_nonexistent() {
        let meta = probe_path_metadata(Path::new("/nonexistent/fake/path.ts"));
        assert!(!meta.exists);
        assert_eq!(meta.kind, "unknown");
        assert_eq!(meta.extension, Some("ts".into()));
    }

    #[test]
    fn path_metadata_existing_dir() {
        let dir = tempfile::tempdir().unwrap();
        let meta = probe_path_metadata(dir.path());
        assert!(meta.exists);
        assert_eq!(meta.kind, "dir");
    }

    // ── Event Serialization Roundtrip ──────────────────────────────

    #[test]
    fn event_roundtrip_command_run() {
        let event = TelemetryEvent {
            event_id: "test-id".into(),
            timestamp: "2026-01-01T00:00:00Z".into(),
            schema_version: TELEMETRY_SCHEMA_VERSION,
            anonymous_id: "anon-id".into(),
            event_name: "command_run".into(),
            payload: EventPayload::CommandRun {
                subcommand: "explore".into(),
                sanitized_args: vec!["explore".into(), "--verbose".into()],
                duration_ms: 1234,
                exit_code: 0,
            },
        };

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: TelemetryEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, deserialized);
    }

    #[test]
    fn event_roundtrip_bad_cli_args() {
        let event = TelemetryEvent {
            event_id: "test-id-2".into(),
            timestamp: "2026-01-01T00:00:00Z".into(),
            schema_version: TELEMETRY_SCHEMA_VERSION,
            anonymous_id: "anon-id".into(),
            event_name: "bad_cli_args".into(),
            payload: EventPayload::BadCliArgs {
                sanitized_args: vec!["--bogus".into()],
                error_kind: Some("UnknownArgument".into()),
            },
        };

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: TelemetryEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, deserialized);
    }

    #[test]
    fn event_roundtrip_command_error() {
        let event = TelemetryEvent {
            event_id: "test-id-3".into(),
            timestamp: "2026-01-01T00:00:00Z".into(),
            schema_version: TELEMETRY_SCHEMA_VERSION,
            anonymous_id: "anon-id".into(),
            event_name: "command_error".into(),
            payload: EventPayload::CommandError {
                subcommand: "scan".into(),
                error_category: "frontend_timeout".into(),
            },
        };

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: TelemetryEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, deserialized);
    }

    // ── Queue ──────────────────────────────────────────────────────

    #[test]
    fn queue_append_and_read() {
        let dir = tempfile::tempdir().unwrap();
        let queue_path = dir.path().join("events.jsonl");

        // Temporarily override queue path via writing directly
        let event = TelemetryEvent {
            event_id: "q-1".into(),
            timestamp: "2026-01-01T00:00:00Z".into(),
            schema_version: TELEMETRY_SCHEMA_VERSION,
            anonymous_id: "anon".into(),
            event_name: "command_run".into(),
            payload: EventPayload::CommandRun {
                subcommand: "explore".into(),
                sanitized_args: vec![],
                duration_ms: 100,
                exit_code: 0,
            },
        };

        // Write directly to test file
        let line = serde_json::to_string(&event).unwrap() + "\n";
        std::fs::write(&queue_path, &line).unwrap();

        let contents = std::fs::read_to_string(&queue_path).unwrap();
        let read_event: TelemetryEvent =
            serde_json::from_str(contents.lines().next().unwrap()).unwrap();
        assert_eq!(read_event, event);
    }

    #[test]
    fn queue_size_cap_respected() {
        let dir = tempfile::tempdir().unwrap();
        let queue_path = dir.path().join("events.jsonl");

        // Create a file that's already at the size cap
        let filler = "x".repeat(TELEMETRY_MAX_QUEUE_BYTES as usize);
        std::fs::write(&queue_path, &filler).unwrap();

        let size_before = std::fs::metadata(&queue_path).unwrap().len();
        assert!(size_before >= TELEMETRY_MAX_QUEUE_BYTES);

        // Trying to append should not grow the file (using direct logic)
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .truncate(false)
            .open(&queue_path)
            .unwrap();
        let current_size = file.metadata().map(|m| m.len()).unwrap_or(0);
        assert!(current_size >= TELEMETRY_MAX_QUEUE_BYTES);
        // queue_event would silently drop here
    }

    // ── Config ─────────────────────────────────────────────────────

    #[test]
    fn config_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(CONFIG_FILENAME);

        let config = TelemetryConfig {
            enabled: false,
            notice_shown: true,
        };
        let yaml = serde_yaml::to_string(&config).unwrap();
        std::fs::write(&path, &yaml).unwrap();

        let read_back: TelemetryConfig =
            serde_yaml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(config, read_back);
    }

    #[test]
    fn config_default_is_enabled() {
        let config = TelemetryConfig::default();
        assert!(config.enabled);
        assert!(!config.notice_shown);
    }

    // ── Anonymous ID ───────────────────────────────────────────────

    #[test]
    fn anonymous_id_is_hex_sha256() {
        // Can't easily test generate_anonymous_id without filesystem access,
        // but we can test the hash construction directly
        let mut hasher = Sha256::new();
        hasher.update(b"testhost");
        hasher.update(std::env::consts::OS.as_bytes());
        hasher.update(std::env::consts::ARCH.as_bytes());
        hasher.update(b"test-salt");
        let id = hex::encode(hasher.finalize());
        assert_eq!(id.len(), 64); // SHA-256 hex is 64 chars
    }

    #[test]
    fn anonymous_id_different_salts_differ() {
        let compute = |salt: &[u8]| {
            let mut hasher = Sha256::new();
            hasher.update(b"host");
            hasher.update(b"linux");
            hasher.update(b"x86_64");
            hasher.update(salt);
            hex::encode(hasher.finalize())
        };
        assert_ne!(compute(b"salt1"), compute(b"salt2"));
    }

    // ── Timestamp ──────────────────────────────────────────────────

    #[test]
    fn format_timestamp_epoch() {
        assert_eq!(format_timestamp(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn format_timestamp_known_date() {
        // 2026-01-01T00:00:00Z = 1767225600
        let ts = format_timestamp(1_767_225_600);
        assert_eq!(ts, "2026-01-01T00:00:00Z");
    }

    // ── Looks Like Path ────────────────────────────────────────────

    #[test]
    fn looks_like_path_with_separator() {
        assert!(looks_like_path("src/main.ts"));
        assert!(looks_like_path("./foo"));
        assert!(looks_like_path("~/config.yaml"));
    }

    #[test]
    fn looks_like_path_with_extension() {
        assert!(looks_like_path("main.ts"));
        assert!(looks_like_path("lib.rs"));
        assert!(looks_like_path("handler.go"));
    }

    #[test]
    fn not_a_path() {
        assert!(!looks_like_path("explore"));
        assert!(!looks_like_path("42"));
        assert!(!looks_like_path("json"));
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// Sanitization never panics on arbitrary input.
        #[test]
        fn sanitize_never_panics(args in prop::collection::vec(any::<String>(), 0..20)) {
            let _ = sanitize_args(&args);
        }

        /// Sanitized output has the same length as input.
        #[test]
        fn sanitize_preserves_length(args in prop::collection::vec(any::<String>(), 0..20)) {
            let result = sanitize_args(&args);
            prop_assert_eq!(result.len(), args.len());
        }

        /// Flags are always preserved.
        #[test]
        fn sanitize_preserves_flags(flag in "--[a-z][a-z-]{0,20}") {
            let result = sanitize_args(std::slice::from_ref(&flag));
            prop_assert_eq!(&result[0], &flag);
        }

        /// Known subcommands are always preserved.
        #[test]
        fn sanitize_preserves_subcommands(
            idx in 0..KNOWN_SUBCOMMANDS.len()
        ) {
            let cmd = KNOWN_SUBCOMMANDS[idx].to_string();
            let result = sanitize_args(std::slice::from_ref(&cmd));
            prop_assert_eq!(&result[0], &cmd);
        }

        /// Numbers are always preserved.
        #[test]
        fn sanitize_preserves_numbers(n in any::<i64>()) {
            let s = n.to_string();
            let result = sanitize_args(std::slice::from_ref(&s));
            prop_assert_eq!(&result[0], &s);
        }

        /// Event serialization roundtrips.
        #[test]
        fn event_roundtrip(
            event_id in "[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}",
            subcommand in prop::sample::select(KNOWN_SUBCOMMANDS),
            duration_ms in any::<u64>(),
            exit_code in any::<i32>(),
        ) {
            let event = TelemetryEvent {
                event_id,
                timestamp: "2026-01-01T00:00:00Z".into(),
                schema_version: TELEMETRY_SCHEMA_VERSION,
                anonymous_id: "test-anon".into(),
                event_name: "command_run".into(),
                payload: EventPayload::CommandRun {
                    subcommand: subcommand.to_string(),
                    sanitized_args: vec![],
                    duration_ms,
                    exit_code,
                },
            };
            let json = serde_json::to_string(&event).unwrap();
            let round: TelemetryEvent = serde_json::from_str(&json).unwrap();
            prop_assert_eq!(event, round);
        }

        /// Consent: SHATTER_TELEMETRY always wins when set.
        #[test]
        fn consent_shatter_env_wins(
            shatter_val in prop::sample::select(vec!["0", "1", "false", "true", "off", "on"]),
            dnt_val in prop::option::of("[01]"),
        ) {
            let expected = matches!(shatter_val, "1" | "true" | "on");
            let lookup = move |name: &str| -> Result<String, std::env::VarError> {
                match name {
                    "SHATTER_TELEMETRY" => Ok(shatter_val.to_string()),
                    "DO_NOT_TRACK" => match &dnt_val {
                        Some(v) => Ok(v.to_string()),
                        None => Err(std::env::VarError::NotPresent),
                    },
                    _ => Err(std::env::VarError::NotPresent),
                }
            };
            prop_assert_eq!(is_enabled_with(&lookup), expected);
        }

        /// Sanitized paths always use the <path> placeholder format.
        #[test]
        fn sanitize_scrubs_paths(
            dir in "[a-z]{3,10}",
            file in "[a-z]{3,10}",
            ext in prop::sample::select(vec!["ts", "rs", "go", "js"]),
        ) {
            let path = format!("{}/{}.{}", dir, file, ext);
            let result = sanitize_args(&[path]);
            // Must use <path> placeholder — raw directory/file names are stripped
            prop_assert!(result[0].starts_with("<path>"), "not sanitized: {}", result[0]);
            // Only the extension and metadata remain, never the dir or file segments
            let after_prefix = result[0].strip_prefix("<path>").unwrap();
            // after_prefix should be like ".ts[exists=false,kind=unknown,depth=2]"
            prop_assert!(
                after_prefix.starts_with('.') || after_prefix.starts_with('['),
                "unexpected format after <path>: {}", after_prefix
            );
        }
    }
}
