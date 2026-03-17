use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimingMode {
    Off,
    Summary,
    Detailed,
}

impl TimingMode {
    pub fn is_enabled(self) -> bool {
        !matches!(self, Self::Off)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimingFormat {
    Text,
    Json,
    Both,
}

impl TimingFormat {
    pub fn shows_text(self) -> bool {
        matches!(self, Self::Text | Self::Both)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TimingOutput {
    File { path: PathBuf },
    Directory { path: PathBuf },
}

impl TimingOutput {
    pub fn resolve_run_path(&self, command: &str, started_at_unix_ms: u128) -> PathBuf {
        match self {
            Self::File { path } => path.clone(),
            Self::Directory { path } => {
                path.join(format!("{command}-{started_at_unix_ms}.timing.json"))
            }
        }
    }

    pub fn ensure_parent_dir(&self, resolved_path: &Path) -> std::io::Result<()> {
        let dir = match self {
            Self::File { .. } => resolved_path.parent().unwrap_or_else(|| Path::new(".")),
            Self::Directory { path } => path.as_path(),
        };
        std::fs::create_dir_all(dir)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimingConfig {
    pub mode: TimingMode,
    pub format: TimingFormat,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<TimingOutput>,
    #[serde(default)]
    pub perf_alias_used: bool,
}

impl TimingConfig {
    pub fn show_text_summary(&self) -> bool {
        self.mode.is_enabled() && self.format.shows_text()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimingPhaseSummary {
    pub phase_path: String,
    pub total_ms: f64,
    pub self_ms: f64,
    pub count: u64,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub attributes: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimingRun {
    pub schema_version: u32,
    pub run_id: String,
    pub command: String,
    pub mode: TimingMode,
    pub format: TimingFormat,
    pub started_at_unix_ms: u128,
    pub duration_ms: u64,
    pub exit_code: i32,
    pub phases: Vec<TimingPhaseSummary>,
}

impl TimingRun {
    pub fn command_only(
        command: impl Into<String>,
        config: &TimingConfig,
        started_at_unix_ms: u128,
        duration_ms: u64,
        exit_code: i32,
    ) -> Self {
        let command = command.into();
        let mut attributes = BTreeMap::new();
        attributes.insert("command".into(), command.clone());
        attributes.insert("exit_code".into(), exit_code.to_string());

        Self {
            schema_version: 1,
            run_id: Uuid::new_v4().to_string(),
            command,
            mode: config.mode,
            format: config.format,
            started_at_unix_ms,
            duration_ms,
            exit_code,
            phases: vec![TimingPhaseSummary {
                phase_path: "cli.command".into(),
                total_ms: duration_ms as f64,
                self_ms: duration_ms as f64,
                count: 1,
                attributes,
            }],
        }
    }

    pub fn persist(&self, output: &TimingOutput) -> std::io::Result<PathBuf> {
        let path = output.resolve_run_path(&self.command, self.started_at_unix_ms);
        output.ensure_parent_dir(&path)?;
        let json = serde_json::to_string_pretty(self)
            .map_err(|err| std::io::Error::other(format!("serialize timing run: {err}")))?;
        std::fs::write(&path, json)?;
        Ok(path)
    }
}

pub fn unix_timestamp_ms_now() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directory_output_uses_generated_filename() {
        let output = TimingOutput::Directory {
            path: PathBuf::from("/tmp/timing"),
        };
        let resolved = output.resolve_run_path("explore", 1234);
        assert_eq!(
            resolved,
            PathBuf::from("/tmp/timing/explore-1234.timing.json")
        );
    }

    #[test]
    fn config_text_summary_respects_mode_and_format() {
        let enabled = TimingConfig {
            mode: TimingMode::Summary,
            format: TimingFormat::Text,
            output: None,
            perf_alias_used: false,
        };
        assert!(enabled.show_text_summary());

        let json_only = TimingConfig {
            format: TimingFormat::Json,
            ..enabled.clone()
        };
        assert!(!json_only.show_text_summary());

        let disabled = TimingConfig {
            mode: TimingMode::Off,
            ..enabled
        };
        assert!(!disabled.show_text_summary());
    }

    #[test]
    fn persist_writes_json_file() {
        let dir = tempfile::tempdir().unwrap();
        let output = TimingOutput::Directory {
            path: dir.path().to_path_buf(),
        };
        let config = TimingConfig {
            mode: TimingMode::Summary,
            format: TimingFormat::Both,
            output: Some(output.clone()),
            perf_alias_used: false,
        };
        let run = TimingRun::command_only("explore", &config, 42, 7, 0);
        let path = run.persist(&output).unwrap();

        let data = std::fs::read_to_string(path).unwrap();
        assert!(data.contains("\"command\": \"explore\""));
        assert!(data.contains("\"phase_path\": \"cli.command\""));
    }
}
