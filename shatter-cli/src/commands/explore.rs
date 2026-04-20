use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use shatter_core::adapter_selection;
use shatter_core::behavior::BehaviorMap;
use shatter_core::cache::{BehaviorMapCache, StoredInputsCache};
use shatter_core::config::{self as shatter_config, GeneticConfig, ShatterConfig};
use shatter_core::executability;
use shatter_core::explorer::{
    self, ExploreConfig, ExploreProgressSnapshot, GeneticStats, ProgressCallback, ReportOptions,
};
use shatter_core::fingerprint::FunctionSignature;
use shatter_core::frontend::{Frontend, FrontendConfig};
use shatter_core::log_level::LogLevel;
use shatter_core::protocol::{Command as ProtoCommand, ResponseResult};
use shatter_core::report::ProgressEvent;
use shatter_core::scope::{ScopeConfig, ScopeMatcher};
use shatter_core::spec::FileSpecBundle;
use tracing::Instrument;

use crate::args::*;
use crate::helpers::*;

/// Result of exploring a single function (final, after all its batches are merged).
/// Used by the sequential Phase 3 processing loop.
struct FuncExploreOutcome {
    work_index: usize,
    func: shatter_core::protocol::FunctionAnalysis,
    mock_symbols: Vec<String>,
    result: Result<shatter_core::explorer::ObservationOutput, String>,
    wall_time: Duration,
    genetic_config: GeneticConfig,
}

/// Result of a single batch (one slice of iterations for one function), returned
/// from the tokio task that ran it. Multiple BatchOutcomes for the same
/// work_index are merged into a single FuncExploreOutcome by the accumulator.
struct BatchExploreOutcome {
    work_index: usize,
    result: Result<shatter_core::explorer::ObservationOutput, String>,
    wall_time: Duration,
    /// Per-batch iteration cap the scheduler issued to this task. Used to
    /// decide whether the batch converged early (fewer iters used → mark
    /// exhausted, don't re-enqueue) or hit the cap (re-enqueue for another
    /// slice).
    batch_iteration_cap: u32,
    /// Resumable orchestrator state for the next batch of this function.
    /// Present only when the concolic path succeeded.
    resume_state: Option<shatter_core::orchestrator::ExploreState>,
}

const EXPLORE_ARTIFACT_VERSION: u32 = 2;

/// Serializable wrapper around `shatter_core::orchestrator::ExploreState`.
///
/// `ExploreState` in orchestrator.rs derives only `Debug, Clone, Default` (no
/// Serialize/Deserialize — that file is owned by another workstream). This
/// wrapper mirrors the fields with serde support for disk persistence of
/// partial-function resume state between interrupted runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedExploreState {
    covered_paths: Vec<u64>,
    discovery_inputs: Vec<Vec<serde_json::Value>>,
}

impl PersistedExploreState {
    fn from_explore_state(state: &shatter_core::orchestrator::ExploreState) -> Self {
        let mut paths: Vec<u64> = state.covered_paths.iter().copied().collect();
        paths.sort_unstable();
        Self {
            covered_paths: paths,
            discovery_inputs: state.discovery_inputs.clone(),
        }
    }

    fn into_explore_state(self) -> shatter_core::orchestrator::ExploreState {
        shatter_core::orchestrator::ExploreState {
            covered_paths: self.covered_paths.into_iter().collect(),
            discovery_inputs: self.discovery_inputs,
        }
    }
}

/// Internal iteration budget per round-robin batch when broad-scope explore
/// cycles across many functions. The scheduler re-enqueues non-exhausted
/// functions at the tail after each batch, so long runs surface early coverage
/// on every function instead of deep coverage on a few.
///
/// 500 is large enough that the fixed per-batch overhead (frontend spawn,
/// instrument, prepare) amortizes well, and small enough to preserve round-
/// robin fairness across moderately-sized broad-scope runs.
///
/// Not exposed through the user-facing CLI — tuning happens here.
const EXPLORE_BATCH_ITERATIONS: u32 = 500;

/// Decide whether a completed batch has exhausted its function.
///
/// A function is exhausted — and should NOT be re-enqueued — when either
/// (a) the batch errored (re-running won't help this run), or
/// (b) the orchestrator converged early, using strictly fewer iterations
///     than the batch cap (nothing left to explore).
/// Otherwise the scheduler re-queues the task for another slice. This is
/// the sole criterion separating true round-robin batching from the
/// degenerate "one batch per function" mode, so it is extracted as a
/// free function to be unit-tested without spinning up a frontend.
fn batch_is_exhausted(
    result: &Result<shatter_core::explorer::ObservationOutput, String>,
    batch_iteration_cap: u32,
) -> bool {
    match result {
        Err(_) => true,
        Ok(obs) => obs.iterations < batch_iteration_cap,
    }
}

/// Persist a behavior map to the cache, stamping the current deep fingerprint
/// when one is available so `BehaviorMapCache::is_fresh` will not drop the
/// entry as a legacy (unfingerprinted) map on the next run.
///
/// Callers in the explore path already compute deep fingerprints for the whole
/// file; routing them through this helper is what keeps persisted maps
/// reusable across identical runs (str-bo4z.11 regression).
fn persist_behavior_map(
    cache: &BehaviorMapCache,
    map: &BehaviorMap,
    fingerprint: Option<&str>,
) -> Result<(), shatter_core::cache::CacheError> {
    match fingerprint {
        Some(fp) => cache.store_with_fingerprint(map, fp),
        None => cache.store(map),
    }
}

/// Count how many branch discoveries in `obs` are not already present in the
/// accumulator's `prior_discoveries`. This is the rerank score passed to
/// `BatchScheduler::record_outcome`: batches that surface novel branches rank
/// higher than batches that only rediscovered known work, so a function on a
/// discovery streak keeps its slot instead of yielding round-robin. Errored
/// batches (obs = None) contribute zero new discoveries and therefore rank 0.
fn new_discoveries_in_batch(
    obs: Option<&shatter_core::explorer::ObservationOutput>,
    prior_discoveries: &HashMap<u32, shatter_core::coverage_metrics::DiscoveryMethod>,
) -> usize {
    match obs {
        None => 0,
        Some(obs) => obs
            .discoveries
            .iter()
            .filter(|(branch_id, _)| !prior_discoveries.contains_key(branch_id))
            .count(),
    }
}

/// Accumulates `ObservationOutput`s from multiple round-robin batches that all
/// explored the same function, and collapses them into a single merged output
/// for the downstream Phase 3 processing loop.
///
/// Merge rules per field:
/// - `iterations`: sum across batches (each batch ran a disjoint slice)
/// - `unique_paths`: recomputed after merge from deduped `discoveries`
/// - `lines_covered` / `total_lines`: max (line sets are not carried through,
///   so we conservatively take the largest single-batch observation)
/// - `discoveries`: deduped by `branch_id`, earliest batch wins (HashMap insert
///   with `or_insert`)
/// - Collection fields (`raw_results`, `new_path_executions`,
///   `nondeterministic_fields`, `float_probe_results`, `boundary_results`,
///   `abandoned_frontiers`, `opaque_suggestions`): concatenated. Downstream
///   consumers already tolerate duplicates; deduping here would require
///   introspecting every contained type's identity.
/// - `shrunk_witnesses`: HashMap merge by key; on collision keep the smaller
///   (more-shrunk) witness.
/// - `mcdc_summary`: component-wise max of (total, independent, opaque).
/// - `shrink_stats`: last-wins. The field is a set of aggregate counters; the
///   last batch's stats reflect the most recent shrink phase.
/// - `stubbed_modules`: concatenated, then sorted + deduped on finalization.
///
/// If *every* batch for a function errored, `into_result` returns the last
/// error so the failure is surfaced in the explore summary.
struct ExploreResultAccumulator {
    function_name: String,
    total_iterations: u32,
    max_lines_covered: usize,
    total_lines: u32,
    raw_results: Vec<(
        Vec<serde_json::Value>,
        Vec<shatter_core::protocol::MockConfig>,
        shatter_core::protocol::ExecuteResult,
    )>,
    discoveries: HashMap<u32, shatter_core::coverage_metrics::DiscoveryMethod>,
    new_path_executions: Vec<shatter_core::explorer::ExecutionSummary>,
    nondeterministic_fields: Vec<shatter_core::nondeterminism::NondeterministicField>,
    float_probe_results: Vec<shatter_core::float_probe::FloatProbeResult>,
    boundary_results: Vec<shatter_core::boundary_search::BoundaryResult>,
    shrunk_witnesses: HashMap<u64, Vec<serde_json::Value>>,
    mcdc_summary: Option<(usize, usize, usize)>,
    shrink_stats: shatter_core::shrink::ShrinkStats,
    abandoned_frontiers: Vec<(u32, u32)>,
    opaque_suggestions: Vec<shatter_core::executability::OpaqueSuggestion>,
    stubbed_modules: Vec<String>,
    last_error: Option<String>,
    successful_batches: u32,
    batches_merged: u32,
}

impl ExploreResultAccumulator {
    fn new(function_name: String) -> Self {
        Self {
            function_name,
            total_iterations: 0,
            max_lines_covered: 0,
            total_lines: 0,
            raw_results: Vec::new(),
            discoveries: HashMap::new(),
            new_path_executions: Vec::new(),
            nondeterministic_fields: Vec::new(),
            float_probe_results: Vec::new(),
            boundary_results: Vec::new(),
            shrunk_witnesses: HashMap::new(),
            mcdc_summary: None,
            shrink_stats: shatter_core::shrink::ShrinkStats::default(),
            abandoned_frontiers: Vec::new(),
            opaque_suggestions: Vec::new(),
            stubbed_modules: Vec::new(),
            last_error: None,
            successful_batches: 0,
            batches_merged: 0,
        }
    }

    fn merge(&mut self, result: Result<shatter_core::explorer::ObservationOutput, String>) {
        self.batches_merged += 1;
        match result {
            Ok(obs) => {
                self.successful_batches += 1;
                if self.function_name.is_empty() {
                    self.function_name = obs.function_name;
                }
                self.total_iterations = self.total_iterations.saturating_add(obs.iterations);
                self.max_lines_covered = self.max_lines_covered.max(obs.lines_covered);
                self.total_lines = self.total_lines.max(obs.total_lines);
                self.raw_results.extend(obs.raw_results);
                for (branch_id, method) in obs.discoveries {
                    self.discoveries.entry(branch_id).or_insert(method);
                }
                self.new_path_executions.extend(obs.new_path_executions);
                self.nondeterministic_fields
                    .extend(obs.nondeterministic_fields);
                self.float_probe_results.extend(obs.float_probe_results);
                self.boundary_results.extend(obs.boundary_results);
                for (k, v) in obs.shrunk_witnesses {
                    self.shrunk_witnesses
                        .entry(k)
                        .and_modify(|cur| {
                            if v.len() < cur.len() {
                                *cur = v.clone();
                            }
                        })
                        .or_insert(v);
                }
                self.mcdc_summary = match (self.mcdc_summary, obs.mcdc_summary) {
                    (Some(cur), Some(new)) => {
                        Some((cur.0.max(new.0), cur.1.max(new.1), cur.2.max(new.2)))
                    }
                    (None, new) => new,
                    (cur, None) => cur,
                };
                self.shrink_stats = obs.shrink_stats;
                self.abandoned_frontiers.extend(obs.abandoned_frontiers);
                self.opaque_suggestions.extend(obs.opaque_suggestions);
                self.stubbed_modules.extend(obs.stubbed_modules);
            }
            Err(e) => {
                self.last_error = Some(e);
            }
        }
    }

    fn into_result(self) -> Result<shatter_core::explorer::ObservationOutput, String> {
        if self.successful_batches == 0 {
            return Err(self
                .last_error
                .unwrap_or_else(|| "no batches executed".to_string()));
        }
        let mut stubbed = self.stubbed_modules;
        stubbed.sort();
        stubbed.dedup();
        let unique_paths = self.discoveries.len();
        Ok(shatter_core::explorer::ObservationOutput {
            function_name: self.function_name,
            iterations: self.total_iterations,
            unique_paths,
            lines_covered: self.max_lines_covered,
            total_lines: self.total_lines,
            new_path_executions: self.new_path_executions,
            raw_results: self.raw_results,
            discoveries: self.discoveries.into_iter().collect(),
            nondeterministic_fields: self.nondeterministic_fields,
            float_probe_results: self.float_probe_results,
            boundary_results: self.boundary_results,
            shrunk_witnesses: self.shrunk_witnesses,
            mcdc_summary: self.mcdc_summary,
            shrink_stats: self.shrink_stats,
            abandoned_frontiers: self.abandoned_frontiers,
            opaque_suggestions: self.opaque_suggestions,
            stubbed_modules: stubbed,
        })
    }
}

/// Per-function explore artifact for serialization (borrows from outcome).
#[derive(Serialize)]
struct ExploreFunctionArtifactWrite<'a> {
    version: u32,
    status: &'a str,
    file: &'a str,
    function_name: &'a str,
    start_line: u32,
    end_line: u32,
    wall_time_ms: u64,
    mock_symbols: &'a [String],
    analysis: &'a shatter_core::protocol::FunctionAnalysis,
    #[serde(skip_serializing_if = "Option::is_none")]
    observation: Option<&'a shatter_core::explorer::ObservationOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<&'a str>,
}

/// Per-function explore artifact read from disk. v2 adds the `analysis` field
/// so that final assembly can be reconstructed from saved artifacts without a
/// live frontend.
#[derive(Debug, Deserialize)]
struct ExploreFunctionArtifact {
    version: u32,
    status: String,
    file: String,
    function_name: String,
    start_line: u32,
    end_line: u32,
    wall_time_ms: u64,
    mock_symbols: Vec<String>,
    analysis: shatter_core::protocol::FunctionAnalysis,
    observation: Option<shatter_core::explorer::ObservationOutput>,
    error: Option<String>,
}

/// Per-function entry in the explore summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExploreSummaryEntry {
    function_name: String,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    artifact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    /// Deep fingerprint at the time this entry was written. Used by the
    /// automatic resume logic to detect stale artifacts when the function
    /// body (or any transitive callee) changed between runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    deep_fingerprint: Option<String>,
}

/// Summary of an entire explore run, written incrementally to enable crash recovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExploreSummary {
    version: u32,
    status: String,
    file: String,
    total_functions: usize,
    completed: usize,
    failed: usize,
    skipped: usize,
    elapsed_secs: f64,
    functions: Vec<ExploreSummaryEntry>,
}

/// One function ready to be scheduled for exploration. Cloned per batch because
/// the scheduler re-enqueues a work item across batches (each with its own
/// per-batch iteration cap).
///
/// Carries its source target's language and file path so the batch loop —
/// unified across targets since str-b2my.10 hoisted the scheduler — can spawn
/// the right frontend and route instrument/prepare calls without cross-
/// referencing the owning PreparedTarget.
#[derive(Clone)]
struct FuncWorkItem {
    func: shatter_core::protocol::FunctionAnalysis,
    explore_config: ExploreConfig,
    mock_symbols: Vec<String>,
    concolic_config: Option<shatter_core::orchestrator::ExploreConfig>,
    seed_inputs: Vec<Vec<serde_json::Value>>,
    user_inputs: Vec<Vec<serde_json::Value>>,
    genetic_config: GeneticConfig,
    language: crate::args::Language,
    file_str: String,
    project_root_str: Option<String>,
    /// Index into the `prepared_targets` vector the owning run_explore call
    /// maintains. Post-processing uses this to find per-target state like the
    /// incremental plan and deep fingerprints without a secondary lookup.
    target_idx: usize,
    /// Pre-computed known uncovered targets from static analysis.
    /// Empty means the function has no branch targets to explore.
    known_targets: Vec<shatter_core::coverage_metrics::KnownTarget>,
}

/// All per-target state produced by the analyze + prepare phase. Held across
/// the unified batch loop (which is shared across targets once str-b2my.10
/// hoists the scheduler out of the per-target loop) and consumed by the
/// post-batch processing pass that writes artifacts, runs GA follow-up, and
/// emits spec bundles per target.
///
/// `work_item_indices` maps into the global `work_items` vector that the main
/// loop owns, so post-processing can iterate a target's own functions after
/// the batch loop has finished merging every function's accumulator.
struct PreparedTarget {
    language: crate::args::Language,
    file_str: String,
    project_root_str: Option<String>,
    functions: Vec<shatter_core::protocol::FunctionAnalysis>,
    #[allow(dead_code)]
    fresh_set: HashSet<String>,
    incremental_plan: Option<(
        shatter_core::spec::IncrementalPlan,
        shatter_core::spec::FileSpecBundle,
    )>,
    deep_fingerprints: HashMap<String, String>,
    skipped_unexecutable: Vec<(String, Vec<executability::SkipReason>)>,
    artifact_root: PathBuf,
    target_start: Instant,
    explore_summary: ExploreSummary,
    #[allow(dead_code)]
    work_item_indices: Vec<usize>,
}

fn explore_artifact_root(project_root: Option<&str>) -> PathBuf {
    project_root
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."))
        .join("shatter-artifacts")
        .join("explore-results")
}

fn sanitize_artifact_component(input: &str) -> String {
    let sanitized: String = input
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = sanitized.trim_matches('_');
    if trimmed.is_empty() {
        "unknown".to_string()
    } else {
        trimmed.to_string()
    }
}

fn explore_artifact_path(
    root: &Path,
    file: &str,
    func: &shatter_core::protocol::FunctionAnalysis,
) -> PathBuf {
    let file_component = sanitize_artifact_component(file);
    let fn_component = sanitize_artifact_component(&func.name);
    root.join(file_component)
        .join(format!("{:05}_{}.json", func.start_line, fn_component))
}

fn write_explore_artifact(
    root: &Path,
    file: &str,
    outcome: &FuncExploreOutcome,
) -> Result<PathBuf, String> {
    let status = if outcome.result.is_ok() {
        "completed"
    } else {
        "failed"
    };
    let artifact = ExploreFunctionArtifactWrite {
        version: EXPLORE_ARTIFACT_VERSION,
        status,
        file,
        function_name: &outcome.func.name,
        start_line: outcome.func.start_line,
        end_line: outcome.func.end_line,
        wall_time_ms: outcome.wall_time.as_millis() as u64,
        mock_symbols: &outcome.mock_symbols,
        analysis: &outcome.func,
        observation: outcome.result.as_ref().ok(),
        error: outcome.result.as_ref().err().map(String::as_str),
    };
    let path = explore_artifact_path(root, file, &outcome.func);
    write_artifact_json(&path, &artifact)?;
    Ok(path)
}

fn write_artifact_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create artifact dir: {e}"))?;
    }
    let json = serde_json::to_string_pretty(value)
        .map_err(|e| format!("failed to serialize artifact: {e}"))?;
    let tmp_path = path.with_extension("json.tmp");
    std::fs::write(&tmp_path, &json)
        .map_err(|e| format!("failed to write artifact temp file: {e}"))?;
    std::fs::rename(&tmp_path, path).map_err(|e| format!("failed to finalize artifact: {e}"))?;
    Ok(())
}

fn explore_summary_path(root: &Path, file: &str) -> PathBuf {
    let file_component = sanitize_artifact_component(file);
    root.join(file_component).join("summary.json")
}

fn write_explore_summary(root: &Path, file: &str, summary: &ExploreSummary) -> Result<(), String> {
    let path = explore_summary_path(root, file);
    write_artifact_json(&path, summary)
}

fn parse_explore_summary(path: &Path) -> Option<ExploreSummary> {
    let json = std::fs::read_to_string(path).ok()?;
    let summary: ExploreSummary = serde_json::from_str(&json).ok()?;
    if summary.version < EXPLORE_ARTIFACT_VERSION {
        return None;
    }
    Some(summary)
}

/// Load a prior explore summary from the artifact directory. Returns `None`
/// when the file is missing, corrupt, or has a version older than the current
/// artifact format (in which case re-exploration is the right call).
fn read_explore_summary(root: &Path, file: &str) -> Option<ExploreSummary> {
    let path = explore_summary_path(root, file);
    parse_explore_summary(&path)
}

/// Try to resume a completed function from a prior explore run.
///
/// Returns the loaded `ObservationOutput` and wall time if **all** of the
/// following hold:
/// 1. The function appears in the prior summary with status "completed".
/// 2. The summary entry carries a `deep_fingerprint` that matches the current
///    deep fingerprint (source + transitive callees unchanged).
/// 3. The artifact file referenced by the summary entry still exists on disk
///    and parses successfully.
/// 4. The artifact contains an `observation` (not just an error).
///
/// Any failure degrades to `None` — the function will be re-explored.
fn try_resume_function(
    artifact_root: &Path,
    func: &shatter_core::protocol::FunctionAnalysis,
    deep_fingerprints: &HashMap<String, String>,
    prior_summary: Option<&ExploreSummary>,
) -> Option<(shatter_core::explorer::ObservationOutput, Duration)> {
    let summary = prior_summary?;
    let entry = summary
        .functions
        .iter()
        .find(|e| e.function_name == func.name && e.status == "completed")?;
    // Require fingerprint match — legacy summaries without fingerprints
    // gracefully cause re-exploration.
    let stored_fp = entry.deep_fingerprint.as_deref()?;
    let current_fp = deep_fingerprints.get(&func.name)?;
    if stored_fp != current_fp {
        return None;
    }
    let artifact_relpath = entry.artifact.as_deref()?;
    let artifact_path = artifact_root.join(artifact_relpath);
    let artifact = read_explore_artifact(&artifact_path).ok()?;
    let observation = artifact.observation?;
    Some((observation, Duration::from_millis(artifact.wall_time_ms)))
}

/// Path to the per-function resume-state sidecar, stored alongside the
/// function's explore artifact.
fn resume_state_path(
    root: &Path,
    file: &str,
    func: &shatter_core::protocol::FunctionAnalysis,
) -> PathBuf {
    let file_component = sanitize_artifact_component(file);
    let fn_component = sanitize_artifact_component(&func.name);
    root.join(file_component).join(format!(
        "{:05}_{}.resume-state.json",
        func.start_line, fn_component
    ))
}

/// Persist the orchestrator's resume state for a partially-explored function.
/// Called after each batch so a subsequent run can skip path rediscovery.
fn write_resume_state(
    root: &Path,
    file: &str,
    func: &shatter_core::protocol::FunctionAnalysis,
    state: &shatter_core::orchestrator::ExploreState,
) -> Result<(), String> {
    let persisted = PersistedExploreState::from_explore_state(state);
    let path = resume_state_path(root, file, func);
    write_artifact_json(&path, &persisted)
}

/// Load a persisted resume state for a partially-explored function.
/// Returns `None` on any error (missing file, corrupt JSON, etc.).
fn read_resume_state(
    root: &Path,
    file: &str,
    func: &shatter_core::protocol::FunctionAnalysis,
) -> Option<shatter_core::orchestrator::ExploreState> {
    let path = resume_state_path(root, file, func);
    let json = std::fs::read_to_string(&path).ok()?;
    let persisted: PersistedExploreState = serde_json::from_str(&json).ok()?;
    Some(persisted.into_explore_state())
}

/// Remove the resume-state sidecar after a function fully completes.
fn cleanup_resume_state(root: &Path, file: &str, func: &shatter_core::protocol::FunctionAnalysis) {
    let path = resume_state_path(root, file, func);
    let _ = std::fs::remove_file(&path);
}

fn read_explore_artifact(path: &Path) -> Result<ExploreFunctionArtifact, String> {
    let json = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read artifact {}: {e}", path.display()))?;
    let artifact: ExploreFunctionArtifact = serde_json::from_str(&json)
        .map_err(|e| format!("failed to parse artifact {}: {e}", path.display()))?;
    if artifact.version < EXPLORE_ARTIFACT_VERSION {
        return Err(format!(
            "artifact {} is version {} (expected {}); re-run explore to generate v2 artifacts",
            path.display(),
            artifact.version,
            EXPLORE_ARTIFACT_VERSION,
        ));
    }
    Ok(artifact)
}

/// Load all explore artifacts from a directory tree.
/// Reads `summary.json` for ordering when available, otherwise scans for `*.json` files.
fn load_explore_artifacts(dir: &Path) -> Result<Vec<ExploreFunctionArtifact>, String> {
    if !dir.is_dir() {
        return Err(format!(
            "artifact directory does not exist: {}",
            dir.display()
        ));
    }

    let mut artifacts = Vec::new();

    // Walk all subdirectories looking for artifact JSON files.
    let mut dirs_to_visit = vec![dir.to_path_buf()];
    while let Some(current_dir) = dirs_to_visit.pop() {
        let entries = std::fs::read_dir(&current_dir)
            .map_err(|e| format!("failed to read directory {}: {e}", current_dir.display()))?;
        for entry in entries {
            let entry = entry.map_err(|e| format!("failed to read dir entry: {e}"))?;
            let path = entry.path();
            if path.is_dir() {
                dirs_to_visit.push(path);
                continue;
            }
            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            // Skip summary, resume-state sidecars, temp files, and non-JSON.
            if file_name == "summary.json"
                || file_name.ends_with(".resume-state.json")
                || file_name.ends_with(".tmp")
                || !file_name.ends_with(".json")
            {
                continue;
            }
            match read_explore_artifact(&path) {
                Ok(artifact) => artifacts.push(artifact),
                Err(e) => log::warn!("Skipping {}: {e}", path.display()),
            }
        }
    }

    // Sort by (file, start_line, end_line) for deterministic ordering.
    artifacts.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then(a.start_line.cmp(&b.start_line))
            .then(a.end_line.cmp(&b.end_line))
    });

    Ok(artifacts)
}

fn load_explore_summaries(dir: &Path) -> Result<Vec<ExploreSummary>, String> {
    if !dir.is_dir() {
        return Err(format!(
            "artifact directory does not exist: {}",
            dir.display()
        ));
    }

    let mut summaries = Vec::new();
    let mut dirs_to_visit = vec![dir.to_path_buf()];
    while let Some(current_dir) = dirs_to_visit.pop() {
        let entries = std::fs::read_dir(&current_dir)
            .map_err(|e| format!("failed to read directory {}: {e}", current_dir.display()))?;
        for entry in entries {
            let entry = entry.map_err(|e| format!("failed to read dir entry: {e}"))?;
            let path = entry.path();
            if path.is_dir() {
                dirs_to_visit.push(path);
                continue;
            }
            if path.file_name().and_then(|name| name.to_str()) != Some("summary.json") {
                continue;
            }
            match parse_explore_summary(&path) {
                Some(summary) => summaries.push(summary),
                None => log::warn!("Skipping invalid explore summary {}", path.display()),
            }
        }
    }

    summaries.sort_by(|a, b| a.file.cmp(&b.file));
    Ok(summaries)
}

/// Map a stored `ExploreSummaryEntry` to an `OutcomeStatus`.
///
/// This is a temporary local bridge: the existing artifact format records
/// status as one of `"completed" | "failed" | "skipped"` plus a free-form
/// `reason` string. Once str-hy9b.A2 plumbs real `InvocationOutcome`s through
/// the executor, the renderer adapter should consume that field directly and
/// this mapping can be deleted.
// TODO(str-hy9b.A2): replace with the real InvocationOutcome on
// ExploreSummaryEntry once outcome plumbing lands.
fn outcome_status_from_entry(entry: &ExploreSummaryEntry) -> shatter_core::protocol::OutcomeStatus {
    use shatter_core::protocol::OutcomeStatus;
    let reason = entry.reason.as_deref().unwrap_or("");
    match entry.status.as_str() {
        "completed" => OutcomeStatus::Completed,
        "failed" => {
            if reason.contains("timeout") || reason.contains("timed out") {
                OutcomeStatus::TimedOut
            } else {
                OutcomeStatus::RuntimeFailed
            }
        }
        "skipped" => {
            if reason.contains("unexecutable") {
                OutcomeStatus::Unsupported
            } else {
                OutcomeStatus::SkippedByPolicy
            }
        }
        // Defensive default: an unknown status string came from a future
        // artifact version. Surface it as runtime_failed so the function still
        // gets a section in the report instead of vanishing.
        _ => OutcomeStatus::RuntimeFailed,
    }
}

/// Default human-readable reason for an entry that lacks one.
fn default_reason_for(entry: &ExploreSummaryEntry) -> String {
    match entry.status.as_str() {
        "completed" => "exploration completed".to_string(),
        "failed" => "exploration failed".to_string(),
        "skipped" => "skipped".to_string(),
        other => format!("status: {other}"),
    }
}

fn combine_explore_markdown(
    md_fragments: &[(String, String)],
    summaries: &[ExploreSummary],
) -> String {
    let detail_by_name: HashMap<&str, &str> = md_fragments
        .iter()
        .map(|(name, md)| (name.as_str(), md.as_str()))
        .collect();

    let entries_owned: Vec<(String, shatter_core::protocol::OutcomeStatus, String)> = summaries
        .iter()
        .flat_map(|summary| summary.functions.iter())
        .map(|entry| {
            let status = outcome_status_from_entry(entry);
            let reason = entry
                .reason
                .clone()
                .unwrap_or_else(|| default_reason_for(entry));
            (entry.function_name.clone(), status, reason)
        })
        .collect();

    let entries: Vec<shatter_core::report::OutcomeRenderEntry<'_>> = entries_owned
        .iter()
        .map(
            |(name, status, reason)| shatter_core::report::OutcomeRenderEntry {
                qualified_name: name.as_str(),
                status: *status,
                reason: reason.as_str(),
                detail_md: detail_by_name.get(name.as_str()).copied(),
            },
        )
        .collect();

    shatter_core::report::render_explore_outcomes(
        &entries,
        "discovery returned no functions for this run",
    )
}

/// Minimum iterations-without-discovery before the periodic progress line
/// appends an `(idle N)` tag. Zero or one would be noise on the very first
/// snapshot right after the explore loop warms up.
const IDLE_STREAK_THRESHOLD: u32 = 2;

/// Render a periodic explore progress snapshot as a single human-readable
/// stderr line. Shared between random and concolic explorer paths so the two
/// produce visually identical output.
///
/// Output example:
///   `[12s] classifyNumber: 847 iters, 5 paths, 8/12 branches, mcdc 3/7, 55.2 iter/s (idle 320)`
fn format_progress_snapshot(snapshot: &ExploreProgressSnapshot) -> String {
    let secs = snapshot.elapsed.as_secs();
    let total_branches_label = snapshot
        .total_branches
        .map_or_else(|| "?".to_string(), |t| t.to_string());
    let rate = if snapshot.elapsed.as_secs_f64() > 0.0 {
        snapshot.iterations as f64 / snapshot.elapsed.as_secs_f64()
    } else {
        0.0
    };

    let branches_segment = match snapshot.branches_covered {
        Some(covered) => format!("{covered}/{total_branches_label} branches"),
        None => format!("{}/{} paths", snapshot.paths_found, total_branches_label),
    };

    let mut line = format!(
        "[{secs}s] {}: {} iters, {} paths, {}, {:.1} iter/s",
        snapshot.function_name, snapshot.iterations, snapshot.paths_found, branches_segment, rate,
    );

    if let Some((total, independent, _opaque)) = snapshot.mcdc_summary {
        line.push_str(&format!(", mcdc {independent}/{total}"));
    }

    if snapshot.iters_since_new_discovery >= IDLE_STREAK_THRESHOLD {
        line.push_str(&format!(" (idle {})", snapshot.iters_since_new_discovery));
    }

    line
}

fn emit_explore_progress(
    function: &str,
    current: usize,
    total: usize,
    elapsed: Duration,
    status: &str,
    emit_json: bool,
) {
    let line = match status {
        "started" => format!("[progress] starting {current}/{total}: {function}"),
        "completed" => format!(
            "[progress] completed {current}/{total}: {function} ({:.1}s)",
            elapsed.as_secs_f64()
        ),
        "failed" => format!(
            "[progress] failed {current}/{total}: {function} ({:.1}s)",
            elapsed.as_secs_f64()
        ),
        other => format!("[progress] {other} {current}/{total}: {function}"),
    };
    eprintln!("{line}");

    if emit_json
        && let Some(json) =
            ProgressEvent::with_status(function, current, total, elapsed.as_millis() as u64, status)
                .to_json()
    {
        eprintln!("{json}");
    }
}

/// Options controlling how a single function result is assembled into report output.
struct AssemblyOpts<'a> {
    show_spec: bool,
    spec_as_json: bool,
    detect_invariants: bool,
    use_concolic: bool,
    solver_timeout_ms: Option<u64>,
    show_perf: bool,
    use_color: bool,
    output_format: crate::args::OutputFormat,
    report_style: shatter_core::report_style::ReportStyle,
    project_root: Option<&'a str>,
    deep_fingerprints: &'a HashMap<String, String>,
    persist_stages: Option<&'a Path>,
    output_path_set: bool,
    stdout: bool,
    report_outputs_empty: bool,
}

/// Accumulator for per-function assembly results.
struct AssemblyAccumulator {
    total_paths: usize,
    total_covered: usize,
    total_lines: u32,
    html_fragments: Vec<String>,
    /// Per-function detail markdown produced for `Completed` outcomes, keyed
    /// by function name so the outcome-driven renderer can join detail to
    /// outcome by name regardless of fragment ordering.
    md_fragments: Vec<(String, String)>,
    file_specs: Vec<shatter_core::spec::FunctionSpec>,
}

impl AssemblyAccumulator {
    fn new() -> Self {
        Self {
            total_paths: 0,
            total_covered: 0,
            total_lines: 0,
            html_fragments: Vec::new(),
            md_fragments: Vec::new(),
            file_specs: Vec::new(),
        }
    }
}

/// Assemble report/spec output for a single completed function result.
/// Shared between the live explore path and the finalize-from-artifacts path.
#[allow(clippy::too_many_arguments)]
fn assemble_function_result(
    func: &shatter_core::protocol::FunctionAnalysis,
    result: &shatter_core::explorer::ObservationOutput,
    file_str: &str,
    wall_time: Duration,
    mock_symbols: &[String],
    ga_stats: Option<GeneticStats>,
    opts: &AssemblyOpts<'_>,
    acc: &mut AssemblyAccumulator,
) {
    // Accumulate stats for footer.
    acc.total_paths += result.unique_paths;
    acc.total_covered += result.lines_covered;
    acc.total_lines += result.total_lines;

    // HTML fragment for -o report files.
    {
        let location = format!("{file_str}:{}-{}", func.start_line, func.end_line);
        acc.html_fragments
            .push(shatter_core::report::render_explore_fn_html(
                result,
                &location,
                opts.project_root.map(std::path::Path::new),
            ));
    }

    // Run the Analyze stage to get coverage metrics and eq classes.
    let analyze_output = {
        let _pipeline_analyze_span = tracing::info_span!("pipeline.analyze").entered();
        shatter_core::pipeline::analyze(result, func)
    };
    let location = format!("{file_str}:{}-{}", func.start_line, func.end_line);

    if let Some(persist_root) = opts.persist_stages
        && let Err(err) = persist_stage_outputs(
            persist_root,
            file_str,
            func,
            result,
            &analyze_output,
            opts.solver_timeout_ms,
            opts.detect_invariants,
        )
    {
        log::error!("failed to persist stage outputs for {}: {err}", func.name);
    }

    // Render report fragments for file output regardless of log level.
    let should_print_report =
        log::log_enabled!(log::Level::Info) && (opts.report_outputs_empty || opts.stdout);
    if log::log_enabled!(log::Level::Trace) {
        let report = {
            let _report_span = tracing::info_span!("report.render").entered();
            explorer::format_exploration_report_verbose(result)
        };
        acc.md_fragments.push((func.name.clone(), report.clone()));
        if should_print_report {
            print!("{report}");
        }
    } else if opts.output_format == crate::args::OutputFormat::Md {
        let view = crate::render::explore_fn_view(
            result,
            crate::render::ExploreRenderOpts {
                location: Some(&location),
                mocks_used: mock_symbols,
                is_concolic: opts.use_concolic,
            },
        );
        let md = {
            let _report_span = tracing::info_span!("report.render").entered();
            crate::render::render_explore_fn(&view)
        };
        acc.md_fragments.push((func.name.clone(), md.clone()));
        if should_print_report {
            print_markdown(&md, opts.use_color);
        }
    } else {
        let report_opts = ReportOptions {
            location: Some(location.clone()),
            show_perf: opts.show_perf,
            wall_time: Some(wall_time),
            coverage_metrics: Some(analyze_output.coverage_metrics.clone()),
            style: opts.report_style.clone(),
            genetic_stats: ga_stats,
        };
        let report = {
            let _report_span = tracing::info_span!("report.render").entered();
            explorer::format_exploration_report(result, &report_opts)
        };
        acc.md_fragments.push((func.name.clone(), report.clone()));
        if should_print_report {
            print!("{report}");
            if !mock_symbols.is_empty() {
                println!("  Mocks used: {}", mock_symbols.join(", "));
            }
            if opts.use_concolic {
                println!("  Explorer: concolic (Z3-backed)");
            }
        }
    }
    if should_print_report {
        println!();
    }

    // Spec output: use eq classes from analyze stage.
    if opts.show_spec || opts.detect_invariants {
        let eq_classes = &analyze_output.eq_classes;
        let location = Some(location);
        let fingerprint = opts.deep_fingerprints.get(&func.name).cloned();

        let spec = {
            let _spec_span = tracing::info_span!("spec.build").entered();
            if opts.detect_invariants {
                shatter_core::spec::build_spec_with_invariants(
                    result,
                    eq_classes,
                    location,
                    fingerprint,
                )
            } else {
                shatter_core::spec::build_spec(result, eq_classes, location, fingerprint)
            }
        };
        if opts.output_path_set {
            acc.file_specs.push(spec);
        } else if opts.spec_as_json {
            match shatter_core::spec::format_spec_json(&spec) {
                Ok(json) => println!("{json}"),
                Err(e) => log::error!("Error serializing spec: {e}"),
            }
        } else {
            print_markdown(
                &shatter_core::spec::format_spec_markdown(&spec),
                opts.use_color,
            );
        }
    }
}

fn persist_stage_outputs(
    persist_root: &Path,
    file_str: &str,
    func: &shatter_core::protocol::FunctionAnalysis,
    observation: &shatter_core::explorer::ObservationOutput,
    analyze_output: &shatter_core::pipeline::AnalyzeOutput,
    solver_timeout_ms: Option<u64>,
    detect_invariants: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let stage_dir = stage_persistence_dir(persist_root, file_str, func);
    let observe_stage = shatter_core::pipeline::ObserveStageOutput {
        observation: serde_json::from_value(serde_json::to_value(observation)?)?,
        analysis: func.clone(),
        file: file_str.to_string(),
    };
    shatter_core::pipeline::write_observe_stage(&observe_stage, &stage_dir.join("observe.json"))?;

    let analyze_stage = shatter_core::pipeline::AnalyzeStageOutput {
        analyze: shatter_core::pipeline::AnalyzeOutput {
            eq_classes: analyze_output.eq_classes.clone(),
            behavior_map: analyze_output.behavior_map.clone(),
            coverage_metrics: analyze_output.coverage_metrics.clone(),
        },
        spec: None,
        function_name: func.name.clone(),
        file: file_str.to_string(),
    };
    shatter_core::pipeline::write_analyze_stage(&analyze_stage, &stage_dir.join("analyze.json"))?;

    let solve_output = shatter_core::pipeline::solve(&observe_stage, solver_timeout_ms);
    let solve_stage = shatter_core::pipeline::SolveStageOutput {
        solve: shatter_core::pipeline::StageSolveOutput {
            solved_branches: solve_output.solved_branches.clone(),
            metrics: solve_output.metrics.clone(),
        },
        function_name: func.name.clone(),
        file: file_str.to_string(),
    };
    shatter_core::pipeline::write_solve_stage(&solve_stage, &stage_dir.join("solve.json"))?;

    let specify_stage = shatter_core::pipeline::specify(
        &observe_stage,
        analyze_output,
        &solve_output,
        detect_invariants,
    );
    shatter_core::pipeline::write_specify_stage(&specify_stage, &stage_dir.join("specify.json"))?;

    Ok(())
}

fn stage_persistence_dir(
    persist_root: &Path,
    file_str: &str,
    func: &shatter_core::protocol::FunctionAnalysis,
) -> std::path::PathBuf {
    let file_component = sanitize_artifact_component(file_str);
    let function_component = sanitize_artifact_component(&func.name);
    persist_root
        .join(file_component)
        .join(format!("{:05}_{function_component}", func.start_line))
}

/// Finalize an explore run from saved artifacts on disk. Reads per-function
/// artifacts, reconstructs reports and specs, and writes output files.
#[allow(clippy::too_many_arguments)]
fn finalize_explore(
    artifact_dir: &Path,
    output_path: Option<&Path>,
    report_outputs: &[PathBuf],
    show_spec: bool,
    spec_as_json: bool,
    detect_invariants: bool,
    use_color: bool,
    output_format: crate::args::OutputFormat,
    format: crate::args::StdoutFormat,
    stdout: bool,
    show_perf: bool,
    use_concolic: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let artifacts = load_explore_artifacts(artifact_dir)?;
    let summaries = load_explore_summaries(artifact_dir)?;
    if artifacts.is_empty() && summaries.is_empty() {
        return Err("no explore artifacts found in the specified directory".into());
    }

    log::info!(
        "Loaded {} artifact(s) from {}",
        artifacts.len(),
        artifact_dir.display()
    );

    let report_style = if use_color {
        shatter_core::report_style::ReportStyle::ansi()
    } else {
        shatter_core::report_style::ReportStyle::default()
    };

    let empty_fingerprints: HashMap<String, String> = HashMap::new();
    let opts = AssemblyOpts {
        show_spec: show_spec || detect_invariants || output_path.is_some(),
        spec_as_json: spec_as_json || output_path.is_some(),
        detect_invariants,
        use_concolic,
        solver_timeout_ms: None,
        show_perf,
        use_color,
        output_format,
        report_style: report_style.clone(),
        project_root: None,
        deep_fingerprints: &empty_fingerprints,
        persist_stages: None,
        output_path_set: output_path.is_some(),
        stdout,
        report_outputs_empty: report_outputs.is_empty(),
    };

    let mut acc = AssemblyAccumulator::new();
    let mut total_function_count: usize = if summaries.is_empty() {
        0
    } else {
        summaries
            .iter()
            .map(|summary| summary.total_functions)
            .sum()
    };

    // Print header.
    if log::log_enabled!(log::Level::Info) {
        if output_format == crate::args::OutputFormat::Md {
            print_markdown(
                "# Shatter Explore (finalized from artifacts)\n\n",
                use_color,
            );
        } else {
            print!(
                "\n{bold}\u{2550}\u{2550}\u{2550} Shatter Explore (finalized) \u{2550}\u{2550}\u{2550}{reset}\n\n",
                bold = report_style.bold,
                reset = report_style.reset,
            );
        }
    }

    for artifact in &artifacts {
        if summaries.is_empty() {
            total_function_count += 1;
        }

        if artifact.status != "completed" {
            let reason = artifact.error.as_deref().unwrap_or("unknown");
            log::info!(
                "Skipping {} (status={}, reason={})",
                artifact.function_name,
                artifact.status,
                reason,
            );
            continue;
        }

        let observation = match &artifact.observation {
            Some(obs) => obs,
            None => {
                log::warn!(
                    "Artifact for {} has status=completed but no observation data",
                    artifact.function_name
                );
                continue;
            }
        };

        let wall_time = Duration::from_millis(artifact.wall_time_ms);

        assemble_function_result(
            &artifact.analysis,
            observation,
            &artifact.file,
            wall_time,
            &artifact.mock_symbols,
            None, // GA stats not available from artifacts
            &opts,
            &mut acc,
        );
    }

    // The trailing "Failed/Skipped" section that used to be printed here is
    // now subsumed by the outcome-driven renderer: every discovered function
    // — including failed and skipped ones — gets its own section in the file
    // report (combine_explore_markdown). When streaming to stdout, per-
    // function progress lines already surface failed/skipped functions, so
    // duplicating them here would only repeat the information.

    // Print summary footer.
    if log::log_enabled!(log::Level::Info) && (report_outputs.is_empty() || stdout) {
        if output_format == crate::args::OutputFormat::Md {
            let coverage_suffix = if acc.total_lines > 0 {
                let pct = ((acc.total_covered as f64 / acc.total_lines as f64) * 100.0)
                    .min(100.0)
                    .round() as u32;
                format!(
                    " · **{pct}%** coverage ({}/{} lines)",
                    acc.total_covered, acc.total_lines
                )
            } else {
                String::new()
            };
            print_markdown(
                &format!(
                    "\n---\n\n**Summary:** {} path(s) across \
                     {total_function_count} function(s){coverage_suffix}\n",
                    acc.total_paths
                ),
                use_color,
            );
        } else {
            print!(
                "{}",
                explorer::format_explore_footer(
                    acc.total_paths,
                    total_function_count,
                    acc.total_covered,
                    acc.total_lines,
                    &report_style,
                )
            );
        }
    }

    // Write report files.
    for path in report_outputs {
        match crate::args::infer_output_format(path) {
            Ok(crate::args::StdoutFormat::Html) => {
                let html = shatter_core::report::wrap_explore_html(
                    &acc.html_fragments,
                    total_function_count,
                    acc.total_paths,
                    acc.total_covered,
                    acc.total_lines,
                );
                if let Some(parent) = path.parent()
                    && !parent.as_os_str().is_empty()
                {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| format!("failed to create directory: {e}"))?;
                }
                std::fs::write(path, html).map_err(|e| {
                    format!("failed to write HTML report to '{}': {e}", path.display())
                })?;
                log::info!("Wrote HTML report to {}", path.display());
            }
            Ok(crate::args::StdoutFormat::Markdown) => {
                let md = combine_explore_markdown(&acc.md_fragments, &summaries);
                if let Some(parent) = path.parent()
                    && !parent.as_os_str().is_empty()
                {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| format!("failed to create directory: {e}"))?;
                }
                std::fs::write(path, &md).map_err(|e| {
                    format!(
                        "failed to write markdown report to '{}': {e}",
                        path.display()
                    )
                })?;
                log::info!("Wrote markdown report to {}", path.display());
            }
            Ok(crate::args::StdoutFormat::Text) => {
                let md = combine_explore_markdown(&acc.md_fragments, &summaries);
                let text = shatter_core::report::strip_markdown_text(&md);
                if let Some(parent) = path.parent()
                    && !parent.as_os_str().is_empty()
                {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| format!("failed to create directory: {e}"))?;
                }
                std::fs::write(path, &text).map_err(|e| {
                    format!("failed to write text report to '{}': {e}", path.display())
                })?;
                log::info!("Wrote text report to {}", path.display());
            }
            Ok(crate::args::StdoutFormat::Json) => {
                if !acc.file_specs.is_empty() {
                    let bundle = FileSpecBundle {
                        file: artifacts
                            .first()
                            .map(|a| a.file.clone())
                            .unwrap_or_default(),
                        functions: acc.file_specs.clone(),
                    };
                    shatter_core::spec::write_file_spec_bundle(&bundle, path).map_err(|e| {
                        format!("failed to write spec bundle to '{}': {e}", path.display())
                    })?;
                    log::info!("Wrote spec bundle to {}", path.display());
                }
            }
            Err(e) => {
                log::error!("{e}");
            }
        }
    }

    // Replay to stdout if report files were also written.
    if !report_outputs.is_empty() && stdout {
        let combined = combine_explore_markdown(&acc.md_fragments, &summaries);
        match format {
            crate::args::StdoutFormat::Text => {
                print!("{}", shatter_core::report::strip_markdown_text(&combined));
            }
            _ => {
                print_markdown(&combined, use_color);
            }
        }
    }

    // Write spec bundle.
    if let Some(out) = output_path
        && !acc.file_specs.is_empty()
    {
        let bundle = FileSpecBundle {
            file: artifacts
                .first()
                .map(|a| a.file.clone())
                .unwrap_or_default(),
            functions: acc.file_specs,
        };
        shatter_core::spec::write_file_spec_bundle(&bundle, out)
            .map_err(|e| format!("failed to write spec bundle to {}: {e}", out.display()))?;
        log::info!("Wrote spec bundle to {}", out.display());
    }

    Ok(())
}

/// Run the explore command.
// Each argument corresponds to a CLI flag; grouping into a struct would add indirection
// without improving clarity since this is only called from one callsite.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_explore(
    targets: &[String],
    max_iterations: Option<u32>,
    timeout: u64,
    timeout_explore: Option<f64>,
    scope_path: Option<&Path>,
    analyze_only: bool,
    _show_clusters: bool,
    cache_dir: Option<&Path>,
    no_cache: bool,
    request_timeout: u64,
    exec_timeout: u64,
    build_timeout: u64,
    release: bool,
    timing_enabled: bool,
    inputs_path: Option<&Path>,
    config_path: Option<&Path>,
    output_path: Option<&Path>,
    log_level: LogLevel,
    show_perf: bool,
    colors: &Colors,
    show_spec: bool,
    spec_as_json: bool,
    detect_invariants: bool,
    use_concolic: bool,
    solver_timeout: Option<u64>,
    memory_limit: Option<u64>,
    clean: bool,
    dry_run: bool,
    project_dir: Option<&Path>,
    loop_buckets_str: &str,
    use_color: bool,
    seeds_dir: &Path,
    no_seeds: bool,
    record: bool,
    set_overrides: &[String],
    meta_config: &shatter_core::strategy::MetaConfig,
    observe_output: Option<&Path>,
    persist_stages: Option<&Path>,
    replay_recorded: bool,
    no_replay: bool,
    refine_budget: usize,
    shrink_budget: usize,
    mcdc: bool,
    isolation: shatter_core::explorer::IsolationMode,
    capture_side_effects: bool,
    output_format: crate::args::OutputFormat,
    report_outputs: &[std::path::PathBuf],
    stdout: bool,
    format: crate::args::StdoutFormat,
    workers: usize,
    cli_genetic: bool,
    cli_genetic_population: Option<u32>,
    cli_genetic_generations: Option<u32>,
    cli_genetic_timeout: Option<u32>,
    from_artifacts: Option<&Path>,
    time_limit: Option<f64>,
    coverage_threshold: Option<f64>,
    max_executions: Option<u64>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Early return: finalize from saved artifacts instead of running exploration.
    if let Some(artifact_dir) = from_artifacts {
        return finalize_explore(
            artifact_dir,
            output_path,
            report_outputs,
            show_spec,
            spec_as_json,
            detect_invariants,
            use_color,
            output_format,
            format,
            stdout,
            show_perf,
            use_concolic,
        );
    }
    let _explore_span = tracing::info_span!("core.explore_command").entered();
    let pool_path = if no_seeds {
        None
    } else {
        Some(seeds_dir.join("pool.json"))
    };
    let loop_buckets = parse_loop_buckets(loop_buckets_str)?;
    let scope_config = match scope_path {
        Some(path) => {
            let config = ScopeConfig::from_file(path)
                .map_err(|e| format!("failed to load scope config: {e}"))?;
            log::info!("Loaded scope config from {}", path.display());
            config
        }
        None => ScopeConfig::default(),
    };

    let _scope_matcher =
        ScopeMatcher::new(&scope_config).map_err(|e| format!("invalid scope config: {e}"))?;

    let cache = if no_cache {
        None
    } else {
        let dir = match cache_dir {
            Some(p) => p.to_path_buf(),
            None => BehaviorMapCache::default_dir(&std::env::current_dir()?),
        };
        Some(BehaviorMapCache::new(dir).map_err(|e| format!("failed to initialize cache: {e}"))?)
    };

    // Stored-inputs sidecar cache (str-bo4z.3). Colocated with behavior maps;
    // provides seed inputs that survive body edits (signature-keyed).
    let stored_inputs_cache = if no_cache {
        None
    } else {
        let dir = match cache_dir {
            Some(p) => p.to_path_buf(),
            None => StoredInputsCache::default_dir(&std::env::current_dir()?),
        };
        StoredInputsCache::new(dir)
            .map_err(|e| {
                log::warn!("failed to initialize stored-inputs cache: {e}");
                e
            })
            .ok()
    };

    let parsed: Vec<Target> = targets
        .iter()
        .map(|t| parse_target(t))
        .collect::<Result<Vec<_>, _>>()?;
    validate_targets(&parsed)?;

    let req_timeout = Duration::from_secs(request_timeout);

    let mut file_spec_bundles: Vec<FileSpecBundle> = Vec::new();
    let mut report_summaries: Vec<ExploreSummary> = Vec::new();

    let report_style = if use_color {
        shatter_core::report_style::ReportStyle::ansi()
    } else {
        shatter_core::report_style::ReportStyle::default()
    };
    let solver_timeout_ms = solver_timeout.map(|seconds| seconds * 1000);

    // Count total functions across all targets for header/footer.
    let mut total_function_count: usize = 0;
    let mut total_paths: usize = 0;
    let mut total_covered: usize = 0;
    let mut total_lines: u32 = 0;
    let mut header_printed = false;

    // Resolve effective worker count: 0 means auto-detect (CPU count).
    let effective_workers = if workers == 0 {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
    } else {
        workers
    };

    // Resolve project root once for harness storage env propagation.
    // Explicit --project-dir wins; otherwise auto-detect from the first target.
    let storage_project_root = resolve_project_root(project_dir, &parsed[0].file);

    // Build per-language FrontendConfig templates for spawning per-function explore
    // frontends.  Also spawn one shared frontend per language for the analysis phase
    // (analysis is fast and doesn't benefit from parallelism).
    let mut frontends: HashMap<crate::args::Language, Frontend> = HashMap::new();
    let mut fe_configs: HashMap<crate::args::Language, FrontendConfig> = HashMap::new();
    let unique_langs: HashSet<crate::args::Language> = parsed.iter().map(|t| t.language).collect();
    for lang in unique_langs {
        let mut config = frontend_config(
            lang,
            req_timeout,
            log_level,
            exec_timeout,
            build_timeout,
            memory_limit,
            None,
            timing_enabled,
            release,
        )?;
        apply_project_storage(&mut config, storage_project_root.as_deref());
        if mcdc {
            config
                .env_vars
                .push(("SHATTER_MCDC".to_string(), "1".to_string()));
        }
        fe_configs.insert(lang, config.clone());
        let frontend = Frontend::spawn(&config)
            .await
            .map_err(|e| format!("failed to spawn {} frontend: {e}", lang.label()))?;
        log::debug!(
            "Frontend connected (language={})",
            frontend.language().unwrap_or("unknown")
        );
        frontends.insert(lang, frontend);
    }
    log::info!(
        "Spawned {} frontend session(s) for {} target(s) ({} parallel worker(s))",
        frontends.len(),
        parsed.len(),
        effective_workers,
    );

    // Accumulate HTML and markdown fragments for -o report files.
    let mut html_fragments: Vec<String> = Vec::new();
    let mut md_fragments: Vec<(String, String)> = Vec::new();

    // --- Shared state across all targets (str-b2my.10) ---
    //
    // Before str-b2my.10, each target ran a self-contained prepare → batch →
    // post-processing cycle. The scheduler hoist unifies the batch loop across
    // targets so newly discovered functions can enter the queue via
    // BatchScheduler::enqueue while workers started by an earlier target are
    // still active. Prepare writes into the shared vectors; the unified batch
    // loop drains them; the post-processing pass walks `prepared_targets`.
    let mut prepared_targets: Vec<PreparedTarget> = Vec::new();
    let mut work_items: Vec<FuncWorkItem> = Vec::new();
    let mut accumulators: Vec<ExploreResultAccumulator> = Vec::new();
    let mut func_wall_time: Vec<Duration> = Vec::new();
    let mut func_first_error: Vec<Option<String>> = Vec::new();

    let mut batch_scheduler =
        shatter_core::batch_scheduler::BatchScheduler::with_individual_budgets(
            &[],
            EXPLORE_BATCH_ITERATIONS,
        );
    let mut batches_launched: u32 = 0;
    let mut batches_completed: u32 = 0;

    let semaphore = Arc::new(tokio::sync::Semaphore::new(effective_workers));
    let completed_functions = Arc::new(AtomicUsize::new(0));
    let mut join_set: tokio::task::JoinSet<BatchExploreOutcome> = tokio::task::JoinSet::new();

    let time_limit_dur = time_limit.map(Duration::from_secs_f64);
    let run_start = Instant::now();

    let mut stop_early = false;
    let mut frontier_exhausted_announced = false;
    let mut stop_reason: Option<String> = None;
    let mut total_executions_count: u64 = 0;
    let mut total_branches_seen: usize = 0;
    let mut total_branches_covered: usize = 0;

    // Periodic progress callback (hoisted — value does not depend on target).
    let periodic_progress: Option<Arc<Box<ProgressCallback>>> = if log_level >= LogLevel::Info {
        Some(Arc::new(Box::new(|snapshot: &ExploreProgressSnapshot| {
            eprintln!("{}", format_progress_snapshot(snapshot));
        })))
    } else {
        None
    };

    // Per-function resume state for the concolic orchestrator (str-b2my.16).
    // Carries covered_paths and discovery inputs between batches so batch 2+
    // skips path rediscovery and starts from frontier-adjacent seeds.
    // Declared before the per-target loop so the resume logic (str-b2my.15)
    // can pre-populate it from persisted sidecars during setup.
    let mut explore_states: HashMap<usize, shatter_core::orchestrator::ExploreState> =
        HashMap::new();
    let mut resumed_total: usize = 0;

    for target in &parsed {
        let file_str = target.file.to_string_lossy();
        let func_display = target.function.as_deref().unwrap_or("(all)");

        let project_root_str = resolve_project_root(project_dir, &target.file);

        if let Some(ref root) = project_root_str {
            log::debug!("Project root: {root}");
        }
        log::debug!(
            "Exploring {file_str}:{func_display} [language={}, max_iterations={}]",
            target.language.label(),
            max_iterations.map_or("unlimited".to_string(), |n| n.to_string()),
        );

        let frontend = frontends
            .get_mut(&target.language)
            .expect("frontend must exist for target language — spawned above");

        // Analyze phase
        let analyze_response = frontend
            .send(ProtoCommand::Analyze {
                file: file_str.to_string(),
                function: target.function.clone(),
                project_root: project_root_str.clone(),
                execution_profile: None,
            })
            .instrument(tracing::info_span!("frontend.analyze"))
            .await
            .map_err(|e| format!("analyze failed: {e}"))?;

        match &analyze_response.result {
            ResponseResult::Analyze { functions } => {
                log::debug!("Found {} function(s):", functions.len());
                for func in functions {
                    log::debug!(
                        "  - {} ({} params, {} branches)",
                        func.name,
                        func.params.len(),
                        func.branches.len(),
                    );
                }
            }
            ResponseResult::Error { code, message, .. } => {
                log::error!("Analyze error ({code:?}): {message}");
                continue;
            }
            other => {
                log::error!("Unexpected analyze response: {other:?}");
                continue;
            }
        }

        let functions = match &analyze_response.result {
            ResponseResult::Analyze { functions } => functions.clone(),
            _ => unreachable!("already matched above"),
        };

        if analyze_only {
            if log::log_enabled!(log::Level::Info) {
                for func in &functions {
                    println!(
                        "{}{}{}  ({file_str}:{})",
                        colors.bold, func.name, colors.reset, func.start_line
                    );
                    println!(
                        "  {}params: {}, branches: {}{}",
                        colors.dim,
                        func.params.len(),
                        func.branches.len(),
                        colors.reset
                    );

                    // Show adapter selection results.
                    if let Ok(selection) =
                        adapter_selection::select_adapters(None, &func.adapter_hints)
                    {
                        for active in &selection.active {
                            println!(
                                "  {}adapter [active]: {} ({}){}",
                                colors.bold, active.adapter.id, active.provenance, colors.reset,
                            );
                        }
                        for suggested in &selection.suggested {
                            println!(
                                "  {}adapter [suggested]: {} [{:?}]{}",
                                colors.dim,
                                suggested.adapter.id,
                                suggested.confidence,
                                colors.reset,
                            );
                        }
                    }
                }
            }
            continue;
        }

        // Load cached fingerprints for cross-file dependencies.
        let external_fingerprints = {
            let _cache_load_span =
                tracing::info_span!("cache.load_external_fingerprints").entered();
            load_external_fingerprints(&functions, cache.as_ref())
        };

        // Incremental plan: compare fingerprints against existing spec when --output is set
        let incremental_plan = if let Some(out) = output_path
            && !clean
            && out.exists()
        {
            match shatter_core::spec::read_file_spec_bundle(out) {
                Ok(existing) => {
                    match shatter_core::spec::compute_incremental_plan(
                        &target.file,
                        &functions,
                        &existing,
                        &external_fingerprints,
                    ) {
                        Ok(plan) => Some((plan, existing)),
                        Err(e) => {
                            log::debug!("Failed to compute incremental plan: {e}");
                            None
                        }
                    }
                }
                Err(e) => {
                    log::debug!("Failed to read existing spec: {e}");
                    None
                }
            }
        } else {
            None
        };

        let fresh_set: HashSet<String> = incremental_plan
            .as_ref()
            .map(|(plan, _)| plan.fresh.iter().cloned().collect())
            .unwrap_or_default();

        // Dry-run mode: print incremental plan and exit
        if dry_run {
            if let Some((ref plan, _)) = incremental_plan {
                if !plan.stale.is_empty() {
                    println!("Stale ({}):", plan.stale.len());
                    for name in &plan.stale {
                        println!("  {name}");
                    }
                }
                if !plan.fresh.is_empty() {
                    println!("Fresh ({}):", plan.fresh.len());
                    for name in &plan.fresh {
                        println!("  {name}");
                    }
                }
                if !plan.removed.is_empty() {
                    println!("Removed ({}):", plan.removed.len());
                    for name in &plan.removed {
                        println!("  {name}");
                    }
                }
            } else {
                println!(
                    "No existing spec to compare against — all {} function(s) are stale.",
                    functions.len()
                );
                for func in &functions {
                    println!("  {}", func.name);
                }
            }
            continue;
        }

        if !fresh_set.is_empty() && log::log_enabled!(log::Level::Info) {
            log::info!("Skipping {} fresh function(s):", fresh_set.len());
            for name in &fresh_set {
                log::info!("  {name}");
            }
        }

        // Load .shatter/ config for this target
        let shatter_configs: Vec<ShatterConfig> = if let Some(cp) = config_path {
            // Explicit config bypasses discovery
            let cfg = shatter_config::parse_config(cp)
                .map_err(|e| format!("failed to load config: {e}"))?;
            log::debug!("Loaded config from {}", cp.display());
            vec![cfg]
        } else {
            // Hierarchical discovery from target file's directory
            let target_dir = target.file.parent().unwrap_or(Path::new("."));
            shatter_config::discover_configs(target_dir)
                .map_err(|e| format!("config discovery error: {e}"))?
        };

        // Compute deep fingerprints (call-graph-aware) for spec output.
        let deep_fingerprints: std::collections::HashMap<String, String> =
            shatter_core::fingerprint::compute_deep_fingerprints(
                &target.file,
                &functions,
                &external_fingerprints,
            )
            .unwrap_or_default();

        // Track function count for header/footer.
        total_function_count += functions.len();

        // Print header on first non-analyze-only target.
        if !analyze_only && !header_printed && log::log_enabled!(log::Level::Info) {
            if output_format == crate::args::OutputFormat::Md {
                print_markdown("# Shatter Explore\n\n", use_color);
            } else {
                print!(
                    "\n{bold}\u{2550}\u{2550}\u{2550} Shatter Explore \u{2550}\u{2550}\u{2550}{reset}\n\n",
                    bold = report_style.bold,
                    reset = report_style.reset,
                );
            }
            header_printed = true;
        }

        // Exploration phase: generate random inputs and execute.
        //
        // Three phases:
        //   1. Collect work items (sequential — config resolution, mock generation)
        //   2. Parallel exploration (tokio::spawn per function, each with its own frontend)
        //   3. Process results (sequential — stats, reports, specs)
        let mut skipped_unexecutable: Vec<(String, Vec<executability::SkipReason>)> = Vec::new();

        // Capture capabilities from the shared analysis frontend for ExploreConfig construction.
        let frontend_caps =
            shatter_core::orchestrator::FrontendCapabilities::from_raw(frontend.capabilities());

        // --- Phase 1: Collect work items for this target ---
        // Pushes into the shared `work_items` vector hoisted out of the per-
        // target loop by str-b2my.10. The scheduler clones each work item per
        // dispatched batch and overrides `max_iterations` to the scheduler's
        // per-batch slice size; multi-batch functions re-enqueue until their
        // budget is exhausted.
        //
        // `first_work_index` captures the slice start in the shared vector so
        // the post-prepare enqueue loop can walk only this target's items.
        let first_work_index = work_items.len();
        for func in &functions {
            // Skip fresh functions in incremental mode
            if fresh_set.contains(&func.name) {
                continue;
            }

            let function_id = format!("{}:{}", file_str, func.name);

            // Resolve per-function config
            let resolved = shatter_config::resolve_function_config_with_inputs(
                &function_id,
                target.file.parent().unwrap_or(Path::new(".")),
                inputs_path,
                max_iterations,
                timeout,
                set_overrides,
            )
            .map_err(|e| format!("config resolution error for {}: {e}", func.name))?;

            // Run adapter selection policy: merge config profile with frontend hints.
            let adapter_selection_result = adapter_selection::select_adapters(
                resolved.execution_profile.as_ref(),
                &func.adapter_hints,
            )
            .map_err(|e| format!("adapter selection error for {}: {e}", func.name))?;

            let resolved_execution_profile = adapter_selection_result.to_execution_profile();

            for active in &adapter_selection_result.active {
                log::info!(
                    "  {} adapter [active]: {} ({})",
                    func.name,
                    active.adapter.id,
                    active.provenance,
                );
            }
            for suggested in &adapter_selection_result.suggested {
                log::info!(
                    "  {} adapter [suggested]: {} [{:?}]",
                    func.name,
                    suggested.adapter.id,
                    suggested.confidence,
                );
            }
            for rejected in &adapter_selection_result.rejected {
                log::warn!(
                    "  {} adapter [rejected]: {} — {}",
                    func.name,
                    rejected.adapter_id,
                    rejected.reason,
                );
            }

            // Merge CLI --genetic flags with config.yaml resolved genetic config.
            // CLI --genetic explicitly enables; when absent, config.yaml provides defaults.
            let effective_genetic = if cli_genetic {
                GeneticConfig {
                    enabled: true,
                    population_size: cli_genetic_population
                        .unwrap_or(resolved.genetic.population_size),
                    max_generations: cli_genetic_generations
                        .unwrap_or(resolved.genetic.max_generations),
                    timeout_secs: cli_genetic_timeout.unwrap_or(resolved.genetic.timeout_secs),
                    ..resolved.genetic
                }
            } else {
                resolved.genetic.clone()
            };

            if resolved.skip {
                log::debug!("Skipping {} (skip=true in config)", func.name);
                continue;
            }

            // Check for unexecutable parameter types (opaque types like net.Socket).
            let skip_reasons = executability::check_executability(&func.params, &[]);
            if !skip_reasons.is_empty() {
                log::debug!("Skipping {} (unexecutable parameter types)", func.name);
                skipped_unexecutable.push((func.name.clone(), skip_reasons));
                continue;
            }

            // Generate mocks: passthrough in record mode, auto-mocks otherwise.
            let (auto_mocks, mock_params) = if record {
                let passthrough =
                    shatter_core::recorded_mocks::build_passthrough_mocks(&func.dependencies);
                (passthrough, vec![])
            } else {
                // Check for recorded mock fixtures to seed from prior --record runs.
                let recorded_configs = if !no_replay {
                    let artifacts_dir = std::path::Path::new("shatter-artifacts");
                    let legacy_dir = std::path::Path::new(".shatter");
                    let should_replay = replay_recorded
                        || artifacts_dir
                            .join(shatter_core::recorded_mocks::RECORDED_MOCKS_DIR)
                            .is_dir()
                        || legacy_dir
                            .join(shatter_core::recorded_mocks::RECORDED_MOCKS_DIR)
                            .is_dir();
                    if should_replay {
                        // Check new location first, then fall back to legacy .shatter/
                        if let Some(mock_path) = shatter_core::recorded_mocks::find_recorded_mocks(
                            artifacts_dir,
                            &file_str,
                            &func.name,
                        )
                        .or_else(|| {
                            shatter_core::recorded_mocks::find_recorded_mocks(
                                legacy_dir, &file_str, &func.name,
                            )
                        }) {
                            match shatter_core::recorded_mocks::load_recorded_mocks(&mock_path) {
                                Ok(mock_file) => {
                                    let configs = shatter_core::recorded_mocks::recorded_mocks_to_mock_configs(&mock_file);
                                    log::info!(
                                        "Loaded {} recorded mock(s) for {} from {}",
                                        configs.len(),
                                        func.name,
                                        mock_path.display(),
                                    );
                                    configs
                                }
                                Err(e) => {
                                    log::warn!(
                                        "Failed to load recorded mocks for {} from {}: {e}",
                                        func.name,
                                        mock_path.display(),
                                    );
                                    vec![]
                                }
                            }
                        } else {
                            vec![]
                        }
                    } else {
                        vec![]
                    }
                } else {
                    vec![]
                };

                let auto_generated = shatter_core::auto_mock::generate_auto_mocks(
                    &func.dependencies,
                    None,
                    &resolved.mock_overrides,
                    &recorded_configs,
                );
                // Recorded configs first (higher priority), then auto-generated for remaining deps.
                let mut mocks = recorded_configs;
                mocks.extend(auto_generated);
                let params = shatter_core::auto_mock::build_mock_params(&func.dependencies, &mocks);
                (mocks, params)
            };
            let mock_symbols: Vec<String> = auto_mocks.iter().map(|m| m.symbol.clone()).collect();

            // Build candidate inputs from config, then extend with cached seeds
            // from prior exploration runs so discovery compounds across runs.
            let mut candidate_inputs: Vec<Vec<serde_json::Value>> = resolved
                .candidate_inputs
                .iter()
                .map(|input| input.args.clone())
                .collect();
            if let Some(ref cache) = cache
                && let Ok(Some(cached_map)) = cache.load(&function_id)
            {
                let cached_seeds = cached_map.extract_seed_inputs();
                if !cached_seeds.is_empty() {
                    log::debug!(
                        "Loaded {} cached seed(s) for {}",
                        cached_seeds.len(),
                        func.name,
                    );
                    candidate_inputs.extend(cached_seeds);
                }
            }

            // Load stored inputs (str-bo4z.4): signature-keyed inputs that
            // survive body edits where BehaviorMapCache would miss.
            if let Some(ref sic) = stored_inputs_cache {
                let sig = FunctionSignature::from_analysis(func);
                match sic.load_compatible(&function_id, &sig) {
                    Ok(Some(stored)) if !stored.is_empty() => {
                        log::debug!("Loaded {} stored input(s) for {}", stored.len(), func.name,);
                        candidate_inputs.extend(stored);
                    }
                    _ => {}
                }
            }

            let explore_config = ExploreConfig {
                file: file_str.to_string(),
                max_iterations: resolved.max_iterations,
                seed: None,
                mocks: auto_mocks,
                mock_params,
                setup_file: resolved.setup.as_ref().map(|p| p.display().to_string()),
                setup_level: resolved.setup_level,
                value_sources: shatter_core::input_gen::resolve_value_sources(
                    &func.params,
                    &resolved.param_generators,
                    &resolved.generators,
                ),
                capabilities: frontend_caps.clone(),
                user_seeds: vec![],
                candidate_inputs,
                pool_seeds: match &pool_path {
                    Some(pp) => match shatter_core::interesting_pool::load_pool(pp) {
                        Ok(Some(pool)) => {
                            shatter_core::input_gen::pool_to_candidate_inputs(&func.params, &pool)
                        }
                        _ => vec![],
                    },
                    None => vec![],
                },
                project_root: project_root_str.clone(),
                execution_profile: resolved_execution_profile.clone(),
                loop_buckets: loop_buckets.clone(),
                timeout_explore: timeout_explore.map(Duration::from_secs_f64),
                meta_config: meta_config.clone(),
                shrink_budget,
                isolation,
                capture_side_effects,
                budget_surplus: None,
                claim_policy: shatter_core::scan_orchestrator::ClaimPolicy::default(),
            };

            // Build concolic-specific config if needed.
            let (concolic_config, seed_inputs, user_inputs) = if use_concolic {
                let mut seeds = shatter_core::boundary_dict::generate_boundary_inputs(&func.params);
                let users: Vec<Vec<serde_json::Value>> = resolved
                    .candidate_inputs
                    .iter()
                    .map(|input| input.args.clone())
                    .collect();

                // Add pool-derived seeds for concolic mode
                if let Some(ref pp) = pool_path
                    && let Ok(Some(pool)) = shatter_core::interesting_pool::load_pool(pp)
                {
                    let pool_candidates =
                        shatter_core::input_gen::pool_to_candidate_inputs(&func.params, &pool);
                    seeds.extend(pool_candidates);
                }

                // Literal-derived seeds: string/number constants from static analysis
                let literal_candidates = shatter_core::input_gen::literals_to_candidate_inputs(
                    &func.params,
                    &func.literals,
                );
                seeds.extend(literal_candidates);

                // Add cached seeds from prior exploration runs.
                if let Some(ref cache) = cache
                    && let Ok(Some(cached_map)) = cache.load(&function_id)
                {
                    let cached_seeds = cached_map.extract_seed_inputs();
                    if !cached_seeds.is_empty() {
                        log::debug!(
                            "Loaded {} cached seed(s) for concolic on {}",
                            cached_seeds.len(),
                            func.name,
                        );
                        seeds.extend(cached_seeds);
                    }
                }

                // Load stored inputs (str-bo4z.4): signature-keyed inputs that
                // survive body edits where BehaviorMapCache would miss.
                if let Some(ref sic) = stored_inputs_cache {
                    let sig = FunctionSignature::from_analysis(func);
                    match sic.load_compatible(&function_id, &sig) {
                        Ok(Some(stored)) if !stored.is_empty() => {
                            log::debug!(
                                "Loaded {} stored input(s) for concolic on {}",
                                stored.len(),
                                func.name,
                            );
                            seeds.extend(stored);
                        }
                        _ => {}
                    }
                }

                let cc = shatter_core::orchestrator::ExploreConfig {
                    max_iterations: explore_config.max_iterations.map(|n| n as usize),
                    max_executions: explore_config.max_iterations.map(|n| (n as usize) * 5),
                    plateau_threshold: if mcdc { 60 } else { 20 },
                    mocks: explore_config.mocks.clone(),
                    mock_params: explore_config.mock_params.clone(),
                    solver_timeout_ms: solver_timeout.map(|s| s * 1000),
                    timeout_explore: timeout_explore.map(Duration::from_secs_f64),
                    branch_profile: None, // standalone concolic has no prior random phase
                    meta_config: meta_config.clone(),
                    execution_profile: explore_config.execution_profile.clone(),
                    loop_convergence_window: 3,
                    refine_budget: if refine_budget > 0 {
                        Some(refine_budget)
                    } else {
                        None
                    },
                    shrink_budget,
                    mcdc,
                    fuzz: resolved.fuzz.clone(),
                };
                (Some(cc), seeds, users)
            } else {
                (None, vec![], vec![])
            };

            if !resolved.candidate_inputs.is_empty() {
                log::debug!(
                    "Exploring {} ({} candidate input(s) from config)...",
                    func.name,
                    resolved.candidate_inputs.len()
                );
            } else {
                log::debug!("Exploring {}...", func.name);
            }

            let _ = &shatter_configs; // suppress unused warning

            let known_targets = shatter_core::coverage_metrics::discover_known_targets(func);
            work_items.push(FuncWorkItem {
                func: func.clone(),
                explore_config,
                mock_symbols,
                concolic_config,
                seed_inputs,
                user_inputs,
                genetic_config: effective_genetic,
                language: target.language,
                file_str: file_str.to_string(),
                project_root_str: project_root_str.clone(),
                target_idx: prepared_targets.len(),
                known_targets,
            });
        }

        // --- Append this target's work items to the shared scheduler. ---
        //
        // The unified batch loop runs after the target loop; it drains the
        // shared scheduler across every prepared target. Walking by index
        // rather than consuming `work_items` keeps the slice available for
        // spawn closures in the batch loop.
        let artifact_root = explore_artifact_root(project_root_str.as_deref());
        let target_start = Instant::now();

        // --- Resume detection (str-b2my.15): load prior summary and skip
        // functions that completed in an earlier run with a fresh fingerprint.
        let prior_summary = read_explore_summary(&artifact_root, &file_str);
        let mut target_resumed_count: usize = 0;

        if let Some(ref prior) = prior_summary {
            log::info!(
                "Found prior explore summary for {} ({} completed, {} failed)",
                file_str,
                prior.completed,
                prior.failed,
            );
        }

        for (work_index, item) in work_items.iter().enumerate().skip(first_work_index) {
            accumulators.push(ExploreResultAccumulator::new(item.func.name.clone()));
            func_wall_time.push(Duration::ZERO);
            func_first_error.push(None);

            // Try to resume from a prior completed artifact.
            if let Some((observation, wall_time)) = try_resume_function(
                &artifact_root,
                &item.func,
                &deep_fingerprints,
                prior_summary.as_ref(),
            ) {
                accumulators[work_index].merge(Ok(observation));
                func_wall_time[work_index] = wall_time;
                target_resumed_count += 1;
                log::info!(
                    "[resumed] {}: {} branches, {:.1}s (prior run)",
                    item.func.name,
                    accumulators[work_index].discoveries.len(),
                    wall_time.as_secs_f64(),
                );
                // Do NOT enqueue in scheduler — this function is already done.
                continue;
            }

            // Check for partial resume state (ExploreState sidecar).
            if let Some(state) = read_resume_state(&artifact_root, &file_str, &item.func) {
                let paths_count = state.covered_paths.len();
                explore_states.insert(work_index, state);
                log::info!(
                    "Loaded partial resume state for {} ({} covered paths)",
                    item.func.name,
                    paths_count,
                );
            }

            // Only enqueue functions with concrete branch targets (str-b2my.11).
            // Functions with no branches have nothing to explore — skip them
            // rather than scheduling speculative work.
            if item.known_targets.is_empty() {
                log::debug!("Skipping {} — no branch targets to explore", item.func.name,);
                continue;
            }

            let budget = item.explore_config.max_iterations;
            batch_scheduler.enqueue(work_index, budget);
        }

        if target_resumed_count > 0 {
            resumed_total += target_resumed_count;
            log::info!(
                "Resumed {target_resumed_count}/{} function(s) from prior artifacts for {}",
                work_items.len() - first_work_index,
                file_str,
            );
        }

        // Initialize explore summary for crash-recovery.
        let explore_summary = ExploreSummary {
            version: EXPLORE_ARTIFACT_VERSION,
            status: "running".to_string(),
            file: file_str.to_string(),
            total_functions: work_items.len() - first_work_index,
            completed: 0,
            failed: 0,
            skipped: skipped_unexecutable.len(),
            elapsed_secs: 0.0,
            functions: skipped_unexecutable
                .iter()
                .map(|(name, _)| ExploreSummaryEntry {
                    function_name: name.clone(),
                    status: "skipped".to_string(),
                    artifact: None,
                    reason: Some("unexecutable parameter types".to_string()),
                    deep_fingerprint: None,
                })
                .collect(),
        };
        if let Err(e) = write_explore_summary(&artifact_root, &file_str, &explore_summary) {
            log::warn!("Failed to write initial explore summary: {e}");
        }

        let work_item_indices: Vec<usize> = (first_work_index..work_items.len()).collect();

        prepared_targets.push(PreparedTarget {
            language: target.language,
            file_str: file_str.to_string(),
            project_root_str: project_root_str.clone(),
            functions: functions.clone(),
            fresh_set: fresh_set.clone(),
            incremental_plan,
            deep_fingerprints,
            skipped_unexecutable,
            artifact_root,
            target_start,
            explore_summary,
            work_item_indices,
        });
    }

    // --- Unified batch loop: drain the shared scheduler across all targets ---
    //
    // The loop keeps up to `effective_workers` batches in flight at once. After
    // each join_next(), it merges the outcome into the owning function's
    // accumulator and records its exhaustion state back to the scheduler,
    // which may re-enqueue the function for another batch if budget remains
    // and it didn't converge early. Because the scheduler is shared across
    // every target's work items (str-b2my.10), a single loop drains the
    // whole run instead of one per target.

    let emit_progress_json =
        format == crate::args::StdoutFormat::Json || log_level >= LogLevel::Debug;

    loop {
        // Launch sub-loop: fill in-flight slots up to --workers.
        while join_set.len() < effective_workers && !stop_early {
            if let Some(limit) = time_limit_dur
                && run_start.elapsed() >= limit
            {
                stop_early = true;
                stop_reason = Some(format!("time limit ({:.1}s)", limit.as_secs_f64()));
                break;
            }
            let batch_config = match batch_scheduler.next_batch() {
                Some(b) => b,
                None => break,
            };
            let work_index = batch_config.task_index;
            let batch_iters = batch_config.batch_size;
            let mut item = work_items[work_index].clone();
            // Clamp per-batch iteration caps so orchestrator::explore /
            // explore_function stop at the scheduler's assigned slice
            // rather than running to the function's full configured cap.
            item.explore_config.max_iterations = Some(batch_iters);
            if let Some(ref mut cc) = item.concolic_config {
                cc.max_iterations = Some(batch_iters as usize);
                cc.max_executions = Some((batch_iters as usize) * 5);
            }

            batches_launched += 1;

            // Extract resume state for this function (if a prior batch completed).
            let resume_state = explore_states.remove(&work_index);

            let sem = Arc::clone(&semaphore);
            let completed_functions = Arc::clone(&completed_functions);
            let fe_config = fe_configs
                .get(&item.language)
                .expect("fe_config must exist for target language")
                .clone();
            let file_str_owned = item.file_str.clone();
            let project_root_owned = item.project_root_str.clone();
            let progress_index = work_index + 1;
            let progress_total = work_items.len();
            let periodic_progress_clone = periodic_progress.clone();

            join_set.spawn(async move {
                let _permit = sem.acquire().await.expect("semaphore is never closed");
                let func_start = Instant::now();
                emit_explore_progress(
                    &item.func.name,
                    progress_index,
                    progress_total,
                    Duration::ZERO,
                    "started",
                    emit_progress_json,
                );

                let mut task_frontend = match Frontend::spawn(&fe_config).await {
                    Ok(fe) => fe,
                    Err(e) => {
                        let completed = completed_functions.fetch_add(1, Ordering::Relaxed) + 1;
                        emit_explore_progress(
                            &item.func.name,
                            completed,
                            progress_total,
                            func_start.elapsed(),
                            "failed",
                            emit_progress_json,
                        );
                        return BatchExploreOutcome {
                            work_index,
                            result: Err(format!("failed to spawn frontend: {e}")),
                            wall_time: func_start.elapsed(),
                            batch_iteration_cap: batch_iters,
                            resume_state: None,
                        };
                    }
                };

                // Build progress hints shared across both explorer paths so
                // concolic and random runs surface identical stat lines.
                let cb_ref: Option<&ProgressCallback> = periodic_progress_clone
                    .as_ref()
                    .map(|arc| arc.as_ref().as_ref());
                let progress_hints = cb_ref.map(|cb| shatter_core::explorer::ProgressHints {
                    callback: cb,
                    total_branches: Some(item.func.branches.len()),
                });

                let instrument_mocks = item
                    .concolic_config
                    .as_ref()
                    .map(|config| config.mocks.as_slice())
                    .unwrap_or_else(|| item.explore_config.mocks.as_slice());
                let mut observe_input = shatter_core::pipeline_orchestrator::ObserveInput {
                    frontend: &mut task_frontend,
                    file: file_str_owned.clone(),
                    function_name: item.func.name.clone(),
                    analysis: item.func.clone(),
                    explore_config: item.explore_config.clone(),
                    use_concolic: item.concolic_config.is_some(),
                    concolic_config: item.concolic_config.clone(),
                    prepare_id: None,
                    project_root: project_root_owned.clone(),
                    extra_seeds: vec![],
                };
                let observe_result = shatter_core::pipeline_orchestrator::run_observe_stage(
                    &mut observe_input,
                    shatter_core::pipeline_orchestrator::ObserveStageOptions {
                        instrument_mocks,
                        concolic_seed_inputs: &item.seed_inputs,
                        concolic_user_inputs: &item.user_inputs,
                        progress_hints,
                        resume_state,
                        ..shatter_core::pipeline_orchestrator::ObserveStageOptions::default()
                    },
                )
                .instrument(tracing::info_span!("pipeline.observe"))
                .await;

                let (result, batch_resume_state) = match observe_result {
                    Ok(stage_result) => (
                        Ok(stage_result.observe.observation),
                        stage_result.resume_state,
                    ),
                    Err(err) => (Err(err.to_string()), None),
                };
                let completed = completed_functions.fetch_add(1, Ordering::Relaxed) + 1;
                emit_explore_progress(
                    &item.func.name,
                    completed,
                    progress_total,
                    func_start.elapsed(),
                    if result.is_ok() {
                        "completed"
                    } else {
                        "failed"
                    },
                    emit_progress_json,
                );

                let _ = task_frontend.shutdown().await;

                BatchExploreOutcome {
                    work_index,
                    result,
                    wall_time: func_start.elapsed(),
                    batch_iteration_cap: batch_iters,
                    resume_state: batch_resume_state,
                }
            });
        }

        if join_set.is_empty() {
            break;
        }

        let batch_outcome = match join_set.join_next().await {
            Some(Ok(o)) => o,
            Some(Err(e)) => {
                log::error!("Task join error: {e}");
                continue;
            }
            None => break,
        };

        let work_index = batch_outcome.work_index;

        // Store resume state for the next batch of this function (str-b2my.16).
        // Also persist to disk so a subsequent run can skip path rediscovery
        // for partially-explored functions (str-b2my.15).
        if let Some(state) = batch_outcome.resume_state {
            let target_idx = work_items[work_index].target_idx;
            if let Some(pt) = prepared_targets.get(target_idx)
                && let Err(e) = write_resume_state(
                    &pt.artifact_root,
                    &pt.file_str,
                    &work_items[work_index].func,
                    &state,
                )
            {
                log::warn!(
                    "Failed to write resume state for {}: {e}",
                    work_items[work_index].func.name
                );
            }
            explore_states.insert(work_index, state);
        }

        let iters_used = batch_outcome
            .result
            .as_ref()
            .map(|obs| obs.iterations)
            .unwrap_or(0);
        let exhausted =
            batch_is_exhausted(&batch_outcome.result, batch_outcome.batch_iteration_cap);

        // Score this batch by the number of branch discoveries it added
        // that the accumulator had never seen before. This is the rerank
        // signal for str-b2my.7: a function still uncovering new paths
        // each batch ranks higher than one whose last batch produced
        // nothing new, so the scheduler keeps running the productive
        // function back-to-back until its yield drops.
        let batch_rank = new_discoveries_in_batch(
            batch_outcome.result.as_ref().ok(),
            &accumulators[work_index].discoveries,
        ) as i64;

        batch_scheduler.record_outcome(shatter_core::batch_scheduler::BatchOutcome {
            task_index: work_index,
            iterations_used: iters_used,
            exhausted,
            rank: batch_rank,
            summary: None,
        });
        batches_completed += 1;

        total_executions_count += iters_used as u64;
        if let Ok(ref obs) = batch_outcome.result {
            total_branches_seen += obs.total_lines as usize;
            total_branches_covered += obs.unique_paths;
        }

        // Snapshot the accumulator's prior state before merge so we can
        // distinguish "first batch for this function" (no prior state yet)
        // from a re-enqueue that added zero new discoveries (the idle
        // signal required by str-cii2).
        let prior_batches_merged = accumulators[work_index].batches_merged;

        if log_level >= LogLevel::Info {
            let paths = batch_outcome
                .result
                .as_ref()
                .map(|obs| obs.unique_paths)
                .unwrap_or(0);
            let status = if batch_outcome.result.is_ok() {
                "ok"
            } else {
                "err"
            };
            // Cumulative per-function stats include the freshly completed
            // batch merged in: covered = prior ∪ this batch's branch IDs,
            // MC/DC = component-wise max across all batches so far.
            let prior_covered = accumulators[work_index].discoveries.len();
            let new_branches_this_batch = batch_rank as usize;
            let cumulative_covered = prior_covered + new_branches_this_batch;
            let total_branches_for_func = work_items[work_index].func.branches.len();
            let cumulative_mcdc = match (
                accumulators[work_index].mcdc_summary,
                batch_outcome
                    .result
                    .as_ref()
                    .ok()
                    .and_then(|obs| obs.mcdc_summary),
            ) {
                (Some(cur), Some(new)) => {
                    Some((cur.0.max(new.0), cur.1.max(new.1), cur.2.max(new.2)))
                }
                (None, new) => new,
                (cur, None) => cur,
            };

            let mut line = format!(
                "[batch {}/{}] {}: {} iters, {} paths, {}/{} branches",
                batches_completed,
                batches_launched,
                accumulators[work_index].function_name,
                iters_used,
                paths,
                cumulative_covered,
                total_branches_for_func,
            );
            if let Some((total, independent, _)) = cumulative_mcdc {
                line.push_str(&format!(", mcdc {independent}/{total}"));
            }
            line.push_str(&format!(
                ", {:.1}s ({})",
                batch_outcome.wall_time.as_secs_f64(),
                status,
            ));

            // Show attempt penalty when a function has consecutive
            // no-progress batches (str-b2my.9).
            let attempt_pen = batch_scheduler.attempt_penalty(work_index);
            if attempt_pen > 0 {
                line.push_str(&format!(", penalty -{attempt_pen}"));
            }

            // Re-enqueue idle signal: prior batches exist, this one added
            // no new branch discoveries, and the function will keep being
            // scheduled because it did not exhaust its iteration cap.
            if prior_batches_merged > 0 && new_branches_this_batch == 0 && !exhausted {
                if batch_scheduler.is_frontier_exhausted() {
                    line.push_str(" (revisiting)");
                } else {
                    line.push_str(" (continuing without new discoveries)");
                }
            }

            // Show cooldown score when non-zero (str-b2my.8).
            let cd = batch_scheduler.cooldown_score(work_index);
            if cd > 0 {
                line.push_str(&format!(" [cooldown: {cd}]"));
            }

            // Append active/queued function status (str-b2my.4).
            if work_items.len() > 1 {
                let active_indices: Vec<usize> = batch_scheduler.in_flight_indices().collect();
                let queued_indices: Vec<usize> = batch_scheduler.queued_indices().collect();

                let format_names = |indices: &[usize], limit: usize| -> String {
                    if indices.is_empty() {
                        return String::new();
                    }
                    let names: Vec<&str> = indices
                        .iter()
                        .take(limit)
                        .map(|&i| work_items[i].func.name.as_str())
                        .collect();
                    let mut s = names.join(", ");
                    if indices.len() > limit {
                        s.push_str(&format!(" +{} more", indices.len() - limit));
                    }
                    s
                };

                let active_names = format_names(&active_indices, 3);
                let queued_names = format_names(&queued_indices, 3);

                let mut parts = Vec::new();
                if !active_indices.is_empty() {
                    parts.push(format!(
                        "active: {} ({})",
                        active_indices.len(),
                        active_names
                    ));
                }
                if !queued_indices.is_empty() {
                    parts.push(format!(
                        "queued: {} ({})",
                        queued_indices.len(),
                        queued_names
                    ));
                }
                if !parts.is_empty() {
                    line.push_str(&format!(" | {}", parts.join(", ")));
                }
            }

            eprintln!("{line}");
        }

        // Cross-function fallback detection (str-b2my.5).
        if batch_scheduler.is_frontier_exhausted() {
            if !frontier_exhausted_announced {
                frontier_exhausted_announced = true;
                if log_level >= LogLevel::Info {
                    eprintln!(
                        "  frontier work exhausted across all functions; \
                             continuing with corpus mutations"
                    );
                }
            }
        } else {
            // New frontier work appeared (e.g., dynamically enqueued
            // function); allow re-announcement if fallback is re-entered.
            frontier_exhausted_announced = false;
        }

        func_wall_time[work_index] += batch_outcome.wall_time;
        if let Err(ref e) = batch_outcome.result
            && func_first_error[work_index].is_none()
        {
            func_first_error[work_index] = Some(e.clone());
        }
        accumulators[work_index].merge(batch_outcome.result);

        if let Some(limit) = time_limit_dur
            && run_start.elapsed() >= limit
        {
            stop_early = true;
            stop_reason = Some(format!("time limit ({:.1}s)", limit.as_secs_f64()));
        }
        if let Some(max_exec) = max_executions
            && total_executions_count >= max_exec
        {
            stop_early = true;
            stop_reason = Some(format!(
                "max executions ({max_exec}, {total_executions_count} total)"
            ));
        }
        if let Some(threshold) = coverage_threshold
            && total_branches_seen > 0
        {
            let coverage_pct = total_branches_covered as f64 / total_branches_seen as f64 * 100.0;
            if coverage_pct >= threshold {
                stop_early = true;
                stop_reason = Some(format!(
                    "coverage threshold ({threshold:.1}%, {coverage_pct:.1}% actual)"
                ));
            }
        }
    }

    if let Some(reason) = &stop_reason {
        log::info!("Stop flag reached: {reason}; draining in-flight batches");
    }
    // Drain remaining in-flight tasks so spawned frontends are cleaned up.
    if stop_early {
        join_set.abort_all();
    }
    while let Some(joined) = join_set.join_next().await {
        if let Ok(batch_outcome) = joined {
            let work_index = batch_outcome.work_index;
            func_wall_time[work_index] += batch_outcome.wall_time;
            accumulators[work_index].merge(batch_outcome.result);
        }
    }

    if resumed_total > 0 {
        log::info!("Resumed {resumed_total} function(s) from prior explore artifacts");
    }

    // --- Phase 3a: Flush every accumulator → outcomes ---
    // Build the full global list, then group by target so each prepared
    // target's post-processing pass can write its own artifacts and spec
    // bundle against its owning context (file_str, artifact_root, etc.).
    let taken_accumulators = std::mem::take(&mut accumulators);
    let mut outcomes_by_target: HashMap<usize, Vec<FuncExploreOutcome>> = HashMap::new();
    for (work_index, accum) in taken_accumulators.into_iter().enumerate() {
        // Skip functions that never had a batch launched (only possible if
        // stop_early fired before any batch for this work_index dispatched).
        if accum.batches_merged == 0 {
            continue;
        }
        let result = accum.into_result();
        let target_idx = work_items[work_index].target_idx;
        let outcome = FuncExploreOutcome {
            work_index,
            func: work_items[work_index].func.clone(),
            mock_symbols: work_items[work_index].mock_symbols.clone(),
            result,
            wall_time: func_wall_time[work_index],
            genetic_config: work_items[work_index].genetic_config.clone(),
        };
        outcomes_by_target
            .entry(target_idx)
            .or_default()
            .push(outcome);
    }

    // --- Phase 3b: Per-target post-processing ---
    // Walk prepared_targets (destructively so we can take ownership of the
    // per-target state carried in `PreparedTarget`). For each target, emit
    // its artifacts, run the GA follow-up pass, assemble reports / spec
    // bundles, and finalize the crash-recovery explore summary.
    let taken_prepared = std::mem::take(&mut prepared_targets);
    for (target_idx, prepared) in taken_prepared.into_iter().enumerate() {
        let mut target_outcomes = outcomes_by_target.remove(&target_idx).unwrap_or_default();
        target_outcomes.sort_by_key(|outcome| outcome.work_index);

        let PreparedTarget {
            language: target_language,
            file_str,
            project_root_str,
            functions: target_functions,
            fresh_set: _,
            incremental_plan,
            deep_fingerprints,
            skipped_unexecutable,
            artifact_root,
            target_start,
            mut explore_summary,
            work_item_indices: _,
        } = prepared;
        let mut file_specs: Vec<shatter_core::spec::FunctionSpec> = Vec::new();

        for outcome in &target_outcomes {
            let artifact_relpath = match write_explore_artifact(&artifact_root, &file_str, outcome)
            {
                Ok(path) => {
                    log::info!(
                        "Wrote explore artifact for {} -> {}",
                        outcome.func.name,
                        path.display()
                    );
                    path.strip_prefix(&artifact_root)
                        .ok()
                        .map(|p| p.to_string_lossy().to_string())
                }
                Err(e) => {
                    log::warn!(
                        "Failed to write explore artifact for {}: {e}",
                        outcome.func.name
                    );
                    None
                }
            };

            let summary_status = if outcome.result.is_ok() {
                explore_summary.completed += 1;
                "completed"
            } else {
                explore_summary.failed += 1;
                "failed"
            };
            explore_summary.functions.push(ExploreSummaryEntry {
                function_name: outcome.func.name.clone(),
                status: summary_status.to_string(),
                artifact: artifact_relpath,
                reason: outcome.result.as_ref().err().cloned(),
                deep_fingerprint: deep_fingerprints.get(&outcome.func.name).cloned(),
            });

            // Clean up the partial resume-state sidecar now that the function
            // is fully done (str-b2my.15).
            cleanup_resume_state(&artifact_root, &file_str, &outcome.func);
        }
        explore_summary.elapsed_secs = target_start.elapsed().as_secs_f64();
        if let Err(e) = write_explore_summary(&artifact_root, &file_str, &explore_summary) {
            log::warn!("Failed to update explore summary: {e}");
        }

        for outcome in target_outcomes {
            let func = &outcome.func;

            match outcome.result {
                Ok(result) => {
                    let wall_time = outcome.wall_time;
                    let mock_symbols = &outcome.mock_symbols;

                    // Harvest interesting inputs into the cross-function pool.
                    if let Some(ref pp) = pool_path {
                        let mut pool = shatter_core::interesting_pool::load_pool(pp)
                            .unwrap_or_else(|e| {
                                log::warn!("failed to load interesting pool: {e}");
                                None
                            })
                            .unwrap_or_default();
                        let harvested = shatter_core::interesting_pool::harvest_from_exploration(
                            &mut pool,
                            &result.raw_results,
                            &func.params,
                            &func.name,
                            if mcdc {
                                shatter_core::interesting_pool::CoverageMode::Mcdc
                            } else {
                                shatter_core::interesting_pool::CoverageMode::Branch
                            },
                        );
                        if harvested > 0
                            && let Err(e) = shatter_core::interesting_pool::save_pool(&pool, pp)
                        {
                            log::warn!("failed to save interesting pool: {e}");
                        }
                    }

                    // Record mode: persist external dependency observations.
                    if record {
                        let behaviors = shatter_core::recorded_mocks::aggregate_recordings(
                            &result.raw_results,
                            &func.dependencies,
                        );
                        if !behaviors.is_empty() {
                            let mock_file = shatter_core::recorded_mocks::build_recorded_mock_file(
                                &func.name, &file_str, behaviors,
                            );
                            let artifacts_dir = std::path::Path::new("shatter-artifacts");
                            match shatter_core::recorded_mocks::save_recorded_mocks(
                                &mock_file,
                                artifacts_dir,
                            ) {
                                Ok(path) => log::info!(
                                    "Recorded {} dep(s) for {} -> {}",
                                    mock_file.dependencies.len(),
                                    func.name,
                                    path.display(),
                                ),
                                Err(e) => log::error!(
                                    "Failed to save recorded mocks for {}: {e}",
                                    func.name,
                                ),
                            }
                        }
                    }

                    // Save raw observation data for offline analysis if requested.
                    if let Some(obs_dir) = observe_output {
                        let safe_name = func
                            .name
                            .replace(|c: char| !c.is_alphanumeric() && c != '_', "_");
                        let obs_path = obs_dir.join(format!("{safe_name}.observe.json"));
                        let stage_json = serde_json::json!({
                            "observation": &result,
                            "analysis": func,
                            "file": file_str,
                        });
                        if let Some(parent) = obs_path.parent() {
                            let _ = std::fs::create_dir_all(parent);
                        }
                        match serde_json::to_string_pretty(&stage_json) {
                            Ok(json) => {
                                if let Err(e) = std::fs::write(&obs_path, json) {
                                    log::error!(
                                        "Failed to write observe output for {}: {e}",
                                        func.name
                                    );
                                } else {
                                    log::info!("Wrote observe output: {}", obs_path.display());
                                }
                            }
                            Err(e) => log::error!(
                                "Failed to serialize observe output for {}: {e}",
                                func.name
                            ),
                        }
                    }

                    // --- Genetic algorithm follow-up phase ---
                    let mut ga_stored_cache = false;
                    let ga_stats: Option<GeneticStats> = if outcome.genetic_config.enabled {
                        let targets =
                            shatter_core::coverage_metrics::extract_targets(func, &result);
                        if targets.is_empty() {
                            log::debug!("No unsolved targets for GA on {}", func.name);
                            None
                        } else {
                            let targets_attempted = targets.len();
                            log::info!(
                                "Starting GA for {} ({} unsolved target(s))",
                                func.name,
                                targets_attempted,
                            );
                            let mut seed_inputs: Vec<Vec<serde_json::Value>> = result
                                .raw_results
                                .iter()
                                .map(|(inputs, _, _)| inputs.clone())
                                .collect();
                            if let Some(ref cache) = cache {
                                let ga_function_id = format!("{}:{}", file_str, func.name);
                                if let Ok(Some(cached_map)) = cache.load(&ga_function_id) {
                                    seed_inputs.extend(cached_map.extract_seed_inputs());
                                }
                            }
                            let ga_fe_config = fe_configs
                                .get(&target_language)
                                .expect("fe_config must exist for target language")
                                .clone();
                            match Frontend::spawn(&ga_fe_config).await {
                                Ok(mut ga_frontend) => {
                                    let mock_symbols_for_ga: Vec<shatter_core::protocol::MockConfig> =
                                        outcome.mock_symbols.iter().map(|s| {
                                            shatter_core::protocol::MockConfig {
                                                symbol: s.clone(),
                                                return_values: vec![],
                                                should_track_calls: false,
                                                default_behavior: shatter_core::protocol::MockBehavior::ReturnGenerated,
                                            }
                                        }).collect();
                                    let _ = ga_frontend
                                        .send(ProtoCommand::Instrument {
                                            file: file_str.clone(),
                                            function: func.name.clone(),
                                            mocks: mock_symbols_for_ga,
                                            project_root: project_root_str.clone(),
                                            execution_profile: None,
                                        })
                                        .await;
                                    match shatter_core::genetic_explorer::genetic_explore(
                                        &mut ga_frontend,
                                        &func.name,
                                        seed_inputs,
                                        targets,
                                        &func.params,
                                        &outcome.genetic_config,
                                    )
                                    .await
                                    {
                                        Ok(ga_result) => {
                                            let stats = GeneticStats {
                                                targets_attempted,
                                                targets_solved: ga_result.targets_solved,
                                                generations_run: ga_result.generations_run,
                                                total_executions: ga_result.total_executions,
                                            };
                                            if !ga_result.discoveries.is_empty() {
                                                log::info!(
                                                    "GA found {} new behavior(s) for {}",
                                                    ga_result.discoveries.len(),
                                                    func.name,
                                                );
                                                let mut bmap = BehaviorMap::from_exploration_result(
                                                    &func.name, &result,
                                                );
                                                let added = bmap
                                                    .merge_ga_discoveries(&ga_result.discoveries);
                                                if added > 0
                                                    && let Some(ref cache) = cache
                                                {
                                                    if let Err(e) = persist_behavior_map(
                                                        cache,
                                                        &bmap,
                                                        deep_fingerprints
                                                            .get(&func.name)
                                                            .map(String::as_str),
                                                    ) {
                                                        log::warn!(
                                                            "failed to cache GA-augmented behavior map for {}: {e}",
                                                            func.name
                                                        );
                                                    } else {
                                                        ga_stored_cache = true;
                                                    }
                                                }
                                            }
                                            let _ = ga_frontend.shutdown().await;
                                            Some(stats)
                                        }
                                        Err(e) => {
                                            log::error!("GA error for {}: {e}", func.name);
                                            let _ = ga_frontend.shutdown().await;
                                            None
                                        }
                                    }
                                }
                                Err(e) => {
                                    log::error!(
                                        "Failed to spawn GA frontend for {}: {e}",
                                        func.name
                                    );
                                    None
                                }
                            }
                        }
                    } else {
                        None
                    };

                    let assembly_opts = AssemblyOpts {
                        show_spec,
                        spec_as_json,
                        detect_invariants,
                        use_concolic,
                        solver_timeout_ms,
                        show_perf,
                        use_color,
                        output_format,
                        report_style: report_style.clone(),
                        project_root: project_root_str.as_deref(),
                        deep_fingerprints: &deep_fingerprints,
                        persist_stages,
                        output_path_set: output_path.is_some(),
                        stdout,
                        report_outputs_empty: report_outputs.is_empty(),
                    };
                    let mut func_acc = AssemblyAccumulator::new();
                    assemble_function_result(
                        func,
                        &result,
                        &file_str,
                        wall_time,
                        mock_symbols,
                        ga_stats,
                        &assembly_opts,
                        &mut func_acc,
                    );
                    total_paths += func_acc.total_paths;
                    total_covered += func_acc.total_covered;
                    total_lines += func_acc.total_lines;
                    html_fragments.extend(func_acc.html_fragments);
                    md_fragments.extend(func_acc.md_fragments);
                    file_specs.extend(func_acc.file_specs);

                    if !ga_stored_cache {
                        let behavior_map =
                            BehaviorMap::from_exploration_result(&func.name, &result);
                        if let Some(ref cache) = cache {
                            let cache_result = {
                                let _cache_store_span =
                                    tracing::info_span!("cache.store").entered();
                                persist_behavior_map(
                                    cache,
                                    &behavior_map,
                                    deep_fingerprints.get(&func.name).map(String::as_str),
                                )
                            };
                            if let Err(e) = cache_result {
                                log::warn!("failed to cache behavior map for {}: {e}", func.name);
                            }
                        }

                        // Persist to stored-inputs cache (str-bo4z.4) so
                        // standalone explore builds the signature-keyed store.
                        if let Some(ref sic) = stored_inputs_cache {
                            let sig = FunctionSignature::from_analysis(func);
                            let inputs: Vec<Vec<serde_json::Value>> = behavior_map
                                .behaviors
                                .iter()
                                .map(|b| b.input_args.clone())
                                .collect();
                            if let Err(e) =
                                sic.store(&format!("{}:{}", file_str, func.name), &sig, &inputs)
                            {
                                log::warn!(
                                    "failed to persist stored inputs for {}: {e}",
                                    func.name
                                );
                            }
                        }
                    }
                }
                Err(e) => {
                    log::error!("Exploration error for {}: {e}", func.name);
                }
            }
        }

        if !skipped_unexecutable.is_empty() && log::log_enabled!(log::Level::Info) {
            log::info!(
                "Skipped {} function(s) (unexecutable parameter types):",
                skipped_unexecutable.len()
            );
            for (name, reasons) in &skipped_unexecutable {
                for reason in reasons {
                    log::info!("  {name}: {}", reason.format_human());
                }
            }
        }

        explore_summary.status = if explore_summary.failed > 0 {
            "failed".to_string()
        } else {
            "completed".to_string()
        };
        explore_summary.elapsed_secs = target_start.elapsed().as_secs_f64();
        if let Err(e) = write_explore_summary(&artifact_root, &file_str, &explore_summary) {
            log::warn!("Failed to finalize explore summary: {e}");
        }
        report_summaries.push(explore_summary.clone());

        if output_path.is_some() {
            let current_function_names: HashSet<String> =
                target_functions.iter().map(|f| f.name.clone()).collect();

            let bundle = if let Some((_, ref existing)) = incremental_plan {
                shatter_core::spec::merge_file_spec_bundles(
                    existing,
                    &file_specs,
                    &current_function_names,
                )
            } else {
                FileSpecBundle {
                    file: file_str.clone(),
                    functions: file_specs,
                }
            };

            if !bundle.functions.is_empty() {
                file_spec_bundles.push(bundle);
            }
        }
    }

    // Shut down all frontend sessions now that all targets are complete.
    for (_, frontend) in frontends {
        if let Err(e) = frontend.shutdown().await {
            log::warn!("frontend shutdown error: {e}");
        }
    }

    // The trailing "Failed/Skipped" section that used to be printed here is
    // now subsumed by the outcome-driven renderer: every discovered function
    // — including failed and skipped ones — gets its own section in the file
    // report (combine_explore_markdown). Streaming to stdout already surfaces
    // failed/skipped functions via per-function progress lines.

    // Print summary footer (only when streaming to stdout).
    if header_printed
        && log::log_enabled!(log::Level::Info)
        && (report_outputs.is_empty() || stdout)
    {
        if output_format == crate::args::OutputFormat::Md {
            let coverage_suffix = if total_lines > 0 {
                let pct = ((total_covered as f64 / total_lines as f64) * 100.0)
                    .min(100.0)
                    .round() as u32;
                format!(" · **{pct}%** coverage ({total_covered}/{total_lines} lines)")
            } else {
                String::new()
            };
            print_markdown(
                &format!(
                    "\n---\n\n**Summary:** {total_paths} path(s) across \
                     {total_function_count} function(s){coverage_suffix}\n"
                ),
                use_color,
            );
        } else {
            print!(
                "{}",
                explorer::format_explore_footer(
                    total_paths,
                    total_function_count,
                    total_covered,
                    total_lines,
                    &report_style,
                )
            );
        }
    }

    // Write exploration reports to -o files.
    for path in report_outputs {
        match crate::args::infer_output_format(path) {
            Ok(crate::args::StdoutFormat::Html) => {
                let html = shatter_core::report::wrap_explore_html(
                    &html_fragments,
                    total_function_count,
                    total_paths,
                    total_covered,
                    total_lines,
                );
                if let Some(parent) = path.parent()
                    && !parent.as_os_str().is_empty()
                {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| format!("failed to create directory: {e}"))?;
                }
                std::fs::write(path, html).map_err(|e| {
                    format!("failed to write HTML report to '{}': {e}", path.display())
                })?;
                log::info!("Wrote HTML report to {}", path.display());
            }
            Ok(crate::args::StdoutFormat::Markdown) => {
                let md = combine_explore_markdown(&md_fragments, &report_summaries);
                if let Some(parent) = path.parent()
                    && !parent.as_os_str().is_empty()
                {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| format!("failed to create directory: {e}"))?;
                }
                std::fs::write(path, &md).map_err(|e| {
                    format!(
                        "failed to write markdown report to '{}': {e}",
                        path.display()
                    )
                })?;
                log::info!("Wrote markdown report to {}", path.display());
            }
            Ok(crate::args::StdoutFormat::Text) => {
                let md = combine_explore_markdown(&md_fragments, &report_summaries);
                let text = shatter_core::report::strip_markdown_text(&md);
                if let Some(parent) = path.parent()
                    && !parent.as_os_str().is_empty()
                {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| format!("failed to create directory: {e}"))?;
                }
                std::fs::write(path, &text).map_err(|e| {
                    format!("failed to write text report to '{}': {e}", path.display())
                })?;
                log::info!("Wrote text report to {}", path.display());
            }
            Ok(crate::args::StdoutFormat::Json) => {
                // JSON output for explore writes spec bundle
                log::warn!(
                    "JSON output for explore writes spec bundle; use --spec-out for explicit spec output"
                );
                if let Some(first_bundle) = file_spec_bundles.first() {
                    shatter_core::spec::write_file_spec_bundle(first_bundle, path).map_err(
                        |e| format!("failed to write spec bundle to '{}': {e}", path.display()),
                    )?;
                    log::info!("Wrote spec bundle to {}", path.display());
                }
            }
            Err(e) => {
                log::error!("{e}");
            }
        }
    }

    // If files were written and --stdout was also requested, replay to stdout.
    if !report_outputs.is_empty() && stdout {
        let combined = combine_explore_markdown(&md_fragments, &report_summaries);
        match format {
            crate::args::StdoutFormat::Text => {
                print!("{}", shatter_core::report::strip_markdown_text(&combined));
            }
            _ => {
                print_markdown(&combined, use_color);
            }
        }
    }

    // Write collected file spec bundles to the output path as a single bundle.
    if let Some(out) = output_path
        && !file_spec_bundles.is_empty()
    {
        // Single-target is the primary Make use case; write the first bundle.
        {
            let _spec_write_span = tracing::info_span!("spec.write_bundle").entered();
            shatter_core::spec::write_file_spec_bundle(&file_spec_bundles[0], out)
                .map_err(|e| format!("failed to write spec bundle to {}: {e}", out.display()))?;
        }
        log::info!(
            "Wrote spec bundle ({} function(s)) to {}",
            file_spec_bundles[0].functions.len(),
            out.display()
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        EXPLORE_ARTIFACT_VERSION, ExploreResultAccumulator, ExploreSummary, ExploreSummaryEntry,
        FuncExploreOutcome, batch_is_exhausted, emit_explore_progress, explore_summary_path,
        finalize_explore, format_progress_snapshot, load_explore_artifacts, persist_stage_outputs,
        read_explore_artifact, sanitize_artifact_component, stage_persistence_dir,
        write_explore_artifact, write_explore_summary,
    };
    use shatter_core::config::GeneticConfig;
    use shatter_core::explorer::ExploreProgressSnapshot;
    use shatter_core::protocol::{FunctionAnalysis, InvocationModel};
    use shatter_core::report::ProgressEvent;
    use shatter_core::types::TypeInfo;
    use std::time::Duration;

    #[test]
    fn progress_event_with_status_serializes() {
        let json = ProgressEvent::with_status("classifyNumber", 2, 5, 1234, "completed")
            .to_json()
            .expect("serialize");
        let event: ProgressEvent = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(event.status.as_deref(), Some("completed"));
        assert_eq!(event.current, 2);
        assert_eq!(event.total, 5);
    }

    #[test]
    fn emit_explore_progress_accepts_started_completed_and_failed() {
        emit_explore_progress("f", 1, 3, Duration::ZERO, "started", true);
        emit_explore_progress("f", 2, 3, Duration::from_millis(250), "completed", true);
        emit_explore_progress("f", 3, 3, Duration::from_millis(500), "failed", true);
    }

    #[test]
    fn emit_explore_progress_suppresses_json_when_gate_is_false() {
        // Should emit human-readable lines but no JSON — verifies no panic
        // when emit_json is false.
        emit_explore_progress("f", 1, 2, Duration::ZERO, "started", false);
        emit_explore_progress("f", 2, 2, Duration::from_millis(100), "completed", false);
    }

    #[test]
    fn progress_json_gate_respects_format_and_log_level() {
        use crate::args::StdoutFormat;
        use shatter_core::log_level::LogLevel;

        // JSON format => emit regardless of log level
        assert!(StdoutFormat::Json == StdoutFormat::Json || LogLevel::Info >= LogLevel::Debug);
        // Markdown + default Info => suppress
        assert!(
            !(StdoutFormat::Markdown == StdoutFormat::Json || LogLevel::Info >= LogLevel::Debug)
        );
        // Markdown + Debug => emit
        assert!(StdoutFormat::Markdown == StdoutFormat::Json || LogLevel::Debug >= LogLevel::Debug);
        // Markdown + Trace => emit
        assert!(StdoutFormat::Markdown == StdoutFormat::Json || LogLevel::Trace >= LogLevel::Debug);
    }

    fn base_snapshot() -> ExploreProgressSnapshot {
        ExploreProgressSnapshot {
            function_name: "classifyNumber".to_string(),
            elapsed: Duration::from_secs(12),
            iterations: 847,
            paths_found: 5,
            total_branches: Some(12),
            branches_covered: Some(8),
            mcdc_summary: None,
            iters_since_new_discovery: 0,
        }
    }

    #[test]
    fn format_progress_snapshot_shows_branches_iters_and_rate() {
        let line = format_progress_snapshot(&base_snapshot());
        assert!(line.starts_with("[12s] classifyNumber:"), "line={line}");
        assert!(line.contains("847 iters"), "line={line}");
        assert!(line.contains("5 paths"), "line={line}");
        assert!(line.contains("8/12 branches"), "line={line}");
        assert!(line.contains("iter/s"), "line={line}");
        // No MC/DC section unless explicitly set.
        assert!(!line.contains("mcdc"), "line={line}");
        // No idle tag on zero streak.
        assert!(!line.contains("idle"), "line={line}");
    }

    #[test]
    fn format_progress_snapshot_renders_mcdc_when_present() {
        let mut snap = base_snapshot();
        snap.mcdc_summary = Some((7, 3, 1));
        let line = format_progress_snapshot(&snap);
        assert!(line.contains("mcdc 3/7"), "line={line}");
    }

    #[test]
    fn format_progress_snapshot_appends_idle_tag_above_threshold() {
        let mut snap = base_snapshot();
        snap.iters_since_new_discovery = 320;
        let line = format_progress_snapshot(&snap);
        assert!(line.contains("(idle 320)"), "line={line}");
    }

    #[test]
    fn format_progress_snapshot_omits_idle_tag_when_below_threshold() {
        let mut snap = base_snapshot();
        snap.iters_since_new_discovery = 1; // below IDLE_STREAK_THRESHOLD (2)
        let line = format_progress_snapshot(&snap);
        assert!(!line.contains("idle"), "line={line}");
    }

    #[test]
    fn format_progress_snapshot_falls_back_to_paths_when_no_branch_count() {
        let mut snap = base_snapshot();
        snap.branches_covered = None;
        snap.total_branches = None;
        let line = format_progress_snapshot(&snap);
        // Branch segment falls back to paths/? when branch tracking is absent.
        assert!(line.contains("5/? paths"), "line={line}");
    }

    fn sample_func_analysis() -> FunctionAnalysis {
        FunctionAnalysis {
            name: "load/user".to_string(),
            exported: true,
            start_line: 12,
            end_line: 20,
            params: vec![],
            branches: vec![],
            dependencies: vec![],
            return_type: TypeInfo::Unknown,
            literals: vec![],
            crypto_boundaries: vec![],
            loops: vec![],
            source_file: None,
            adapter_hints: vec![],
            invocation_model: InvocationModel::Direct,
        }
    }

    fn sample_observation() -> shatter_core::explorer::ObservationOutput {
        shatter_core::explorer::ObservationOutput {
            function_name: "load/user".to_string(),
            iterations: 1,
            unique_paths: 1,
            lines_covered: 3,
            total_lines: 8,
            new_path_executions: vec![],
            raw_results: vec![],
            discoveries: vec![],
            nondeterministic_fields: vec![],
            float_probe_results: vec![],
            boundary_results: vec![],
            shrunk_witnesses: std::collections::HashMap::new(),
            mcdc_summary: None,
            shrink_stats: shatter_core::shrink::ShrinkStats::default(),
            abandoned_frontiers: vec![],
            opaque_suggestions: vec![],
            stubbed_modules: vec![],
        }
    }

    fn sample_outcome() -> FuncExploreOutcome {
        FuncExploreOutcome {
            work_index: 0,
            func: sample_func_analysis(),
            mock_symbols: vec!["dep".to_string()],
            result: Ok(sample_observation()),
            wall_time: Duration::from_millis(25),
            genetic_config: GeneticConfig::default(),
        }
    }

    // --- ExploreResultAccumulator unit tests (str-b2my.6) ---

    fn obs_with(
        iterations: u32,
        lines_covered: usize,
        total_lines: u32,
        discoveries: Vec<(u32, shatter_core::coverage_metrics::DiscoveryMethod)>,
        stubbed: Vec<String>,
    ) -> shatter_core::explorer::ObservationOutput {
        shatter_core::explorer::ObservationOutput {
            function_name: "load/user".to_string(),
            iterations,
            unique_paths: discoveries.len(),
            lines_covered,
            total_lines,
            new_path_executions: vec![],
            raw_results: vec![],
            discoveries,
            nondeterministic_fields: vec![],
            float_probe_results: vec![],
            boundary_results: vec![],
            shrunk_witnesses: std::collections::HashMap::new(),
            mcdc_summary: None,
            shrink_stats: shatter_core::shrink::ShrinkStats::default(),
            abandoned_frontiers: vec![],
            opaque_suggestions: vec![],
            stubbed_modules: stubbed,
        }
    }

    #[test]
    fn accumulator_additive_sums_iterations() {
        use shatter_core::coverage_metrics::DiscoveryMethod;
        let mut acc = ExploreResultAccumulator::new("load/user".to_string());
        acc.merge(Ok(obs_with(
            200,
            2,
            10,
            vec![(1, DiscoveryMethod::Z3)],
            vec![],
        )));
        acc.merge(Ok(obs_with(
            500,
            3,
            10,
            vec![(2, DiscoveryMethod::Random)],
            vec![],
        )));
        acc.merge(Ok(obs_with(
            50,
            1,
            10,
            vec![(3, DiscoveryMethod::Z3)],
            vec![],
        )));
        let obs = acc.into_result().expect("ok");
        assert_eq!(obs.iterations, 750);
        assert_eq!(obs.unique_paths, 3);
    }

    #[test]
    fn accumulator_union_dedupes_discoveries_first_wins() {
        use shatter_core::coverage_metrics::DiscoveryMethod;
        let mut acc = ExploreResultAccumulator::new("load/user".to_string());
        acc.merge(Ok(obs_with(
            100,
            1,
            10,
            vec![(5, DiscoveryMethod::Z3)],
            vec![],
        )));
        // Second batch re-discovers branch 5 via Random — first-wins keeps Z3.
        acc.merge(Ok(obs_with(
            100,
            1,
            10,
            vec![(5, DiscoveryMethod::Random), (7, DiscoveryMethod::Random)],
            vec![],
        )));
        let obs = acc.into_result().expect("ok");
        assert_eq!(obs.unique_paths, 2);
        let by_id: std::collections::HashMap<u32, DiscoveryMethod> =
            obs.discoveries.into_iter().collect();
        assert_eq!(by_id.get(&5), Some(&DiscoveryMethod::Z3));
        assert_eq!(by_id.get(&7), Some(&DiscoveryMethod::Random));
    }

    #[test]
    fn accumulator_monotone_max_on_coverage() {
        let mut acc = ExploreResultAccumulator::new("load/user".to_string());
        acc.merge(Ok(obs_with(100, 2, 10, vec![], vec![])));
        acc.merge(Ok(obs_with(100, 7, 10, vec![], vec![])));
        acc.merge(Ok(obs_with(100, 5, 10, vec![], vec![])));
        let obs = acc.into_result().expect("ok");
        assert_eq!(obs.lines_covered, 7);
        assert_eq!(obs.total_lines, 10);
    }

    #[test]
    fn batch_is_exhausted_covers_all_four_branches() {
        // (a) Error → exhausted, regardless of cap.
        let err: Result<shatter_core::explorer::ObservationOutput, String> =
            Err("boom".to_string());
        assert!(batch_is_exhausted(&err, 500));

        // (b) Ok with iters < cap → converged early → exhausted.
        assert!(batch_is_exhausted(
            &Ok(obs_with(499, 0, 0, vec![], vec![])),
            500
        ));
        assert!(batch_is_exhausted(
            &Ok(obs_with(0, 0, 0, vec![], vec![])),
            500
        ));

        // (c) Ok with iters == cap → NOT exhausted → scheduler re-enqueues.
        assert!(!batch_is_exhausted(
            &Ok(obs_with(500, 0, 0, vec![], vec![])),
            500
        ));

        // (d) Ok with iters > cap (defensive; shouldn't happen but must not
        //     flip the polarity) → NOT exhausted.
        assert!(!batch_is_exhausted(
            &Ok(obs_with(600, 0, 0, vec![], vec![])),
            500
        ));
    }

    #[test]
    fn scheduler_and_accumulator_drive_multi_batch_round_robin() {
        use shatter_core::batch_scheduler::{BatchOutcome, BatchScheduler};
        use shatter_core::coverage_metrics::DiscoveryMethod;

        // Two unbounded functions, batch cap = 500. Simulate what the
        // run_explore launch loop does on each tick: pop a batch,
        // synthesise a completed ObservationOutput, run batch_is_exhausted
        // → record_outcome → merge. This exercises the critical re-enqueue
        // branch (exhausted: false) that separates round-robin from
        // Option-A degenerate mode.
        const CAP: u32 = 500;
        let mut scheduler = BatchScheduler::with_individual_budgets(&[None, None], CAP);
        let mut accs = vec![
            ExploreResultAccumulator::new("fn_a".to_string()),
            ExploreResultAccumulator::new("fn_b".to_string()),
        ];

        // Scripted per-batch outcomes: (iterations, discoveries).
        // fn_a: three full-cap batches then converges early on batch 4.
        // fn_b: two full-cap batches then converges early on batch 3.
        type Discovery = (u32, DiscoveryMethod);
        type BatchScript = Vec<(u32, Vec<Discovery>)>;
        let scripts: Vec<BatchScript> = vec![
            vec![
                (500, vec![(1, DiscoveryMethod::Z3)]),
                (500, vec![(2, DiscoveryMethod::Z3)]),
                (500, vec![(1, DiscoveryMethod::Random)]), // re-discover branch 1
                (200, vec![(3, DiscoveryMethod::Z3)]),     // early convergence
            ],
            vec![
                (500, vec![(10, DiscoveryMethod::Z3)]),
                (500, vec![(11, DiscoveryMethod::Random)]),
                (100, vec![]), // early convergence, no new branches
            ],
        ];
        let mut cursors = [0usize, 0usize];
        let mut order: Vec<usize> = Vec::new();
        let mut not_exhausted_count = 0u32;

        while let Some(batch_cfg) = scheduler.next_batch() {
            order.push(batch_cfg.task_index);
            let cursor = &mut cursors[batch_cfg.task_index];
            let (iters, discoveries) = scripts[batch_cfg.task_index][*cursor].clone();
            *cursor += 1;

            let result: Result<shatter_core::explorer::ObservationOutput, String> =
                Ok(obs_with(iters, 1, 5, discoveries, vec![]));
            let exhausted = batch_is_exhausted(&result, batch_cfg.batch_size);
            if !exhausted {
                not_exhausted_count += 1;
            }
            accs[batch_cfg.task_index].merge(result);
            // Hard-code rank=0 to exercise the rank-0 degenerate path
            // (strict round-robin via FIFO tie-break). The rerank behavior
            // is covered by
            // `scheduler_and_accumulator_rerank_picks_streaking_function`
            // below.
            scheduler.record_outcome(BatchOutcome {
                task_index: batch_cfg.task_index,
                iterations_used: iters,
                exhausted,
                rank: 0,
                summary: None,
            });
        }

        // Round-robin: a,b,a,b,a,b,a — fn_b early-converges on its 3rd batch
        // (position 6), leaving fn_a's 4th batch at position 7.
        assert_eq!(order, vec![0, 1, 0, 1, 0, 1, 0]);
        // record_outcome(exhausted: false) fires for the 5 full-cap batches
        // (fn_a batches 1..=3 and fn_b batches 1..=2).
        assert_eq!(not_exhausted_count, 5);
        assert!(scheduler.is_complete());

        // Accumulator semantics: fn_a summed 1700 iters across 4 batches and
        // unique_paths is the cardinality of the discovery-id union
        // (branches 1, 2, 3 — branch 1 re-discovered, not double-counted).
        let fn_a = accs.remove(0).into_result().expect("fn_a merged");
        assert_eq!(fn_a.iterations, 1700);
        assert_eq!(fn_a.unique_paths, 3);

        let fn_b = accs.remove(0).into_result().expect("fn_b merged");
        assert_eq!(fn_b.iterations, 1100);
        assert_eq!(fn_b.unique_paths, 2);
    }

    #[test]
    fn scheduler_and_accumulator_rerank_picks_streaking_function() {
        use shatter_core::batch_scheduler::{BatchOutcome, BatchScheduler};
        use shatter_core::coverage_metrics::DiscoveryMethod;

        // str-b2my.7 + str-b2my.8 regression: after each batch the
        // scheduler re-ranks by new branch discoveries, but recency
        // cooldown (str-b2my.8) deprioritizes recently-completed
        // functions, promoting breadth-first exploration.
        //
        // Two unbounded functions, batch cap = 500. Scripted scenario:
        //
        //   pick A (rank 0 tie → FIFO). A discovers 1 new branch → rank 1.
        //   cooldown pushes A to effective -2; B (effective 0) wins.
        //   pick B. B discovers 3 new → rank 3. B exhausts on next pick.
        //   pick B again (effective 0 > A effective -1). B converges early.
        //   pick A (only left). A discovers 5 new → rank 5.
        //   pick A. A discovers 0 new → rank 0.
        //   pick A. A converges early, exhausted.
        //
        // Expected pick order: A, B, B, A, A, A.
        //
        // Cooldown promotes B earlier than pure rank would: breadth-first
        // exploration interleaved with rank-driven streaking.
        const CAP: u32 = 500;
        let mut scheduler = BatchScheduler::with_individual_budgets(&[None, None], CAP);
        let mut accs = vec![
            ExploreResultAccumulator::new("fn_a".to_string()),
            ExploreResultAccumulator::new("fn_b".to_string()),
        ];

        // (iters_used, discoveries) per batch, indexed by task then
        // invocation order for that task.
        type BatchScript = (u32, Vec<(u32, DiscoveryMethod)>);
        let scripts: Vec<Vec<BatchScript>> = vec![
            vec![
                (500, vec![(1, DiscoveryMethod::Z3)]),
                (
                    500,
                    vec![
                        (2, DiscoveryMethod::Z3),
                        (3, DiscoveryMethod::Z3),
                        (4, DiscoveryMethod::Z3),
                        (5, DiscoveryMethod::Z3),
                        (6, DiscoveryMethod::Z3),
                    ],
                ),
                (500, vec![(1, DiscoveryMethod::Random)]), // re-discovery: 0 new
                (200, vec![]),                             // early convergence
            ],
            vec![
                (
                    500,
                    vec![
                        (10, DiscoveryMethod::Z3),
                        (11, DiscoveryMethod::Z3),
                        (12, DiscoveryMethod::Z3),
                    ],
                ),
                (100, vec![]), // early convergence
            ],
        ];
        let mut cursors = [0usize, 0usize];
        let mut order: Vec<usize> = Vec::new();
        let mut ranks_recorded: Vec<i64> = Vec::new();

        while let Some(batch_cfg) = scheduler.next_batch() {
            order.push(batch_cfg.task_index);
            let cursor = &mut cursors[batch_cfg.task_index];
            let (iters, discoveries) = scripts[batch_cfg.task_index][*cursor].clone();
            *cursor += 1;

            let result: Result<shatter_core::explorer::ObservationOutput, String> =
                Ok(obs_with(iters, 1, 5, discoveries, vec![]));
            let exhausted = batch_is_exhausted(&result, batch_cfg.batch_size);

            // Compute the rerank score BEFORE merging, matching the
            // production order in `run_explore`.
            let rank = super::new_discoveries_in_batch(
                result.as_ref().ok(),
                &accs[batch_cfg.task_index].discoveries,
            ) as i64;
            ranks_recorded.push(rank);
            accs[batch_cfg.task_index].merge(result);
            scheduler.record_outcome(BatchOutcome {
                task_index: batch_cfg.task_index,
                iterations_used: iters,
                exhausted,
                rank,
                summary: None,
            });
        }

        // Pick order with attempt penalty (str-b2my.9): A's first batch
        // (rank 1) gives it cooldown 3, making its effective rank -2;
        // fresh B (effective 0) gets picked next, streaks twice (rank 3
        // then exhausted), then A runs its remaining batches.
        assert_eq!(
            order,
            vec![0, 1, 1, 0, 0, 0],
            "cooldown and attempt penalty must yield to a fresh peer after the first batch"
        );

        // Rank trace: A=1 new, B=3 new, B=0 (converge), A=5 new, A=0 (re-discover), A=0 (converge).
        assert_eq!(ranks_recorded, vec![1, 3, 0, 5, 0, 0]);

        assert!(scheduler.is_complete());

        // Accumulator totals: fn_a ran 4 batches (1700 iters, 6 unique
        // branches — branch 1 re-discovered on the 3rd batch is not
        // double-counted). fn_b ran 2 batches (600 iters, 3 branches).
        let fn_a = accs.remove(0).into_result().expect("fn_a merged");
        assert_eq!(fn_a.iterations, 1700);
        assert_eq!(fn_a.unique_paths, 6);

        let fn_b = accs.remove(0).into_result().expect("fn_b merged");
        assert_eq!(fn_b.iterations, 600);
        assert_eq!(fn_b.unique_paths, 3);
    }

    #[test]
    fn scheduler_enqueue_admits_later_target_work_items_mid_run() {
        // Regression for str-b2my.10: the per-target batch loop was hoisted
        // into a single unified batch loop that drains the shared scheduler,
        // so newly discovered functions from a later target are admitted via
        // `BatchScheduler::enqueue` while prior-target batches may still be
        // pending. This test drives that sequence directly: enqueue target 0,
        // pop some batches, then enqueue target 1 and verify every function
        // from both targets is drained before the scheduler reports complete.
        use shatter_core::batch_scheduler::{BatchOutcome, BatchScheduler};
        use shatter_core::coverage_metrics::DiscoveryMethod;

        const CAP: u32 = 500;
        let mut scheduler = BatchScheduler::with_individual_budgets(&[], CAP);

        // Target 0 prepares with two unbounded functions.
        scheduler.enqueue(0, None);
        scheduler.enqueue(1, None);
        let mut accs = vec![
            ExploreResultAccumulator::new("t0_f0".to_string()),
            ExploreResultAccumulator::new("t0_f1".to_string()),
        ];

        // Pop and complete one batch for each to simulate a little work.
        let mut order: Vec<usize> = Vec::new();
        for _ in 0..2 {
            let cfg = scheduler.next_batch().expect("t0 queue non-empty");
            order.push(cfg.task_index);
            let obs = obs_with(
                CAP,
                1,
                5,
                vec![(cfg.task_index as u32, DiscoveryMethod::Z3)],
                vec![],
            );
            accs[cfg.task_index].merge(Ok(obs));
            scheduler.record_outcome(BatchOutcome {
                task_index: cfg.task_index,
                iterations_used: CAP,
                exhausted: false,
                rank: 1,
                summary: None,
            });
        }

        // Target 1 finishes preparing mid-run and enqueues two more functions.
        // This is the path str-b2my.17 exposes and str-b2my.10 exercises in
        // the CLI: indices are appended to the global work_items vector and
        // the scheduler accepts them without being reset.
        scheduler.enqueue(2, None);
        scheduler.enqueue(3, None);
        accs.push(ExploreResultAccumulator::new("t1_f0".to_string()));
        accs.push(ExploreResultAccumulator::new("t1_f1".to_string()));

        // Drain every remaining batch. Every task must surface at least once.
        let mut seen = [false; 4];
        for &i in &order {
            seen[i] = true;
        }
        while let Some(cfg) = scheduler.next_batch() {
            seen[cfg.task_index] = true;
            // Converge early so each function exhausts after one more batch.
            let obs = obs_with(100, 1, 5, vec![], vec![]);
            accs[cfg.task_index].merge(Ok(obs));
            scheduler.record_outcome(BatchOutcome {
                task_index: cfg.task_index,
                iterations_used: 100,
                exhausted: true,
                rank: 0,
                summary: None,
            });
        }

        assert!(scheduler.is_complete(), "scheduler must drain every task");
        assert!(
            seen.iter().all(|&s| s),
            "every task across both targets must be picked at least once: {seen:?}",
        );
        assert_eq!(accs.len(), 4);
        for acc in accs {
            assert!(
                acc.batches_merged >= 1,
                "every accumulator must receive at least one batch"
            );
        }
    }

    #[test]
    fn fallback_transition_across_scheduler_lifecycle() {
        // str-b2my.5: verify is_frontier_exhausted() transitions correctly
        // when driven through the explore-loop pattern.
        use shatter_core::batch_scheduler::{BatchOutcome, BatchScheduler};
        use shatter_core::coverage_metrics::DiscoveryMethod;

        const CAP: u32 = 500;
        let mut scheduler = BatchScheduler::new(2, None, CAP);
        let mut accs = [
            ExploreResultAccumulator::new("funcA".to_string()),
            ExploreResultAccumulator::new("funcB".to_string()),
        ];

        // Round 1: both functions find new branches — NOT in fallback.
        for _ in 0..2 {
            let cfg = scheduler.next_batch().unwrap();
            let obs = obs_with(
                CAP,
                1,
                5,
                vec![(cfg.task_index as u32, DiscoveryMethod::Random)],
                vec![],
            );
            let rank =
                super::new_discoveries_in_batch(Some(&obs), &accs[cfg.task_index].discoveries)
                    as i64;
            assert!(rank > 0, "first batch should find new branches");
            accs[cfg.task_index].merge(Ok(obs));
            scheduler.record_outcome(BatchOutcome {
                task_index: cfg.task_index,
                iterations_used: CAP,
                exhausted: false,
                rank,
                summary: None,
            });
        }
        assert!(
            !scheduler.is_frontier_exhausted(),
            "both functions had discoveries"
        );

        // Round 2: both functions find nothing new — fallback.
        for _ in 0..2 {
            let cfg = scheduler.next_batch().unwrap();
            let obs = obs_with(
                CAP,
                1,
                5,
                vec![(cfg.task_index as u32, DiscoveryMethod::Random)],
                vec![],
            );
            let rank =
                super::new_discoveries_in_batch(Some(&obs), &accs[cfg.task_index].discoveries)
                    as i64;
            assert_eq!(rank, 0, "rediscovery should yield rank 0");
            accs[cfg.task_index].merge(Ok(obs));
            scheduler.record_outcome(BatchOutcome {
                task_index: cfg.task_index,
                iterations_used: CAP,
                exhausted: false,
                rank,
                summary: None,
            });
        }
        assert!(
            scheduler.is_frontier_exhausted(),
            "all functions explored with no new discoveries"
        );

        // Round 3: funcA finds something new — exits fallback.
        let cfg = scheduler.next_batch().unwrap();
        let obs = obs_with(CAP, 1, 5, vec![(99, DiscoveryMethod::Z3)], vec![]);
        let rank =
            super::new_discoveries_in_batch(Some(&obs), &accs[cfg.task_index].discoveries) as i64;
        assert!(rank > 0, "new branch should yield positive rank");
        accs[cfg.task_index].merge(Ok(obs));
        scheduler.record_outcome(BatchOutcome {
            task_index: cfg.task_index,
            iterations_used: CAP,
            exhausted: false,
            rank,
            summary: None,
        });
        assert!(
            !scheduler.is_frontier_exhausted(),
            "funcA has positive rank — no longer in fallback"
        );
    }

    #[test]
    fn new_discoveries_in_batch_counts_only_novel_branches() {
        use shatter_core::coverage_metrics::DiscoveryMethod;
        use std::collections::HashMap;

        let mut prior: HashMap<u32, DiscoveryMethod> = HashMap::new();
        prior.insert(1, DiscoveryMethod::Z3);
        prior.insert(2, DiscoveryMethod::Random);

        let obs = obs_with(
            100,
            1,
            5,
            vec![
                (1, DiscoveryMethod::Random), // already seen
                (2, DiscoveryMethod::Z3),     // already seen
                (3, DiscoveryMethod::Z3),     // new
                (4, DiscoveryMethod::Z3),     // new
            ],
            vec![],
        );
        assert_eq!(super::new_discoveries_in_batch(Some(&obs), &prior), 2);

        // Errored batches (obs = None) contribute no new discoveries.
        assert_eq!(super::new_discoveries_in_batch(None, &prior), 0);

        // Empty prior: every discovery counts as new.
        let empty: HashMap<u32, DiscoveryMethod> = HashMap::new();
        assert_eq!(super::new_discoveries_in_batch(Some(&obs), &empty), 4);
    }

    #[test]
    fn accumulator_dedupes_stubbed_modules_and_reports_error_when_no_success() {
        let mut acc = ExploreResultAccumulator::new("load/user".to_string());
        acc.merge(Ok(obs_with(
            10,
            1,
            5,
            vec![],
            vec!["fs".to_string(), "net".to_string()],
        )));
        acc.merge(Ok(obs_with(
            10,
            1,
            5,
            vec![],
            vec!["net".to_string(), "crypto".to_string()],
        )));
        let obs = acc.into_result().expect("ok");
        assert_eq!(
            obs.stubbed_modules,
            vec!["crypto".to_string(), "fs".to_string(), "net".to_string()],
        );

        // All-errors accumulator surfaces the last error.
        let mut fail_acc = ExploreResultAccumulator::new("f".to_string());
        fail_acc.merge(Err("boom".to_string()));
        fail_acc.merge(Err("fatal".to_string()));
        let err = fail_acc.into_result().expect_err("should fail");
        assert_eq!(err, "fatal");
    }

    #[test]
    fn write_explore_artifact_persists_completed_v2_result() {
        let dir = tempfile::tempdir().expect("tempdir");
        let outcome = sample_outcome();

        let path =
            write_explore_artifact(dir.path(), "src/user.ts", &outcome).expect("write artifact");
        let json = std::fs::read_to_string(&path).expect("read artifact");
        let value: serde_json::Value = serde_json::from_str(&json).expect("json");

        assert_eq!(value["version"], EXPLORE_ARTIFACT_VERSION);
        assert_eq!(value["status"], "completed");
        assert_eq!(value["function_name"], "load/user");
        assert_eq!(value["mock_symbols"][0], "dep");
        assert_eq!(value["observation"]["function_name"], "load/user");
        // v2: analysis field present
        assert_eq!(value["analysis"]["name"], "load/user");
        assert_eq!(value["analysis"]["start_line"], 12);
    }

    #[test]
    fn write_then_read_explore_artifact_roundtrips() {
        let dir = tempfile::tempdir().expect("tempdir");
        let outcome = sample_outcome();

        let path =
            write_explore_artifact(dir.path(), "src/user.ts", &outcome).expect("write artifact");

        let artifact = read_explore_artifact(&path).expect("read artifact");

        assert_eq!(artifact.version, EXPLORE_ARTIFACT_VERSION);
        assert_eq!(artifact.status, "completed");
        assert_eq!(artifact.function_name, "load/user");
        assert_eq!(artifact.file, "src/user.ts");
        assert_eq!(artifact.start_line, 12);
        assert_eq!(artifact.end_line, 20);
        assert_eq!(artifact.wall_time_ms, 25);
        assert_eq!(artifact.mock_symbols, vec!["dep"]);
        assert_eq!(artifact.analysis.name, "load/user");
        assert!(artifact.observation.is_some());
        assert!(artifact.error.is_none());
    }

    #[test]
    fn load_explore_artifacts_reads_directory() {
        let dir = tempfile::tempdir().expect("tempdir");
        let outcome1 = sample_outcome();
        let mut outcome2 = sample_outcome();
        outcome2.func.name = "validate".to_string();
        outcome2.func.start_line = 25;
        outcome2.func.end_line = 30;
        outcome2.work_index = 1;

        write_explore_artifact(dir.path(), "src/user.ts", &outcome1).expect("write 1");
        write_explore_artifact(dir.path(), "src/user.ts", &outcome2).expect("write 2");

        let artifacts = load_explore_artifacts(dir.path()).expect("load");
        assert_eq!(artifacts.len(), 2);
        // Sorted by start_line
        assert_eq!(artifacts[0].function_name, "load/user");
        assert_eq!(artifacts[1].function_name, "validate");
    }

    #[test]
    fn load_explore_artifacts_skips_corrupt_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let subdir = dir.path().join("src_user.ts");
        std::fs::create_dir_all(&subdir).expect("mkdir");

        // Write a valid artifact
        let outcome = sample_outcome();
        write_explore_artifact(dir.path(), "src/user.ts", &outcome).expect("write");

        // Write a corrupt file
        std::fs::write(subdir.join("00099_corrupt.json"), "not valid json").expect("write corrupt");

        let artifacts = load_explore_artifacts(dir.path()).expect("load");
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].function_name, "load/user");
    }

    #[test]
    fn load_explore_artifacts_skips_summary_and_tmp_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let subdir = dir.path().join("src_user.ts");
        std::fs::create_dir_all(&subdir).expect("mkdir");

        let outcome = sample_outcome();
        write_explore_artifact(dir.path(), "src/user.ts", &outcome).expect("write");

        // Write summary and tmp files that should be skipped
        std::fs::write(subdir.join("summary.json"), "{}").expect("write summary");
        std::fs::write(subdir.join("00001_foo.json.tmp"), "{}").expect("write tmp");

        let artifacts = load_explore_artifacts(dir.path()).expect("load");
        assert_eq!(artifacts.len(), 1);
    }

    #[test]
    fn explore_summary_roundtrips() {
        let summary = ExploreSummary {
            version: EXPLORE_ARTIFACT_VERSION,
            status: "completed".to_string(),
            file: "src/user.ts".to_string(),
            total_functions: 3,
            completed: 2,
            failed: 1,
            skipped: 0,
            elapsed_secs: 1.5,
            functions: vec![
                ExploreSummaryEntry {
                    function_name: "load".to_string(),
                    status: "completed".to_string(),
                    artifact: Some("src_user.ts/00012_load.json".to_string()),
                    reason: None,
                    deep_fingerprint: None,
                },
                ExploreSummaryEntry {
                    function_name: "save".to_string(),
                    status: "failed".to_string(),
                    artifact: Some("src_user.ts/00025_save.json".to_string()),
                    reason: Some("timeout".to_string()),
                    deep_fingerprint: None,
                },
            ],
        };

        let json = serde_json::to_string_pretty(&summary).expect("serialize");
        let parsed: ExploreSummary = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(parsed.version, EXPLORE_ARTIFACT_VERSION);
        assert_eq!(parsed.status, "completed");
        assert_eq!(parsed.total_functions, 3);
        assert_eq!(parsed.completed, 2);
        assert_eq!(parsed.failed, 1);
        assert_eq!(parsed.functions.len(), 2);
        assert_eq!(parsed.functions[0].function_name, "load");
        assert_eq!(parsed.functions[1].reason.as_deref(), Some("timeout"));
    }

    #[test]
    fn write_and_read_explore_summary() {
        let dir = tempfile::tempdir().expect("tempdir");
        let summary = ExploreSummary {
            version: EXPLORE_ARTIFACT_VERSION,
            status: "running".to_string(),
            file: "src/user.ts".to_string(),
            total_functions: 1,
            completed: 0,
            failed: 0,
            skipped: 0,
            elapsed_secs: 0.0,
            functions: vec![],
        };

        write_explore_summary(dir.path(), "src/user.ts", &summary).expect("write");
        let path = explore_summary_path(dir.path(), "src/user.ts");
        assert!(path.exists());

        let json = std::fs::read_to_string(&path).expect("read");
        let parsed: ExploreSummary = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.status, "running");
    }

    #[test]
    fn finalize_explore_markdown_includes_failed_and_skipped_functions() {
        let dir = tempfile::tempdir().expect("tempdir");
        let report_path = dir.path().join("report.md");
        let func = sample_func_analysis();
        let outcome = sample_outcome();

        write_explore_artifact(dir.path(), "src/user.ts", &outcome).expect("write artifact");
        let artifact_relpath = super::explore_artifact_path(dir.path(), "src/user.ts", &func)
            .strip_prefix(dir.path())
            .expect("relative path")
            .to_string_lossy()
            .to_string();

        let summary = ExploreSummary {
            version: EXPLORE_ARTIFACT_VERSION,
            status: "failed".to_string(),
            file: "src/user.ts".to_string(),
            total_functions: 3,
            completed: 1,
            failed: 1,
            skipped: 1,
            elapsed_secs: 1.0,
            functions: vec![
                ExploreSummaryEntry {
                    function_name: func.name.clone(),
                    status: "completed".to_string(),
                    artifact: Some(artifact_relpath),
                    reason: None,
                    deep_fingerprint: None,
                },
                ExploreSummaryEntry {
                    function_name: "save/user".to_string(),
                    status: "failed".to_string(),
                    artifact: None,
                    reason: Some("timeout".to_string()),
                    deep_fingerprint: None,
                },
                ExploreSummaryEntry {
                    function_name: "skip/user".to_string(),
                    status: "skipped".to_string(),
                    artifact: None,
                    reason: Some("unexecutable parameter types".to_string()),
                    deep_fingerprint: None,
                },
            ],
        };
        write_explore_summary(dir.path(), "src/user.ts", &summary).expect("write summary");

        finalize_explore(
            dir.path(),
            None,
            std::slice::from_ref(&report_path),
            false,
            false,
            false,
            false,
            crate::args::OutputFormat::Md,
            crate::args::StdoutFormat::Markdown,
            false,
            false,
            false,
        )
        .expect("finalize explore");

        let markdown = std::fs::read_to_string(&report_path).expect("read markdown");
        assert!(
            markdown.contains("load/user"),
            "completed function should remain"
        );
        assert!(
            markdown.contains("## save/user"),
            "failed function should get its own heading"
        );
        assert!(
            markdown.contains("**Status:** `timed_out`"),
            "timeout reason should map to the timed_out outcome status"
        );
        assert!(
            markdown.contains("## skip/user"),
            "skipped function should get its own heading"
        );
        assert!(
            markdown.contains("**Status:** `unsupported`"),
            "unexecutable-parameter skip should map to the unsupported outcome status"
        );
        assert!(
            markdown.contains("unexecutable parameter types"),
            "skipped reason text should be rendered"
        );
    }

    #[test]
    fn finalize_explore_markdown_supports_skipped_only_summary() {
        let dir = tempfile::tempdir().expect("tempdir");
        let report_path = dir.path().join("report.md");
        let summary = ExploreSummary {
            version: EXPLORE_ARTIFACT_VERSION,
            status: "completed".to_string(),
            file: "src/user.ts".to_string(),
            total_functions: 1,
            completed: 0,
            failed: 0,
            skipped: 1,
            elapsed_secs: 0.5,
            functions: vec![ExploreSummaryEntry {
                function_name: "skip/user".to_string(),
                status: "skipped".to_string(),
                artifact: None,
                reason: Some("unexecutable parameter types".to_string()),
                deep_fingerprint: None,
            }],
        };
        write_explore_summary(dir.path(), "src/user.ts", &summary).expect("write summary");

        finalize_explore(
            dir.path(),
            None,
            std::slice::from_ref(&report_path),
            false,
            false,
            false,
            false,
            crate::args::OutputFormat::Md,
            crate::args::StdoutFormat::Markdown,
            false,
            false,
            false,
        )
        .expect("finalize skipped-only explore");

        let markdown = std::fs::read_to_string(&report_path).expect("read markdown");
        assert!(
            markdown.contains("## skip/user"),
            "skipped function should get its own heading"
        );
        assert!(markdown.contains("**Status:** `unsupported`"));
        assert!(markdown.contains("unexecutable parameter types"));
    }

    #[test]
    fn persist_stage_outputs_writes_all_stage_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let func = sample_func_analysis();
        let observation = sample_observation();
        let analyze_output = shatter_core::pipeline::analyze(&observation, &func);

        persist_stage_outputs(
            dir.path(),
            "src/user.ts",
            &func,
            &observation,
            &analyze_output,
            Some(5_000),
            false,
        )
        .expect("persist stage outputs");

        let stage_dir = stage_persistence_dir(dir.path(), "src/user.ts", &func);
        let observe_stage =
            shatter_core::pipeline::read_observe_stage(&stage_dir.join("observe.json"))
                .expect("read observe");
        let analyze_stage =
            shatter_core::pipeline::read_analyze_stage(&stage_dir.join("analyze.json"))
                .expect("read analyze");
        let solve_stage = shatter_core::pipeline::read_solve_stage(&stage_dir.join("solve.json"))
            .expect("read solve");
        let specify_stage =
            shatter_core::pipeline::read_specify_stage(&stage_dir.join("specify.json"))
                .expect("read specify");

        assert_eq!(observe_stage.file, "src/user.ts");
        assert_eq!(observe_stage.observation.function_name, func.name);
        assert_eq!(analyze_stage.function_name, func.name);
        assert_eq!(solve_stage.function_name, func.name);
        assert_eq!(specify_stage.function_name, func.name);
    }

    #[test]
    fn persist_stage_outputs_returns_error_when_root_is_a_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root_file = dir.path().join("stage-root-file");
        std::fs::write(&root_file, "not a directory").expect("write root file");

        let func = sample_func_analysis();
        let observation = sample_observation();
        let analyze_output = shatter_core::pipeline::analyze(&observation, &func);

        let result = persist_stage_outputs(
            &root_file,
            "src/user.ts",
            &func,
            &observation,
            &analyze_output,
            None,
            false,
        );

        assert!(result.is_err(), "file-backed root must fail");
    }

    #[test]
    fn read_explore_artifact_rejects_v1_missing_analysis() {
        let dir = tempfile::tempdir().expect("tempdir");
        // v1 artifacts lack the `analysis` field and cannot be deserialized.
        let v1_json = serde_json::json!({
            "version": 1,
            "status": "completed",
            "file": "src/user.ts",
            "function_name": "load",
            "start_line": 1,
            "end_line": 10,
            "wall_time_ms": 100,
            "mock_symbols": [],
            "observation": null
        });
        let path = dir.path().join("00001_load.json");
        std::fs::write(&path, serde_json::to_string(&v1_json).unwrap()).expect("write");

        let result = read_explore_artifact(&path);
        assert!(result.is_err(), "v1 artifact should fail to load");
    }

    #[test]
    fn sanitize_artifact_component_replaces_path_separators() {
        assert_eq!(sanitize_artifact_component("src/user.ts"), "src_user.ts");
        assert_eq!(sanitize_artifact_component(""), "unknown");
    }

    // --- persist_behavior_map regression (str-bo4z.11) ---
    //
    // Before str-bo4z.11, run_explore persisted behavior maps via
    // `cache.store(&bmap)` with no fingerprint. The resulting entry carried
    // `fingerprint: None`, so the next explore run's `is_fresh` check dropped
    // the file immediately. These tests pin the helper that replaces both
    // legacy call sites.

    use super::persist_behavior_map;
    use shatter_core::behavior::BehaviorMap;
    use shatter_core::cache::BehaviorMapCache;

    fn make_empty_map(function_id: &str) -> BehaviorMap {
        let obs = sample_observation();
        BehaviorMap::from_exploration_result(function_id, &obs)
    }

    #[test]
    fn persist_with_fingerprint_survives_is_fresh_on_identical_source() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cache = BehaviorMapCache::new(dir.path().to_path_buf()).expect("cache");
        let function_id = "load/user";
        let fingerprint = "deadbeefcafebabe";

        let map = make_empty_map(function_id);
        persist_behavior_map(&cache, &map, Some(fingerprint)).expect("persist");

        // Second explore run against unchanged source: same deep fingerprint,
        // so is_fresh must return true and must NOT delete the entry. Before
        // the fix the stored entry carried fingerprint: None and is_fresh
        // dropped it.
        assert!(
            cache.is_fresh(function_id, fingerprint).expect("is_fresh"),
            "freshly persisted map should be fresh under the same fingerprint",
        );
        assert!(
            cache.load(function_id).expect("load").is_some(),
            "cached map should still be present after is_fresh",
        );
    }

    #[test]
    fn persist_without_fingerprint_falls_back_to_legacy_store() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cache = BehaviorMapCache::new(dir.path().to_path_buf()).expect("cache");
        let function_id = "load/user";

        let map = make_empty_map(function_id);
        persist_behavior_map(&cache, &map, None).expect("persist");

        // No fingerprint recorded → is_fresh against any current fingerprint
        // must report stale and prune the entry.
        assert!(
            !cache.is_fresh(function_id, "any-fp").expect("is_fresh"),
            "unfingerprinted map must not be considered fresh",
        );
    }

    // --- Resume logic tests (str-b2my.15) ---

    use super::{
        PersistedExploreState, cleanup_resume_state, read_explore_summary, read_resume_state,
        resume_state_path, try_resume_function, write_resume_state,
    };

    #[test]
    fn persisted_explore_state_roundtrips() {
        let original = shatter_core::orchestrator::ExploreState {
            covered_paths: [42, 99, 7].into_iter().collect(),
            discovery_inputs: vec![
                vec![serde_json::json!(1), serde_json::json!("hello")],
                vec![serde_json::json!(null)],
            ],
        };
        let persisted = PersistedExploreState::from_explore_state(&original);
        // covered_paths should be sorted for deterministic serialization
        assert_eq!(persisted.covered_paths, vec![7, 42, 99]);

        let json = serde_json::to_string(&persisted).expect("serialize");
        let deserialized: PersistedExploreState = serde_json::from_str(&json).expect("deserialize");
        let restored = deserialized.into_explore_state();

        assert_eq!(restored.covered_paths, original.covered_paths);
        assert_eq!(restored.discovery_inputs, original.discovery_inputs);
    }

    #[test]
    fn read_explore_summary_loads_valid_summary() {
        let dir = tempfile::tempdir().expect("tempdir");
        let summary = ExploreSummary {
            version: EXPLORE_ARTIFACT_VERSION,
            status: "completed".to_string(),
            file: "src/user.ts".to_string(),
            total_functions: 2,
            completed: 1,
            failed: 1,
            skipped: 0,
            elapsed_secs: 5.0,
            functions: vec![
                ExploreSummaryEntry {
                    function_name: "load".to_string(),
                    status: "completed".to_string(),
                    artifact: Some("src_user.ts/00012_load.json".to_string()),
                    reason: None,
                    deep_fingerprint: Some("abc123".to_string()),
                },
                ExploreSummaryEntry {
                    function_name: "save".to_string(),
                    status: "failed".to_string(),
                    artifact: None,
                    reason: Some("timeout".to_string()),
                    deep_fingerprint: Some("def456".to_string()),
                },
            ],
        };
        write_explore_summary(dir.path(), "src/user.ts", &summary).expect("write");
        let loaded = read_explore_summary(dir.path(), "src/user.ts");
        assert!(loaded.is_some(), "should load valid summary");
        let loaded = loaded.unwrap();
        assert_eq!(loaded.completed, 1);
        assert_eq!(loaded.functions.len(), 2);
        assert_eq!(
            loaded.functions[0].deep_fingerprint.as_deref(),
            Some("abc123")
        );
    }

    #[test]
    fn read_explore_summary_returns_none_on_missing_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(read_explore_summary(dir.path(), "nonexistent.ts").is_none());
    }

    #[test]
    fn read_explore_summary_returns_none_on_corrupt_json() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = explore_summary_path(dir.path(), "src/user.ts");
        std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir");
        std::fs::write(&path, "not valid json").expect("write");
        assert!(read_explore_summary(dir.path(), "src/user.ts").is_none());
    }

    #[test]
    fn read_explore_summary_returns_none_on_old_version() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = explore_summary_path(dir.path(), "src/user.ts");
        std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir");
        let old_summary = serde_json::json!({
            "version": 1,
            "status": "completed",
            "file": "src/user.ts",
            "total_functions": 1,
            "completed": 1,
            "failed": 0,
            "skipped": 0,
            "elapsed_secs": 1.0,
            "functions": []
        });
        std::fs::write(&path, old_summary.to_string()).expect("write");
        assert!(read_explore_summary(dir.path(), "src/user.ts").is_none());
    }

    #[test]
    fn try_resume_matching_fingerprint_succeeds() {
        let dir = tempfile::tempdir().expect("tempdir");
        let func = sample_func_analysis();
        let outcome = sample_outcome();

        // Write a completed artifact.
        write_explore_artifact(dir.path(), "src/user.ts", &outcome).expect("write artifact");
        let artifact_relpath = {
            let p = super::explore_artifact_path(dir.path(), "src/user.ts", &func);
            p.strip_prefix(dir.path())
                .unwrap()
                .to_string_lossy()
                .to_string()
        };

        let summary = ExploreSummary {
            version: EXPLORE_ARTIFACT_VERSION,
            status: "completed".to_string(),
            file: "src/user.ts".to_string(),
            total_functions: 1,
            completed: 1,
            failed: 0,
            skipped: 0,
            elapsed_secs: 1.0,
            functions: vec![ExploreSummaryEntry {
                function_name: func.name.clone(),
                status: "completed".to_string(),
                artifact: Some(artifact_relpath),
                reason: None,
                deep_fingerprint: Some("fp-abc".to_string()),
            }],
        };

        let mut deep_fps = std::collections::HashMap::new();
        deep_fps.insert(func.name.clone(), "fp-abc".to_string());

        let result = try_resume_function(dir.path(), &func, &deep_fps, Some(&summary));
        assert!(result.is_some(), "should resume with matching fingerprint");
        let (obs, wall_time) = result.unwrap();
        assert_eq!(obs.function_name, "load/user");
        assert_eq!(wall_time, Duration::from_millis(25));
    }

    #[test]
    fn try_resume_mismatched_fingerprint_returns_none() {
        let dir = tempfile::tempdir().expect("tempdir");
        let func = sample_func_analysis();

        let summary = ExploreSummary {
            version: EXPLORE_ARTIFACT_VERSION,
            status: "completed".to_string(),
            file: "src/user.ts".to_string(),
            total_functions: 1,
            completed: 1,
            failed: 0,
            skipped: 0,
            elapsed_secs: 1.0,
            functions: vec![ExploreSummaryEntry {
                function_name: func.name.clone(),
                status: "completed".to_string(),
                artifact: Some("src_user.ts/00012_load_user.json".to_string()),
                reason: None,
                deep_fingerprint: Some("fp-old".to_string()),
            }],
        };

        let mut deep_fps = std::collections::HashMap::new();
        deep_fps.insert(func.name.clone(), "fp-new".to_string());

        let result = try_resume_function(dir.path(), &func, &deep_fps, Some(&summary));
        assert!(
            result.is_none(),
            "should not resume with mismatched fingerprint"
        );
    }

    #[test]
    fn try_resume_missing_fingerprint_returns_none() {
        let summary = ExploreSummary {
            version: EXPLORE_ARTIFACT_VERSION,
            status: "completed".to_string(),
            file: "src/user.ts".to_string(),
            total_functions: 1,
            completed: 1,
            failed: 0,
            skipped: 0,
            elapsed_secs: 1.0,
            functions: vec![ExploreSummaryEntry {
                function_name: "load/user".to_string(),
                status: "completed".to_string(),
                artifact: Some("src_user.ts/00012_load_user.json".to_string()),
                reason: None,
                deep_fingerprint: None, // legacy summary
            }],
        };

        let func = sample_func_analysis();
        let mut deep_fps = std::collections::HashMap::new();
        deep_fps.insert(func.name.clone(), "fp-abc".to_string());

        let dir = tempfile::tempdir().expect("tempdir");
        let result = try_resume_function(dir.path(), &func, &deep_fps, Some(&summary));
        assert!(
            result.is_none(),
            "should not resume without stored fingerprint"
        );
    }

    #[test]
    fn try_resume_failed_status_returns_none() {
        let summary = ExploreSummary {
            version: EXPLORE_ARTIFACT_VERSION,
            status: "completed".to_string(),
            file: "src/user.ts".to_string(),
            total_functions: 1,
            completed: 0,
            failed: 1,
            skipped: 0,
            elapsed_secs: 1.0,
            functions: vec![ExploreSummaryEntry {
                function_name: "load/user".to_string(),
                status: "failed".to_string(),
                artifact: None,
                reason: Some("timeout".to_string()),
                deep_fingerprint: Some("fp-abc".to_string()),
            }],
        };

        let func = sample_func_analysis();
        let mut deep_fps = std::collections::HashMap::new();
        deep_fps.insert(func.name.clone(), "fp-abc".to_string());

        let dir = tempfile::tempdir().expect("tempdir");
        let result = try_resume_function(dir.path(), &func, &deep_fps, Some(&summary));
        assert!(result.is_none(), "should not resume failed function");
    }

    #[test]
    fn try_resume_missing_artifact_returns_none() {
        let summary = ExploreSummary {
            version: EXPLORE_ARTIFACT_VERSION,
            status: "completed".to_string(),
            file: "src/user.ts".to_string(),
            total_functions: 1,
            completed: 1,
            failed: 0,
            skipped: 0,
            elapsed_secs: 1.0,
            functions: vec![ExploreSummaryEntry {
                function_name: "load/user".to_string(),
                status: "completed".to_string(),
                artifact: Some("src_user.ts/00012_nonexistent.json".to_string()),
                reason: None,
                deep_fingerprint: Some("fp-abc".to_string()),
            }],
        };

        let func = sample_func_analysis();
        let mut deep_fps = std::collections::HashMap::new();
        deep_fps.insert(func.name.clone(), "fp-abc".to_string());

        let dir = tempfile::tempdir().expect("tempdir");
        let result = try_resume_function(dir.path(), &func, &deep_fps, Some(&summary));
        assert!(
            result.is_none(),
            "should not resume when artifact file missing"
        );
    }

    #[test]
    fn load_explore_artifacts_skips_resume_state_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let subdir = dir.path().join("src_user.ts");
        std::fs::create_dir_all(&subdir).expect("mkdir");

        // Write a valid artifact.
        let outcome = sample_outcome();
        write_explore_artifact(dir.path(), "src/user.ts", &outcome).expect("write");

        // Write a resume-state sidecar.
        let func = sample_func_analysis();
        let state = shatter_core::orchestrator::ExploreState {
            covered_paths: [1, 2].into_iter().collect(),
            discovery_inputs: vec![],
        };
        write_resume_state(dir.path(), "src/user.ts", &func, &state).expect("write state");

        let artifacts = load_explore_artifacts(dir.path()).expect("load");
        assert_eq!(artifacts.len(), 1, "resume-state file should be skipped");
        assert_eq!(artifacts[0].function_name, "load/user");
    }

    #[test]
    fn resume_state_write_read_roundtrip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let func = sample_func_analysis();
        let state = shatter_core::orchestrator::ExploreState {
            covered_paths: [10, 20, 30].into_iter().collect(),
            discovery_inputs: vec![
                vec![serde_json::json!(42)],
                vec![serde_json::json!("test"), serde_json::json!(true)],
            ],
        };

        write_resume_state(dir.path(), "src/user.ts", &func, &state).expect("write");
        let loaded = read_resume_state(dir.path(), "src/user.ts", &func);
        assert!(loaded.is_some(), "should load resume state");
        let loaded = loaded.unwrap();
        assert_eq!(loaded.covered_paths, state.covered_paths);
        assert_eq!(loaded.discovery_inputs, state.discovery_inputs);
    }

    #[test]
    fn cleanup_resume_state_removes_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let func = sample_func_analysis();
        let state = shatter_core::orchestrator::ExploreState::default();

        write_resume_state(dir.path(), "src/user.ts", &func, &state).expect("write");
        let path = resume_state_path(dir.path(), "src/user.ts", &func);
        assert!(path.exists(), "sidecar should exist before cleanup");

        cleanup_resume_state(dir.path(), "src/user.ts", &func);
        assert!(!path.exists(), "sidecar should be removed after cleanup");
    }

    #[test]
    fn summary_entry_fingerprint_backward_compatible() {
        // Simulate a legacy summary entry without deep_fingerprint field.
        let json = r#"{
            "function_name": "load",
            "status": "completed",
            "artifact": "src_user.ts/00012_load.json"
        }"#;
        let entry: ExploreSummaryEntry = serde_json::from_str(json).expect("deserialize");
        assert_eq!(entry.function_name, "load");
        assert!(
            entry.deep_fingerprint.is_none(),
            "missing field should default to None"
        );
    }

    #[test]
    fn try_resume_no_prior_summary_returns_none() {
        let dir = tempfile::tempdir().expect("tempdir");
        let func = sample_func_analysis();
        let mut deep_fps = std::collections::HashMap::new();
        deep_fps.insert(func.name.clone(), "fp-abc".to_string());

        let result = try_resume_function(dir.path(), &func, &deep_fps, None);
        assert!(result.is_none(), "should not resume without prior summary");
    }

    #[test]
    fn accumulator_with_resumed_observation_flows_through() {
        // Simulate the resume path: merge a loaded observation, then finalize.
        let obs = sample_observation();
        let mut acc = ExploreResultAccumulator::new("load/user".to_string());
        acc.merge(Ok(obs));

        assert_eq!(acc.batches_merged, 1);
        assert_eq!(acc.successful_batches, 1);

        let result = acc.into_result();
        assert!(
            result.is_ok(),
            "resumed accumulator should produce Ok result"
        );
        let output = result.unwrap();
        assert_eq!(output.function_name, "load/user");
        assert_eq!(output.iterations, 1);
    }
}
