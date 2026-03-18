use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tracing::subscriber::SetGlobalDefaultError;
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::prelude::*;
use tracing_subscriber::registry::{LookupSpan, Registry};
use uuid::Uuid;

static GLOBAL_TIMING_HANDLE: OnceLock<TimingHandle> = OnceLock::new();

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

    pub fn from_phase_summaries(
        command: impl Into<String>,
        config: &TimingConfig,
        started_at_unix_ms: u128,
        duration_ms: u64,
        exit_code: i32,
        mut phases: Vec<TimingPhaseSummary>,
    ) -> Self {
        let command = command.into();
        if phases.iter().all(|phase| phase.phase_path != "cli.command") {
            let mut attributes = BTreeMap::new();
            attributes.insert("command".into(), command.clone());
            attributes.insert("exit_code".into(), exit_code.to_string());
            phases.push(TimingPhaseSummary {
                phase_path: "cli.command".into(),
                total_ms: duration_ms as f64,
                self_ms: duration_ms as f64,
                count: 1,
                attributes,
            });
        }

        phases.sort_by(|a, b| a.phase_path.cmp(&b.phase_path));
        Self {
            schema_version: 1,
            run_id: Uuid::new_v4().to_string(),
            command,
            mode: config.mode,
            format: config.format,
            started_at_unix_ms,
            duration_ms,
            exit_code,
            phases,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct TimingHandle {
    inner: Arc<Mutex<BTreeMap<String, TimingPhaseSummary>>>,
}

impl TimingHandle {
    pub fn dispatch(&self) -> tracing::Dispatch {
        tracing::Dispatch::new(Registry::default().with(TimingLayer::new(self.clone())))
    }

    pub fn install_global(&self) -> Result<(), SetGlobalDefaultError> {
        let _ = GLOBAL_TIMING_HANDLE.set(self.clone());
        tracing::subscriber::set_global_default(Registry::default().with(TimingLayer::new(self.clone())))
    }

    pub fn snapshot(&self) -> Vec<TimingPhaseSummary> {
        self.inner.lock().unwrap().values().cloned().collect()
    }

    fn record(&self, phase_path: String, total_ms: f64, self_ms: f64) {
        let mut guard = self.inner.lock().unwrap();
        let entry = guard.entry(phase_path.clone()).or_insert_with(|| TimingPhaseSummary {
            phase_path,
            total_ms: 0.0,
            self_ms: 0.0,
            count: 0,
            attributes: BTreeMap::new(),
        });
        entry.total_ms += total_ms;
        entry.self_ms += self_ms;
        entry.count += 1;
    }

    fn record_summary(&self, summary: TimingPhaseSummary) {
        let mut guard = self.inner.lock().unwrap();
        let entry = guard
            .entry(summary.phase_path.clone())
            .or_insert_with(|| TimingPhaseSummary {
                phase_path: summary.phase_path.clone(),
                total_ms: 0.0,
                self_ms: 0.0,
                count: 0,
                attributes: BTreeMap::new(),
            });
        entry.total_ms += summary.total_ms;
        entry.self_ms += summary.self_ms;
        entry.count += summary.count;
        for (key, value) in summary.attributes {
            entry.attributes.entry(key).or_insert(value);
        }
    }
}

pub fn record_protocol_timing(summary: &crate::protocol::TimingSummary) {
    let Some(handle) = GLOBAL_TIMING_HANDLE.get() else {
        return;
    };

    for phase in &summary.phases {
        let phase_path = format!("frontend.remote.{}", phase.phase_path);
        handle.record_summary(TimingPhaseSummary {
            phase_path,
            total_ms: phase.total_ms,
            self_ms: phase.self_ms,
            count: phase.count,
            attributes: phase.attributes.clone(),
        });
    }
}

#[derive(Debug)]
struct SpanTimingData {
    phase_path: String,
    start: Instant,
    child_ms: f64,
}

#[derive(Debug, Clone)]
struct TimingLayer {
    handle: TimingHandle,
}

impl TimingLayer {
    fn new(handle: TimingHandle) -> Self {
        Self { handle }
    }
}

impl<S> Layer<S> for TimingLayer
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(
        &self,
        _attrs: &tracing::span::Attributes<'_>,
        id: &tracing::span::Id,
        ctx: Context<'_, S>,
    ) {
        let Some(span) = ctx.span(id) else {
            return;
        };
        let parent_path = span.parent().and_then(|parent| {
            parent
                .extensions()
                .get::<SpanTimingData>()
                .map(|data| data.phase_path.clone())
        });
        let phase_path = match parent_path {
            Some(parent) => format!("{parent}.{}", span.metadata().name()),
            None => span.metadata().name().to_string(),
        };
        span.extensions_mut().insert(SpanTimingData {
            phase_path,
            start: Instant::now(),
            child_ms: 0.0,
        });
    }

    fn on_close(&self, id: tracing::span::Id, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(&id) else {
            return;
        };
        let parent = span.parent().map(|parent| parent.id());
        let Some(data) = span.extensions_mut().remove::<SpanTimingData>() else {
            return;
        };

        let total_ms = data.start.elapsed().as_secs_f64() * 1000.0;
        let self_ms = (total_ms - data.child_ms).max(0.0);
        self.handle.record(data.phase_path, total_ms, self_ms);

        if let Some(parent_id) = parent
            && let Some(parent_span) = ctx.span(&parent_id)
            && let Some(parent_data) = parent_span.extensions_mut().get_mut::<SpanTimingData>()
        {
            parent_data.child_ms += total_ms;
        }
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

    #[test]
    fn tracing_handle_aggregates_nested_spans() {
        let handle = TimingHandle::default();
        let dispatch = handle.dispatch();
        tracing::dispatcher::with_default(&dispatch, || {
            let outer = tracing::info_span!("cli.command");
            let _outer_entered = outer.enter();
            let inner = tracing::info_span!("core.explore");
            let _inner_entered = inner.enter();
        });

        let phases = handle.snapshot();
        assert!(phases.iter().any(|phase| phase.phase_path == "cli.command"));
        assert!(phases.iter().any(|phase| phase.phase_path == "cli.command.core.explore"));
    }
}
