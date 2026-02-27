//! Log level types for controlling output verbosity.

use std::fmt;
use std::str::FromStr;

/// Log verbosity level, from least to most verbose.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LogLevel {
    Error,
    Warn,
    #[default]
    Info,
    Debug,
    Trace,
}

impl LogLevel {
    /// The env var name used to communicate log level to frontend subprocesses.
    pub const ENV_VAR: &'static str = "SHATTER_LOG_LEVEL";

    /// Returns the string representation used for env vars and CLI flags.
    pub fn as_str(self) -> &'static str {
        match self {
            LogLevel::Error => "error",
            LogLevel::Warn => "warn",
            LogLevel::Info => "info",
            LogLevel::Debug => "debug",
            LogLevel::Trace => "trace",
        }
    }
}

impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for LogLevel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "error" => Ok(LogLevel::Error),
            "warn" => Ok(LogLevel::Warn),
            "info" => Ok(LogLevel::Info),
            "debug" => Ok(LogLevel::Debug),
            "trace" => Ok(LogLevel::Trace),
            other => Err(format!(
                "unknown log level '{other}': expected error, warn, info, debug, or trace"
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_level_ordering() {
        assert!(LogLevel::Error < LogLevel::Warn);
        assert!(LogLevel::Warn < LogLevel::Info);
        assert!(LogLevel::Info < LogLevel::Debug);
        assert!(LogLevel::Debug < LogLevel::Trace);
    }

    #[test]
    fn log_level_default_is_info() {
        assert_eq!(LogLevel::default(), LogLevel::Info);
    }

    #[test]
    fn log_level_round_trip_from_str() {
        for level in [
            LogLevel::Error,
            LogLevel::Warn,
            LogLevel::Info,
            LogLevel::Debug,
            LogLevel::Trace,
        ] {
            let parsed: LogLevel = level.as_str().parse().unwrap();
            assert_eq!(parsed, level);
        }
    }

    #[test]
    fn log_level_from_str_case_insensitive() {
        assert_eq!("INFO".parse::<LogLevel>().unwrap(), LogLevel::Info);
        assert_eq!("Debug".parse::<LogLevel>().unwrap(), LogLevel::Debug);
        assert_eq!("TRACE".parse::<LogLevel>().unwrap(), LogLevel::Trace);
    }

    #[test]
    fn log_level_from_str_rejects_unknown() {
        assert!("verbose".parse::<LogLevel>().is_err());
        assert!("".parse::<LogLevel>().is_err());
    }

    #[test]
    fn log_level_display() {
        assert_eq!(LogLevel::Error.to_string(), "error");
        assert_eq!(LogLevel::Info.to_string(), "info");
        assert_eq!(LogLevel::Trace.to_string(), "trace");
    }
}
