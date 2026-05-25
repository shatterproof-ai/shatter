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
            ..Default::default()
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

/// Why a per-function explore artifact is *not* present on disk.
///
/// str-jeen.4: this is a typed projection of the previously free-form
/// `ExploreSummaryEntry::reason` string. The artifact-reference contract is:
/// for any summary entry, exactly one of the following holds —
///   * `artifact: Some(path)` and the file at `<artifact_root>/<path>` exists;
///   * `artifact: None` and a typed `UnavailableReason` is recorded so report
///     consumers can classify the row instead of chasing a missing path.
///
/// Variants:
/// * `BuildFailed` — instrumentation, compilation, or wrapper build failed.
///   Maps to `OutcomeStatus::BuildFailed`. Persisted reason text uses the
///   token `spec_not_produced_due_to_build_failed` for downstream parsers.
/// * `RuntimeFailed` — frontend execution raised a runtime error / panic.
/// * `TimedOut` — exceeded the per-function time budget.
/// * `Unsupported` — pre-skipped: the analyzer flagged unexecutable parameter
///   types and no work item was scheduled.
/// * `SkippedByPolicy` — explicitly skipped by user / config policy.
/// * `WriteFailed` — the function ran (or attempted to run) but the artifact
///   JSON itself could not be persisted (disk error, rename failure, etc.).
///
/// The string projection (`as_token()` / `Display`) is what gets written into
/// the on-disk `reason` field today. When str-jeen.16 introduces a typed TSV
/// status export, the same enum will be the `reason` field's first-class type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum UnavailableReason {
    BuildFailed,
    RuntimeFailed,
    TimedOut,
    Unsupported,
    SkippedByPolicy,
    WriteFailed,
    /// Environment preflight failed — env fault outside the function under
    /// test (str-jeen.40). Distinct from `Unsupported`, which is a
    /// frontend-capability gap.
    PreflightFailed,
}

impl UnavailableReason {
    /// Stable string token used for on-disk `reason` strings and for matching
    /// the kapow-validation broad-run wrapper's existing `unavailable_reason`
    /// taxonomy. Kept distinct from the bare snake_case serde form so the
    /// `spec_not_produced_due_to_*` family stays readable to downstream
    /// consumers that scan the summary.json text.
    fn as_token(self) -> &'static str {
        match self {
            UnavailableReason::BuildFailed => "spec_not_produced_due_to_build_failed",
            UnavailableReason::RuntimeFailed => "spec_not_produced_due_to_runtime_failed",
            UnavailableReason::TimedOut => "spec_not_produced_due_to_timed_out",
            UnavailableReason::Unsupported => "spec_not_produced_due_to_unsupported",
            UnavailableReason::SkippedByPolicy => "spec_not_produced_due_to_skipped_by_policy",
            UnavailableReason::WriteFailed => "artifact_write_failed",
            UnavailableReason::PreflightFailed => "spec_not_produced_due_to_preflight_failed",
        }
    }
}

impl std::fmt::Display for UnavailableReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_token())
    }
}

/// Per-function entry in the explore summary.
///
/// Artifact-reference contract (str-jeen.4): if `artifact` is `Some(path)`,
/// the file at `<artifact_root>/<path>` must exist at finalization. Otherwise
/// `artifact` must be `None` and the row's `reason` should be populated with
/// a token derived from [`UnavailableReason`]. Construct via the
/// [`ExploreSummaryEntry::available`] / [`ExploreSummaryEntry::unavailable`]
/// helpers; `debug_assert!` calls inside those helpers enforce the invariant
/// in test builds.
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
    /// Source-line span for the analyzed function (`end_line - start_line + 1`).
    /// Populated when a `FunctionAnalysis` is in scope at construction time;
    /// `0` for entries seeded without analyzer metadata. Used by the Go
    /// broad-run root-cause aggregator (str-jeen.31) to weight build-failure
    /// categories by lines of source they suppressed, so a 200-line file
    /// failing on a single category outweighs five 5-line stubs.
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    line_count: u32,
    /// Source lines actually exercised by exploration (str-jeen.41), sourced
    /// from `ObservationOutput::lines_covered` and capped at `line_count` so
    /// the `covered + uncovered = total` partition holds even if the
    /// instrumenter's line-id space disagrees with the analyzer's source span.
    /// Only populated for outcomes whose `OutcomeStatus` resolves to
    /// `Completed` / `CompletedWithFindings`; `0` otherwise. Used by the
    /// run-JSON line-weighted outcome buckets to split completed-function
    /// lines into covered vs uncovered.
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    covered_lines: u32,
}

fn is_zero_u32(n: &u32) -> bool {
    *n == 0
}

/// `skip_serializing_if` predicate for `usize` fields — used by additive
/// `ExploreSummary` counters (e.g. `preflight_failed`, str-jeen.40) so that
/// runs which never hit the new outcome bucket continue to serialize the
/// pre-existing JSON shape.
fn is_zero_usize(n: &usize) -> bool {
    *n == 0
}

impl ExploreSummaryEntry {
    /// Construct an entry whose artifact JSON exists on disk.
    fn available(
        function_name: String,
        status: String,
        artifact_relpath: String,
        reason: Option<String>,
        deep_fingerprint: Option<String>,
    ) -> Self {
        debug_assert!(
            !artifact_relpath.is_empty(),
            "available() requires a non-empty artifact path; use unavailable() for missing artifacts"
        );
        Self {
            function_name,
            status,
            artifact: Some(artifact_relpath),
            reason,
            deep_fingerprint,
            line_count: 0,
            covered_lines: 0,
        }
    }

    /// Construct an entry whose artifact is intentionally absent. The
    /// `unavailable_reason` enum is folded into the on-disk `reason` field
    /// (prefixed with the typed token, then any free-form detail).
    fn unavailable(
        function_name: String,
        status: String,
        unavailable_reason: UnavailableReason,
        detail: Option<String>,
        deep_fingerprint: Option<String>,
    ) -> Self {
        let reason_text = match detail {
            Some(d) if !d.is_empty() => format!("{}: {}", unavailable_reason.as_token(), d),
            _ => unavailable_reason.as_token().to_string(),
        };
        Self {
            function_name,
            status,
            artifact: None,
            reason: Some(reason_text),
            deep_fingerprint,
            line_count: 0,
            covered_lines: 0,
        }
    }

    /// Attach a source-line span. Returns the entry by value so the
    /// outcome-time construction site can chain it onto either the
    /// `available` or `unavailable` constructor without restating the
    /// shared fields. See `line_count` doc on the struct (str-jeen.31).
    fn with_line_count(mut self, line_count: u32) -> Self {
        self.line_count = line_count;
        self
    }

    /// Attach the executor's `lines_covered` count for a successfully
    /// completed outcome (str-jeen.41). Capped at `line_count` so the
    /// covered / uncovered partition surfaced in the run JSON cannot
    /// exceed the function's analyzer span. Returns the entry by value
    /// so the construction site can chain it after `with_line_count`.
    fn with_covered_lines(mut self, covered_lines: u32) -> Self {
        self.covered_lines = covered_lines.min(self.line_count);
        self
    }
}

/// Summary of an entire explore run, written incrementally to enable crash recovery.
///
/// Field history:
/// - `completed` / `failed` / `skipped` are legacy tri-bucket counters retained
///   for backward compatibility. They equal the sums of the per-`OutcomeStatus`
///   buckets below: `failed = build_failed + runtime_failed + timed_out`,
///   `skipped = unsupported + skipped_by_policy`. Readers that only need the
///   coarse bucketing keep working unchanged.
/// - The per-`OutcomeStatus` buckets, `produced_coverage`, and
///   `no_target_reason` were added by str-oo31 so callers can distinguish
///   build/runtime/timeout failures, expose an executable-coverage denominator
///   separate from "discovered" or "attempted", and explain why a file
///   produced no targets. Old artifacts default each new field to its zero
///   value via serde, so `parse_explore_summary` keeps reading them.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ExploreSummary {
    version: u32,
    status: String,
    file: String,
    total_functions: usize,
    completed: usize,
    failed: usize,
    skipped: usize,
    elapsed_secs: f64,
    /// Functions that returned an `InstrumentationFailed` / "build failed"
    /// reason from the executor. Subset of the legacy `failed` count.
    #[serde(default)]
    build_failed: usize,
    /// Functions that failed at runtime (panic, thrown error, frontend error)
    /// without matching the build-failure or timeout reason heuristics.
    /// Subset of the legacy `failed` count.
    #[serde(default)]
    runtime_failed: usize,
    /// Functions whose execution exceeded the per-function time budget.
    /// Subset of the legacy `failed` count.
    #[serde(default)]
    timed_out: usize,
    /// Pre-skipped because the analyzer flagged unexecutable parameter types
    /// (no compatible value generators). Subset of the legacy `skipped`
    /// count.
    #[serde(default)]
    unsupported: usize,
    /// Skipped by an explicit user/config policy rather than because of an
    /// unsupported signature. Subset of the legacy `skipped` count.
    #[serde(default)]
    skipped_by_policy: usize,
    /// Functions whose run was skipped because the frontend's environment
    /// preflight failed (str-jeen.40). Distinct from `unsupported` so
    /// env-level faults bucket separately from frontend-capability gaps.
    /// Field is omitted from JSON when zero so existing snapshots remain
    /// stable for runs that never hit a preflight failure.
    #[serde(default, skip_serializing_if = "is_zero_usize")]
    preflight_failed: usize,
    /// Functions that produced at least one explored path. The
    /// "produced-coverage denominator" — distinct from `total_functions`
    /// (discovered) and from `completed` (no exception, but possibly zero
    /// paths because the function had no branches to exercise).
    #[serde(default)]
    produced_coverage: usize,
    /// Closed-taxonomy reason populated only when `total_functions == 0`.
    /// Surfaces *why* shatter found nothing to attempt for this file.
    ///
    /// Schema (str-jeen.21): the variant is one of the
    /// `shatter_core::protocol::NoTargetReason` tokens. Default is
    /// `unclassified` for any zero-target file until per-language
    /// (str-jeen.22–.24) or frontend-agnostic (str-jeen.25) classifiers
    /// refine it. `None` for files that produced at least one target.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    no_target_reason: Option<shatter_core::protocol::NoTargetReason>,
    /// Go-only root-cause breakdown of `build_failed` outcomes for this
    /// file (str-jeen.31). Populated at finalization time when the file
    /// extension is `.go` and at least one `build_failed` outcome was
    /// recorded; absent on TS / Rust files and on Go files with no
    /// build failures.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    go_root_causes: Option<GoRootCauseBreakdown>,
    /// TS-only root-cause breakdown of `build_failed` / `runtime_failed`
    /// outcomes for this file (str-jeen.6). Populated at finalization
    /// time when the file extension is `.ts` / `.tsx` / `.js` / `.jsx`
    /// and at least one matching outcome was recorded; absent otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ts_root_causes: Option<TsRootCauseBreakdown>,
    /// Sum of `line_count` across every discovered function, regardless of
    /// outcome (str-jeen.18). Includes pre-skipped (unsupported) entries so
    /// downstream broad-run reports (str-jeen.19, str-jeen.20) can quote a
    /// stable "lines-of-source we even saw" denominator independent of how
    /// many functions we then scheduled or finished.
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    discovered_function_span_lines: u32,
    /// Sum of `line_count` across functions whose exploration was actually
    /// attempted (str-jeen.18) — i.e., outcome status is one of
    /// {`completed`, `completed_with_findings`, `build_failed`,
    /// `runtime_failed`, `timed_out`}. Pre-skipped (`unsupported`,
    /// `skipped_by_policy`) entries are excluded.
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    attempted_function_span_lines: u32,
    /// Sum of `line_count` across functions whose exploration finished
    /// without a build / runtime / timeout failure (str-jeen.18).
    /// Outcome status ∈ {`completed`, `completed_with_findings`}.
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    completed_function_span_lines: u32,
    /// Sum of executor-reported covered lines across completed functions
    /// (str-jeen.41). Each entry's contribution is capped at its `line_count`
    /// so `covered_completed_lines + uncovered_completed_lines ==
    /// completed_function_span_lines`.
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    covered_completed_lines: u32,
    /// Completed-function span lines that were not exercised by any
    /// observed execution (str-jeen.41). Computed as `line_count -
    /// covered_lines` per completed entry, summed across the file.
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    uncovered_completed_lines: u32,
    /// Sum of `line_count` across functions whose final outcome was
    /// `BuildFailed` (str-jeen.41). Line-weighted impact of build /
    /// instrumentation failures so a failure on a 200-line function counts
    /// for more than a failure on a 5-line stub.
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    build_failed_function_lines: u32,
    /// Sum of `line_count` across functions whose final outcome was
    /// `RuntimeFailed` (str-jeen.41).
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    runtime_failed_function_lines: u32,
    /// Sum of `line_count` across functions whose final outcome was
    /// `TimedOut` (str-jeen.41).
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    timed_out_function_lines: u32,
    /// Sum of `line_count` across functions whose final outcome was
    /// `Unsupported` (str-jeen.41), i.e. pre-skipped because the analyzer
    /// flagged unexecutable parameter types.
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    unsupported_function_lines: u32,
    functions: Vec<ExploreSummaryEntry>,
}

/// Reason an explore run terminates with a nonzero process exit even though
/// no internal `Err` propagated up to `main` (str-960w).
///
/// Before this type existed, `shatter explore` returned `Ok(())` whenever the
/// pipeline completed end-to-end, regardless of whether any target produced
/// usable output. Runs where every attempted target hit `build_failed`
/// silently exited 0 and CI / agent workflows could not detect the failure.
#[derive(Debug)]
enum ExploreFailure {
    /// At least one target was attempted (scheduled one or more functions)
    /// and not a single attempted target produced a `completed` function.
    AllAttemptedTargetsFailed {
        attempted_targets: usize,
        build_failed: usize,
        runtime_failed: usize,
        timed_out: usize,
    },
}

impl std::fmt::Display for ExploreFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExploreFailure::AllAttemptedTargetsFailed {
                attempted_targets,
                build_failed,
                runtime_failed,
                timed_out,
            } => write!(
                f,
                "explore: all {attempted_targets} attempted target(s) failed \
                 (build_failed={build_failed}, runtime_failed={runtime_failed}, \
                 timed_out={timed_out}); no completed functions"
            ),
        }
    }
}

impl std::error::Error for ExploreFailure {}

/// Decide whether an explore run should exit nonzero based on its per-file
/// `ExploreSummary` rows (str-960w).
///
/// A target is considered "attempted" when it scheduled at least one
/// function for execution — i.e.
/// `completed + build_failed + runtime_failed + timed_out > 0`. Targets
/// that produced zero functions (e.g. test files, declaration-only modules,
/// or any other `no_target_reason` row) are not "attempted" and do not by
/// themselves flip the exit code.
///
/// Partial-success policy (machine-readable): the run exits 0 whenever at
/// least one attempted target produced a `completed` function, even if
/// other attempted targets failed. Per-target counters in each
/// `summary.json` (`completed`, `build_failed`, `runtime_failed`,
/// `timed_out`) remain the source of truth for callers wanting a stricter
/// gate. Returns `Err(ExploreFailure)` only when every attempted target
/// has zero `completed` outcomes.
fn decide_explore_exit_status(summaries: &[ExploreSummary]) -> Result<(), ExploreFailure> {
    let mut attempted_targets = 0usize;
    let mut successful_targets = 0usize;
    let mut total_build_failed = 0usize;
    let mut total_runtime_failed = 0usize;
    let mut total_timed_out = 0usize;
    for summary in summaries {
        let attempted_in_target =
            summary.completed + summary.build_failed + summary.runtime_failed + summary.timed_out;
        // str-ni32: surface analyze/preflight failures via the same exit-code
        // path that already covers build_failed/runtime_failed/timed_out. A
        // file that never reached the exploration loop has zero counts in
        // each bucket, but its summary carries a `parser_failure:` status —
        // count it as an attempted-and-failed target so `explore -o
        // file.json` against e.g. a missing `go.mod` exits nonzero instead
        // of looking like a no-op success.
        let analyze_failed_in_target =
            attempted_in_target == 0 && summary.status.starts_with("parser_failure");
        if attempted_in_target == 0 && !analyze_failed_in_target {
            continue;
        }
        attempted_targets += 1;
        if summary.completed > 0 {
            successful_targets += 1;
        }
        total_build_failed += summary.build_failed;
        total_runtime_failed += summary.runtime_failed;
        total_timed_out += summary.timed_out;
    }
    if attempted_targets >= 1 && successful_targets == 0 {
        return Err(ExploreFailure::AllAttemptedTargetsFailed {
            attempted_targets,
            build_failed: total_build_failed,
            runtime_failed: total_runtime_failed,
            timed_out: total_timed_out,
        });
    }
    Ok(())
}

/// Sum of `ExploreSummaryEntry::line_count` partitioned by outcome status,
/// producing the three named denominators surfaced in the run JSON
/// (str-jeen.18). The split mirrors the discovery → attempt → completion
/// lifecycle:
/// * `discovered`  — every entry, including pre-skipped (unsupported /
///   skipped-by-policy) functions.
/// * `attempted`   — entries whose status is one of completed,
///   completed-with-findings, build_failed, runtime_failed, timed_out.
/// * `completed`   — entries whose status is completed or
///   completed_with_findings.
///
/// The same `outcome_status_from_entry` mapping that powers
/// `bucket_counts_from_entries` is used here so the per-status counts and
/// the per-status line spans cannot drift.
///
/// str-jeen.41 extended the struct with line-weighted outcome buckets so the
/// run JSON can quote the same totals broken down by final status. The
/// invariants are:
/// * `covered_completed + uncovered_completed == completed`
/// * `build_failed + runtime_failed + timed_out == attempted - completed`
///   (skipped-by-policy contributes nothing to `attempted` or any failure
///   bucket)
/// * `unsupported + (skipped-by-policy lines, not surfaced) ==
///   discovered - attempted`
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct SpanLineDenominators {
    discovered: u32,
    attempted: u32,
    completed: u32,
    /// str-jeen.41: covered lines from completed entries, capped at
    /// `line_count` per entry by `with_covered_lines`.
    covered_completed: u32,
    /// str-jeen.41: `line_count - covered_lines` summed over completed
    /// entries. Sums with `covered_completed` to `completed`.
    uncovered_completed: u32,
    /// str-jeen.41: line-weighted bucket for `BuildFailed` outcomes.
    build_failed: u32,
    /// str-jeen.41: line-weighted bucket for `RuntimeFailed` outcomes.
    runtime_failed: u32,
    /// str-jeen.41: line-weighted bucket for `TimedOut` outcomes.
    timed_out: u32,
    /// str-jeen.41: line-weighted bucket for `Unsupported` outcomes
    /// (pre-skipped because the analyzer flagged unexecutable parameter
    /// types). `SkippedByPolicy` is intentionally not surfaced here:
    /// it represents user-driven exclusion, not a coverage gap.
    unsupported: u32,
}

fn span_line_denominators_from_entries(entries: &[ExploreSummaryEntry]) -> SpanLineDenominators {
    use shatter_core::protocol::OutcomeStatus;
    let mut totals = SpanLineDenominators::default();
    for entry in entries {
        let lines = entry.line_count;
        totals.discovered = totals.discovered.saturating_add(lines);
        match outcome_status_from_entry(entry) {
            OutcomeStatus::Completed | OutcomeStatus::CompletedWithFindings => {
                totals.attempted = totals.attempted.saturating_add(lines);
                totals.completed = totals.completed.saturating_add(lines);
                // `with_covered_lines` already caps `covered_lines` at
                // `line_count`, but defend against artifact entries that
                // pre-date the cap by re-applying it here. This keeps the
                // covered + uncovered = completed invariant unconditional.
                let covered = entry.covered_lines.min(lines);
                let uncovered = lines.saturating_sub(covered);
                totals.covered_completed = totals.covered_completed.saturating_add(covered);
                totals.uncovered_completed = totals.uncovered_completed.saturating_add(uncovered);
            }
            OutcomeStatus::BuildFailed => {
                totals.attempted = totals.attempted.saturating_add(lines);
                totals.build_failed = totals.build_failed.saturating_add(lines);
            }
            OutcomeStatus::RuntimeFailed => {
                totals.attempted = totals.attempted.saturating_add(lines);
                totals.runtime_failed = totals.runtime_failed.saturating_add(lines);
            }
            OutcomeStatus::TimedOut => {
                totals.attempted = totals.attempted.saturating_add(lines);
                totals.timed_out = totals.timed_out.saturating_add(lines);
            }
            OutcomeStatus::Unsupported => {
                // Pre-skipped: counted in `discovered` and surfaced as a
                // dedicated outcome bucket (str-jeen.41).
                totals.unsupported = totals.unsupported.saturating_add(lines);
            }
            OutcomeStatus::SkippedByPolicy => {
                // Pre-skipped: counted only in `discovered`. No outcome
                // bucket — policy exclusion is user-driven, not a gap.
            }
            OutcomeStatus::PreflightFailed => {
                // Env-preflight failure (str-jeen.40): the fault is outside
                // the function under test, so the lines are not "attempted"
                // and there is no per-function gap to surface. Counted only
                // in `discovered`.
            }
        }
    }
    totals
}

/// Per-`OutcomeStatus` counts derived from a slice of `ExploreSummaryEntry`.
///
/// Keep in sync with `bucket_counts_from_entries` and with the
/// `outcome_status_from_entry` mapping. Used both for footer rendering and
/// for the `ExploreSummary` bucket fields, so a single source of truth keeps
/// the persisted artifact and the live footer agreeing on counts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct OutcomeBuckets {
    completed: usize,
    runtime_failed: usize,
    build_failed: usize,
    timed_out: usize,
    unsupported: usize,
    skipped_by_policy: usize,
    /// Env-preflight failure bucket (str-jeen.40). Distinct from
    /// `unsupported` so the run-JSON can reflect env faults separately.
    preflight_failed: usize,
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
    skipped_unexecutable: Vec<(String, u32, Vec<executability::SkipReason>)>,
    artifact_root: PathBuf,
    target_start: Instant,
    explore_summary: ExploreSummary,
    #[allow(dead_code)]
    work_item_indices: Vec<usize>,
}

/// Call the frontend's invocation planner (when `--planner` is active) and
/// return seed inputs materialized from its plans.
///
/// This runs once per target before the observe stage. On any failure
/// (capability missing, analyze error, non-planner response), we log and
/// return an empty vec so exploration falls through to its regular seed
/// sources. Primes `task_frontend`'s analysis cache with an extra analyze
/// because task frontends are freshly spawned and have no cached target
/// metadata; the planner's target_id lookup needs that cache.
async fn fetch_planner_extra_seeds(
    task_frontend: &mut shatter_core::frontend::Frontend,
    explore_config: &shatter_core::explorer::ExploreConfig,
    func: &shatter_core::protocol::FunctionAnalysis,
    file_str: &str,
    project_root: Option<&str>,
) -> (
    Vec<Vec<serde_json::Value>>,
    Option<shatter_core::protocol::InvocationPlan>,
) {
    let Some(_planner_name) = explore_config.planner.as_deref() else {
        return (Vec::new(), None);
    };

    // Prime the task frontend's analysis cache so get_invocation_plan can
    // resolve the target_id via its analyzed-by-name lookup.
    let analyze_result = task_frontend
        .send(shatter_core::protocol::Command::Analyze {
            file: file_str.to_string(),
            function: Some(func.name.clone()),
            project_root: project_root.map(str::to_string),
            execution_profile: explore_config.execution_profile.clone(),
        })
        .await;
    if let Err(e) = analyze_result {
        tracing::warn!("planner: analyze priming failed for {}: {e}", func.name);
        return (Vec::new(), None);
    }

    // Free functions: target_id carries only the bare symbol. Our Go handler
    // falls back to linear scan by FunctionAnalysis.Name when the colon
    // prefix is absent, so `:{name}` is sufficient for the MVP (method
    // targets would need a resolved package path).
    let target_id = format!(":{}", func.name);
    match shatter_core::planner_consumer::fetch_planner_seeds(
        task_frontend,
        &target_id,
        &func.params,
    )
    .await
    {
        Ok(bundle) => {
            tracing::info!(
                "planner: target={} seeds={} plans={} unsatisfied={}",
                func.name,
                bundle.seeds.len(),
                bundle.plans.len(),
                bundle.unsatisfied.len(),
            );
            let first_plan = bundle.plans.into_iter().next();
            (bundle.seeds, first_plan)
        }
        Err(e) => {
            tracing::warn!("planner: fetch failed for {}: {e}", func.name);
            (Vec::new(), None)
        }
    }
}

fn explore_artifact_root(project_root: Option<&str>) -> PathBuf {
    let root = project_root
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));
    shatter_core::harness_storage::HarnessStorage::resolve_artifact_root(&root)
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

/// Path to the per-target artifact subdirectory (the directory containing
/// `summary.json` plus per-function artifact JSON and resume-state sidecars
/// for `file`).
fn target_artifact_dir(root: &Path, file: &str) -> PathBuf {
    root.join(sanitize_artifact_component(file))
}

/// Remove all explore artifacts associated with `file` under `root`. Used to
/// implement `--clean` semantics for `shatter explore`: after this call,
/// resume detection cannot find a prior summary, per-function artifact, or
/// resume-state sidecar for the target.
///
/// Missing directories are not an error (idempotent). I/O failures are logged
/// at debug level and otherwise ignored — `--clean` is best-effort cleanup
/// and the subsequent run will overwrite or skip any leftover files anyway.
fn clean_target_artifacts(root: &Path, file: &str) {
    let dir = target_artifact_dir(root, file);
    if !dir.exists() {
        return;
    }
    if let Err(e) = std::fs::remove_dir_all(&dir) {
        log::debug!(
            "Failed to remove explore artifacts at {} for --clean: {e}",
            dir.display(),
        );
    }
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

// ---------------------------------------------------------------------------
// str-jeen.4: artifact-reference validator
// ---------------------------------------------------------------------------

/// Logical artifact reference fed into [`validate_artifact_refs`].
///
/// Decouples the validator from the on-disk shape it was first written
/// against (`ExploreSummary`) so commands that emit different shapes —
/// notably `run`, which writes per-function `<safe_name>.md` files
/// alongside a top-level `run.json` — can validate the same contract
/// (str-ux7q). `file` is a logical owning unit (source-file path for
/// explore, qualified function name for run); the validator only uses
/// it to attribute issue diagnostics.
#[derive(Debug, Clone)]
pub(crate) struct ArtifactRef {
    pub file: String,
    pub function_name: String,
    pub status: String,
    pub artifact: Option<String>,
    pub reason: Option<String>,
}

/// Per-call configuration for [`validate_artifact_refs`]. Different
/// commands write different artifact shapes (explore: per-function
/// `*.json`, plus `summary.json` and `*.resume-state.json` control
/// files; run: per-function `*.md`, plus a top-level `run.json`
/// summary). The config tells the stale-extra scanner which file
/// names count as artifacts vs. control files.
#[derive(Debug, Clone)]
pub(crate) struct ArtifactValidationOptions<'a> {
    /// File extensions (with leading dot) that count as artifact files
    /// for stale-extra detection. A file under `artifact_root` whose
    /// name ends with one of these and is not referenced by any
    /// `ArtifactRef` is reported as `stale_extra`.
    pub artifact_extensions: &'a [&'a str],
    /// Exact basenames to skip during the stale-extra scan (control
    /// files like `summary.json` or `run.json`).
    pub skip_filenames: &'a [&'a str],
    /// Filename suffixes to skip during the stale-extra scan
    /// (sidecars like `.resume-state.json` or atomic-write `.tmp`).
    pub skip_suffixes: &'a [&'a str],
}

impl ArtifactValidationOptions<'static> {
    /// Defaults matching the explore command's on-disk shape: per-
    /// function `*.json` artifacts plus `summary.json` /
    /// `*.resume-state.json` control files.
    pub(crate) fn explore_defaults() -> Self {
        const EXPLORE_EXTENSIONS: &[&str] = &[".json"];
        const EXPLORE_SKIP_FILENAMES: &[&str] = &["summary.json"];
        const EXPLORE_SKIP_SUFFIXES: &[&str] = &[".resume-state.json", ".tmp"];
        ArtifactValidationOptions {
            artifact_extensions: EXPLORE_EXTENSIONS,
            skip_filenames: EXPLORE_SKIP_FILENAMES,
            skip_suffixes: EXPLORE_SKIP_SUFFIXES,
        }
    }

    /// Defaults matching the run command's on-disk shape (str-ux7q):
    /// per-function `*.md` artifacts plus a top-level `run.json`
    /// summary that the validator must not flag as a stale extra.
    pub(crate) fn run_defaults() -> Self {
        const RUN_EXTENSIONS: &[&str] = &[".md"];
        const RUN_SKIP_FILENAMES: &[&str] = &["run.json"];
        const RUN_SKIP_SUFFIXES: &[&str] = &[".tmp"];
        ArtifactValidationOptions {
            artifact_extensions: RUN_EXTENSIONS,
            skip_filenames: RUN_SKIP_FILENAMES,
            skip_suffixes: RUN_SKIP_SUFFIXES,
        }
    }
}

/// One contract violation surfaced by [`validate_artifact_refs`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ArtifactValidationIssue {
    /// Entry claims `artifact: Some(path)` but the file is absent.
    MissingArtifact {
        file: String,
        function_name: String,
        artifact_relpath: String,
    },
    /// Entry has neither an artifact path nor an `unavailable_reason` token —
    /// downstream consumers can't classify the row.
    MissingUnavailableReason {
        file: String,
        function_name: String,
        status: String,
    },
    /// File on disk under the artifact root that is not referenced by any
    /// entry in any summary. Reported (per str-jeen.4 issue text) rather than
    /// deleted — deletion is destructive and the wrapper that owns the
    /// directory may legitimately stage extras.
    StaleExtra { absolute_path: PathBuf },
}

impl std::fmt::Display for ArtifactValidationIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArtifactValidationIssue::MissingArtifact {
                file,
                function_name,
                artifact_relpath,
            } => write!(
                f,
                "missing_artifact: file={file} function={function_name} path={artifact_relpath}"
            ),
            ArtifactValidationIssue::MissingUnavailableReason {
                file,
                function_name,
                status,
            } => write!(
                f,
                "missing_unavailable_reason: file={file} function={function_name} status={status}"
            ),
            ArtifactValidationIssue::StaleExtra { absolute_path } => {
                write!(f, "stale_extra: path={}", absolute_path.display())
            }
        }
    }
}

/// Result of validating an artifact-reference set against an artifact
/// directory. The integration test in `tests/artifact_references.rs`
/// asserts `issues` is empty after a normal run.
#[derive(Debug, Clone, Default)]
pub(crate) struct ArtifactValidationReport {
    pub(crate) issues: Vec<ArtifactValidationIssue>,
}

impl ArtifactValidationReport {
    pub(crate) fn is_clean(&self) -> bool {
        self.issues.is_empty()
    }
}

/// Walk `summaries` and assert the artifact-reference contract:
///
/// 1. For every entry with `artifact: Some(relpath)`, the file at
///    `<artifact_root>/<relpath>` must exist.
/// 2. For every entry whose status is not `"completed"`, either an
///    `artifact` or a typed unavailable-reason token in `reason` must be
///    present so downstream consumers can classify the row without chasing a
///    dangling path.
/// 3. Every per-function `*.json` artifact file under `artifact_root` (other
///    than `summary.json` and `*.resume-state.json` sidecars) must be
///    referenced by at least one entry's `artifact` field. Unreferenced
///    extras are reported as `stale_extra` rather than deleted.
fn validate_artifact_references(
    artifact_root: &Path,
    summaries: &[ExploreSummary],
) -> ArtifactValidationReport {
    let mut refs: Vec<ArtifactRef> = Vec::new();
    for summary in summaries {
        for entry in &summary.functions {
            refs.push(ArtifactRef {
                file: summary.file.clone(),
                function_name: entry.function_name.clone(),
                status: entry.status.clone(),
                artifact: entry.artifact.clone(),
                reason: entry.reason.clone(),
            });
        }
    }
    validate_artifact_refs(
        artifact_root,
        &refs,
        &ArtifactValidationOptions::explore_defaults(),
    )
}

/// str-ux7q: shape-agnostic artifact-reference validator. Both the
/// explore command (`ExploreSummary` rows -> per-function `*.json`)
/// and the run command (per-function `*.md` results -> exploration
/// outcomes) feed their references in via [`ArtifactRef`] so the same
/// three-rule contract — referenced files exist, non-completed rows
/// carry an unavailable-reason token, and no unreferenced extras
/// linger under the artifact root — applies uniformly.
pub(crate) fn validate_artifact_refs(
    artifact_root: &Path,
    refs: &[ArtifactRef],
    opts: &ArtifactValidationOptions,
) -> ArtifactValidationReport {
    let mut report = ArtifactValidationReport::default();
    let referenced = check_ref_paths(artifact_root, refs, &mut report);
    scan_stale_extras(artifact_root, &referenced, &mut report, opts);
    report
}

/// Per-target compatibility wrapper that preserves the
/// `check_summary_paths` entry point used by the per-target slice of
/// the contract (where `artifact_root` is shared across sibling
/// targets, so the full stale-extra sweep must be skipped).
fn check_summary_paths(
    artifact_root: &Path,
    summaries: &[ExploreSummary],
    report: &mut ArtifactValidationReport,
) -> std::collections::HashSet<PathBuf> {
    let mut refs: Vec<ArtifactRef> = Vec::new();
    for summary in summaries {
        for entry in &summary.functions {
            refs.push(ArtifactRef {
                file: summary.file.clone(),
                function_name: entry.function_name.clone(),
                status: entry.status.clone(),
                artifact: entry.artifact.clone(),
                reason: entry.reason.clone(),
            });
        }
    }
    check_ref_paths(artifact_root, &refs, report)
}

/// Path-existence + unavailable-reason half of the contract. Returns the set
/// of absolute artifact paths referenced (and verified to exist). The
/// per-target call site uses this directly to avoid false-positive
/// `stale_extra` reports against sibling targets that share `artifact_root`.
fn check_ref_paths(
    artifact_root: &Path,
    refs: &[ArtifactRef],
    report: &mut ArtifactValidationReport,
) -> std::collections::HashSet<PathBuf> {
    let mut referenced: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
    for entry in refs {
        match &entry.artifact {
            Some(relpath) if !relpath.is_empty() => {
                let abs = artifact_root.join(relpath);
                if !abs.is_file() {
                    report
                        .issues
                        .push(ArtifactValidationIssue::MissingArtifact {
                            file: entry.file.clone(),
                            function_name: entry.function_name.clone(),
                            artifact_relpath: relpath.clone(),
                        });
                } else {
                    referenced.insert(abs);
                }
            }
            _ => {
                let reason_text = entry.reason.as_deref().unwrap_or("");
                if reason_text.is_empty() {
                    report
                        .issues
                        .push(ArtifactValidationIssue::MissingUnavailableReason {
                            file: entry.file.clone(),
                            function_name: entry.function_name.clone(),
                            status: entry.status.clone(),
                        });
                }
            }
        }
    }
    referenced
}

/// Walk `artifact_root` and report any artifact-extension files that
/// no `ArtifactRef` references. Skips control filenames and sidecar
/// suffixes per `opts`. Files whose extension is not in
/// `opts.artifact_extensions` are ignored entirely so unrelated
/// auxiliary outputs (logs, READMEs, etc.) do not get flagged.
fn scan_stale_extras(
    artifact_root: &Path,
    referenced: &std::collections::HashSet<PathBuf>,
    report: &mut ArtifactValidationReport,
    opts: &ArtifactValidationOptions,
) {
    let mut dirs_to_visit = vec![artifact_root.to_path_buf()];
    while let Some(current_dir) = dirs_to_visit.pop() {
        let entries = match std::fs::read_dir(&current_dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                dirs_to_visit.push(path);
                continue;
            }
            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if opts.skip_filenames.contains(&file_name) {
                continue;
            }
            if opts
                .skip_suffixes
                .iter()
                .any(|suffix| file_name.ends_with(suffix))
            {
                continue;
            }
            let is_artifact_extension = opts
                .artifact_extensions
                .iter()
                .any(|ext| file_name.ends_with(ext));
            if !is_artifact_extension {
                continue;
            }
            if !referenced.contains(&path) {
                report.issues.push(ArtifactValidationIssue::StaleExtra {
                    absolute_path: path,
                });
            }
        }
    }
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
    let reason_lower = reason.to_lowercase();
    // str-jeen.4: prefer the typed UnavailableReason token when present so a
    // build_failed or timed_out classification doesn't silently regress to
    // RuntimeFailed because the new token uses underscores instead of spaces.
    // Preflight check first: a preflight-failure reason can also contain the
    // substring "unsupported" via the legacy stopgap message; matching the
    // dedicated preflight token before unsupported keeps the bucket honest.
    if reason_lower.contains(UnavailableReason::PreflightFailed.as_token())
        || reason_lower.contains("preflight_failed")
    {
        return OutcomeStatus::PreflightFailed;
    }
    if reason_lower.contains(UnavailableReason::TimedOut.as_token()) {
        return OutcomeStatus::TimedOut;
    }
    if reason_lower.contains(UnavailableReason::BuildFailed.as_token()) {
        return OutcomeStatus::BuildFailed;
    }
    if reason_lower.contains(UnavailableReason::RuntimeFailed.as_token()) {
        return OutcomeStatus::RuntimeFailed;
    }
    if reason_lower.contains(UnavailableReason::Unsupported.as_token()) {
        return OutcomeStatus::Unsupported;
    }
    if reason_lower.contains(UnavailableReason::SkippedByPolicy.as_token()) {
        return OutcomeStatus::SkippedByPolicy;
    }
    match entry.status.as_str() {
        "completed" => OutcomeStatus::Completed,
        "failed" => {
            if reason_lower.contains("timeout") || reason_lower.contains("timed out") {
                OutcomeStatus::TimedOut
            } else if reason_lower.contains("instrumentationfailed")
                || reason_lower.contains("build failed")
                || reason_lower.contains("compilation failed")
            {
                // str-oo31: instrumentation/build failures are distinct from
                // a runtime panic and deserve their own bucket so root-cause
                // signal isn't lost in aggregation.
                OutcomeStatus::BuildFailed
            } else {
                OutcomeStatus::RuntimeFailed
            }
        }
        "skipped" => {
            if reason_lower.contains("unexecutable") {
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

/// Bucket entries by `OutcomeStatus`. Single source of truth for both the
/// persisted `ExploreSummary` counters and the live footer breakdown.
fn bucket_counts_from_entries(entries: &[ExploreSummaryEntry]) -> OutcomeBuckets {
    use shatter_core::protocol::OutcomeStatus;
    let mut buckets = OutcomeBuckets::default();
    for entry in entries {
        match outcome_status_from_entry(entry) {
            // The explore command does not currently emit
            // `CompletedWithFindings`; treat both completed variants as the
            // same bucket so the count stays meaningful if that changes
            // upstream (str-hy9b.A2 follow-up).
            OutcomeStatus::Completed | OutcomeStatus::CompletedWithFindings => {
                buckets.completed += 1;
            }
            OutcomeStatus::RuntimeFailed => buckets.runtime_failed += 1,
            OutcomeStatus::BuildFailed => buckets.build_failed += 1,
            OutcomeStatus::TimedOut => buckets.timed_out += 1,
            OutcomeStatus::Unsupported => buckets.unsupported += 1,
            OutcomeStatus::SkippedByPolicy => buckets.skipped_by_policy += 1,
            OutcomeStatus::PreflightFailed => buckets.preflight_failed += 1,
        }
    }
    buckets
}

/// Classify why a file produced no targets to attempt. Returns `None` when
/// `total_functions > 0` (the file is not a no-target case).
///
/// `total_functions` is the count of work items the explorer scheduled
/// (post-resume, post-eligibility filtering). `pre_skipped` is the count of
/// functions the analyzer rejected as unexecutable before scheduling.
/// Classify a per-function exploration outcome into the (status, reason)
/// pair persisted in `ExploreSummaryEntry`.
///
/// Single source of truth for the str-gz8j rule that a successful
/// `Result<ObservationOutput>` whose `timed_out` flag is `true` must surface
/// as `status = "failed"` with an explicit per-function-budget reason — not
/// as `"completed"` with a silent zero-paths run. Without this downgrade the
/// `timed_out` bucket added in str-oo31 stays empty for the most common
/// timeout scenario (orchestrator's per-function timer), and slow functions
/// are indistinguishable from clean completions.
///
/// `wall_time` is the per-function clock used when synthesising the timeout
/// reason; the caller already tracks it for progress logging, so we reuse
/// it instead of plumbing the budget separately.
fn classify_outcome_status(
    result: &Result<shatter_core::explorer::ObservationOutput, String>,
    wall_time: Duration,
) -> (&'static str, Option<String>) {
    match result {
        Ok(obs) if obs.timed_out => (
            "failed",
            Some(format!(
                "function timed out after {:.1}s (per-function budget)",
                wall_time.as_secs_f64()
            )),
        ),
        Ok(_) => ("completed", None),
        Err(e) => ("failed", Some(e.clone())),
    }
}

/// Classify why a file produced no targets to attempt. Returns `None`
/// when `total_functions > 0` (the file is not a no-target case).
///
/// `total_functions` is the count of work items the explorer scheduled
/// (post-resume, post-eligibility filtering). `pre_skipped` is the
/// count of functions the analyzer rejected as unexecutable before
/// scheduling.
///
/// Per-language refinement is delegated to a language-specific helper
/// when the file path's language matches one with a registered classifier
/// (str-jeen.22–.24). The Rust classifier (str-jeen.24) lives below as
/// `rust_classify_no_target_reason`; see also the parity-matrix entry
/// `shared_wire_types.no_target_reason` (`implemented_via:
/// cli_classifier`). Files that fall through every per-language check
/// land on `Unclassified` so all zero-target files retain a default
/// taxonomy slot.
fn classify_no_target_reason(
    total_functions: usize,
    _pre_skipped: usize,
    language: crate::args::Language,
    file: &Path,
) -> Option<shatter_core::protocol::NoTargetReason> {
    if total_functions > 0 {
        return None;
    }
    if language == crate::args::Language::Rust
        && let Some(reason) = rust_classify_no_target_reason(file)
    {
        return Some(reason);
    }
    if language == crate::args::Language::TypeScript
        && let Some(reason) = ts_classify_no_target_reason(file)
    {
        return Some(reason);
    }
    if language == crate::args::Language::Go
        && let Some(reason) = go_classify_no_target_reason(file)
    {
        return Some(reason);
    }
    Some(shatter_core::protocol::NoTargetReason::Unclassified)
}

/// Filename Cargo treats as a build script when present at a crate root.
const RUST_BUILD_SCRIPT_FILENAME: &str = "build.rs";

/// Manifest filename that anchors a Cargo crate root. A `build.rs` file is
/// only treated as a build script when this manifest sits alongside it.
const RUST_CRATE_MANIFEST_FILENAME: &str = "Cargo.toml";

/// Cargo's integration-test directory segment. Files under any directory
/// named exactly `tests` (Cargo's convention for `tests/*.rs` integration
/// tests) classify as `test_module` regardless of content.
const RUST_INTEGRATION_TEST_DIR: &str = "tests";

/// Suffix conventions for Rust test files. Either suffix on the basename
/// is treated as a `test_module` signal even outside a `tests/` directory.
const RUST_TEST_FILE_SUFFIXES: &[&str] = &["_test.rs", "_tests.rs"];

/// Maximum bytes of source we read when running content-based heuristics
/// for a Rust no-target file. The file already returned zero analyzer
/// targets, so a coarse line scan is sufficient — a hard cap protects
/// against the rare oversized fixture without changing classifier behavior
/// on real Rust files (well under this size in practice).
const RUST_CONTENT_SCAN_BYTE_CAP: usize = 64 * 1024;

/// Per-Rust no-target reason classifier (str-jeen.24). Returns the
/// taxonomy variant a Rust file matches, or `None` if no Rust-specific
/// signal applies (caller falls back to `Unclassified`). Hosted CLI-side
/// per the str-jeen.25 precedent — see `parity-matrix.yaml`
/// `shared_wire_types.no_target_reason`.
///
/// Order of checks (first match wins):
///   1. `build_script` — basename `build.rs` AND sibling `Cargo.toml`.
///   2. `test_module` (path) — file under any `tests/` directory segment,
///      OR basename ends in `_test.rs` / `_tests.rs` and is not a crate
///      root file. Cargo's integration-test convention is unambiguous.
///   3. `declaration_only` — content scan finds no items beyond
///      `mod` / `use` / `pub use` / `extern crate` declarations and
///      attribute lines. Heuristic is conservative: if the scanner finds
///      anything it doesn't recognize as a pure declaration (notably
///      `macro_rules!`, `include!`, or any token that might expand into
///      a definition), the function returns `None` rather than mislabel
///      a macro-heavy file.
///   4. `test_module` (content fallback) — every non-attribute item
///      sits under a `#[cfg(test)]` gate or carries `#[test]`.
///
/// Anything else returns `None` so the caller emits `Unclassified`.
fn rust_classify_no_target_reason(file: &Path) -> Option<shatter_core::protocol::NoTargetReason> {
    use shatter_core::protocol::NoTargetReason;

    if rust_is_build_script(file) {
        return Some(NoTargetReason::BuildScript);
    }
    if rust_is_test_module_by_path(file) {
        return Some(NoTargetReason::TestModule);
    }

    // Content-based heuristics share a single source read.
    let source = rust_read_capped_source(file)?;
    if rust_is_declaration_only(&source) {
        return Some(NoTargetReason::DeclarationOnly);
    }
    if rust_is_test_module_by_content(&source) {
        return Some(NoTargetReason::TestModule);
    }

    None
}

/// True when `file` is exactly `build.rs` AND a sibling `Cargo.toml`
/// sits in the same directory (Cargo's crate-root convention). A
/// `build.rs` deep in a fixtures tree without a sibling manifest does
/// NOT classify, per the planning notes in str-jeen.24.
fn rust_is_build_script(file: &Path) -> bool {
    let basename_matches = file
        .file_name()
        .and_then(|s| s.to_str())
        .is_some_and(|name| name == RUST_BUILD_SCRIPT_FILENAME);
    if !basename_matches {
        return false;
    }
    let Some(parent) = file.parent() else {
        return false;
    };
    parent.join(RUST_CRATE_MANIFEST_FILENAME).is_file()
}

/// True when `file` lives under a `tests/` directory segment OR its
/// basename ends in `_test.rs` / `_tests.rs` (and is not the crate root,
/// which Cargo names `lib.rs` or `main.rs`). Path-based test detection
/// is unambiguous in Cargo so we accept it without reading the source.
fn rust_is_test_module_by_path(file: &Path) -> bool {
    let in_tests_dir = file
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .any(|seg| seg == RUST_INTEGRATION_TEST_DIR);
    if in_tests_dir {
        return true;
    }
    let Some(basename) = file.file_name().and_then(|s| s.to_str()) else {
        return false;
    };
    RUST_TEST_FILE_SUFFIXES
        .iter()
        .any(|suffix| basename.ends_with(suffix))
}

/// Read up to `RUST_CONTENT_SCAN_BYTE_CAP` bytes of the file as UTF-8.
/// Returns `None` on IO error or non-UTF-8 prefix; the caller treats
/// that as "no Rust-specific signal" and falls back to `Unclassified`.
fn rust_read_capped_source(file: &Path) -> Option<String> {
    use std::io::Read;
    let mut handle = std::fs::File::open(file).ok()?;
    let mut buf = vec![0u8; RUST_CONTENT_SCAN_BYTE_CAP];
    let n = handle.read(&mut buf).ok()?;
    buf.truncate(n);
    String::from_utf8(buf).ok()
}

/// True when the source contains only declarations: `mod`, `use`,
/// `pub use`, `pub mod`, `extern crate`, attribute lines, comments,
/// and blank lines. A single `fn`, `impl`, `trait`, `struct`, `enum`,
/// `const`, `static`, `type`, `union`, `macro_rules!`, or any unknown
/// non-blank statement disqualifies the file — the heuristic stays
/// conservative so a macro-heavy file (e.g. `include!(...)` or
/// re-exporting macros) returns `false` and the caller emits
/// `Unclassified` rather than a wrong taxonomy slot.
fn rust_is_declaration_only(source: &str) -> bool {
    let mut saw_declaration = false;
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || rust_line_is_pure_attribute_or_comment(trimmed) {
            continue;
        }
        if rust_line_is_pure_declaration(trimmed) {
            saw_declaration = true;
            continue;
        }
        // Anything else (a top-level item body, a macro invocation, an
        // inline `mod foo { ... }` block-open, etc.) is conservatively
        // "not pure declaration" — return false so the caller falls
        // through to the content-fallback test_module check or
        // Unclassified.
        return false;
    }
    saw_declaration
}

/// True for an attribute (`#[...]` / `#![...]`), a line comment
/// (`// ...` or `/// ...`), or the trivial single-line block-comment
/// form (`/* ... */`). Multi-line block comments are conservatively
/// rejected — they're rare in declaration-only files and a content scanner
/// for them adds complexity without behavioral wins.
fn rust_line_is_pure_attribute_or_comment(line: &str) -> bool {
    if line.starts_with("//") {
        return true;
    }
    if line.starts_with("#[") || line.starts_with("#![") {
        return true;
    }
    if line.starts_with("/*") && line.ends_with("*/") {
        return true;
    }
    false
}

/// True for a single-line declaration: `mod x;`, `pub mod x;`,
/// `use a::b;`, `pub use a::b::*;`, `extern crate foo;`. Lines that open
/// an inline module (`mod x {`) or contain anything beyond a terminating
/// `;` are NOT pure declarations — the conservative scanner returns
/// `false` and the caller falls through.
fn rust_line_is_pure_declaration(line: &str) -> bool {
    if !line.ends_with(';') {
        return false;
    }
    let stripped = line.trim_end_matches(';').trim();
    let head = stripped.split_whitespace().next().unwrap_or("");
    match head {
        "mod" | "use" | "extern" => true,
        "pub" => {
            // `pub mod x`, `pub use a::b`, `pub(crate) mod x`, etc.
            let after_pub = stripped.split_whitespace().nth(1).unwrap_or("");
            // Allow `pub(...)` visibility forms — extract the next token
            // after the visibility marker.
            let after_pub = if after_pub.starts_with('(') {
                stripped.split_whitespace().nth(2).unwrap_or("")
            } else {
                after_pub
            };
            matches!(after_pub, "mod" | "use" | "extern")
        }
        _ => false,
    }
}

/// True when every non-attribute item in the source sits under a
/// `#[cfg(test)]` gate (module or item-level) or carries a `#[test]`
/// attribute. The heuristic walks lines, tracking whether the most
/// recent attribute(s) before an item include `#[cfg(test)]` /
/// `#[test]`. Conservative: if any top-level item lacks such a gate,
/// returns `false`.
fn rust_is_test_module_by_content(source: &str) -> bool {
    let mut pending_cfg_test = false;
    let mut pending_test_attr = false;
    let mut saw_gated_item = false;

    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }
        if trimmed.starts_with("#[") || trimmed.starts_with("#![") {
            // Attribute. Track cfg(test) / test markers until the next
            // non-attribute line consumes them.
            if trimmed.contains("cfg(test)") {
                pending_cfg_test = true;
            }
            if trimmed.contains("#[test]") || trimmed == "#[test]" {
                pending_test_attr = true;
            }
            continue;
        }
        // Non-attribute line — must be gated by a test-related attribute.
        if pending_cfg_test || pending_test_attr {
            saw_gated_item = true;
            pending_cfg_test = false;
            pending_test_attr = false;
            continue;
        }
        return false;
    }
    saw_gated_item
}

// ── TypeScript no-target reason classifier (str-jeen.22) ──────────────────
//
// Mirrors the Rust classifier above. Hosted CLI-side per the str-jeen.25
// frontend-agnostic precedent — see `parity-matrix.yaml`
// `shared_wire_types.no_target_reason`. Refines zero-target TS files into
// one of the three TS-specific taxonomy variants declared in str-jeen.21:
// `declaration_only`, `jsx_component_only`, `test_or_spec`. Anything that
// fails every TS-specific signal returns `None` so the caller emits
// `Unclassified`.

/// Filename suffixes that mark TS ambient declaration files. `.d.cts` and
/// `.d.mts` are the CommonJS / ESM module-flavored declaration variants.
const TS_DECLARATION_FILE_SUFFIXES: &[&str] = &[".d.ts", ".d.cts", ".d.mts"];

/// Filename infixes that mark TS test, spec, story, or demo files. The
/// dotted form (`*.test.ts`, `*.spec.tsx`, `*.stories.tsx`) is the
/// dominant convention across Jest, Vitest, and Storybook.
const TS_TEST_FILE_INFIXES: &[&str] = &[".test.", ".spec.", ".stories."];

/// Path segments that indicate a test or fixture directory. Files under
/// any of these segments classify as `test_or_spec` regardless of
/// filename.
const TS_TEST_DIR_SEGMENTS: &[&str] = &["__tests__", "__mocks__", "tests"];

/// JSX/TSX file extensions. Only files with these extensions are eligible
/// for the `jsx_component_only` content check — a `.ts` file containing
/// `<` for unrelated reasons must not be classified as a component file.
const TS_JSX_FILE_EXTENSIONS: &[&str] = &[".tsx", ".jsx"];

/// Maximum bytes of source we read when running TS content heuristics.
/// Same rationale as the Rust classifier: the file already returned zero
/// analyzer targets, so a coarse line scan is sufficient and a hard cap
/// protects against oversized fixtures.
const TS_CONTENT_SCAN_BYTE_CAP: usize = 64 * 1024;

/// Per-TypeScript no-target reason classifier (str-jeen.22). Returns the
/// taxonomy variant a TS file matches, or `None` if no TS-specific signal
/// applies (caller falls back to `Unclassified`).
///
/// Order of checks (first match wins):
///   1. `declaration_only` (path) — basename ends in `.d.ts`, `.d.cts`,
///      or `.d.mts`. Ambient-declaration files are unambiguous regardless
///      of contents.
///   2. `test_or_spec` (path) — file under any `__tests__`, `__mocks__`,
///      or `tests` directory segment, OR basename contains a `.test.` /
///      `.spec.` / `.stories.` infix.
///   3. `jsx_component_only` (content) — file extension is `.tsx` /
///      `.jsx` AND the source contains a JSX closing-tag form (`</`),
///      which is uniquely JSX (TS generics use `<T>` but never `</T>`).
///   4. `declaration_only` (content) — every depth-zero, non-blank,
///      non-comment line is one of: `import` / `export type` /
///      `export interface` / `export *` / `export {` / `export default
///      type` / `type ` / `interface ` / `declare ` / `export declare`.
///      Conservative: if any line looks like a value-level definition
///      (`function`, `const`, `let`, `var`, `class`, `enum`,
///      `export function`, etc.) the function returns `false` and the
///      caller falls through to `Unclassified`.
fn ts_classify_no_target_reason(file: &Path) -> Option<shatter_core::protocol::NoTargetReason> {
    use shatter_core::protocol::NoTargetReason;

    if ts_is_declaration_file_by_path(file) {
        return Some(NoTargetReason::DeclarationOnly);
    }
    if ts_is_test_or_spec_by_path(file) {
        return Some(NoTargetReason::TestOrSpec);
    }

    let source = ts_read_capped_source(file)?;
    if ts_is_jsx_extension(file) && ts_contains_jsx_component(&source) {
        return Some(NoTargetReason::JsxComponentOnly);
    }
    if ts_is_declaration_only(&source) {
        return Some(NoTargetReason::DeclarationOnly);
    }

    None
}

/// True when the basename ends in one of the ambient-declaration
/// extensions. The longest-suffix list contains `.d.ts`, `.d.cts`, and
/// `.d.mts`.
fn ts_is_declaration_file_by_path(file: &Path) -> bool {
    let Some(basename) = file.file_name().and_then(|s| s.to_str()) else {
        return false;
    };
    TS_DECLARATION_FILE_SUFFIXES
        .iter()
        .any(|suffix| basename.ends_with(suffix))
}

/// True when `file` lives under a recognized test directory segment OR
/// its basename contains a recognized test/spec/story infix.
fn ts_is_test_or_spec_by_path(file: &Path) -> bool {
    let in_test_segment = file
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .any(|seg| TS_TEST_DIR_SEGMENTS.contains(&seg));
    if in_test_segment {
        return true;
    }
    let Some(basename) = file.file_name().and_then(|s| s.to_str()) else {
        return false;
    };
    TS_TEST_FILE_INFIXES
        .iter()
        .any(|infix| basename.contains(infix))
}

/// True when the file extension is `.tsx` or `.jsx`.
fn ts_is_jsx_extension(file: &Path) -> bool {
    let Some(basename) = file.file_name().and_then(|s| s.to_str()) else {
        return false;
    };
    TS_JSX_FILE_EXTENSIONS
        .iter()
        .any(|suffix| basename.ends_with(suffix))
}

/// Read up to `TS_CONTENT_SCAN_BYTE_CAP` bytes of the file as UTF-8.
/// Returns `None` on IO error or non-UTF-8 prefix; the caller treats that
/// as "no TS-specific content signal".
fn ts_read_capped_source(file: &Path) -> Option<String> {
    use std::io::Read;
    let mut handle = std::fs::File::open(file).ok()?;
    let mut buf = vec![0u8; TS_CONTENT_SCAN_BYTE_CAP];
    let n = handle.read(&mut buf).ok()?;
    buf.truncate(n);
    String::from_utf8(buf).ok()
}

/// True when the source contains a JSX closing-tag form (`</`). TS
/// generics use `<T>` but never `</T>`, so the closing-tag form is a
/// reliable JSX signal that doesn't mistake `Array<number>` for a
/// component. Conservative against the empty file: a `.tsx` with no
/// `</` returns false and the caller falls through.
fn ts_contains_jsx_component(source: &str) -> bool {
    source.contains("</")
}

/// True when every depth-zero, non-blank, non-comment line is a pure
/// type-level declaration (import, type alias, interface, ambient
/// declaration). Tracks brace depth so multi-line `interface Foo { ... }`
/// bodies are accepted; when at depth 0 a non-declaration line appears,
/// returns false and the caller falls through.
fn ts_is_declaration_only(source: &str) -> bool {
    let mut depth: usize = 0;
    let mut saw_declaration = false;
    let mut in_block_comment = false;
    for line in source.lines() {
        let trimmed = line.trim();
        // Multi-line block-comment tracking: skip any line entirely
        // contained inside an unterminated `/* ... */` region.
        if in_block_comment {
            if trimmed.contains("*/") {
                in_block_comment = false;
            }
            continue;
        }
        if trimmed.starts_with("/*") && !trimmed.contains("*/") {
            in_block_comment = true;
            continue;
        }
        if trimmed.is_empty() || ts_line_is_pure_comment(trimmed) {
            continue;
        }
        if depth == 0 {
            if ts_line_is_pure_declaration(trimmed) {
                saw_declaration = true;
            } else if !ts_line_is_pure_brace_close(trimmed) {
                return false;
            }
        }
        // Track brace depth from the raw line (string literals in TS
        // declaration files are rare enough that a line-level scan is
        // adequate; we deliberately stay coarse to avoid mislabeling).
        for ch in line.chars() {
            match ch {
                '{' => depth = depth.saturating_add(1),
                '}' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
    }
    saw_declaration && depth == 0
}

/// True for `// ...` line comments and trivial single-line block
/// comments. Multi-line block comments are handled by the caller's
/// in-block-comment state machine.
fn ts_line_is_pure_comment(line: &str) -> bool {
    if line.starts_with("//") {
        return true;
    }
    if line.starts_with("/*") && line.ends_with("*/") {
        return true;
    }
    false
}

/// True for a depth-zero line that opens a pure type-level declaration:
/// `import ...`, `export type ...`, `export interface ...`, `export *
/// from ...`, `export { ... } from ...`, `export declare ...`, `type X =
/// ...`, `interface X { ... }`, `declare ...`. Anything else (including
/// `export function`, `export const`, `function`, `class`, `enum`) is
/// rejected so value-level definitions do not slip through.
fn ts_line_is_pure_declaration(line: &str) -> bool {
    // Order matters: longer prefixes first so `export type` is matched
    // before the bare-`export` rejection path could fire.
    const ALLOWED_PREFIXES: &[&str] = &[
        "import ",
        "import\t",
        "import{",
        "import\"",
        "import'",
        "export type ",
        "export interface ",
        "export declare ",
        "export default type ",
        "export default interface ",
        "export * ",
        "export *\t",
        "export *;",
        "export {",
        "type ",
        "interface ",
        "declare ",
    ];
    for prefix in ALLOWED_PREFIXES {
        if line.starts_with(prefix) {
            return true;
        }
    }
    // Bare `import;` / `import "side-effect";` style with no space.
    if line == "import;" {
        return true;
    }
    false
}

/// True for a depth-zero closing brace (with optional trailing
/// semicolon/comma) — the line that terminates a multi-line `interface`
/// or `type` body. Treated as neutral: it neither asserts nor refutes
/// declaration-only status on its own.
fn ts_line_is_pure_brace_close(line: &str) -> bool {
    matches!(line, "}" | "};" | "},")
}

// ── Go no-target reason classifier (str-jeen.23) ──────────────────────────
//
// Mirrors the TS and Rust classifiers above. Hosted CLI-side per the
// str-jeen.25 frontend-agnostic precedent — see `parity-matrix.yaml`
// `shared_wire_types.no_target_reason`. Refines zero-target Go files into
// one of the three Go-relevant taxonomy variants from str-jeen.21:
// `test_file`, `generated`, `receiver_method_gap`. Anything that fails
// every Go-specific signal returns `None` so the caller emits
// `Unclassified`.

/// Filename suffix that marks Go test files. Go's testing convention is
/// unambiguous: `go test` only runs files matching this suffix.
const GO_TEST_FILE_SUFFIX: &str = "_test.go";

/// Canonical "Code generated ... DO NOT EDIT." marker recognized by
/// `go generate`, gofmt, and the go/ast generated-file detection (see
/// <https://pkg.go.dev/cmd/go#hdr-Generate_Go_files>). The marker must
/// appear on its own line as a `//`-style comment before the package
/// clause for the file to be treated as machine-generated.
const GO_GENERATED_MARKER_PREFIX: &str = "// Code generated ";
const GO_GENERATED_MARKER_SUFFIX: &str = " DO NOT EDIT.";

/// Maximum bytes of source we read when running Go content heuristics.
/// Same rationale as the TS and Rust classifiers: the file already
/// returned zero analyzer targets, so a coarse line scan is sufficient
/// and a hard cap protects against oversized fixtures.
const GO_CONTENT_SCAN_BYTE_CAP: usize = 64 * 1024;

/// Per-Go no-target reason classifier (str-jeen.23). Returns the
/// taxonomy variant a Go file matches, or `None` if no Go-specific signal
/// applies (caller falls back to `Unclassified`). Hosted CLI-side per the
/// str-jeen.25 precedent — see `parity-matrix.yaml`
/// `shared_wire_types.no_target_reason`.
///
/// Order of checks (first match wins):
///   1. `test_file` (path) — basename ends in `_test.go`. Go's testing
///      convention is unambiguous regardless of file body.
///   2. `generated` (content) — file's pre-package-clause prologue
///      contains a line matching the canonical Go generated-file marker
///      (`// Code generated ... DO NOT EDIT.`).
///   3. `receiver_method_gap` (content) — file declares one or more
///      methods (`func (recv Recv) Name(...)`) and zero free top-level
///      functions (`func Name(...)`). Conservative: if any free function
///      is present, the file should have produced a target and we fall
///      through to `None` so the caller emits `Unclassified`.
///
/// Anything else returns `None` so the caller emits `Unclassified`.
fn go_classify_no_target_reason(file: &Path) -> Option<shatter_core::protocol::NoTargetReason> {
    use shatter_core::protocol::NoTargetReason;

    if go_is_test_file_by_path(file) {
        return Some(NoTargetReason::TestFile);
    }

    let source = go_read_capped_source(file)?;
    if go_is_generated_by_content(&source) {
        return Some(NoTargetReason::Generated);
    }
    if go_is_receiver_method_gap_by_content(&source) {
        return Some(NoTargetReason::ReceiverMethodGap);
    }

    None
}

/// True when the basename ends in `_test.go` — Go's unambiguous testing
/// suffix consumed by `go test`.
fn go_is_test_file_by_path(file: &Path) -> bool {
    file.file_name()
        .and_then(|s| s.to_str())
        .is_some_and(|name| name.ends_with(GO_TEST_FILE_SUFFIX))
}

/// Read up to `GO_CONTENT_SCAN_BYTE_CAP` bytes of the file as UTF-8.
/// Returns `None` on IO error or non-UTF-8 prefix; the caller treats that
/// as "no Go-specific content signal".
fn go_read_capped_source(file: &Path) -> Option<String> {
    use std::io::Read;
    let mut handle = std::fs::File::open(file).ok()?;
    let mut buf = vec![0u8; GO_CONTENT_SCAN_BYTE_CAP];
    let n = handle.read(&mut buf).ok()?;
    buf.truncate(n);
    String::from_utf8(buf).ok()
}

/// True when the file's pre-`package` prologue contains the canonical Go
/// generated-file marker `// Code generated ... DO NOT EDIT.` on a line
/// by itself. Per the Go convention, the marker must appear before the
/// `package` clause; markers buried inside function bodies or after the
/// package clause are not recognized.
fn go_is_generated_by_content(source: &str) -> bool {
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("package ") || trimmed == "package" {
            return false;
        }
        if trimmed.starts_with(GO_GENERATED_MARKER_PREFIX)
            && trimmed.ends_with(GO_GENERATED_MARKER_SUFFIX)
        {
            return true;
        }
    }
    false
}

/// True when the file declares at least one method (a `func` whose name
/// is preceded by a parenthesized receiver) and zero free top-level
/// functions (a `func` whose name follows the `func` keyword directly).
/// Coarse line scan: only depth-zero `func ` opens count, so a `func`
/// nested inside a struct literal or a string body cannot trigger a
/// false positive. Conservative: if any free function is present the
/// file should have produced a target, so we return `false` and the
/// caller falls through to `Unclassified`.
fn go_is_receiver_method_gap_by_content(source: &str) -> bool {
    let mut saw_method = false;
    let mut depth: usize = 0;
    for line in source.lines() {
        let trimmed = line.trim();
        if depth == 0 && trimmed.starts_with("func ") {
            if go_func_line_has_receiver(trimmed) {
                saw_method = true;
            } else {
                // Free top-level function — file is not a pure
                // receiver-method-gap candidate.
                return false;
            }
        }
        for ch in line.chars() {
            match ch {
                '{' => depth = depth.saturating_add(1),
                '}' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
    }
    saw_method
}

/// True when a `func ` line opens a method declaration: the token after
/// `func` is a parenthesized receiver list (`func (r Recv) Name(...)`).
/// Free functions (`func Name(...)`) have a non-`(` token after `func`.
fn go_func_line_has_receiver(line: &str) -> bool {
    let after_func = line.strip_prefix("func ").unwrap_or("").trim_start();
    after_func.starts_with('(')
}

/// Format a one-line breakdown of non-completed buckets and the
/// produced-coverage denominator. Returns `None` when every non-completed
/// bucket is zero — the happy path the demo exercises — so the standard
/// one-line footer stays uncluttered (per str-oo31 walkthrough guidance).
fn format_outcome_breakdown(buckets: &OutcomeBuckets, produced_coverage: usize) -> Option<String> {
    let any_non_completed = buckets.runtime_failed
        + buckets.build_failed
        + buckets.timed_out
        + buckets.unsupported
        + buckets.skipped_by_policy
        + buckets.preflight_failed
        > 0;
    if !any_non_completed {
        return None;
    }
    // Append only non-zero buckets so the line stays short on partial runs.
    let mut parts: Vec<String> = Vec::new();
    let mut push = |label: &str, count: usize| {
        if count > 0 {
            parts.push(format!("{label}: {count}"));
        }
    };
    push("runtime_failed", buckets.runtime_failed);
    push("build_failed", buckets.build_failed);
    push("timed_out", buckets.timed_out);
    push("unsupported", buckets.unsupported);
    push("skipped_by_policy", buckets.skipped_by_policy);
    push("preflight_failed", buckets.preflight_failed);
    Some(format!(
        "Outcome breakdown: produced coverage: {produced_coverage} · {}",
        parts.join(" · ")
    ))
}

/// Root-cause categories for Go `build_failed` outcomes in a broad run.
/// Mirrors the buckets the Kapow validation analysis (see
/// `docs/validation/2026-04-go-frontend-kapow-rerun.md`) used to explain why
/// a Go scan's `build_failed` rows clustered into a small number of recurring
/// failure modes. The categories are mutually exclusive at classify time;
/// `Other` captures `build_failed` reasons that don't match any heuristic so
/// the aggregator's totals always equal the per-category sum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GoBuildFailureCategory {
    /// Wrapper imported a `.../internal/...` package the target's import path
    /// is not allowed to reach (Go's `internal/` visibility rule).
    InternalPackage,
    /// Wrapper failed to compile because of a missing or unused import — the
    /// stitched harness referenced (or omitted) a symbol the rewriter did
    /// not resolve.
    MissingImport,
    /// AST rewrite produced syntactically invalid Go (`syntax error`,
    /// `expected ...`, type-checker rejection that points at rewriter output).
    RewriteSyntax,
    /// Wrapper and target ended up in different `package` declarations in
    /// the same directory — Go forbids mixing package names per directory.
    MixedPackage,
    /// Build-time refusal because a parameter type has no compatible value
    /// generator, surfaced through a build error rather than the analyzer's
    /// pre-skip path (e.g. unexported type referenced through a wrapper).
    UnsupportedParamType,
    /// Receiver-plan / arity mismatch on a constructor or call site
    /// (str-jeen.6): wrapper invokes a constructor with the wrong number of
    /// arguments, or otherwise fails Go's call arity check. Distinguished
    /// from generic `MissingImport` because the fix is a planner change,
    /// not an import edit.
    ConstructorArity,
    /// `go build` refused because it could not stamp VCS metadata
    /// (str-jeen.6, Zolem 2026-05-02 evidence): typically
    /// `error obtaining VCS status` on a fresh / detached worktree. The fix
    /// is `-buildvcs=false` in the wrapper invocation, not a code change.
    BuildVcsStamping,
    /// `build_failed` reason text did not match any of the recognized
    /// patterns. Kept distinct so totals reconcile and so a future drift in
    /// frontend wording surfaces as a rising `other` bucket rather than
    /// silently distorting an existing category.
    Other,
}

impl GoBuildFailureCategory {
    fn as_str(self) -> &'static str {
        match self {
            Self::InternalPackage => "internal_package",
            Self::MissingImport => "missing_import",
            Self::RewriteSyntax => "rewrite_syntax",
            Self::MixedPackage => "mixed_package",
            Self::UnsupportedParamType => "unsupported_param_type",
            Self::ConstructorArity => "constructor_arity",
            Self::BuildVcsStamping => "build_vcs_stamping",
            Self::Other => "other",
        }
    }
}

/// Classify a Go `build_failed` reason into a root-cause bucket.
///
/// Heuristics match against the lowercased reason text. Order matters: the
/// most specific patterns come first so a reason that mentions both
/// "internal package" and "import" lands in `InternalPackage` rather than
/// degrading to `MissingImport`.
fn classify_go_build_failure(reason: &str) -> GoBuildFailureCategory {
    let r = reason.to_lowercase();
    // VCS-stamping refusal (str-jeen.6). `go build` emits
    // "error obtaining VCS status" / "error obtaining vcs status" when it
    // cannot read git metadata. Comes first because the message can also
    // mention "not a git repository" which we do NOT want to misclassify as
    // a missing import.
    if r.contains("error obtaining vcs status")
        || r.contains("error obtaining vcs information")
        || r.contains("vcs stamp")
        || r.contains("-buildvcs=false")
    {
        return GoBuildFailureCategory::BuildVcsStamping;
    }
    // Internal-package visibility rule. Go's compiler and `go list` both
    // surface this as "use of internal package ... not allowed".
    if r.contains("internal package") || r.contains("use of internal") {
        return GoBuildFailureCategory::InternalPackage;
    }
    // Mixed-package directory: `found packages X and Y in <dir>`. Match on
    // the distinguishing prefix so we don't false-positive on the standard
    // "package main" line.
    if r.contains("found packages ") || r.contains("multiple packages") {
        return GoBuildFailureCategory::MixedPackage;
    }
    // Unsupported parameter type (build-time variant). The analyzer's
    // pre-skip path already lands on Unsupported; this branch picks up the
    // residual cases where the build harness chokes on a parameter shape.
    if r.contains("unsupported parameter type")
        || r.contains("no value generator")
        || r.contains("cannot synthesize value for")
    {
        return GoBuildFailureCategory::UnsupportedParamType;
    }
    // Constructor arity / receiver-plan mismatch (str-jeen.6). Go's
    // type checker emits "not enough arguments in call to <fn>" or
    // "too many arguments in call to <fn>" when the wrapper invokes a
    // constructor with the wrong arity; "cannot use ... as ... value
    // in argument" surfaces a related receiver-plan miss. These are
    // distinct from `MissingImport` because the fix is in the
    // invocation planner, not the import set. Match before
    // `MissingImport` so the more specific arity hint wins over the
    // generic "undefined:" fallback when both phrases appear.
    if r.contains("not enough arguments in call to")
        || r.contains("too many arguments in call to")
        || r.contains("cannot use ") && r.contains(" as ") && r.contains(" value in argument")
    {
        return GoBuildFailureCategory::ConstructorArity;
    }
    // Missing or undeclared imports. `go build` emits "imported and not
    // used", `go list` emits "no required module provides package", and the
    // type checker emits "undefined: <pkg>.<sym>" / "undeclared name".
    if r.contains("imported and not used")
        || r.contains("no required module provides package")
        || r.contains("undefined:")
        || r.contains("undeclared name")
        || r.contains("missing import")
        || r.contains("could not import")
    {
        return GoBuildFailureCategory::MissingImport;
    }
    // Rewriter output that does not parse / type-check. `syntax error` is
    // the canonical Go parser message; `expected '...'` covers the parser's
    // diagnostic prefix; `not a type` and similar fire when the rewriter
    // emits an identifier in a position the type checker rejects.
    if r.contains("syntax error")
        || r.contains("expected '")
        || r.contains("expected operand")
        || r.contains("expected type")
        || r.contains("not a type")
        || r.contains("invalid recursive type")
    {
        return GoBuildFailureCategory::RewriteSyntax;
    }
    GoBuildFailureCategory::Other
}

/// Per-category count and line-weight totals for Go `build_failed` outcomes
/// across a broad run. Serialized into the per-file `ExploreSummary` JSON
/// (str-jeen.31) so downstream tooling and the broad-run markdown both read
/// from a single rollup.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct GoRootCauseBreakdown {
    /// Wrapper imported a `.../internal/...` package not visible to its
    /// import path.
    internal_package: GoRootCauseBucket,
    /// Wrapper had a missing, undeclared, or unused import.
    missing_import: GoRootCauseBucket,
    /// Rewriter emitted Go that does not parse or type-check.
    rewrite_syntax: GoRootCauseBucket,
    /// Wrapper and target ended up in different `package` declarations.
    mixed_package: GoRootCauseBucket,
    /// Build-time unsupported parameter type (residual to the analyzer's
    /// pre-skip path).
    unsupported_param_type: GoRootCauseBucket,
    /// Constructor arity / receiver-plan mismatch (str-jeen.6).
    #[serde(default, skip_serializing_if = "GoRootCauseBucket::is_zero")]
    constructor_arity: GoRootCauseBucket,
    /// `go build` could not stamp VCS metadata (str-jeen.6). Fix is
    /// `-buildvcs=false` in the wrapper invocation.
    #[serde(default, skip_serializing_if = "GoRootCauseBucket::is_zero")]
    build_vcs_stamping: GoRootCauseBucket,
    /// `build_failed` reasons that didn't match any heuristic. Surfaces
    /// drift in frontend wording as a rising bucket instead of silently
    /// reweighting an existing category.
    other: GoRootCauseBucket,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct GoRootCauseBucket {
    count: u32,
    line_weight: u32,
}

impl GoRootCauseBucket {
    fn is_zero(&self) -> bool {
        self.count == 0 && self.line_weight == 0
    }
}

impl GoRootCauseBreakdown {
    fn record(&mut self, category: GoBuildFailureCategory, line_count: u32) {
        let bucket = match category {
            GoBuildFailureCategory::InternalPackage => &mut self.internal_package,
            GoBuildFailureCategory::MissingImport => &mut self.missing_import,
            GoBuildFailureCategory::RewriteSyntax => &mut self.rewrite_syntax,
            GoBuildFailureCategory::MixedPackage => &mut self.mixed_package,
            GoBuildFailureCategory::UnsupportedParamType => &mut self.unsupported_param_type,
            GoBuildFailureCategory::ConstructorArity => &mut self.constructor_arity,
            GoBuildFailureCategory::BuildVcsStamping => &mut self.build_vcs_stamping,
            GoBuildFailureCategory::Other => &mut self.other,
        };
        bucket.count = bucket.count.saturating_add(1);
        bucket.line_weight = bucket.line_weight.saturating_add(line_count);
    }

    fn merge(&mut self, other: &GoRootCauseBreakdown) {
        for (dst, src) in [
            (&mut self.internal_package, &other.internal_package),
            (&mut self.missing_import, &other.missing_import),
            (&mut self.rewrite_syntax, &other.rewrite_syntax),
            (&mut self.mixed_package, &other.mixed_package),
            (
                &mut self.unsupported_param_type,
                &other.unsupported_param_type,
            ),
            (&mut self.constructor_arity, &other.constructor_arity),
            (&mut self.build_vcs_stamping, &other.build_vcs_stamping),
            (&mut self.other, &other.other),
        ] {
            dst.count = dst.count.saturating_add(src.count);
            dst.line_weight = dst.line_weight.saturating_add(src.line_weight);
        }
    }

    fn is_empty(&self) -> bool {
        self.internal_package.count == 0
            && self.missing_import.count == 0
            && self.rewrite_syntax.count == 0
            && self.mixed_package.count == 0
            && self.unsupported_param_type.count == 0
            && self.constructor_arity.count == 0
            && self.build_vcs_stamping.count == 0
            && self.other.count == 0
    }

    /// Iterate categories in a stable display order. The non-`Other`
    /// categories come first so the markdown table reads down the
    /// well-known buckets before the catch-all.
    fn iter_buckets(&self) -> [(GoBuildFailureCategory, &GoRootCauseBucket); 8] {
        [
            (
                GoBuildFailureCategory::InternalPackage,
                &self.internal_package,
            ),
            (GoBuildFailureCategory::MissingImport, &self.missing_import),
            (GoBuildFailureCategory::RewriteSyntax, &self.rewrite_syntax),
            (GoBuildFailureCategory::MixedPackage, &self.mixed_package),
            (
                GoBuildFailureCategory::UnsupportedParamType,
                &self.unsupported_param_type,
            ),
            (
                GoBuildFailureCategory::ConstructorArity,
                &self.constructor_arity,
            ),
            (
                GoBuildFailureCategory::BuildVcsStamping,
                &self.build_vcs_stamping,
            ),
            (GoBuildFailureCategory::Other, &self.other),
        ]
    }
}

/// Aggregate Go `build_failed` outcomes across `entries` into a
/// per-category breakdown. Caller is responsible for filtering to entries
/// from Go targets (typically by file extension on the owning summary).
fn aggregate_go_root_causes_from_entries(entries: &[ExploreSummaryEntry]) -> GoRootCauseBreakdown {
    use shatter_core::protocol::OutcomeStatus;
    let mut breakdown = GoRootCauseBreakdown::default();
    for entry in entries {
        if outcome_status_from_entry(entry) != OutcomeStatus::BuildFailed {
            continue;
        }
        let reason = entry.reason.as_deref().unwrap_or("");
        let category = classify_go_build_failure(reason);
        breakdown.record(category, entry.line_count);
    }
    breakdown
}

/// Aggregate Go `build_failed` outcomes across all per-file summaries in a
/// broad run. Filters by `.go` file extension so a mixed-language run only
/// reports Go rows here.
fn aggregate_go_root_causes(summaries: &[ExploreSummary]) -> GoRootCauseBreakdown {
    let mut total = GoRootCauseBreakdown::default();
    for summary in summaries {
        if !summary.file.to_lowercase().ends_with(".go") {
            continue;
        }
        let per_file = aggregate_go_root_causes_from_entries(&summary.functions);
        total.merge(&per_file);
    }
    total
}

/// Render the Go root-cause breakdown as a markdown subsection. Returns
/// `None` when no Go `build_failed` outcomes were recorded so a clean
/// non-Go run does not get an empty Go header in its footer.
fn format_go_root_causes_md(breakdown: &GoRootCauseBreakdown) -> Option<String> {
    if breakdown.is_empty() {
        return None;
    }
    let mut out = String::from(
        "**Go build-failure root causes** (line-weighted)\n\n\
         | Category | Count | Lines |\n\
         | --- | ---: | ---: |\n",
    );
    for (category, bucket) in breakdown.iter_buckets() {
        if bucket.count == 0 {
            continue;
        }
        out.push_str(&format!(
            "| `{}` | {} | {} |\n",
            category.as_str(),
            bucket.count,
            bucket.line_weight,
        ));
    }
    Some(out)
}

// ---------------------------------------------------------------------------
// str-jeen.6: TypeScript root-cause classifier (parallel to Go above).
// ---------------------------------------------------------------------------

/// Coarse-grained category of a TypeScript-side `build_failed` /
/// `runtime_failed` reason text. Bucket boundaries match the issue body's
/// first-class TS root-cause groups so a maintainer can answer
/// "which TS root cause blocks the most code" from the markdown footer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TsBuildFailureCategory {
    /// Target name not exported from the module — Shatter discovered the
    /// symbol but the runtime harness could not import it (private helper,
    /// `export`-less function, or shaken-out tree node).
    PrivateFunctionNotFound,
    /// Module resolver failed: missing dependency, missing path alias, or
    /// `tsconfig` paths/baseUrl mismatch.
    MissingDependencyOrAlias,
    /// Parse / JSX / type-runtime failure: the harness or transpiler
    /// rejected the source before the function ran.
    ParseJsxOrTypeRuntime,
    /// Reference to a browser-only or DOM global that's absent in the
    /// Shatter executor sandbox (`window`, `document`, `localStorage`).
    MissingBrowserApi,
    /// Runtime refused a parameter type for which no value generator
    /// exists. Mirrors the Go `UnsupportedParamType` bucket — kept under
    /// the same name so cross-language rollups can collapse it cleanly.
    UnsupportedParamType,
    /// Reason text did not match any heuristic. Drift in frontend wording
    /// surfaces here rather than silently distorting an existing bucket.
    Other,
}

impl TsBuildFailureCategory {
    fn as_str(self) -> &'static str {
        match self {
            Self::PrivateFunctionNotFound => "private_function_not_found",
            Self::MissingDependencyOrAlias => "missing_dependency_or_alias",
            Self::ParseJsxOrTypeRuntime => "parse_jsx_or_type_runtime",
            Self::MissingBrowserApi => "missing_browser_api",
            Self::UnsupportedParamType => "unsupported_param_type",
            Self::Other => "other",
        }
    }
}

/// Classify a TS `build_failed` / `runtime_failed` reason into a
/// root-cause bucket. Heuristics match against the lowercased reason
/// text. Order matters: more specific patterns come first so a reason
/// mentioning both "cannot find" and "window" does not misclassify.
fn classify_ts_build_failure(reason: &str) -> TsBuildFailureCategory {
    let r = reason.to_lowercase();
    // Browser-only globals leak into the executor sandbox as
    // ReferenceError. Match first because the same trace can include
    // a "module not found" follow-on.
    if r.contains("window is not defined")
        || r.contains("document is not defined")
        || r.contains("localstorage is not defined")
        || r.contains("sessionstorage is not defined")
        || r.contains("navigator is not defined")
        || r.contains("xmlhttprequest is not defined")
        || r.contains("requestanimationframe is not defined")
    {
        return TsBuildFailureCategory::MissingBrowserApi;
    }
    // Private / non-exported target. Shatter's TS frontend reports a
    // distinct "is not exported" / "is not a function" trace when the
    // discovered symbol is module-private.
    if r.contains("is not exported")
        || r.contains("not exported from")
        || r.contains("export not found")
        || r.contains("private function not found")
        || (r.contains("is not a function") && r.contains("import"))
    {
        return TsBuildFailureCategory::PrivateFunctionNotFound;
    }
    // Module resolver / alias failures. Match before the generic parse
    // bucket because a "Cannot find module" message can also include
    // "SyntaxError" follow-ons from downstream loaders.
    if r.contains("cannot find module")
        || r.contains("module not found")
        || r.contains("cannot resolve module")
        || r.contains("err_module_not_found")
        || r.contains("path alias")
        || r.contains("tsconfig paths")
        || r.contains("baseurl")
    {
        return TsBuildFailureCategory::MissingDependencyOrAlias;
    }
    // Unsupported parameter shape — runtime-side rejection. Distinct
    // from the analyzer's pre-skip path which lands on
    // `OutcomeStatus::Unsupported`.
    if r.contains("unsupported parameter type")
        || r.contains("no value generator")
        || r.contains("cannot synthesize value for")
    {
        return TsBuildFailureCategory::UnsupportedParamType;
    }
    // Parse / JSX / TypeScript runtime errors. Catches both the loader's
    // SyntaxError and the type-checker's TSxxxx diagnostic prefix.
    if r.contains("syntaxerror")
        || r.contains("unexpected token")
        || r.contains("unterminated")
        || r.contains("jsx")
        || r.contains("expected ")
        || r.contains("ts1") // diagnostic codes TS1xxx (parse)
        || r.contains("ts2") // TS2xxx (semantic)
        || r.contains("parse error")
    {
        return TsBuildFailureCategory::ParseJsxOrTypeRuntime;
    }
    TsBuildFailureCategory::Other
}

/// Per-category breakdown of TS `build_failed` / `runtime_failed`
/// outcomes (str-jeen.6). Mirrors `GoRootCauseBreakdown` so a future
/// cross-language rollup can stitch the two without bespoke field
/// mapping.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct TsRootCauseBreakdown {
    #[serde(default, skip_serializing_if = "GoRootCauseBucket::is_zero")]
    private_function_not_found: GoRootCauseBucket,
    #[serde(default, skip_serializing_if = "GoRootCauseBucket::is_zero")]
    missing_dependency_or_alias: GoRootCauseBucket,
    #[serde(default, skip_serializing_if = "GoRootCauseBucket::is_zero")]
    parse_jsx_or_type_runtime: GoRootCauseBucket,
    #[serde(default, skip_serializing_if = "GoRootCauseBucket::is_zero")]
    missing_browser_api: GoRootCauseBucket,
    #[serde(default, skip_serializing_if = "GoRootCauseBucket::is_zero")]
    unsupported_param_type: GoRootCauseBucket,
    #[serde(default, skip_serializing_if = "GoRootCauseBucket::is_zero")]
    other: GoRootCauseBucket,
}

impl TsRootCauseBreakdown {
    fn record(&mut self, category: TsBuildFailureCategory, line_count: u32) {
        let bucket = match category {
            TsBuildFailureCategory::PrivateFunctionNotFound => &mut self.private_function_not_found,
            TsBuildFailureCategory::MissingDependencyOrAlias => {
                &mut self.missing_dependency_or_alias
            }
            TsBuildFailureCategory::ParseJsxOrTypeRuntime => &mut self.parse_jsx_or_type_runtime,
            TsBuildFailureCategory::MissingBrowserApi => &mut self.missing_browser_api,
            TsBuildFailureCategory::UnsupportedParamType => &mut self.unsupported_param_type,
            TsBuildFailureCategory::Other => &mut self.other,
        };
        bucket.count = bucket.count.saturating_add(1);
        bucket.line_weight = bucket.line_weight.saturating_add(line_count);
    }

    fn merge(&mut self, other: &TsRootCauseBreakdown) {
        for (dst, src) in [
            (
                &mut self.private_function_not_found,
                &other.private_function_not_found,
            ),
            (
                &mut self.missing_dependency_or_alias,
                &other.missing_dependency_or_alias,
            ),
            (
                &mut self.parse_jsx_or_type_runtime,
                &other.parse_jsx_or_type_runtime,
            ),
            (&mut self.missing_browser_api, &other.missing_browser_api),
            (
                &mut self.unsupported_param_type,
                &other.unsupported_param_type,
            ),
            (&mut self.other, &other.other),
        ] {
            dst.count = dst.count.saturating_add(src.count);
            dst.line_weight = dst.line_weight.saturating_add(src.line_weight);
        }
    }

    fn is_empty(&self) -> bool {
        self.private_function_not_found.count == 0
            && self.missing_dependency_or_alias.count == 0
            && self.parse_jsx_or_type_runtime.count == 0
            && self.missing_browser_api.count == 0
            && self.unsupported_param_type.count == 0
            && self.other.count == 0
    }

    fn iter_buckets(&self) -> [(TsBuildFailureCategory, &GoRootCauseBucket); 6] {
        [
            (
                TsBuildFailureCategory::PrivateFunctionNotFound,
                &self.private_function_not_found,
            ),
            (
                TsBuildFailureCategory::MissingDependencyOrAlias,
                &self.missing_dependency_or_alias,
            ),
            (
                TsBuildFailureCategory::ParseJsxOrTypeRuntime,
                &self.parse_jsx_or_type_runtime,
            ),
            (
                TsBuildFailureCategory::MissingBrowserApi,
                &self.missing_browser_api,
            ),
            (
                TsBuildFailureCategory::UnsupportedParamType,
                &self.unsupported_param_type,
            ),
            (TsBuildFailureCategory::Other, &self.other),
        ]
    }
}

/// Aggregate TS failure outcomes across `entries` into a per-category
/// breakdown. Includes both `BuildFailed` and `RuntimeFailed` entries —
/// TS module-load and JSX errors arrive on the runtime path on Node, but
/// represent the same maintainer-facing root causes as Go's build-time
/// errors. Caller is responsible for filtering to entries from TS
/// targets.
fn aggregate_ts_root_causes_from_entries(entries: &[ExploreSummaryEntry]) -> TsRootCauseBreakdown {
    use shatter_core::protocol::OutcomeStatus;
    let mut breakdown = TsRootCauseBreakdown::default();
    for entry in entries {
        let status = outcome_status_from_entry(entry);
        if !matches!(
            status,
            OutcomeStatus::BuildFailed | OutcomeStatus::RuntimeFailed
        ) {
            continue;
        }
        let reason = entry.reason.as_deref().unwrap_or("");
        let category = classify_ts_build_failure(reason);
        breakdown.record(category, entry.line_count);
    }
    breakdown
}

/// Aggregate TS failure outcomes across all per-file summaries in a
/// broad run. Filters by `.ts` / `.tsx` / `.js` / `.jsx` file extension
/// so a mixed-language run only reports TS rows here.
fn aggregate_ts_root_causes(summaries: &[ExploreSummary]) -> TsRootCauseBreakdown {
    let mut total = TsRootCauseBreakdown::default();
    for summary in summaries {
        let lower = summary.file.to_lowercase();
        let is_ts = lower.ends_with(".ts")
            || lower.ends_with(".tsx")
            || lower.ends_with(".js")
            || lower.ends_with(".jsx")
            || lower.ends_with(".mjs")
            || lower.ends_with(".cjs");
        if !is_ts {
            continue;
        }
        let per_file = aggregate_ts_root_causes_from_entries(&summary.functions);
        total.merge(&per_file);
    }
    total
}

/// Render the TS root-cause breakdown as a markdown subsection. Returns
/// `None` when no TS failures were recorded so a Go-only run does not
/// get an empty TS header.
fn format_ts_root_causes_md(breakdown: &TsRootCauseBreakdown) -> Option<String> {
    if breakdown.is_empty() {
        return None;
    }
    let mut out = String::from(
        "**TypeScript failure root causes** (line-weighted)\n\n\
         | Category | Count | Lines |\n\
         | --- | ---: | ---: |\n",
    );
    for (category, bucket) in breakdown.iter_buckets() {
        if bucket.count == 0 {
            continue;
        }
        out.push_str(&format!(
            "| `{}` | {} | {} |\n",
            category.as_str(),
            bucket.count,
            bucket.line_weight,
        ));
    }
    Some(out)
}

// ---------------------------------------------------------------------------
// str-jeen.6: cross-language failure-impact rollup. Answers "which root
// cause blocks the most code", with affected-file and affected-function
// counts alongside the line-weighted span impact and a denominator
// percentage against attempted span lines.
// ---------------------------------------------------------------------------

/// One row of the run-wide failure-impact rollup. Serialized into the
/// run-level markdown footer and (when the per-file
/// `summary.json` is rolled up) into the run-level JSON.
///
/// The three line counters answer different questions:
/// * `affected_function_span_lines` — sum of `line_count` across the
///   functions that hit this root cause. The "blast radius" of the
///   failure inside discovered function bodies.
/// * `selected_source_lines` — sum of `discovered_function_span_lines`
///   across the distinct files that contain at least one
///   affected-by-this-cause function. Tracks "how much of the
///   selected-by-shatter code lived in the same files." A high ratio
///   here means the failure concentrates in files where most of the
///   selected code is at risk; a low ratio means the failure landed in
///   files that also contain a lot of unaffected code.
/// * `percent_attempted_span_lines` — `affected_function_span_lines`
///   divided by the run's total `attempted_function_span_lines` (the
///   str-jeen.41 contract). The headline ratio: "what fraction of code
///   we even tried to explore is blocked by this cause".
#[derive(Debug, Clone, PartialEq, Serialize)]
struct FailureImpactRow {
    /// Stable wire token (e.g. `internal_package`,
    /// `private_function_not_found`, or the generic outcome buckets
    /// `build_failed` / `runtime_failed` / `timed_out` / `unsupported`).
    category: &'static str,
    /// Originating language scope: `go`, `ts`, or `any` for outcome-only
    /// rollups that don't pin a language.
    language: &'static str,
    /// Number of affected functions == affected_functions. Kept as a
    /// distinct field so downstream automation does not have to know
    /// the equality.
    count: u32,
    /// Distinct files containing at least one affected function.
    affected_files: u32,
    /// Affected function count. Equal to `count`; explicit per the
    /// str-jeen.6 acceptance criteria.
    affected_functions: u32,
    /// Sum of `line_count` across affected functions.
    affected_function_span_lines: u32,
    /// Sum of `discovered_function_span_lines` across distinct affected
    /// files. Computed from the str-jeen.18 per-file counter, not from
    /// the path-only str-jeen.37 source-set classifier — see struct
    /// docstring for the rationale.
    selected_source_lines: u32,
    /// `affected_function_span_lines * 100 /
    /// attempted_function_span_lines`, clamped to `[0.0, 100.0]`. Zero
    /// when the run attempted no span lines.
    percent_attempted_span_lines: f64,
}

/// Compute the run-wide failure-impact rollup.
///
/// Iterates every entry across every per-file summary, classifies each
/// failure into a root cause (per-language Go / TS classifiers, plus
/// generic outcome-only rows), and produces one row per non-empty
/// category sorted by `affected_function_span_lines` descending so the
/// markdown footer reads top-down by blast radius. Ties break on
/// (language, category) for deterministic output.
fn aggregate_failure_impact(summaries: &[ExploreSummary]) -> Vec<FailureImpactRow> {
    use shatter_core::protocol::OutcomeStatus;

    // Total attempted span lines across the run, used as the denominator
    // for `percent_attempted_span_lines`. Pull from the persisted field
    // rather than recomputing so the rollup matches the str-jeen.41
    // contract surfaced in the per-file JSON.
    let total_attempted: u32 = summaries
        .iter()
        .map(|s| s.attempted_function_span_lines)
        .sum();

    // (language, category) -> (affected_functions, span_lines, set of file paths)
    let mut acc: std::collections::BTreeMap<
        (&'static str, &'static str),
        (u32, u32, std::collections::BTreeSet<String>),
    > = std::collections::BTreeMap::new();
    // file -> selected source lines (discovered_function_span_lines).
    let mut file_selected: HashMap<&str, u32> = HashMap::new();
    for summary in summaries {
        file_selected.insert(
            summary.file.as_str(),
            summary.discovered_function_span_lines,
        );
    }

    for summary in summaries {
        let lower = summary.file.to_lowercase();
        let is_go = lower.ends_with(".go");
        let is_ts = lower.ends_with(".ts")
            || lower.ends_with(".tsx")
            || lower.ends_with(".js")
            || lower.ends_with(".jsx")
            || lower.ends_with(".mjs")
            || lower.ends_with(".cjs");
        for entry in &summary.functions {
            let status = outcome_status_from_entry(entry);
            let reason = entry.reason.as_deref().unwrap_or("");

            // Per-language root-cause row (Go BuildFailed / TS Build|Runtime).
            let lang_row: Option<(&'static str, &'static str)> = match (status, is_go, is_ts) {
                (OutcomeStatus::BuildFailed, true, _) => {
                    Some(("go", classify_go_build_failure(reason).as_str()))
                }
                (OutcomeStatus::BuildFailed, _, true) | (OutcomeStatus::RuntimeFailed, _, true) => {
                    Some(("ts", classify_ts_build_failure(reason).as_str()))
                }
                _ => None,
            };
            if let Some(key) = lang_row {
                let slot = acc.entry(key).or_default();
                slot.0 = slot.0.saturating_add(1);
                slot.1 = slot.1.saturating_add(entry.line_count);
                slot.2.insert(summary.file.clone());
            }

            // Outcome-only "any"-language row so the rollup also reflects
            // failures from frontends without a dedicated classifier
            // (e.g. Rust runtime failures or Go runtime failures that
            // don't have a Go-specific bucket).
            let outcome_token: Option<&'static str> = match status {
                OutcomeStatus::BuildFailed => Some("build_failed"),
                OutcomeStatus::RuntimeFailed => Some("runtime_failed"),
                OutcomeStatus::TimedOut => Some("timed_out"),
                OutcomeStatus::Unsupported => Some("unsupported"),
                _ => None,
            };
            if let Some(tok) = outcome_token {
                let slot = acc.entry(("any", tok)).or_default();
                slot.0 = slot.0.saturating_add(1);
                slot.1 = slot.1.saturating_add(entry.line_count);
                slot.2.insert(summary.file.clone());
            }
        }
    }

    let mut rows: Vec<FailureImpactRow> = acc
        .into_iter()
        .map(|((language, category), (count, span_lines, files))| {
            let selected: u32 = files
                .iter()
                .map(|f| file_selected.get(f.as_str()).copied().unwrap_or(0))
                .sum();
            let percent = if total_attempted > 0 {
                ((span_lines as f64 / total_attempted as f64) * 100.0).clamp(0.0, 100.0)
            } else {
                0.0
            };
            FailureImpactRow {
                category,
                language,
                count,
                affected_files: files.len() as u32,
                affected_functions: count,
                affected_function_span_lines: span_lines,
                selected_source_lines: selected,
                percent_attempted_span_lines: percent,
            }
        })
        .collect();
    // Sort by span_lines descending, then by (language, category) for a
    // deterministic tiebreak.
    rows.sort_by(|a, b| {
        b.affected_function_span_lines
            .cmp(&a.affected_function_span_lines)
            .then(a.language.cmp(b.language))
            .then(a.category.cmp(b.category))
    });
    rows
}

/// Render the failure-impact rollup as a markdown subsection. Returns
/// `None` when no rows were produced so a clean run stays clean.
fn format_failure_impact_md(rows: &[FailureImpactRow]) -> Option<String> {
    if rows.is_empty() {
        return None;
    }
    let mut out = String::from(
        "**Failure impact** (line-weighted, sorted by span lines)\n\n\
         | Category | Lang | Functions | Files | Span lines | Selected lines | % attempted |\n\
         | --- | --- | ---: | ---: | ---: | ---: | ---: |\n",
    );
    for row in rows {
        out.push_str(&format!(
            "| `{}` | `{}` | {} | {} | {} | {} | {:.1}% |\n",
            row.category,
            row.language,
            row.affected_functions,
            row.affected_files,
            row.affected_function_span_lines,
            row.selected_source_lines,
            row.percent_attempted_span_lines,
        ));
    }
    Some(out)
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

    // str-jeen.21: when the run produced zero targets across every file,
    // surface the per-file `no_target_reason` taxonomy as a markdown
    // table column. Files that did produce targets carry `None` and
    // contribute nothing to the table; only the no-target rows render.
    let no_target_rows: Vec<(&str, shatter_core::protocol::NoTargetReason)> = summaries
        .iter()
        .filter_map(|s| s.no_target_reason.map(|r| (s.file.as_str(), r)))
        .collect();
    let empty_reason = if no_target_rows.is_empty() {
        "discovery returned no functions for this run".to_string()
    } else {
        format_no_target_reason_table(
            "discovery returned no functions for this run",
            &no_target_rows,
        )
    };

    shatter_core::report::render_explore_outcomes(&entries, &empty_reason)
}

/// Render the per-file no-target-reason table appended to the
/// "## No targets discovered" markdown section (str-jeen.21).
///
/// Two-column markdown table: file path and the snake_case
/// `NoTargetReason` token. One row per zero-target file in input order;
/// callers must pre-filter to only files that actually produced no
/// targets, since the table is rendered unconditionally when any rows
/// are present.
fn format_no_target_reason_table(
    intro: &str,
    rows: &[(&str, shatter_core::protocol::NoTargetReason)],
) -> String {
    let mut out = String::new();
    out.push_str(intro);
    out.push_str("\n\n| File | Reason |\n|---|---|\n");
    for (file, reason) in rows {
        out.push_str(&format!("| {file} | `{}` |\n", reason.as_token()));
    }
    out
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
    // str-6c6p: human-readable progress lines are info-level chatter, not
    // command result. `--quiet` lowers log level below Info to silence
    // exactly this kind of noise — honor it here even though we bypass
    // the `log` crate's macros (the `eprintln!` predates the log routing).
    // The structured JSON event below is preserved unconditionally when
    // `emit_json` is set: machine-consumed progress streams are an opt-in
    // contract that should not be silenced by a verbosity knob.
    if log::log_enabled!(log::Level::Info) {
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
    }

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
    // str-6c6p: the per-function report is the user-requested command result
    // when stdout is the sink (no -o files, or `--stdout`). It must NOT be
    // gated on the info log level — `--quiet` suppresses progress/info
    // logging but must still emit the requested result.
    let should_print_report = opts.report_outputs_empty || opts.stdout;
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

    // str-jeen.4: validate the artifact-reference contract against the loaded
    // summaries. Issues are logged at warn level so a finalize against a
    // partially-corrupt directory still produces a report; the integration
    // test asserts the report is clean for healthy runs.
    let validation = validate_artifact_references(artifact_dir, &summaries);
    if !validation.is_clean() {
        log::warn!(
            "artifact-reference validation surfaced {} issue(s) in {}:",
            validation.issues.len(),
            artifact_dir.display()
        );
        for issue in &validation.issues {
            log::warn!("  {issue}");
        }
    }

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
    // str-6c6p: header is part of the requested report output. Do not gate
    // on info log level — quiet suppresses info/progress logs, not results.
    {
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
    // str-6c6p: footer is part of the requested report output. Do not gate
    // on info log level — quiet suppresses info/progress logs, not results.
    if report_outputs.is_empty() || stdout {
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
                        ..FileSpecBundle::default()
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
    if let Some(out) = output_path {
        let file_path = artifacts
            .first()
            .map(|a| a.file.clone())
            .unwrap_or_default();
        let bundle = if acc.file_specs.is_empty() {
            // str-jeen.67: emit a machine-readable no-target marker bundle so
            // batch tooling can distinguish "file analyzed, nothing to
            // explore" from "spec output missing / pipeline crashed".
            build_no_target_spec_bundle(&file_path, &summaries)
        } else {
            FileSpecBundle {
                file: file_path,
                functions: acc.file_specs,
                ..FileSpecBundle::default()
            }
        };
        shatter_core::spec::write_file_spec_bundle(&bundle, out)
            .map_err(|e| format!("failed to write spec bundle to {}: {e}", out.display()))?;
        if matches!(bundle.status, Some(shatter_core::spec::FileSpecBundleStatus::NoTargets)) {
            log::info!(
                "Wrote no-target spec marker (reason={}) to {}",
                bundle
                    .no_target_reason
                    .map(|r| r.as_token())
                    .unwrap_or("unclassified"),
                out.display()
            );
        } else {
            log::info!("Wrote spec bundle to {}", out.display());
        }
    }

    // str-960w: surface a nonzero exit when every attempted target failed,
    // even though reports/specs were written successfully above.
    decide_explore_exit_status(&summaries)?;
    Ok(())
}

/// Build a no-target spec bundle from collected per-file summaries (str-jeen.67).
///
/// Picks the file path and `no_target_reason` from the first summary the
/// finalize/run path collected. Falls back to `Unclassified` when no
/// summary carries a refined reason — same default as `ExploreSummary`
/// (str-jeen.21) so producers and consumers agree on the wire format.
fn build_no_target_spec_bundle(
    file_path: &str,
    summaries: &[ExploreSummary],
) -> FileSpecBundle {
    let reason = summaries
        .iter()
        .find_map(|s| s.no_target_reason)
        .unwrap_or(shatter_core::protocol::NoTargetReason::Unclassified);
    let bundle_file = if file_path.is_empty() {
        summaries
            .first()
            .map(|s| s.file.clone())
            .unwrap_or_default()
    } else {
        file_path.to_string()
    };
    FileSpecBundle {
        file: bundle_file,
        functions: Vec::new(),
        status: Some(shatter_core::spec::FileSpecBundleStatus::NoTargets),
        no_target_reason: Some(reason),
    }
}

/// Run the explore command.
// Each argument corresponds to a CLI flag; grouping into a struct would add indirection
// without improving clarity since this is only called from one callsite.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_explore(
    targets: &[String],
    max_iterations: Option<u32>,
    user_max_iterations: Option<u32>,
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
    planner: Option<&str>,
    parallelism_bounds: crate::helpers::ParallelismBounds,
    require_rust: bool,
    observer_pool: usize,
    candidate_queue_capacity: Option<usize>,
    external_audit_mode: bool,
    oracle_bundle: Option<shatter_core::oracle::OracleBundle>,
) -> Result<(), Box<dyn std::error::Error>> {
    // str-k9y5: clean external-audit runs (`--output <external> --no-cache
    // --no-seeds`) reroute harness storage (cache/scratch/artifacts and the
    // Go workspace root) under the OS temp dir so the CLI process, the
    // spawned frontend subprocesses, and the explore-results writer all
    // share the same out-of-project base. Mirrors scan's
    // `apply_external_audit_storage` but applied at the CLI-process level
    // so `explore_artifact_root` (CLI-side) also picks it up via the
    // `SHATTER_ARTIFACT_DIR` env var.
    let _audit_session: Option<std::path::PathBuf> = if external_audit_mode {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let session_id = format!(
            "shatter-audit-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed),
        );
        let base = std::env::temp_dir().join(&session_id);
        let cache_root = base.join("harness");
        let scratch_root = base.join("scratch");
        let artifact_root = base.join("artifacts");
        let go_workspace_root = base.join("go-workspace");
        let storage = shatter_core::harness_storage::HarnessStorage::new(
            cache_root,
            scratch_root,
            artifact_root,
        );
        // Safety: CLI is single-threaded at this point (before spawning
        // any frontend or tokio worker task). Mirrors the SHATTER_SETUP_TIMEOUT
        // pattern in main.rs.
        unsafe {
            for (k, v) in storage.env_vars() {
                std::env::set_var(k, v);
            }
            std::env::set_var(
                "SHATTER_GO_WORKSPACE_ROOT",
                go_workspace_root.to_string_lossy().as_ref(),
            );
        }
        log::debug!(
            "explore: external audit mode active (-o + --no-cache + --no-seeds); \
             redirecting harness storage to {}",
            base.display(),
        );
        Some(base)
    } else {
        None
    };
    if let Some(name) = planner
        && name != "go"
    {
        return Err(format!("--planner={name}: only `go` is currently supported.").into());
    }

    // str-mpwp / str-tzbr: `--format json` is no longer accepted by clap for
    // explore (see `ExploreStdoutFormat`), so a stdout-JSON request can never
    // reach this path. JSON output is available via `-o <file>.json`, which
    // writes a spec bundle from the live or finalize-from-artifacts path.

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

    let expanded_targets = expand_target_args(targets)?;
    let mut parsed: Vec<Target> = expanded_targets
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

    // Resolve effective worker count: 0 means auto-detect, otherwise honor the
    // user value. Both paths are clamped into `[bounds.floor, bounds.ceiling]`
    // — built-in defaults from str-eam2, override-aware via str-v01r.
    let effective_workers = resolve_parallelism_with_bounds(workers, parallelism_bounds);

    // Resolve project root once for harness storage env propagation.
    // Explicit --project-dir wins; otherwise auto-detect from the first target.
    let storage_project_root = resolve_project_root(project_dir, &parsed[0].file);

    // Build per-language FrontendConfig templates for spawning per-function explore
    // frontends.  Also spawn one shared frontend per language for the analysis phase
    // (analysis is fast and doesn't benefit from parallelism).
    let mut frontends: HashMap<crate::args::Language, Frontend> = HashMap::new();
    let mut fe_configs: HashMap<crate::args::Language, FrontendConfig> = HashMap::new();
    let mut unique_langs: HashSet<crate::args::Language> =
        parsed.iter().map(|t| t.language).collect();

    // str-bnsw / str-jeen.13: precheck frontend availability for every
    // requested language BEFORE walking targets / spawning processes.
    //
    // Default policy (str-jeen.13): unavailable language frontends are NOT
    // treated as hard target failures when other targets remain runnable.
    // For each unavailable language we emit one structured
    // `skipped_by_unavailable_frontend` STATUS line per skipped target so
    // broad-run wrappers (e.g. Kapow re-runs) can classify the row as
    // environmental rather than as a generic spawn failure, then drop those
    // targets from the run and proceed with the rest.
    //
    // We still hard-fail when:
    //   - every requested target lives in an unavailable language (nothing to
    //     run), or
    //   - the user explicitly demanded the language with `--require-rust`.
    let mut unavailable_langs: HashMap<crate::args::Language, &'static str> = HashMap::new();
    for lang in &unique_langs {
        let availability = crate::helpers::check_frontend_availability(*lang, None);
        if let crate::helpers::FrontendAvailability::Unavailable { install_hint, .. } = availability
        {
            unavailable_langs.insert(*lang, install_hint);
        }
    }
    if !unavailable_langs.is_empty() {
        let total_targets = parsed.len();
        let unavailable_target_count = parsed
            .iter()
            .filter(|t| unavailable_langs.contains_key(&t.language))
            .count();
        let all_unavailable = unavailable_target_count == total_targets;
        let require_rust_violated =
            require_rust && unavailable_langs.contains_key(&crate::args::Language::Rust);

        if all_unavailable || require_rust_violated {
            // Hard failure: nothing else to do, or user demanded the language.
            // Emit per-target status lines first so wrappers still see the
            // structured classification before exit.
            for t in parsed
                .iter()
                .filter(|t| unavailable_langs.contains_key(&t.language))
            {
                let hint = unavailable_langs[&t.language];
                crate::helpers::emit_skipped_unavailable_frontend(&t.file, t.language, hint);
            }
            let detail: Vec<String> = unavailable_langs
                .iter()
                .map(|(lang, hint)| {
                    let count = parsed.iter().filter(|t| t.language == *lang).count();
                    format!(
                        "{} frontend unavailable for {} target(s): shatter-{} frontend not found: {}",
                        lang.label(),
                        count,
                        lang.label(),
                        hint
                    )
                })
                .collect();
            let prefix = if require_rust_violated {
                "rust frontend unavailable and --require-rust is set"
            } else {
                "no available frontends for requested targets"
            };
            return Err(format!("{prefix}: {}", detail.join("; ")).into());
        }

        // Mixed run: warn, emit structured status per skipped target, drop
        // them from the run, continue with the available subset.
        for (lang, hint) in &unavailable_langs {
            let skipped: Vec<&Target> = parsed.iter().filter(|t| t.language == *lang).collect();
            log::warn!(
                "skipping {} {} target(s): shatter-{} frontend not found: {} \
                 (run will continue with available languages; \
                 pass --require-rust to fail instead)",
                skipped.len(),
                lang.label(),
                lang.label(),
                hint,
            );
            for t in skipped {
                crate::helpers::emit_skipped_unavailable_frontend(&t.file, *lang, hint);
            }
        }
        parsed.retain(|t| !unavailable_langs.contains_key(&t.language));
        unique_langs = parsed.iter().map(|t| t.language).collect();
    }

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

        // str-jeen.25: frontend-agnostic no-target precheck. Tag files
        // matched by user-config policy or the generated-schema heuristic
        // before spending an analyze round-trip on them. The post-analyze
        // `parser_failure` arm below tags files whose Analyze fails.
        let project_root_path = project_root_str.as_deref().map(Path::new);
        let project_cfg_for_target = project_root_path.and_then(|root| {
            shatter_core::config::load_project_config(root)
                .ok()
                .flatten()
        });
        if let Some(reason) = pre_classify_no_target_reason(
            &target.file,
            project_root_path,
            project_cfg_for_target.as_ref(),
        ) {
            let artifact_root = explore_artifact_root(project_root_str.as_deref());
            let summary = build_skip_summary(&file_str, reason);
            if let Err(e) = write_explore_summary(&artifact_root, &file_str, &summary) {
                log::warn!("Failed to write skip summary for {file_str}: {e}");
            }
            report_summaries.push(summary);
            log::info!(
                "Skipping {file_str}: {} (frontend-agnostic precheck)",
                reason.as_token()
            );
            continue;
        }

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
                // str-jeen.25: surface parser/analyze failures in the
                // per-file run record instead of dropping them silently.
                // Emits a stub ExploreSummary tagged `parser_failure` so
                // broad-run reports can classify the row.
                log::error!("Analyze error ({code:?}): {message}");
                let artifact_root = explore_artifact_root(project_root_str.as_deref());
                let mut summary = build_skip_summary(
                    &file_str,
                    shatter_core::protocol::NoTargetReason::ParserFailure,
                );
                summary.status = format!("parser_failure: {code:?}");
                if let Err(e) = write_explore_summary(&artifact_root, &file_str, &summary) {
                    log::warn!("Failed to write parser-failure summary for {file_str}: {e}");
                }
                report_summaries.push(summary);
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
        // str-6c6p: header is part of the report output, not an info log;
        // emit regardless of log level so `--quiet` still prints the report.
        if !analyze_only && !header_printed {
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
        let mut skipped_unexecutable: Vec<(String, u32, Vec<executability::SkipReason>)> =
            Vec::new();

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

            // Resolve per-function config. Passing `user_max_iterations`
            // (only Some(_) when the user set --max-iterations explicitly)
            // ensures CLI flags beat `defaults.max_iterations` in
            // `.shatter/config.yaml` (str-4uem). The post-budget
            // `max_iterations` is used below as the fallback when neither
            // CLI nor config supplied a value.
            let mut resolved = shatter_config::resolve_function_config_with_inputs(
                &function_id,
                target.file.parent().unwrap_or(Path::new(".")),
                inputs_path,
                user_max_iterations,
                timeout,
                set_overrides,
            )
            .map_err(|e| format!("config resolution error for {}: {e}", func.name))?;
            if resolved.max_iterations.is_none() {
                resolved.max_iterations = max_iterations;
            }

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
            // str-n10u: adapter-owned functions bypass this — the adapter provides
            // synthetic params that are always executable.
            if matches!(func.invocation_model, shatter_core::protocol::InvocationModel::Direct) {
                let skip_reasons = executability::check_executability(&func.params, &[]);
                if !skip_reasons.is_empty() {
                    log::debug!("Skipping {} (unexecutable parameter types)", func.name);
                    // str-jeen.18: capture the function span so the discovered
                    // span-line denominator includes pre-skipped functions, not
                    // just attempted ones.
                    let span_lines = func
                        .end_line
                        .saturating_sub(func.start_line)
                        .saturating_add(1);
                    skipped_unexecutable.push((func.name.clone(), span_lines, skip_reasons));
                    continue;
                }
            }

            // str-n10u: skip partial adapter helpers — medium-confidence adapter
            // hints (e.g. only http.ResponseWriter) without a full adapter model.
            if matches!(func.invocation_model, shatter_core::protocol::InvocationModel::Direct) {
                let partial_adapter = func.adapter_hints.iter().any(|h| {
                    h.confidence == shatter_core::nondeterminism::Confidence::Medium
                });
                if partial_adapter {
                    log::debug!(
                        "Skipping {} (partial adapter signature, not an exact handler)",
                        func.name,
                    );
                    let span_lines = func
                        .end_line
                        .saturating_sub(func.start_line)
                        .saturating_add(1);
                    skipped_unexecutable.push((func.name.clone(), span_lines, vec![]));
                    continue;
                }
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

            // str-frc.6: surface the observer-pool/queue-capacity knobs.
            // observer_frontend_config is populated from the per-language
            // FrontendConfig template so each pool slot can spawn its own
            // independent subprocess. Pool size <= 1 keeps the legacy single
            // -process path (default-preserving).
            let resolved_observer_pool = observer_pool.max(1);
            let observer_frontend_config = if resolved_observer_pool > 1 {
                fe_configs.get(&target.language).cloned()
            } else {
                None
            };
            let explore_config = ExploreConfig {
                file: file_str.to_string(),
                max_iterations: resolved.max_iterations,
                observer_pool: resolved_observer_pool,
                observer_frontend_config,
                candidate_queue_capacity,
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
                planner: planner.map(str::to_string),
                default_execute_plan: None,
            prepare_id_override: None,
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
                    // str-nqrz: `--max-iterations N` is a hard cap on total
                    // executions; the concolic orchestrator's `max_iterations`
                    // bounds unique-path discovery, while `max_executions`
                    // bounds total execute calls. Setting both to the same
                    // value keeps the user-visible iteration cap honored.
                    max_executions: explore_config.max_iterations.map(|n| n as usize),
                    plateau_threshold: if mcdc { 60 } else { 20 },
                    mocks: explore_config.mocks.clone(),
                    mock_params: explore_config.mock_params.clone(),
                    solver_timeout_ms: solver_timeout.map(|s| s * 1000),
                    seed: None,
                    solver_offload: true,
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
                    planner: planner.map(str::to_string),
                    default_execute_plan: None,
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
        // str-060a: `--clean` forces fresh exploration. Discard prior artifacts
        // for this target before resume detection so neither the summary nor
        // any per-function artifact / resume-state sidecar can be reused.
        if clean {
            clean_target_artifacts(&artifact_root, &file_str);
        }
        let prior_summary = if clean {
            None
        } else {
            read_explore_summary(&artifact_root, &file_str)
        };
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
            // str-060a: `--clean` already removed the target artifact directory
            // above, but keep the explicit gate so future code paths that add
            // new resume sources can't silently bypass `--clean`.
            if !clean && let Some(state) = read_resume_state(&artifact_root, &file_str, &item.func)
            {
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
        // str-oo31: pre-skipped (unexecutable) functions go straight into the
        // `unsupported` bucket. The legacy `skipped` counter and per-status
        // bucket move together to keep the invariant
        // `skipped == unsupported + skipped_by_policy` true.
        let pre_skipped = skipped_unexecutable.len();
        let attempted = work_items.len() - first_work_index;
        let mut explore_summary = ExploreSummary {
            version: EXPLORE_ARTIFACT_VERSION,
            status: "running".to_string(),
            file: file_str.to_string(),
            total_functions: attempted,
            completed: 0,
            failed: 0,
            skipped: pre_skipped,
            elapsed_secs: 0.0,
            build_failed: 0,
            runtime_failed: 0,
            timed_out: 0,
            unsupported: pre_skipped,
            skipped_by_policy: 0,
            preflight_failed: 0,
            produced_coverage: 0,
            no_target_reason: classify_no_target_reason(
                attempted,
                pre_skipped,
                target.language,
                &target.file,
            ),
            go_root_causes: None,
            ts_root_causes: None,
            functions: skipped_unexecutable
                .iter()
                .map(|(name, span_lines, _)| {
                    ExploreSummaryEntry::unavailable(
                        name.clone(),
                        "skipped".to_string(),
                        UnavailableReason::Unsupported,
                        Some("unexecutable parameter types".to_string()),
                        None,
                    )
                    .with_line_count(*span_lines)
                })
                .collect(),
            // str-jeen.18: filled in at finalization once attempted outcomes
            // have landed in `functions`. Pre-skipped entries already carry
            // their span via `with_line_count` above so the eventual
            // `discovered_function_span_lines` reflects every function we
            // saw, not just every function we scheduled.
            discovered_function_span_lines: 0,
            attempted_function_span_lines: 0,
            completed_function_span_lines: 0,
            // str-jeen.41: filled in alongside the .18 denominators. Pre-skipped
            // entries seeded above carry their `line_count` into the
            // `unsupported_function_lines` bucket on the next recompute.
            covered_completed_lines: 0,
            uncovered_completed_lines: 0,
            build_failed_function_lines: 0,
            runtime_failed_function_lines: 0,
            timed_out_function_lines: 0,
            unsupported_function_lines: 0,
        };
        // str-jeen.18: seed the discovered denominator with pre-skipped
        // entries' spans so the crash-recovery JSON written before any
        // outcomes land already reports a non-zero "discovered" total.
        // Finalization recomputes from the full entry list and overwrites.
        let initial_spans = span_line_denominators_from_entries(&explore_summary.functions);
        explore_summary.discovered_function_span_lines = initial_spans.discovered;
        explore_summary.attempted_function_span_lines = initial_spans.attempted;
        explore_summary.completed_function_span_lines = initial_spans.completed;
        // str-jeen.41: seed the per-outcome line buckets so the crash-recovery
        // JSON written before any outcomes land already reports pre-skipped
        // entries in `unsupported_function_lines`.
        explore_summary.covered_completed_lines = initial_spans.covered_completed;
        explore_summary.uncovered_completed_lines = initial_spans.uncovered_completed;
        explore_summary.build_failed_function_lines = initial_spans.build_failed;
        explore_summary.runtime_failed_function_lines = initial_spans.runtime_failed;
        explore_summary.timed_out_function_lines = initial_spans.timed_out;
        explore_summary.unsupported_function_lines = initial_spans.unsupported;
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
                // str-nqrz: keep the per-batch execution cap aligned with
                // the user-visible iteration cap. Granting 5x headroom let
                // single batches blow past `--max-iterations`.
                cc.max_executions = Some(batch_iters as usize);
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
            let task_oracle_bundle = oracle_bundle.clone();

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

                // --- Invocation planner consultation ---
                //
                // When `--planner` is set, consult the frontend's planner
                // (str-hy9b.H2). The resulting seeds feed BOTH the random
                // explorer (via user_seeds) and the concolic orchestrator
                // (via build_seed_inputs_with_extras) through the shared
                // `extra_seeds` channel on ObserveStageOptions, preserving
                // the single-source-of-truth rule for parallel explorer and
                // orchestrator paths.
                let (planner_extra_seeds, planner_default_plan) = fetch_planner_extra_seeds(
                    &mut task_frontend,
                    &item.explore_config,
                    &item.func,
                    &file_str_owned,
                    project_root_owned.as_deref(),
                )
                .await;

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
                        extra_seeds: &planner_extra_seeds,
                        execute_plan: planner_default_plan,
                        oracle_bundle: task_oracle_bundle,
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
                // str-gz8j: keep the live progress line consistent with the
                // persisted summary status. A timed-out function is reported
                // as "failed" so users see the timeout in the streaming log,
                // not a misleading "completed".
                let progress_status = match &result {
                    Ok(obs) if obs.timed_out => "failed",
                    Ok(_) => "completed",
                    Err(_) => "failed",
                };
                emit_explore_progress(
                    &item.func.name,
                    completed,
                    progress_total,
                    func_start.elapsed(),
                    progress_status,
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

            // str-gz8j: route through classify_outcome_status so an Ok result
            // whose ObservationOutput.timed_out is set lands as "failed" with
            // an explicit per-function-budget reason (and downstream into the
            // timed_out bucket via outcome_status_from_entry's reason match)
            // instead of silently looking like a Completed run.
            let (summary_status, summary_reason) =
                classify_outcome_status(&outcome.result, outcome.wall_time);
            if summary_status == "completed" {
                explore_summary.completed += 1;
            } else {
                explore_summary.failed += 1;
            }
            // str-oo31: also bump the precise per-OutcomeStatus bucket and
            // the produced-coverage denominator. The bucket assignment must
            // match `outcome_status_from_entry`, so we route through it
            // rather than re-deriving here.
            // str-jeen.4: route construction through the typed helpers so the
            // artifact-reference contract (Some(path) ⇒ file exists; None ⇒
            // typed UnavailableReason) is enforced at the construction site.
            // When `artifact_relpath` is None, the artifact JSON itself failed
            // to land on disk; classify the outcome's logical failure mode
            // (build / runtime / timeout) so the row's `reason` carries both
            // the persistence failure and the underlying cause.
            let entry_fingerprint = deep_fingerprints.get(&outcome.func.name).cloned();
            // str-jeen.31: capture function span so the Go broad-run
            // root-cause aggregator can line-weight build_failed outcomes.
            // `end_line >= start_line` is the analyzer's contract; saturating
            // arithmetic guards a malformed frontend response.
            let entry_line_count = outcome
                .func
                .end_line
                .saturating_sub(outcome.func.start_line)
                .saturating_add(1);
            // str-jeen.41: capture executor-reported covered lines for
            // completed outcomes only. Pre-skipped, build / runtime /
            // timeout outcomes contribute their full `line_count` to their
            // outcome bucket rather than splitting into covered/uncovered.
            // `lines_covered` is `usize`; saturate at `u32::MAX` and let
            // `with_covered_lines` cap at `entry_line_count`.
            let entry_covered_lines: u32 = match (&outcome.result, summary_status) {
                (Ok(obs), "completed") => u32::try_from(obs.lines_covered).unwrap_or(u32::MAX),
                _ => 0,
            };
            let bucket_entry = match artifact_relpath.clone() {
                Some(path) => ExploreSummaryEntry::available(
                    outcome.func.name.clone(),
                    summary_status.to_string(),
                    path,
                    summary_reason.clone(),
                    entry_fingerprint,
                )
                .with_line_count(entry_line_count)
                .with_covered_lines(entry_covered_lines),
                None => {
                    let inferred = match (&outcome.result, summary_status) {
                        (Ok(_), _) => UnavailableReason::WriteFailed,
                        (Err(_), "completed") => UnavailableReason::WriteFailed,
                        (Err(msg), _) => {
                            let lower = msg.to_lowercase();
                            if lower.contains("timeout") || lower.contains("timed out") {
                                UnavailableReason::TimedOut
                            } else if lower.contains("build failed")
                                || lower.contains("compilation failed")
                                || lower.contains("instrumentationfailed")
                            {
                                UnavailableReason::BuildFailed
                            } else {
                                UnavailableReason::RuntimeFailed
                            }
                        }
                    };
                    ExploreSummaryEntry::unavailable(
                        outcome.func.name.clone(),
                        summary_status.to_string(),
                        inferred,
                        summary_reason.clone(),
                        entry_fingerprint,
                    )
                    .with_line_count(entry_line_count)
                    .with_covered_lines(entry_covered_lines)
                }
            };
            match outcome_status_from_entry(&bucket_entry) {
                shatter_core::protocol::OutcomeStatus::Completed
                | shatter_core::protocol::OutcomeStatus::CompletedWithFindings => {}
                shatter_core::protocol::OutcomeStatus::RuntimeFailed => {
                    explore_summary.runtime_failed += 1;
                }
                shatter_core::protocol::OutcomeStatus::BuildFailed => {
                    explore_summary.build_failed += 1;
                }
                shatter_core::protocol::OutcomeStatus::TimedOut => {
                    explore_summary.timed_out += 1;
                }
                // str-jeen.40: env-preflight failure observed mid-batch.
                // Surface in the dedicated bucket so the run JSON can show
                // a single `preflight_failed` row instead of N noisy
                // runtime_failed entries.
                shatter_core::protocol::OutcomeStatus::PreflightFailed => {
                    explore_summary.preflight_failed += 1;
                }
                // Skipped variants don't appear here: this branch only runs
                // for scheduled work items (completed | failed). Pre-skipped
                // functions are seeded into `unsupported` at summary init.
                shatter_core::protocol::OutcomeStatus::Unsupported
                | shatter_core::protocol::OutcomeStatus::SkippedByPolicy => {}
            }
            if let Ok(ref result) = outcome.result
                && result.unique_paths > 0
            {
                explore_summary.produced_coverage += 1;
            }
            explore_summary.functions.push(bucket_entry);

            // Clean up the partial resume-state sidecar now that the function
            // is fully done (str-b2my.15).
            cleanup_resume_state(&artifact_root, &file_str, &outcome.func);
        }
        explore_summary.elapsed_secs = target_start.elapsed().as_secs_f64();
        // str-jeen.18: aggregate per-target outcome records into the three
        // named span-line denominators surfaced in the run JSON. Computed
        // once at finalization (after every outcome has been pushed) so the
        // serialized values match the final entry set; downstream readers
        // (str-jeen.19, str-jeen.20) consume these fields directly.
        let span_lines = span_line_denominators_from_entries(&explore_summary.functions);
        explore_summary.discovered_function_span_lines = span_lines.discovered;
        explore_summary.attempted_function_span_lines = span_lines.attempted;
        explore_summary.completed_function_span_lines = span_lines.completed;
        // str-jeen.41: line-weighted outcome buckets, recomputed from the
        // final entry set so the serialized values match (covered/uncovered
        // for completed entries; full span for build/runtime/timeout/unsupported).
        explore_summary.covered_completed_lines = span_lines.covered_completed;
        explore_summary.uncovered_completed_lines = span_lines.uncovered_completed;
        explore_summary.build_failed_function_lines = span_lines.build_failed;
        explore_summary.runtime_failed_function_lines = span_lines.runtime_failed;
        explore_summary.timed_out_function_lines = span_lines.timed_out;
        explore_summary.unsupported_function_lines = span_lines.unsupported;
        // str-jeen.31: attach the Go root-cause breakdown for this file when
        // the target is Go and at least one build_failed outcome was seen.
        // Computed once at finalization rather than incrementally so the
        // serialized JSON reflects the final entry set.
        if matches!(target_language, crate::args::Language::Go) {
            let breakdown = aggregate_go_root_causes_from_entries(&explore_summary.functions);
            if !breakdown.is_empty() {
                explore_summary.go_root_causes = Some(breakdown);
            }
        }
        // str-jeen.6: parallel TS root-cause breakdown attached when the
        // target is TS and at least one build/runtime failure was seen.
        if matches!(target_language, crate::args::Language::TypeScript) {
            let breakdown = aggregate_ts_root_causes_from_entries(&explore_summary.functions);
            if !breakdown.is_empty() {
                explore_summary.ts_root_causes = Some(breakdown);
            }
        }
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
            for (name, _span_lines, reasons) in &skipped_unexecutable {
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

        // str-jeen.4: per-target slice of the artifact-reference contract.
        // Only the path-existence + unavailable-reason half is checked here
        // because `artifact_root` is shared across targets in the same run —
        // sibling targets' artifacts would otherwise look like `stale_extra`.
        // The full stale-extras sweep happens once at finalize time.
        let per_target_summaries = std::slice::from_ref(&explore_summary);
        let mut target_validation = ArtifactValidationReport::default();
        check_summary_paths(&artifact_root, per_target_summaries, &mut target_validation);
        if !target_validation.is_clean() {
            log::warn!(
                "artifact-reference validation surfaced {} issue(s) for target {}:",
                target_validation.issues.len(),
                file_str
            );
            for issue in &target_validation.issues {
                log::warn!("  {issue}");
            }
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
                    ..FileSpecBundle::default()
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
    // str-6c6p: report content, not info logging — emit regardless of
    // log level so `--quiet` still surfaces the requested result.
    if header_printed && (report_outputs.is_empty() || stdout) {
        // str-oo31: aggregate per-OutcomeStatus buckets across every target
        // for the run-wide breakdown line. We bucket from each summary's
        // per-function entries via `bucket_counts_from_entries` rather than
        // summing the persisted bucket fields. That keeps the breakdown
        // accurate even for legacy summaries written before the bucket
        // fields existed (their field counts default to zero, but the
        // per-function `status` + `reason` strings still classify correctly).
        let mut run_buckets = OutcomeBuckets::default();
        let mut run_produced_coverage = 0usize;
        for summary in &report_summaries {
            let b = bucket_counts_from_entries(&summary.functions);
            run_buckets.completed += b.completed;
            run_buckets.runtime_failed += b.runtime_failed;
            run_buckets.build_failed += b.build_failed;
            run_buckets.timed_out += b.timed_out;
            run_buckets.unsupported += b.unsupported;
            run_buckets.skipped_by_policy += b.skipped_by_policy;
            run_buckets.preflight_failed += b.preflight_failed;
            run_produced_coverage += summary.produced_coverage;
        }
        let breakdown = format_outcome_breakdown(&run_buckets, run_produced_coverage);
        // str-jeen.31: aggregate Go build_failed root-causes across the
        // whole run so a broad-run footer surfaces the per-category counts
        // and line weights alongside the existing outcome breakdown.
        let go_breakdown = aggregate_go_root_causes(&report_summaries);
        let go_md = format_go_root_causes_md(&go_breakdown);
        // str-jeen.6: TS root-cause breakdown + cross-language failure-
        // impact rollup. Both are `None` on a clean run.
        let ts_breakdown = aggregate_ts_root_causes(&report_summaries);
        let ts_md = format_ts_root_causes_md(&ts_breakdown);
        let failure_impact_rows = aggregate_failure_impact(&report_summaries);
        let failure_impact_md = format_failure_impact_md(&failure_impact_rows);
        if output_format == crate::args::OutputFormat::Md {
            let coverage_suffix = if total_lines > 0 {
                let pct = ((total_covered as f64 / total_lines as f64) * 100.0)
                    .min(100.0)
                    .round() as u32;
                format!(" · **{pct}%** coverage ({total_covered}/{total_lines} lines)")
            } else {
                String::new()
            };
            let breakdown_suffix = breakdown
                .as_deref()
                .map(|line| format!("\n\n{line}"))
                .unwrap_or_default();
            let go_suffix = go_md
                .as_deref()
                .map(|s| format!("\n\n{s}"))
                .unwrap_or_default();
            let ts_suffix = ts_md
                .as_deref()
                .map(|s| format!("\n\n{s}"))
                .unwrap_or_default();
            let impact_suffix = failure_impact_md
                .as_deref()
                .map(|s| format!("\n\n{s}"))
                .unwrap_or_default();
            print_markdown(
                &format!(
                    "\n---\n\n**Summary:** {total_paths} path(s) across \
                     {total_function_count} function(s){coverage_suffix}{breakdown_suffix}{go_suffix}{ts_suffix}{impact_suffix}\n"
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
            if let Some(line) = breakdown.as_deref() {
                println!("{line}");
            }
            if let Some(s) = go_md.as_deref() {
                println!("\n{s}");
            }
            if let Some(s) = ts_md.as_deref() {
                println!("\n{s}");
            }
            if let Some(s) = failure_impact_md.as_deref() {
                println!("\n{s}");
            }
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
                } else {
                    // str-ni32: analyze/preflight failed before any spec was
                    // produced. Mirror the --spec-out branch below by
                    // writing a machine-readable no-target marker bundle so
                    // `-o file.json` always yields a file on disk, and let
                    // `decide_explore_exit_status` (extended in str-ni32)
                    // surface the failure via a nonzero exit.
                    let file_path = parsed
                        .first()
                        .map(|t| t.file.display().to_string())
                        .unwrap_or_default();
                    let bundle = build_no_target_spec_bundle(&file_path, &report_summaries);
                    shatter_core::spec::write_file_spec_bundle(&bundle, path).map_err(|e| {
                        format!(
                            "failed to write no-target spec marker to '{}': {e}",
                            path.display()
                        )
                    })?;
                    log::info!(
                        "Wrote no-target spec marker (reason={}) to {}",
                        bundle
                            .no_target_reason
                            .map(|r| r.as_token())
                            .unwrap_or("unclassified"),
                        path.display()
                    );
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
    if let Some(out) = output_path {
        let _spec_write_span = tracing::info_span!("spec.write_bundle").entered();
        if let Some(bundle) = file_spec_bundles.first() {
            // Single-target is the primary Make use case; write the first bundle.
            shatter_core::spec::write_file_spec_bundle(bundle, out)
                .map_err(|e| format!("failed to write spec bundle to {}: {e}", out.display()))?;
            log::info!(
                "Wrote spec bundle ({} function(s)) to {}",
                bundle.functions.len(),
                out.display()
            );
        } else {
            // str-jeen.67: no targets discovered — emit a machine-readable
            // marker bundle so batch tooling can classify this run as
            // skipped/no-target instead of inferring a partial failure from
            // a missing spec file.
            let file_path = parsed
                .first()
                .map(|t| t.file.display().to_string())
                .unwrap_or_default();
            let bundle = build_no_target_spec_bundle(&file_path, &report_summaries);
            shatter_core::spec::write_file_spec_bundle(&bundle, out).map_err(|e| {
                format!("failed to write no-target spec marker to {}: {e}", out.display())
            })?;
            log::info!(
                "Wrote no-target spec marker (reason={}) to {}",
                bundle
                    .no_target_reason
                    .map(|r| r.as_token())
                    .unwrap_or("unclassified"),
                out.display()
            );
        }
    }

    // str-qnp0: emit LLM oracle summary when oracle was active.
    if oracle_bundle.is_some() {
        // Aggregate oracle stats from all per-function observations that
        // carried oracle telemetry. Each function's OracleSlotMap was
        // independently constructed from the shared bundle, so stats are
        // already per-function; we sum them for the run-wide line.
        let agg = shatter_core::oracle::OracleStats::default();
        for summary in &report_summaries {
            for entry in &summary.functions {
                // Oracle stats are carried on the ObservationOutput; however
                // the summary entries don't hold observation data directly.
                // For now, the stats aggregate stays at zero until per-function
                // oracle stats propagation is fully wired (follow-up issue).
                let _ = entry;
            }
        }
        // Fallback: read budget from the oracle bundle config.
        let budget = oracle_bundle
            .as_ref()
            .map(|b| b.config.max_token_budget)
            .unwrap_or(0);
        eprintln!(
            "LLM oracle: {} queries · {} tokens · {} candidates accepted  [budget: {} / {}]",
            agg.queries_fired,
            agg.tokens_used,
            agg.candidates_accepted,
            agg.tokens_used,
            budget,
        );
    }

    // str-960w: a run that completed the full pipeline but where every
    // attempted target failed (e.g. all build_failed) must exit nonzero so
    // CI and agent workflows can detect the failure. Reports and artifacts
    // have already been written above, so callers still get machine-readable
    // per-target status in `summary.json`.
    decide_explore_exit_status(&report_summaries)?;
    Ok(())
}

// =============================================================================
// str-jeen.25: Frontend-agnostic no-target classifier
// =============================================================================
//
// Three cross-cutting reasons populated *before* per-frontend detection so
// they apply uniformly to TS / Go / Rust:
//
//   * `policy_excluded`  — the file path matches a user-configured policy
//                          glob (`shatter.config.json` `exclude` or
//                          `.shatterignore`).
//   * `generated_schema` — heuristic match on filename infix, path segment,
//                          or leading-comment marker (`@generated`,
//                          `DO NOT EDIT`, `Code generated by`).
//   * `parser_failure`   — frontend `Analyze` returned an error response.
//
// Precedence: `policy_excluded` > `generated_schema` (pre-analyze pair),
// and `parser_failure` is its own arm at the analyze-error site. Per
// str-jeen.21, `Unclassified` remains the fallback for any zero-target
// file no classifier matched. Per-language refinements (str-jeen.22/.24
// and siblings) tag specific kinds of empty files post-analyze.

/// Filename infixes that mark a file as generated-schema output. Each
/// pattern carries explicit dot/underscore boundaries to avoid matching
/// hand-written files like `generator.ts` or `gen.ts`.
const GENERATED_FILENAME_INFIXES: &[&str] =
    &[".gen.", ".pb.", "_pb.", "_generated.", ".generated."];

/// Path components (segment-equal match) that mark a directory as a
/// generated-code dump. Matching is exact: `gen` matches `src/gen/foo.ts`
/// but not `src/regen/foo.ts`.
const GENERATED_DIR_SEGMENTS: &[&str] = &["generated", "codegen", "__generated__", "gen"];

/// Phrases in the file's leading bytes that indicate generated content.
/// Case-sensitive — generators conventionally use these exact phrases.
const GENERATED_HEADER_MARKERS: &[&str] = &["DO NOT EDIT", "Code generated by", "@generated"];

/// Maximum bytes scanned from the head of a file when looking for a
/// generated-code marker. Bounded to keep the precheck cheap and to match
/// the "few comment lines at the top" convention for code-generation
/// markers. The scan also stops at the first blank-line boundary inside
/// this window — markers live in the leading comment block.
const HEADER_SCAN_BYTE_CAP: usize = 512;

/// Filename of the project-local exclusion file. Loaded from the project
/// root; one glob pattern per non-comment line. Mirrors `.gitignore`
/// semantics in spirit but is scoped to shatter's classifier.
const SHATTERIGNORE_FILENAME: &str = ".shatterignore";

/// Frontend-agnostic pre-classifier. Returns the no-target reason a file
/// matches *before* the frontend `Analyze` runs, or `None` if no
/// pre-classifier matched (in which case the analyze step proceeds and a
/// per-language classifier may refine `Unclassified` later).
///
/// Precedence is `policy_excluded` > `generated_schema` so explicit user
/// intent wins over a path/content heuristic.
fn pre_classify_no_target_reason(
    file: &Path,
    project_root: Option<&Path>,
    project_cfg: Option<&shatter_core::config::ProjectConfig>,
) -> Option<shatter_core::protocol::NoTargetReason> {
    if matches_policy_exclude(file, project_root, project_cfg) {
        return Some(shatter_core::protocol::NoTargetReason::PolicyExcluded);
    }
    if matches_generated_schema(file) {
        return Some(shatter_core::protocol::NoTargetReason::GeneratedSchema);
    }
    None
}

/// True when the file matches any user-configured policy exclusion: the
/// `exclude` globs in `shatter.config.json` or any non-comment line in
/// the project-root `.shatterignore` file. Matching is performed against
/// the project-root-relative path.
fn matches_policy_exclude(
    file: &Path,
    project_root: Option<&Path>,
    project_cfg: Option<&shatter_core::config::ProjectConfig>,
) -> bool {
    let Some(root) = project_root else {
        // Without a project root we have no anchor for relative-path
        // glob matching; defer to the analyze step.
        return false;
    };
    let relative = file.strip_prefix(root).unwrap_or(file);

    if let Some(cfg) = project_cfg
        && let Ok(Some(set)) = build_policy_globset(&cfg.exclude)
        && set.is_match(relative)
    {
        return true;
    }

    let ignore_path = root.join(SHATTERIGNORE_FILENAME);
    if let Ok(patterns) = read_shatterignore(&ignore_path)
        && let Ok(Some(set)) = build_policy_globset(&patterns)
        && set.is_match(relative)
    {
        return true;
    }

    false
}

fn build_policy_globset(patterns: &[String]) -> Result<Option<globset::GlobSet>, globset::Error> {
    if patterns.is_empty() {
        return Ok(None);
    }
    let mut builder = globset::GlobSetBuilder::new();
    for p in patterns {
        builder.add(globset::Glob::new(p)?);
    }
    Ok(Some(builder.build()?))
}

fn read_shatterignore(path: &Path) -> std::io::Result<Vec<String>> {
    let contents = std::fs::read_to_string(path)?;
    Ok(contents
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect())
}

/// True when the file matches the generated-schema heuristic on filename,
/// directory segment, or leading-comment marker.
fn matches_generated_schema(file: &Path) -> bool {
    if let Some(name) = file.file_name().and_then(|s| s.to_str())
        && GENERATED_FILENAME_INFIXES.iter().any(|p| name.contains(p))
    {
        return true;
    }
    if file
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .any(|seg| GENERATED_DIR_SEGMENTS.contains(&seg))
    {
        return true;
    }
    leading_bytes_match_generated_marker(file)
}

/// True when the first `HEADER_SCAN_BYTE_CAP` bytes of the file (capped
/// further at the first blank-line boundary) contain any
/// `GENERATED_HEADER_MARKERS` phrase. IO errors and non-UTF-8 prefixes
/// fall through as "not matched" — the analyze step may still produce
/// its own `parser_failure` classification.
fn leading_bytes_match_generated_marker(file: &Path) -> bool {
    use std::io::Read;
    let Ok(mut handle) = std::fs::File::open(file) else {
        return false;
    };
    let mut buf = [0u8; HEADER_SCAN_BYTE_CAP];
    let Ok(n) = handle.read(&mut buf) else {
        return false;
    };
    let Ok(text) = std::str::from_utf8(&buf[..n]) else {
        return false;
    };
    let head = text.split("\n\n").next().unwrap_or(text);
    GENERATED_HEADER_MARKERS.iter().any(|m| head.contains(m))
}

/// Build a stub `ExploreSummary` for a file we never ran analyze on (or
/// whose analyze failed). All bucket counters are zero; only `file`,
/// `status`, and `no_target_reason` carry information. The caller writes
/// this via `write_explore_summary` and pushes it onto `report_summaries`
/// so the markdown "## No targets discovered" section surfaces the row.
fn build_skip_summary(
    file: &str,
    reason: shatter_core::protocol::NoTargetReason,
) -> ExploreSummary {
    ExploreSummary {
        version: EXPLORE_ARTIFACT_VERSION,
        status: "skipped".to_string(),
        file: file.to_string(),
        no_target_reason: Some(reason),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ArtifactValidationIssue, ArtifactValidationReport, EXPLORE_ARTIFACT_VERSION,
        ExploreFailure, ExploreResultAccumulator, ExploreSummary, ExploreSummaryEntry,
        FuncExploreOutcome, GoBuildFailureCategory, GoRootCauseBreakdown, TsBuildFailureCategory,
        TsRootCauseBreakdown, UnavailableReason, aggregate_failure_impact,
        aggregate_go_root_causes, aggregate_go_root_causes_from_entries, aggregate_ts_root_causes,
        batch_is_exhausted, bucket_counts_from_entries, build_skip_summary, check_summary_paths,
        classify_go_build_failure, classify_no_target_reason, classify_outcome_status,
        classify_ts_build_failure, decide_explore_exit_status, emit_explore_progress,
        explore_summary_path, finalize_explore, format_failure_impact_md, format_go_root_causes_md,
        format_no_target_reason_table, format_outcome_breakdown, format_progress_snapshot,
        format_ts_root_causes_md, go_classify_no_target_reason, go_is_generated_by_content,
        go_is_receiver_method_gap_by_content, go_is_test_file_by_path,
        leading_bytes_match_generated_marker, load_explore_artifacts, matches_generated_schema,
        matches_policy_exclude, outcome_status_from_entry, persist_stage_outputs,
        pre_classify_no_target_reason, read_explore_artifact, rust_classify_no_target_reason,
        rust_is_build_script, rust_is_declaration_only, rust_is_test_module_by_content,
        rust_is_test_module_by_path, sanitize_artifact_component,
        span_line_denominators_from_entries, stage_persistence_dir, ts_classify_no_target_reason,
        ts_contains_jsx_component, ts_is_declaration_file_by_path, ts_is_declaration_only,
        ts_is_test_or_spec_by_path, validate_artifact_references, write_explore_artifact,
        write_explore_summary,
    };
    use shatter_core::config::GeneticConfig;
    use shatter_core::explorer::ExploreProgressSnapshot;
    use shatter_core::protocol::{FunctionAnalysis, InvocationModel};
    use shatter_core::report::ProgressEvent;
    use shatter_core::types::TypeInfo;
    use std::time::Duration;

    /// Helper to build a minimal `ExploreSummary` with the bucket counters
    /// the str-960w exit-status decision actually reads.
    fn summary_with_buckets(
        file: &str,
        completed: usize,
        build_failed: usize,
        runtime_failed: usize,
        timed_out: usize,
    ) -> ExploreSummary {
        ExploreSummary {
            version: EXPLORE_ARTIFACT_VERSION,
            status: "x".to_string(),
            file: file.to_string(),
            completed,
            failed: build_failed + runtime_failed + timed_out,
            build_failed,
            runtime_failed,
            timed_out,
            ..Default::default()
        }
    }

    #[test]
    fn decide_exit_status_ok_when_no_summaries() {
        assert!(decide_explore_exit_status(&[]).is_ok());
    }

    #[test]
    fn decide_exit_status_ok_when_no_target_was_attempted() {
        // A file with `no_target_reason` (e.g. test file, declaration-only)
        // contributes zero attempted functions and must not flip the exit
        // code on its own — the analyzer correctly determined there was
        // nothing to run.
        let summaries = vec![summary_with_buckets("declarations.d.ts", 0, 0, 0, 0)];
        assert!(decide_explore_exit_status(&summaries).is_ok());
    }

    #[test]
    fn decide_exit_status_ok_when_some_target_completed() {
        let summaries = vec![summary_with_buckets("ok.ts", 3, 0, 0, 0)];
        assert!(decide_explore_exit_status(&summaries).is_ok());
    }

    #[test]
    fn decide_exit_status_err_when_single_target_all_build_failed() {
        // Bug scenario: focused target produces only build_failed outcomes
        // and zero completions. Previously exited 0; must now exit nonzero.
        let summaries = vec![summary_with_buckets("broken.ts", 0, 4, 0, 0)];
        let err = decide_explore_exit_status(&summaries).expect_err("should fail");
        match err {
            ExploreFailure::AllAttemptedTargetsFailed {
                attempted_targets,
                build_failed,
                runtime_failed,
                timed_out,
            } => {
                assert_eq!(attempted_targets, 1);
                assert_eq!(build_failed, 4);
                assert_eq!(runtime_failed, 0);
                assert_eq!(timed_out, 0);
            }
        }
    }

    #[test]
    fn decide_exit_status_err_when_every_attempted_target_failed() {
        // Multiple attempted targets, none completed — mix of failure modes.
        let summaries = vec![
            summary_with_buckets("a.ts", 0, 2, 0, 0),
            summary_with_buckets("b.ts", 0, 0, 1, 0),
            summary_with_buckets("c.ts", 0, 0, 0, 3),
        ];
        let err = decide_explore_exit_status(&summaries).expect_err("should fail");
        let ExploreFailure::AllAttemptedTargetsFailed {
            attempted_targets,
            build_failed,
            runtime_failed,
            timed_out,
        } = err;
        assert_eq!(attempted_targets, 3);
        assert_eq!(build_failed, 2);
        assert_eq!(runtime_failed, 1);
        assert_eq!(timed_out, 3);
    }

    #[test]
    fn decide_exit_status_ok_partial_success_with_some_failed_targets() {
        // Partial-success policy: at least one attempted target completed
        // a function, so the run as a whole exits 0 even though another
        // target is entirely build_failed.
        let summaries = vec![
            summary_with_buckets("good.ts", 2, 0, 0, 0),
            summary_with_buckets("bad.ts", 0, 5, 0, 0),
        ];
        assert!(decide_explore_exit_status(&summaries).is_ok());
    }

    #[test]
    fn decide_exit_status_skip_only_summaries_do_not_force_failure() {
        // A run where nothing was attempted (every target was a no-target
        // skip row) is not a failure even though no completion happened.
        let summaries = vec![
            summary_with_buckets("a.d.ts", 0, 0, 0, 0),
            summary_with_buckets("b_test.ts", 0, 0, 0, 0),
        ];
        assert!(decide_explore_exit_status(&summaries).is_ok());
    }

    #[test]
    fn decide_exit_status_err_when_only_parser_failure_summary() {
        // str-ni32: when the explore pipeline never reaches the per-function
        // loop because Analyze returned an error (parser/preflight failure),
        // the run still has zero {completed,build_failed,runtime_failed,
        // timed_out} but it is *not* a no-target skip — it is a hard
        // analyze failure and must surface a nonzero exit.
        let summaries = vec![ExploreSummary {
            version: EXPLORE_ARTIFACT_VERSION,
            status: "parser_failure: PreflightFailed".to_string(),
            file: "broken.go".to_string(),
            ..Default::default()
        }];
        let err = decide_explore_exit_status(&summaries).expect_err("should fail");
        let ExploreFailure::AllAttemptedTargetsFailed {
            attempted_targets, ..
        } = err;
        assert_eq!(attempted_targets, 1);
    }

    #[test]
    fn explore_failure_display_includes_bucket_counts() {
        let err = ExploreFailure::AllAttemptedTargetsFailed {
            attempted_targets: 2,
            build_failed: 3,
            runtime_failed: 1,
            timed_out: 0,
        };
        let msg = format!("{err}");
        assert!(msg.contains("all 2 attempted target"), "{msg}");
        assert!(msg.contains("build_failed=3"), "{msg}");
        assert!(msg.contains("runtime_failed=1"), "{msg}");
        assert!(msg.contains("timed_out=0"), "{msg}");
    }

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
            ..Default::default()
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
            ..Default::default()
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
                    line_count: 0,
                    covered_lines: 0,
                },
                ExploreSummaryEntry {
                    function_name: "save".to_string(),
                    status: "failed".to_string(),
                    artifact: Some("src_user.ts/00025_save.json".to_string()),
                    reason: Some("timeout".to_string()),
                    deep_fingerprint: None,
                    line_count: 0,
                    covered_lines: 0,
                },
            ],
            ..Default::default()
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
            ..Default::default()
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
                    line_count: 0,
                    covered_lines: 0,
                },
                ExploreSummaryEntry {
                    function_name: "save/user".to_string(),
                    status: "failed".to_string(),
                    artifact: None,
                    reason: Some("timeout".to_string()),
                    deep_fingerprint: None,
                    line_count: 0,
                    covered_lines: 0,
                },
                ExploreSummaryEntry {
                    function_name: "skip/user".to_string(),
                    status: "skipped".to_string(),
                    artifact: None,
                    reason: Some("unexecutable parameter types".to_string()),
                    deep_fingerprint: None,
                    line_count: 0,
                    covered_lines: 0,
                },
            ],
            ..Default::default()
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
                line_count: 0,
                covered_lines: 0,
            }],
            ..Default::default()
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
                    line_count: 0,
                    covered_lines: 0,
                },
                ExploreSummaryEntry {
                    function_name: "save".to_string(),
                    status: "failed".to_string(),
                    artifact: None,
                    reason: Some("timeout".to_string()),
                    deep_fingerprint: Some("def456".to_string()),
                    line_count: 0,
                    covered_lines: 0,
                },
            ],
            ..Default::default()
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
                line_count: 0,
                covered_lines: 0,
            }],
            ..Default::default()
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
                line_count: 0,
                covered_lines: 0,
            }],
            ..Default::default()
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
                line_count: 0,
                covered_lines: 0,
            }],
            ..Default::default()
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
                line_count: 0,
                covered_lines: 0,
            }],
            ..Default::default()
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
                line_count: 0,
                covered_lines: 0,
            }],
            ..Default::default()
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

    // --- str-060a: --clean must wipe prior explore artifacts ---

    use super::{clean_target_artifacts, target_artifact_dir};

    #[test]
    fn clean_target_artifacts_removes_summary_and_per_function_artifacts() {
        // Regression for str-060a: a stale-artifact-then-clean-rerun must not
        // leave any prior summary, per-function artifact, or resume-state
        // sidecar on disk for the cleaned target. If any of these survive, the
        // resume detection block at the top of the per-target explore loop
        // would log "Found prior explore summary" and reuse stale results
        // even when the user explicitly passed `--clean`.
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        let file = "src/user.ts";

        // Pre-populate a prior summary as a previous run would have written.
        let summary = ExploreSummary {
            version: EXPLORE_ARTIFACT_VERSION,
            status: "completed".to_string(),
            file: file.to_string(),
            total_functions: 1,
            completed: 1,
            failed: 0,
            skipped: 0,
            elapsed_secs: 1.0,
            functions: vec![ExploreSummaryEntry {
                function_name: "load".to_string(),
                status: "completed".to_string(),
                artifact: Some("src_user.ts/00012_load.json".to_string()),
                reason: None,
                deep_fingerprint: Some("fp-abc".to_string()),
                line_count: 0,
                covered_lines: 0,
            }],
            ..Default::default()
        };
        write_explore_summary(root, file, &summary).expect("write summary");

        // Pre-populate a per-function resume-state sidecar.
        let func = sample_func_analysis();
        let state = shatter_core::orchestrator::ExploreState::default();
        write_resume_state(root, file, &func, &state).expect("write resume state");

        // Sanity: prior artifacts visible before clean.
        let target_dir = target_artifact_dir(root, file);
        assert!(target_dir.exists(), "target artifact dir should exist");
        assert!(
            read_explore_summary(root, file).is_some(),
            "prior summary must be loadable before --clean",
        );
        assert!(
            read_resume_state(root, file, &func).is_some(),
            "resume state must be loadable before --clean",
        );

        // Apply --clean semantics for this target.
        clean_target_artifacts(root, file);

        // Both the summary and the resume-state sidecar must be gone.
        assert!(
            !target_dir.exists(),
            "target artifact dir must be removed by --clean",
        );
        assert!(
            read_explore_summary(root, file).is_none(),
            "prior summary must not survive --clean",
        );
        assert!(
            read_resume_state(root, file, &func).is_none(),
            "resume-state sidecar must not survive --clean",
        );
    }

    #[test]
    fn clean_target_artifacts_is_idempotent_when_no_prior_run() {
        // First-ever explore: no artifacts yet. --clean must be a no-op
        // (no panic, no error, no spurious directory creation).
        let dir = tempfile::tempdir().expect("tempdir");
        clean_target_artifacts(dir.path(), "src/never-explored.ts");
        let target_dir = target_artifact_dir(dir.path(), "src/never-explored.ts");
        assert!(
            !target_dir.exists(),
            "clean must not create the target artifact dir when none existed",
        );
    }

    #[test]
    fn clean_target_artifacts_does_not_touch_other_targets() {
        // --clean must scope its deletion to the named target only. Artifacts
        // for sibling targets (a different source file under the same project)
        // must survive so a focused `--clean` rerun on one file does not wipe
        // the rest of the project's explore cache.
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();

        let other_summary = ExploreSummary {
            version: EXPLORE_ARTIFACT_VERSION,
            status: "completed".to_string(),
            file: "src/other.ts".to_string(),
            total_functions: 0,
            completed: 0,
            failed: 0,
            skipped: 0,
            elapsed_secs: 0.0,
            functions: vec![],
            ..Default::default()
        };
        write_explore_summary(root, "src/other.ts", &other_summary).expect("write other");

        let target_summary = ExploreSummary {
            version: EXPLORE_ARTIFACT_VERSION,
            status: "completed".to_string(),
            file: "src/user.ts".to_string(),
            total_functions: 0,
            completed: 0,
            failed: 0,
            skipped: 0,
            elapsed_secs: 0.0,
            functions: vec![],
            ..Default::default()
        };
        write_explore_summary(root, "src/user.ts", &target_summary).expect("write target");

        clean_target_artifacts(root, "src/user.ts");

        assert!(
            read_explore_summary(root, "src/user.ts").is_none(),
            "cleaned target's summary must be gone",
        );
        assert!(
            read_explore_summary(root, "src/other.ts").is_some(),
            "sibling target's summary must survive a scoped --clean",
        );
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

    // ── str-oo31: per-OutcomeStatus aggregation, no-target classification ──

    fn entry(name: &str, status: &str, reason: Option<&str>) -> ExploreSummaryEntry {
        ExploreSummaryEntry {
            function_name: name.to_string(),
            status: status.to_string(),
            artifact: None,
            reason: reason.map(|s| s.to_string()),
            deep_fingerprint: None,
            line_count: 0,
            covered_lines: 0,
        }
    }

    fn entry_with_lines(
        name: &str,
        status: &str,
        reason: Option<&str>,
        line_count: u32,
    ) -> ExploreSummaryEntry {
        let mut e = entry(name, status, reason);
        e.line_count = line_count;
        e
    }

    /// str-jeen.41: completed-entry builder with both `line_count` and
    /// `covered_lines` set so tests can pin the covered/uncovered split.
    fn completed_entry_with_coverage(
        name: &str,
        line_count: u32,
        covered_lines: u32,
    ) -> ExploreSummaryEntry {
        let mut e = entry_with_lines(name, "completed", None, line_count);
        e.covered_lines = covered_lines.min(line_count);
        e
    }

    #[test]
    fn bucket_counts_split_failed_into_runtime_build_and_timed_out() {
        // Mixed status fixture covering each OutcomeStatus the explore command
        // produces. The legacy tri-bucket (completed/failed/skipped) collapses
        // every non-Completed function-level status into a single "failed"
        // bucket; this test pins the new split.
        let entries = vec![
            entry("ok1", "completed", None),
            entry("ok2", "completed", None),
            entry("rt", "failed", Some("panic: nil pointer")),
            entry(
                "build",
                "failed",
                Some("execute error (InstrumentationFailed): build failed: exit 1"),
            ),
            entry("timed", "failed", Some("function timed out after 30s")),
            entry("unsup", "skipped", Some("unexecutable parameter types")),
            entry("policy", "skipped", Some("explicitly excluded by user")),
        ];
        let buckets = bucket_counts_from_entries(&entries);
        assert_eq!(buckets.completed, 2);
        assert_eq!(buckets.runtime_failed, 1);
        assert_eq!(buckets.build_failed, 1);
        assert_eq!(buckets.timed_out, 1);
        assert_eq!(buckets.unsupported, 1);
        assert_eq!(buckets.skipped_by_policy, 1);
        // Invariant: bucket totals must sum to entry count (no dropped status).
        let total = buckets.completed
            + buckets.runtime_failed
            + buckets.build_failed
            + buckets.timed_out
            + buckets.unsupported
            + buckets.skipped_by_policy;
        assert_eq!(total, entries.len());
    }

    // ── str-jeen.18: function-span line denominators ──

    #[test]
    fn span_line_denominators_discovery_only_path_counts_in_discovered_only() {
        // Discovery-only: a function the analyzer rejected as unexecutable
        // (e.g. opaque parameter type) shows up in the summary as
        // status=skipped / reason=unexecutable parameter types but never
        // gets attempted. Its span counts toward `discovered` only — not
        // `attempted`, not `completed`.
        let entries = vec![entry_with_lines(
            "openSocket",
            "skipped",
            Some("unexecutable parameter types"),
            12,
        )];
        let totals = span_line_denominators_from_entries(&entries);
        assert_eq!(totals.discovered, 12);
        assert_eq!(totals.attempted, 0);
        assert_eq!(totals.completed, 0);
    }

    #[test]
    fn span_line_denominators_attempt_failure_path_counts_in_discovered_and_attempted() {
        // Attempt-failure: each of the three failure modes (build /
        // runtime / timeout) was actually scheduled and run. They count
        // toward `discovered` and `attempted`, but not `completed`.
        let entries = vec![
            entry_with_lines(
                "buildFailed",
                "failed",
                Some("execute error (InstrumentationFailed): build failed: exit 1"),
                7,
            ),
            entry_with_lines("rtFailed", "failed", Some("panic: nil pointer"), 11),
            entry_with_lines("timed", "failed", Some("function timed out after 30s"), 5),
        ];
        let totals = span_line_denominators_from_entries(&entries);
        let expected_total = 7 + 11 + 5;
        assert_eq!(totals.discovered, expected_total);
        assert_eq!(totals.attempted, expected_total);
        assert_eq!(totals.completed, 0);
    }

    #[test]
    fn span_line_denominators_completion_path_counts_in_all_three() {
        // Completion: a function that ran to completion contributes to
        // every denominator. Pair with a pre-skipped entry to confirm the
        // partition: completed lines flow through all three; pre-skipped
        // lines stop at `discovered`.
        let entries = vec![
            entry_with_lines("ok", "completed", None, 9),
            entry_with_lines("unsup", "skipped", Some("unexecutable parameter types"), 4),
        ];
        let totals = span_line_denominators_from_entries(&entries);
        assert_eq!(totals.discovered, 9 + 4);
        assert_eq!(totals.attempted, 9);
        assert_eq!(totals.completed, 9);
    }

    // ── str-jeen.41: line-weighted outcome buckets ──

    #[test]
    fn span_line_outcome_buckets_partition_mixed_file_by_final_status() {
        // Mixed-file fixture: one of every outcome status the run JSON's
        // line-weighted buckets must distinguish — completed (with a
        // covered/uncovered split), build_failed, runtime_failed, timed_out,
        // and unsupported. The acceptance criterion for str-jeen.41 is that
        // every one of these contributes to its dedicated bucket without
        // any line vanishing from accounting.
        const COMPLETED_LINES: u32 = 20;
        const COMPLETED_COVERED: u32 = 13;
        const BUILD_FAILED_LINES: u32 = 7;
        const RUNTIME_FAILED_LINES: u32 = 11;
        const TIMED_OUT_LINES: u32 = 5;
        const UNSUPPORTED_LINES: u32 = 4;

        let entries = vec![
            completed_entry_with_coverage("ok", COMPLETED_LINES, COMPLETED_COVERED),
            entry_with_lines(
                "build",
                "failed",
                Some("execute error (InstrumentationFailed): build failed: exit 1"),
                BUILD_FAILED_LINES,
            ),
            entry_with_lines(
                "runtime",
                "failed",
                Some("panic: nil pointer"),
                RUNTIME_FAILED_LINES,
            ),
            entry_with_lines(
                "timed",
                "failed",
                Some("function timed out after 30s"),
                TIMED_OUT_LINES,
            ),
            entry_with_lines(
                "unsup",
                "skipped",
                Some("unexecutable parameter types"),
                UNSUPPORTED_LINES,
            ),
        ];
        let totals = span_line_denominators_from_entries(&entries);

        // Per-outcome line buckets land in their own bins.
        assert_eq!(totals.covered_completed, COMPLETED_COVERED);
        assert_eq!(
            totals.uncovered_completed,
            COMPLETED_LINES - COMPLETED_COVERED
        );
        assert_eq!(totals.build_failed, BUILD_FAILED_LINES);
        assert_eq!(totals.runtime_failed, RUNTIME_FAILED_LINES);
        assert_eq!(totals.timed_out, TIMED_OUT_LINES);
        assert_eq!(totals.unsupported, UNSUPPORTED_LINES);

        // Invariant: covered + uncovered for completed entries equals the
        // .18 `completed` denominator. No completed lines may leak into a
        // failure bucket.
        assert_eq!(
            totals.covered_completed + totals.uncovered_completed,
            totals.completed,
        );

        // Invariant: every discovered line is accounted for in exactly one
        // outcome bucket (since this fixture has no skipped_by_policy entries,
        // the six surfaced buckets must sum to `discovered`).
        let bucket_sum = totals.covered_completed
            + totals.uncovered_completed
            + totals.build_failed
            + totals.runtime_failed
            + totals.timed_out
            + totals.unsupported;
        assert_eq!(bucket_sum, totals.discovered);
    }

    #[test]
    fn span_line_outcome_buckets_skipped_by_policy_is_in_discovered_only() {
        // SkippedByPolicy entries count toward `discovered` (and nothing else
        // surfaced in str-jeen.41): user-driven exclusion is not a coverage
        // gap, so it gets no dedicated outcome bucket.
        let entries = vec![entry_with_lines(
            "policy",
            "skipped",
            Some("explicitly excluded by user"),
            6,
        )];
        let totals = span_line_denominators_from_entries(&entries);
        assert_eq!(totals.discovered, 6);
        assert_eq!(totals.attempted, 0);
        assert_eq!(totals.completed, 0);
        assert_eq!(totals.covered_completed, 0);
        assert_eq!(totals.uncovered_completed, 0);
        assert_eq!(totals.build_failed, 0);
        assert_eq!(totals.runtime_failed, 0);
        assert_eq!(totals.timed_out, 0);
        assert_eq!(totals.unsupported, 0);
    }

    #[test]
    fn span_line_outcome_buckets_cap_covered_at_line_count() {
        // Defensive cap: an artifact written before the cap landed could
        // carry `covered_lines > line_count` (instrumenter line-id space
        // disagreeing with the analyzer's source span). The aggregator must
        // re-cap so the partition invariant
        // `covered + uncovered == completed` always holds.
        let mut e = completed_entry_with_coverage("clamp", 10, 10);
        e.covered_lines = 25; // bypass `with_covered_lines` cap to simulate stale artifact
        let totals = span_line_denominators_from_entries(&[e]);
        assert_eq!(totals.completed, 10);
        assert_eq!(totals.covered_completed, 10);
        assert_eq!(totals.uncovered_completed, 0);
    }

    // ── str-jeen.31: Go broad-run root-cause aggregation ──

    #[test]
    fn classify_go_build_failure_routes_each_category_via_canonical_reason_text() {
        use super::GoBuildFailureCategory as G;
        // Each canonical Go-toolchain wording must land in its category. The
        // assertion is per-pattern so a future heuristic regression points
        // at the exact wording that drifted.
        assert_eq!(
            classify_go_build_failure(
                "use of internal package github.com/x/y/internal/foo not allowed"
            ),
            G::InternalPackage,
        );
        assert_eq!(
            classify_go_build_failure("found packages foo (foo.go) and bar (bar.go) in /tmp/x"),
            G::MixedPackage,
        );
        assert_eq!(
            classify_go_build_failure("imported and not used: \"fmt\""),
            G::MissingImport,
        );
        assert_eq!(
            classify_go_build_failure("undefined: pkg.DoThing"),
            G::MissingImport,
        );
        assert_eq!(
            classify_go_build_failure("syntax error: unexpected newline, expecting comma"),
            G::RewriteSyntax,
        );
        assert_eq!(
            classify_go_build_failure("expected operand, found ')'"),
            G::RewriteSyntax,
        );
        assert_eq!(
            classify_go_build_failure("unsupported parameter type chan<- int"),
            G::UnsupportedParamType,
        );
        // Unmatched wording falls into Other so totals reconcile.
        assert_eq!(
            classify_go_build_failure("disk full while linking"),
            G::Other,
        );
        // Internal-package + missing-import collision: the more specific
        // bucket wins so a single reason can't be double-counted.
        assert_eq!(
            classify_go_build_failure("use of internal package x not allowed; undefined: x.Do"),
            G::InternalPackage,
        );
    }

    #[test]
    fn aggregate_go_root_causes_line_weights_synthetic_mix() {
        // Synthetic mix of build_failed entries spanning every category plus
        // a non-build_failed row that must NOT contribute. The aggregator
        // must report (a) per-category counts, (b) line-weight equal to the
        // sum of contributing entries' line_count, (c) zero counts for
        // categories nothing matched.
        let entries = vec![
            // Two internal-package failures totaling 30 lines.
            entry_with_lines(
                "InternalA",
                "failed",
                Some(
                    "execute error (InstrumentationFailed): build failed: use of internal package x not allowed",
                ),
                10,
            ),
            entry_with_lines(
                "InternalB",
                "failed",
                Some(
                    "execute error (InstrumentationFailed): build failed: use of internal package y not allowed",
                ),
                20,
            ),
            // One missing-import failure of 5 lines.
            entry_with_lines(
                "MissingImp",
                "failed",
                Some("execute error (InstrumentationFailed): build failed: undefined: pkg.X"),
                5,
            ),
            // One rewrite-syntax failure of 100 lines (heavy weight).
            entry_with_lines(
                "Syntax",
                "failed",
                Some(
                    "execute error (InstrumentationFailed): build failed: syntax error: unexpected '}'",
                ),
                100,
            ),
            // One mixed-package failure of 7 lines.
            entry_with_lines(
                "MixedPkg",
                "failed",
                Some(
                    "execute error (InstrumentationFailed): build failed: found packages foo and bar in /tmp",
                ),
                7,
            ),
            // One unsupported-param-type build failure of 3 lines.
            entry_with_lines(
                "BadParam",
                "failed",
                Some(
                    "execute error (InstrumentationFailed): build failed: unsupported parameter type chan int",
                ),
                3,
            ),
            // One unmatched build_failed reason of 1 line lands in Other.
            entry_with_lines(
                "Mystery",
                "failed",
                Some("execute error (InstrumentationFailed): build failed: linker exit 1"),
                1,
            ),
            // Non-build_failed rows must NOT contribute to any category.
            entry_with_lines("Slow", "failed", Some("function timed out after 30s"), 999),
            entry_with_lines("Done", "completed", None, 50),
        ];
        let breakdown = aggregate_go_root_causes_from_entries(&entries);
        assert_eq!(breakdown.internal_package.count, 2);
        assert_eq!(breakdown.internal_package.line_weight, 30);
        assert_eq!(breakdown.missing_import.count, 1);
        assert_eq!(breakdown.missing_import.line_weight, 5);
        assert_eq!(breakdown.rewrite_syntax.count, 1);
        assert_eq!(breakdown.rewrite_syntax.line_weight, 100);
        assert_eq!(breakdown.mixed_package.count, 1);
        assert_eq!(breakdown.mixed_package.line_weight, 7);
        assert_eq!(breakdown.unsupported_param_type.count, 1);
        assert_eq!(breakdown.unsupported_param_type.line_weight, 3);
        assert_eq!(breakdown.other.count, 1);
        assert_eq!(breakdown.other.line_weight, 1);
        // Markdown render exists when any non-zero bucket is present.
        let md = format_go_root_causes_md(&breakdown).expect("non-empty breakdown renders");
        assert!(md.contains("`internal_package`"));
        assert!(md.contains("`rewrite_syntax`"));
        assert!(md.contains("100"), "syntax line weight must surface: {md}");
        // Empty breakdown renders to None so non-Go runs stay clean.
        assert!(format_go_root_causes_md(&GoRootCauseBreakdown::default()).is_none());
    }

    #[test]
    fn aggregate_go_root_causes_filters_to_go_files() {
        // Mixed-language run: a TS file with a build_failed row must NOT
        // contribute to the Go breakdown, even though its reason text
        // would otherwise match a Go classifier heuristic.
        let go_summary = make_summary(
            "src/foo.go",
            vec![entry_with_lines(
                "Foo",
                "failed",
                Some(
                    "execute error (InstrumentationFailed): build failed: use of internal package",
                ),
                10,
            )],
        );
        let ts_summary = make_summary(
            "src/foo.ts",
            vec![entry_with_lines(
                "Bar",
                "failed",
                Some(
                    "execute error (InstrumentationFailed): build failed: use of internal package",
                ),
                10,
            )],
        );
        let breakdown = aggregate_go_root_causes(&[go_summary, ts_summary]);
        // Only the Go entry contributes.
        assert_eq!(breakdown.internal_package.count, 1);
        assert_eq!(breakdown.internal_package.line_weight, 10);
    }

    #[test]
    fn classify_no_target_reason_defaults_to_unclassified_for_zero_target_files() {
        use shatter_core::protocol::NoTargetReason;
        use std::path::PathBuf;
        // Path-less Rust file with a non-rust suffix exercises the
        // fall-through: zero-target file, no Rust signal, defaults to
        // `unclassified`.
        let ts_path = PathBuf::from("src/empty.ts");
        assert_eq!(
            classify_no_target_reason(0, 0, crate::args::Language::TypeScript, &ts_path),
            Some(NoTargetReason::Unclassified),
        );
        assert_eq!(
            classify_no_target_reason(0, 7, crate::args::Language::Go, &ts_path),
            Some(NoTargetReason::Unclassified),
        );
        // total_functions>0: not a no-target file; reason is None.
        assert_eq!(
            classify_no_target_reason(3, 0, crate::args::Language::TypeScript, &ts_path),
            None,
        );
        assert_eq!(
            classify_no_target_reason(3, 2, crate::args::Language::Rust, &ts_path),
            None,
        );
    }

    #[test]
    fn explore_summary_no_target_reason_roundtrips_as_enum_token() {
        // str-jeen.21: the field is a closed enum; serde must emit the
        // snake_case token and parse it back into the typed variant.
        use shatter_core::protocol::NoTargetReason;
        let mut summary = ExploreSummary {
            version: EXPLORE_ARTIFACT_VERSION,
            status: "completed".to_string(),
            file: "src/empty.ts".to_string(),
            no_target_reason: Some(NoTargetReason::Unclassified),
            ..Default::default()
        };
        let json = serde_json::to_string(&summary).expect("serialize");
        assert!(
            json.contains("\"no_target_reason\":\"unclassified\""),
            "expected snake_case token in JSON, got: {json}"
        );
        let parsed: ExploreSummary = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.no_target_reason, Some(NoTargetReason::Unclassified));

        // Each non-default variant must roundtrip too — the schema
        // enumerates these so siblings can emit them without further
        // protocol change.
        for variant in [
            NoTargetReason::DeclarationOnly,
            NoTargetReason::JsxComponentOnly,
            NoTargetReason::TestOrSpec,
            NoTargetReason::ReceiverMethodGap,
            NoTargetReason::Generated,
            NoTargetReason::TestFile,
            NoTargetReason::TestModule,
            NoTargetReason::BuildScript,
            NoTargetReason::PolicyExcluded,
            NoTargetReason::ParserFailure,
            NoTargetReason::GeneratedSchema,
        ] {
            summary.no_target_reason = Some(variant);
            let j = serde_json::to_string(&summary).expect("serialize");
            let token = variant.as_token();
            assert!(
                j.contains(&format!("\"no_target_reason\":\"{token}\"")),
                "expected token {token} in JSON, got: {j}"
            );
            let p: ExploreSummary = serde_json::from_str(&j).expect("deserialize");
            assert_eq!(p.no_target_reason, Some(variant));
        }

        // Markdown rendering: zero-target files surface in a "File / Reason"
        // markdown column appended to the "No targets discovered" section.
        let rows = vec![
            ("src/types.d.ts", NoTargetReason::DeclarationOnly),
            ("src/empty.ts", NoTargetReason::Unclassified),
        ];
        let table = format_no_target_reason_table("intro line", &rows);
        assert!(
            table.contains("| File | Reason |"),
            "table header missing: {table}"
        );
        assert!(
            table.contains("| src/types.d.ts | `declaration_only` |"),
            "missing row: {table}"
        );
        assert!(
            table.contains("| src/empty.ts | `unclassified` |"),
            "missing row: {table}"
        );

        // None roundtrips as a missing field (skip_serializing_if).
        summary.no_target_reason = None;
        let none_json = serde_json::to_string(&summary).expect("serialize");
        assert!(
            !none_json.contains("no_target_reason"),
            "None variant must be omitted from JSON, got: {none_json}"
        );
    }

    #[test]
    fn format_outcome_breakdown_returns_none_on_happy_path() {
        // Per team-lead direction: when only `completed` is non-zero, suppress
        // the breakdown so the demo footer stays one line.
        let buckets = super::OutcomeBuckets {
            completed: 5,
            ..Default::default()
        };
        assert!(format_outcome_breakdown(&buckets, 5).is_none());
    }

    #[test]
    fn format_outcome_breakdown_emits_line_when_non_completed_buckets_present() {
        let buckets = super::OutcomeBuckets {
            completed: 31,
            runtime_failed: 430,
            build_failed: 0,
            timed_out: 5,
            unsupported: 0,
            skipped_by_policy: 0,
            preflight_failed: 0,
        };
        let line = format_outcome_breakdown(&buckets, 31)
            .expect("breakdown line should be Some when failures exist");
        // Must surface runtime_failed and timed_out separately.
        assert!(line.contains("runtime_failed: 430"), "line was: {line}");
        assert!(line.contains("timed_out: 5"), "line was: {line}");
        // produced_coverage denominator must be unambiguous (not "completed").
        assert!(
            line.contains("produced coverage: 31"),
            "should label produced-coverage denominator clearly; got: {line}"
        );
        // Empty buckets must be omitted to keep the line compact.
        assert!(!line.contains("build_failed: 0"), "got: {line}");
        assert!(!line.contains("unsupported: 0"), "got: {line}");
    }

    #[test]
    fn explore_summary_serde_defaults_for_new_fields() {
        // Old artifacts written before str-oo31 lack the bucket fields. They
        // must still parse, with bucket counts defaulting to zero and
        // no_target_reason defaulting to None.
        let legacy_json = r#"{
            "version": 4,
            "status": "completed",
            "file": "src/foo.ts",
            "total_functions": 2,
            "completed": 2,
            "failed": 0,
            "skipped": 0,
            "elapsed_secs": 0.5,
            "functions": []
        }"#;
        let parsed: ExploreSummary =
            serde_json::from_str(legacy_json).expect("legacy artifact must still parse");
        assert_eq!(parsed.completed, 2);
        assert_eq!(parsed.runtime_failed, 0);
        assert_eq!(parsed.build_failed, 0);
        assert_eq!(parsed.timed_out, 0);
        assert_eq!(parsed.unsupported, 0);
        assert_eq!(parsed.skipped_by_policy, 0);
        assert_eq!(parsed.produced_coverage, 0);
        assert_eq!(parsed.no_target_reason, None);
    }

    // ── str-gz8j: per-function timeout surfaces as TimedOut, not Completed ──

    /// A successful `Result<ObservationOutput>` whose `timed_out` flag is
    /// true means exploration ran out of its per-function budget mid-flight.
    /// `classify_outcome_status` must downgrade it to status="failed" with
    /// an explicit timeout reason so it lands in the `timed_out` bucket
    /// (str-oo31). The reason wording must mention the budget so users can
    /// tell *why* a function failed without reading the artifact.
    #[test]
    fn classify_outcome_status_timed_out_observation_becomes_failed_with_explicit_reason() {
        let mut obs = make_named_observation("slowFn");
        obs.timed_out = true;
        let result: Result<shatter_core::explorer::ObservationOutput, String> = Ok(obs);
        let (status, reason) = classify_outcome_status(&result, Duration::from_millis(31_500));
        assert_eq!(status, "failed");
        let reason_str = reason.expect("timed-out outcome must have a reason");
        let lower = reason_str.to_lowercase();
        assert!(
            lower.contains("timed out") || lower.contains("timeout"),
            "reason must include timeout keyword for outcome_status_from_entry to bucket as TimedOut; got: {reason_str}"
        );
        assert!(
            lower.contains("per-function"),
            "reason must make timeout scope explicit (per-function) per str-gz8j AC #3; got: {reason_str}"
        );
        assert!(
            reason_str.contains("31.5"),
            "reason should record elapsed seconds so users see how long the function ran; got: {reason_str}"
        );
        // Round-trip through outcome_status_from_entry → must classify as
        // TimedOut (which then bumps the timed_out bucket).
        let entry = ExploreSummaryEntry {
            function_name: "slowFn".into(),
            status: status.to_string(),
            artifact: None,
            reason: Some(reason_str),
            deep_fingerprint: None,
            line_count: 0,
            covered_lines: 0,
        };
        assert_eq!(
            outcome_status_from_entry(&entry),
            shatter_core::protocol::OutcomeStatus::TimedOut,
            "timed-out observation must round-trip into TimedOut bucket"
        );
    }

    #[test]
    fn classify_outcome_status_normal_completion_stays_completed() {
        let obs = make_named_observation("ok");
        let result: Result<shatter_core::explorer::ObservationOutput, String> = Ok(obs);
        let (status, reason) = classify_outcome_status(&result, Duration::from_millis(120));
        assert_eq!(status, "completed");
        assert!(
            reason.is_none(),
            "completed outcome must not synthesize a reason"
        );
    }

    #[test]
    fn classify_outcome_status_error_preserves_original_message() {
        let result: Result<shatter_core::explorer::ObservationOutput, String> =
            Err("frontend crashed: signal 11".into());
        let (status, reason) = classify_outcome_status(&result, Duration::from_millis(50));
        assert_eq!(status, "failed");
        assert_eq!(reason.as_deref(), Some("frontend crashed: signal 11"));
    }

    fn make_named_observation(name: &str) -> shatter_core::explorer::ObservationOutput {
        shatter_core::explorer::ObservationOutput {
            function_name: name.to_string(),
            iterations: 1,
            unique_paths: 0,
            lines_covered: 0,
            total_lines: 0,
            new_path_executions: vec![],
            raw_results: vec![],
            discoveries: vec![],
            nondeterministic_fields: vec![],
            float_probe_results: vec![],
            boundary_results: vec![],
            shrunk_witnesses: Default::default(),
            mcdc_summary: None,
            shrink_stats: Default::default(),
            abandoned_frontiers: vec![],
            opaque_suggestions: vec![],
            stubbed_modules: vec![],
            timed_out: false,
            oracle_stats: None,
        }
    }

    // -----------------------------------------------------------------
    // str-jeen.4: artifact-reference contract tests
    // -----------------------------------------------------------------

    fn write_dummy_artifact(root: &std::path::Path, relpath: &str) {
        let abs = root.join(relpath);
        if let Some(parent) = abs.parent() {
            std::fs::create_dir_all(parent).expect("artifact parent");
        }
        std::fs::write(&abs, b"{}").expect("write dummy artifact");
    }

    fn make_summary(file: &str, entries: Vec<ExploreSummaryEntry>) -> ExploreSummary {
        ExploreSummary {
            version: EXPLORE_ARTIFACT_VERSION,
            status: "completed".to_string(),
            file: file.to_string(),
            total_functions: entries.len(),
            functions: entries,
            ..Default::default()
        }
    }

    #[test]
    fn unavailable_reason_token_is_stable() {
        // Downstream parsers depend on these literal strings; keep them
        // anchored even if the variant order changes.
        assert_eq!(
            UnavailableReason::BuildFailed.as_token(),
            "spec_not_produced_due_to_build_failed"
        );
        assert_eq!(
            UnavailableReason::TimedOut.as_token(),
            "spec_not_produced_due_to_timed_out"
        );
        assert_eq!(
            UnavailableReason::WriteFailed.as_token(),
            "artifact_write_failed"
        );
    }

    #[test]
    fn entry_helpers_enforce_mutex_invariant() {
        let avail = ExploreSummaryEntry::available(
            "f".into(),
            "completed".into(),
            "src.ts/00010_f.json".into(),
            None,
            None,
        );
        assert!(avail.artifact.is_some());

        let unav = ExploreSummaryEntry::unavailable(
            "g".into(),
            "failed".into(),
            UnavailableReason::BuildFailed,
            Some("compiler exit 1".into()),
            None,
        );
        assert!(unav.artifact.is_none());
        let reason = unav.reason.expect("unavailable() always populates reason");
        assert!(reason.contains(UnavailableReason::BuildFailed.as_token()));
        assert!(reason.contains("compiler exit 1"));
    }

    #[test]
    fn validator_clean_when_artifact_present_and_referenced() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_dummy_artifact(dir.path(), "src.ts/00010_load.json");
        let summary = make_summary(
            "src.ts",
            vec![ExploreSummaryEntry::available(
                "load".into(),
                "completed".into(),
                "src.ts/00010_load.json".into(),
                None,
                None,
            )],
        );
        write_explore_summary(dir.path(), "src.ts", &summary).expect("write summary");
        let report = validate_artifact_references(dir.path(), &[summary]);
        assert!(
            report.is_clean(),
            "healthy artifact dir must validate clean, got {:?}",
            report.issues
        );
    }

    #[test]
    fn validator_flags_missing_artifact_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        // Note: no file written for the referenced artifact.
        let summary = make_summary(
            "src.ts",
            vec![ExploreSummaryEntry::available(
                "load".into(),
                "completed".into(),
                "src.ts/00010_load.json".into(),
                None,
                None,
            )],
        );
        let report = validate_artifact_references(dir.path(), &[summary]);
        assert!(
            report
                .issues
                .iter()
                .any(|i| matches!(i, ArtifactValidationIssue::MissingArtifact { .. })),
            "missing artifact path must be reported, got {:?}",
            report.issues
        );
    }

    #[test]
    fn validator_flags_unavailable_without_reason() {
        // Construct a hand-rolled invalid entry (bypassing the helpers) to
        // simulate a legacy artifact that pre-dates the contract.
        let dir = tempfile::tempdir().expect("tempdir");
        let entry = ExploreSummaryEntry {
            function_name: "g".into(),
            status: "failed".into(),
            artifact: None,
            reason: None,
            deep_fingerprint: None,
            line_count: 0,
            covered_lines: 0,
        };
        let summary = make_summary("src.ts", vec![entry]);
        let report = validate_artifact_references(dir.path(), &[summary]);
        assert!(
            report
                .issues
                .iter()
                .any(|i| matches!(i, ArtifactValidationIssue::MissingUnavailableReason { .. })),
            "entry with neither artifact nor reason must be reported, got {:?}",
            report.issues
        );
    }

    #[test]
    fn validator_reports_stale_extras() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_dummy_artifact(dir.path(), "src.ts/00010_load.json");
        write_dummy_artifact(dir.path(), "src.ts/00099_orphan.json");
        let summary = make_summary(
            "src.ts",
            vec![ExploreSummaryEntry::available(
                "load".into(),
                "completed".into(),
                "src.ts/00010_load.json".into(),
                None,
                None,
            )],
        );
        let report = validate_artifact_references(dir.path(), &[summary]);
        let stale: Vec<_> = report
            .issues
            .iter()
            .filter_map(|i| match i {
                ArtifactValidationIssue::StaleExtra { absolute_path } => Some(absolute_path),
                _ => None,
            })
            .collect();
        assert_eq!(
            stale.len(),
            1,
            "exactly one stale extra expected, got {:?}",
            report.issues
        );
        assert!(
            stale[0].ends_with("00099_orphan.json"),
            "stale extra must point at the orphan file, got {:?}",
            stale[0]
        );
    }

    #[test]
    fn per_target_check_does_not_flag_sibling_target_artifacts_as_stale() {
        // Per-target validation must NOT walk the whole artifact_root for
        // stale extras — sibling targets share the directory. Only the
        // run-end finalize sweep should report stale extras.
        let dir = tempfile::tempdir().expect("tempdir");
        write_dummy_artifact(dir.path(), "src.ts/00010_load.json");
        write_dummy_artifact(dir.path(), "other.ts/00020_save.json"); // sibling
        let summary = make_summary(
            "src.ts",
            vec![ExploreSummaryEntry::available(
                "load".into(),
                "completed".into(),
                "src.ts/00010_load.json".into(),
                None,
                None,
            )],
        );
        let mut report = ArtifactValidationReport::default();
        check_summary_paths(dir.path(), std::slice::from_ref(&summary), &mut report);
        assert!(
            report.is_clean(),
            "per-target check must ignore sibling artifacts, got {:?}",
            report.issues
        );
    }

    #[test]
    fn validator_unavailable_entry_does_not_falsely_flag_referenced_files() {
        // An entry that legitimately has no artifact (build failed) plus a
        // sibling completed artifact: the validator must treat the sibling as
        // referenced and not surface either as stale or missing.
        let dir = tempfile::tempdir().expect("tempdir");
        write_dummy_artifact(dir.path(), "src.ts/00010_load.json");
        let entries = vec![
            ExploreSummaryEntry::available(
                "load".into(),
                "completed".into(),
                "src.ts/00010_load.json".into(),
                None,
                None,
            ),
            ExploreSummaryEntry::unavailable(
                "save".into(),
                "failed".into(),
                UnavailableReason::BuildFailed,
                Some("rustc exit 101".into()),
                None,
            ),
        ];
        let summary = make_summary("src.ts", entries);
        let report = validate_artifact_references(dir.path(), &[summary]);
        assert!(
            report.is_clean(),
            "mixed available+unavailable summary should validate clean, got {:?}",
            report.issues
        );
    }

    // -------------------------------------------------------------------------
    // str-jeen.25: frontend-agnostic no-target classifier tests
    // -------------------------------------------------------------------------

    use super::HEADER_SCAN_BYTE_CAP;
    use shatter_core::config::ProjectConfig;
    use shatter_core::protocol::NoTargetReason;

    /// Helper: write `contents` into `path` (creating parents) for test fixtures.
    fn write_fixture(path: &std::path::Path, contents: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent");
        }
        std::fs::write(path, contents).expect("write fixture");
    }

    #[test]
    fn matches_policy_exclude_uses_project_cfg_exclude_globs() {
        // str-jeen.25: shatter.config.json `exclude` globs match
        // project-root-relative paths and produce policy_excluded.
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        let file = root.join("vendor/foo.ts");
        write_fixture(&file, "export const x = 1;");

        let cfg = ProjectConfig {
            exclude: vec!["vendor/**".to_string()],
            ..ProjectConfig::default()
        };

        assert!(matches_policy_exclude(&file, Some(root), Some(&cfg)));

        // Unrelated path is not matched.
        let other = root.join("src/foo.ts");
        write_fixture(&other, "export const x = 1;");
        assert!(!matches_policy_exclude(&other, Some(root), Some(&cfg)));
    }

    #[test]
    fn matches_policy_exclude_reads_shatterignore_at_project_root() {
        // str-jeen.25: `.shatterignore` at the project root is honored by
        // the cross-cutting precheck.
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        let file = root.join("legacy/old.ts");
        write_fixture(&file, "export const x = 1;");
        write_fixture(
            &root.join(".shatterignore"),
            "# comment\nlegacy/**\n\n# another comment\n",
        );

        assert!(matches_policy_exclude(&file, Some(root), None));

        // Files outside the ignored glob are not matched.
        let other = root.join("src/foo.ts");
        write_fixture(&other, "export const x = 1;");
        assert!(!matches_policy_exclude(&other, Some(root), None));
    }

    #[test]
    fn matches_policy_exclude_returns_false_without_project_root() {
        // No project root → no anchor for relative-path glob matching.
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("foo.ts");
        write_fixture(&file, "export const x = 1;");
        let cfg = ProjectConfig {
            exclude: vec!["**/*.ts".to_string()],
            ..ProjectConfig::default()
        };
        assert!(!matches_policy_exclude(&file, None, Some(&cfg)));
    }

    #[test]
    fn matches_generated_schema_filename_infixes() {
        // Each documented filename infix tags the file; superficially
        // similar names without the punctuated infix do not.
        for name in &[
            "schema.gen.ts",
            "service.pb.go",
            "wire_pb.go",
            "bindings_generated.rs",
            "types.generated.ts",
        ] {
            assert!(
                matches_generated_schema(std::path::Path::new(name)),
                "expected match for {name}"
            );
        }
        for name in &["generator.ts", "gen.ts", "regen.go", "pbcat.rs"] {
            assert!(
                !matches_generated_schema(std::path::Path::new(name)),
                "expected NO match for {name}"
            );
        }
    }

    #[test]
    fn matches_generated_schema_directory_segments() {
        // Path segments equal to a generated-dir token tag the file;
        // partial matches like `regen` or `generator` do not.
        for path in &[
            "src/generated/api.ts",
            "lib/codegen/types.go",
            "app/__generated__/foo.ts",
            "build/gen/spec.rs",
        ] {
            assert!(
                matches_generated_schema(std::path::Path::new(path)),
                "expected match for {path}"
            );
        }
        for path in &[
            "src/regenerator/foo.ts",
            "lib/generators/types.go",
            "app/general/foo.ts",
        ] {
            assert!(
                !matches_generated_schema(std::path::Path::new(path)),
                "expected NO match for {path}"
            );
        }
    }

    #[test]
    fn matches_generated_schema_leading_comment_markers() {
        // Each documented marker in the leading 512 bytes triggers a
        // match. A marker that appears past the byte cap or past a
        // blank-line boundary does NOT match.
        let dir = tempfile::tempdir().expect("tempdir");

        for (i, marker) in ["DO NOT EDIT", "Code generated by tool", "@generated"]
            .iter()
            .enumerate()
        {
            let path = dir.path().join(format!("h{i}.ts"));
            write_fixture(&path, &format!("// {marker}\nexport const x = 1;\n"));
            assert!(
                leading_bytes_match_generated_marker(&path),
                "expected match for marker `{marker}`"
            );
        }

        // Marker past blank-line boundary is ignored.
        let p = dir.path().join("post_blank.ts");
        write_fixture(&p, "// header\n\n// @generated\nexport const x = 1;\n");
        assert!(!leading_bytes_match_generated_marker(&p));

        // Marker past the byte cap is ignored.
        let p = dir.path().join("past_cap.ts");
        let filler = "// ".to_string() + &"a".repeat(HEADER_SCAN_BYTE_CAP) + "\n@generated\n";
        write_fixture(&p, &filler);
        assert!(!leading_bytes_match_generated_marker(&p));

        // Plain hand-written file is not matched.
        let p = dir.path().join("plain.ts");
        write_fixture(&p, "// my header\nexport const x = 1;\n");
        assert!(!leading_bytes_match_generated_marker(&p));

        // Missing file is treated as not-matched (analyze step will
        // produce its own classification).
        assert!(!leading_bytes_match_generated_marker(
            &dir.path().join("nonexistent.ts")
        ));
    }

    #[test]
    fn pre_classify_policy_wins_over_generated_schema() {
        // Precedence: a file matching BOTH user policy and the
        // generated-schema heuristic gets `policy_excluded` because user
        // intent is the strongest signal.
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        // File matches generated-schema (filename infix) AND policy glob.
        let file = root.join("src/api.gen.ts");
        write_fixture(&file, "// @generated\nexport const x = 1;\n");

        let cfg = ProjectConfig {
            exclude: vec!["src/**".to_string()],
            ..ProjectConfig::default()
        };

        assert_eq!(
            pre_classify_no_target_reason(&file, Some(root), Some(&cfg)),
            Some(NoTargetReason::PolicyExcluded),
        );
    }

    #[test]
    fn pre_classify_returns_generated_schema_when_no_policy_matches() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        let file = root.join("src/api.gen.ts");
        write_fixture(&file, "export const x = 1;\n");

        assert_eq!(
            pre_classify_no_target_reason(&file, Some(root), None),
            Some(NoTargetReason::GeneratedSchema),
        );
    }

    #[test]
    fn pre_classify_returns_none_for_ordinary_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        let file = root.join("src/foo.ts");
        write_fixture(&file, "export const x = 1;\n");

        assert!(pre_classify_no_target_reason(&file, Some(root), None).is_none());
    }

    #[test]
    fn build_skip_summary_carries_reason_and_file() {
        // Stub summary built for skipped/parser-failed files must carry
        // the reason on the wire and zero out every bucket counter so
        // the markdown renderer treats it as a no-target row.
        let summary = build_skip_summary("src/empty.ts", NoTargetReason::PolicyExcluded);
        assert_eq!(summary.file, "src/empty.ts");
        assert_eq!(
            summary.no_target_reason,
            Some(NoTargetReason::PolicyExcluded)
        );
        assert_eq!(summary.total_functions, 0);
        assert_eq!(summary.completed, 0);
        assert_eq!(summary.failed, 0);
        assert_eq!(summary.skipped, 0);
        assert_eq!(summary.build_failed, 0);
        assert_eq!(summary.runtime_failed, 0);
        assert_eq!(summary.timed_out, 0);
        assert_eq!(summary.unsupported, 0);
        assert_eq!(summary.skipped_by_policy, 0);
        assert_eq!(summary.produced_coverage, 0);

        // Round-trips through serde with the stable snake_case token.
        let json = serde_json::to_string(&summary).expect("serialize");
        assert!(json.contains("\"no_target_reason\":\"policy_excluded\""));
    }

    #[test]
    fn build_skip_summary_writes_to_artifact_dir() {
        // The stub must round-trip through `write_explore_summary` /
        // `read_explore_summary` so a re-run sees the prior state.
        let dir = tempfile::tempdir().expect("tempdir");
        let summary = build_skip_summary("src/x.ts", NoTargetReason::ParserFailure);
        super::write_explore_summary(dir.path(), "src/x.ts", &summary).expect("write summary");
        let loaded = super::read_explore_summary(dir.path(), "src/x.ts").expect("loaded summary");
        assert_eq!(loaded.no_target_reason, Some(NoTargetReason::ParserFailure));
        assert_eq!(loaded.file, "src/x.ts");
    }

    // ---------------------------------------------------------------
    // str-jeen.24: Rust no-target reason classifier
    // ---------------------------------------------------------------

    /// Helper: write `contents` to `dir/relative` and return the absolute path.
    fn write_rust_fixture(
        dir: &std::path::Path,
        relative: &str,
        contents: &str,
    ) -> std::path::PathBuf {
        let path = dir.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create fixture parent dir");
        }
        std::fs::write(&path, contents).expect("write fixture");
        path
    }

    #[test]
    fn rust_classifier_recognizes_build_script_at_crate_root() {
        // build.rs sitting next to a Cargo.toml is the canonical Cargo
        // build-script convention. Both files must be present.
        let dir = tempfile::tempdir().expect("tempdir");
        write_rust_fixture(dir.path(), "Cargo.toml", "[package]\nname = \"x\"\n");
        let build_rs = write_rust_fixture(dir.path(), "build.rs", "fn main() {}\n");
        assert_eq!(
            rust_classify_no_target_reason(&build_rs),
            Some(NoTargetReason::BuildScript),
        );
        assert!(rust_is_build_script(&build_rs));
    }

    #[test]
    fn rust_classifier_does_not_misclassify_build_rs_without_manifest() {
        // A `build.rs` deep in a fixtures tree without a sibling
        // Cargo.toml must NOT classify as build_script — the planning
        // notes explicitly call this out.
        let dir = tempfile::tempdir().expect("tempdir");
        let stray = write_rust_fixture(dir.path(), "fixtures/nested/build.rs", "fn main() {}\n");
        assert!(!rust_is_build_script(&stray));
        // The content scan sees a fn body, so it falls all the way
        // through to None (caller emits Unclassified).
        assert_eq!(rust_classify_no_target_reason(&stray), None);
    }

    #[test]
    fn rust_classifier_recognizes_integration_tests_directory() {
        // Cargo's `tests/*.rs` integration-test convention: any file
        // with a `tests` path segment classifies as test_module.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write_rust_fixture(
            dir.path(),
            "crate/tests/integration.rs",
            "#[test] fn t() {}\n",
        );
        assert!(rust_is_test_module_by_path(&path));
        assert_eq!(
            rust_classify_no_target_reason(&path),
            Some(NoTargetReason::TestModule),
        );
    }

    #[test]
    fn rust_classifier_recognizes_test_filename_suffix() {
        // `_test.rs` and `_tests.rs` suffixes are conventional Rust
        // test-module names even outside the integration-tests dir.
        let dir = tempfile::tempdir().expect("tempdir");
        let path_singular =
            write_rust_fixture(dir.path(), "src/foo_test.rs", "#[test] fn t() {}\n");
        let path_plural = write_rust_fixture(dir.path(), "src/foo_tests.rs", "#[test] fn t() {}\n");
        assert!(rust_is_test_module_by_path(&path_singular));
        assert!(rust_is_test_module_by_path(&path_plural));
        assert_eq!(
            rust_classify_no_target_reason(&path_singular),
            Some(NoTargetReason::TestModule),
        );
    }

    #[test]
    fn rust_classifier_recognizes_declaration_only_lib() {
        // A re-export-only lib.rs is the canonical declaration_only
        // case. Comments, attributes, and visibility modifiers are
        // permitted; no item bodies appear.
        let source = "//! Crate root.\n\
            #![warn(missing_docs)]\n\
            \n\
            pub mod a;\n\
            pub mod b;\n\
            pub use a::Thing;\n\
            pub use b::*;\n\
            extern crate alloc;\n";
        assert!(rust_is_declaration_only(source));
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write_rust_fixture(dir.path(), "src/lib.rs", source);
        assert_eq!(
            rust_classify_no_target_reason(&path),
            Some(NoTargetReason::DeclarationOnly),
        );
    }

    #[test]
    fn rust_classifier_rejects_inline_module_body_as_declaration_only() {
        // `mod x { ... }` opens a body — not a pure declaration. The
        // conservative scanner returns false rather than mislabel.
        let source = "pub mod x {\n    pub fn y() {}\n}\n";
        assert!(!rust_is_declaration_only(source));
    }

    #[test]
    fn rust_classifier_macro_only_file_returns_unclassified_not_declaration_only() {
        // Per team-lead's note 2: macro-heavy files (e.g. include!,
        // pub use crate::m::*; with macro re-exports) must not be
        // misclassified as declaration_only. A bare macro invocation
        // at top level is conservatively rejected.
        let source = "include!(\"generated.rs\");\n";
        assert!(!rust_is_declaration_only(source));
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write_rust_fixture(dir.path(), "src/macros.rs", source);
        // Falls through every Rust-specific check — caller emits
        // Unclassified, which the rust classifier surfaces by
        // returning None.
        assert_eq!(rust_classify_no_target_reason(&path), None);
    }

    #[test]
    fn rust_classifier_content_fallback_recognizes_cfg_test_only_module() {
        // A file whose every item is gated on cfg(test) classifies as
        // test_module via the content fallback even when the path
        // doesn't match `tests/` or a `_test.rs` suffix.
        let source = "#[cfg(test)]\n\
            mod inner;\n\
            #[cfg(test)]\n\
            #[test]\n\
            fn smoke() {}\n";
        assert!(rust_is_test_module_by_content(source));
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write_rust_fixture(dir.path(), "src/random_name.rs", source);
        assert_eq!(
            rust_classify_no_target_reason(&path),
            Some(NoTargetReason::TestModule),
        );
    }

    #[test]
    fn rust_classifier_returns_none_for_ungated_top_level_fn() {
        // A top-level non-test fn must NOT classify as test_module via
        // content fallback — the heuristic is conservative.
        let source = "pub fn public_api() {}\n";
        assert!(!rust_is_test_module_by_content(source));
        assert!(!rust_is_declaration_only(source));
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write_rust_fixture(dir.path(), "src/normal.rs", source);
        assert_eq!(rust_classify_no_target_reason(&path), None);
    }

    #[test]
    fn rust_classifier_check_order_build_script_wins_over_test_module_path() {
        // Putting a `build.rs` under a `tests/` segment is pathological,
        // but the documented order says build_script comes first.
        // Cargo doesn't recognize this as a build script (no sibling
        // Cargo.toml), so it should still fall through correctly.
        let dir = tempfile::tempdir().expect("tempdir");
        // No Cargo.toml sibling -> build_script check fails ->
        // test_module by path wins because the file sits in tests/.
        let path = write_rust_fixture(dir.path(), "tests/build.rs", "fn main() {}\n");
        assert_eq!(
            rust_classify_no_target_reason(&path),
            Some(NoTargetReason::TestModule),
        );

        // With a sibling Cargo.toml, build_script wins (still under
        // tests/). The order of checks ensures build_script precedence.
        let dir2 = tempfile::tempdir().expect("tempdir");
        write_rust_fixture(dir2.path(), "tests/Cargo.toml", "[package]\nname = \"x\"\n");
        let p2 = write_rust_fixture(dir2.path(), "tests/build.rs", "fn main() {}\n");
        assert_eq!(
            rust_classify_no_target_reason(&p2),
            Some(NoTargetReason::BuildScript),
        );
    }

    #[test]
    fn rust_classifier_returns_none_on_io_error() {
        // Nonexistent file: build_script and test_module-by-path both
        // require deterministic predicates over the path, so they may
        // still trigger; but a path with no `tests/` segment, no
        // build.rs basename, and no readable bytes must return None.
        let path = std::path::PathBuf::from("/nonexistent/dir/random.rs");
        assert_eq!(rust_classify_no_target_reason(&path), None);
    }

    // Property-based invariants (formal-methods-policy: proptest for
    // the classifier). The grammar is intentionally small — it generates
    // top-level Rust-ish lines from a fixed alphabet so the classifier's
    // contract can be asserted without depending on a full parser.
    proptest::proptest! {
        /// Invariant 1: declaration_only is monotone — adding a pure
        /// declaration line to a declaration-only source preserves the
        /// classification.
        #[test]
        fn declaration_only_is_monotone_under_appending_declarations(
            ref leading in proptest::collection::vec(
                proptest::sample::select(vec![
                    "mod a;",
                    "pub mod b;",
                    "use a::b;",
                    "pub use a::b::*;",
                    "extern crate alloc;",
                    "// comment",
                    "#[allow(dead_code)]",
                    "",
                ]),
                0..16,
            ),
            ref appended in proptest::sample::select(vec![
                "mod c;",
                "pub use d::e;",
                "extern crate core;",
            ]),
        ) {
            let base = leading.join("\n") + "\n";
            let extended = base.clone() + appended + "\n";
            // If the base classifies as declaration_only, the extended
            // version (one more pure declaration) must too.
            if rust_is_declaration_only(&base) {
                proptest::prop_assert!(
                    rust_is_declaration_only(&extended),
                    "appending a declaration broke declaration_only:\nbase=\n{base}\nappended={appended}",
                );
            }
        }

        /// Invariant 2: a top-level non-test fn body never classifies
        /// as declaration_only or test_module.
        #[test]
        fn non_test_fn_never_classifies_as_declaration_or_test(
            ref prefix in proptest::collection::vec(
                proptest::sample::select(vec![
                    "mod a;",
                    "use b::c;",
                    "// trailing comment",
                    "",
                ]),
                0..8,
            ),
        ) {
            // Synthesize a file with a real top-level fn body. Any
            // such file MUST NOT classify as declaration_only or as
            // test_module by content.
            let source = format!(
                "{}\npub fn definitely_real() {{ let _ = 1 + 1; }}\n",
                prefix.join("\n"),
            );
            proptest::prop_assert!(!rust_is_declaration_only(&source));
            proptest::prop_assert!(!rust_is_test_module_by_content(&source));
        }

        /// Invariant 3: build.rs at a crate root (sibling Cargo.toml)
        /// always classifies as build_script regardless of contents.
        #[test]
        fn build_rs_with_sibling_manifest_always_classifies_as_build_script(
            ref body in "[a-zA-Z0-9 \\n_(){}]{0,200}",
        ) {
            let dir = tempfile::tempdir().expect("tempdir");
            write_rust_fixture(dir.path(), "Cargo.toml", "[package]\nname = \"x\"\n");
            let path = write_rust_fixture(dir.path(), "build.rs", body);
            proptest::prop_assert_eq!(
                rust_classify_no_target_reason(&path),
                Some(NoTargetReason::BuildScript),
            );
        }
    }

    // ── TypeScript no-target classifier (str-jeen.22) ────────────────────

    /// Helper: write `contents` to `dir/relative` and return the
    /// absolute path. Mirrors `write_rust_fixture` for symmetry.
    fn write_ts_fixture(
        dir: &std::path::Path,
        relative: &str,
        contents: &str,
    ) -> std::path::PathBuf {
        let path = dir.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create fixture parent dir");
        }
        std::fs::write(&path, contents).expect("write fixture");
        path
    }

    #[test]
    fn ts_classifier_recognizes_declaration_file_by_extension() {
        // `.d.ts` is the canonical TS ambient declaration extension.
        // The classifier must accept the path-based signal regardless
        // of contents — ambient files often contain only types.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write_ts_fixture(
            dir.path(),
            "src/types.d.ts",
            "export interface User { id: string; name: string; }\n",
        );
        assert!(ts_is_declaration_file_by_path(&path));
        assert_eq!(
            ts_classify_no_target_reason(&path),
            Some(NoTargetReason::DeclarationOnly),
        );
    }

    #[test]
    fn ts_classifier_recognizes_module_flavored_declaration_extensions() {
        // `.d.cts` and `.d.mts` are CommonJS / ESM declaration variants.
        let dir = tempfile::tempdir().expect("tempdir");
        let cts = write_ts_fixture(dir.path(), "src/cjs-types.d.cts", "export {};\n");
        let mts = write_ts_fixture(dir.path(), "src/esm-types.d.mts", "export {};\n");
        assert!(ts_is_declaration_file_by_path(&cts));
        assert!(ts_is_declaration_file_by_path(&mts));
        assert_eq!(
            ts_classify_no_target_reason(&cts),
            Some(NoTargetReason::DeclarationOnly),
        );
        assert_eq!(
            ts_classify_no_target_reason(&mts),
            Some(NoTargetReason::DeclarationOnly),
        );
    }

    #[test]
    fn ts_classifier_recognizes_test_filename_infixes() {
        // `.test.`, `.spec.`, and `.stories.` infixes mark Jest/Vitest/
        // Storybook files. All three classify as test_or_spec.
        let dir = tempfile::tempdir().expect("tempdir");
        let test_path = write_ts_fixture(
            dir.path(),
            "src/foo.test.ts",
            "describe('foo', () => { it('bar', () => {}); });\n",
        );
        let spec_path = write_ts_fixture(
            dir.path(),
            "src/foo.spec.tsx",
            "describe('foo', () => {});\n",
        );
        let story_path = write_ts_fixture(
            dir.path(),
            "src/Button.stories.tsx",
            "export default { component: Button };\n",
        );
        for path in [&test_path, &spec_path, &story_path] {
            assert!(
                ts_is_test_or_spec_by_path(path),
                "expected test_or_spec by path: {path:?}",
            );
            assert_eq!(
                ts_classify_no_target_reason(path),
                Some(NoTargetReason::TestOrSpec),
            );
        }
    }

    #[test]
    fn ts_classifier_recognizes_test_directory_segments() {
        // Files under `__tests__`, `__mocks__`, or `tests` segments
        // classify regardless of basename.
        let dir = tempfile::tempdir().expect("tempdir");
        for relative in [
            "src/__tests__/helper.ts",
            "src/__mocks__/api.ts",
            "tests/integration.ts",
        ] {
            let path = write_ts_fixture(dir.path(), relative, "export const x = 1;\n");
            assert!(
                ts_is_test_or_spec_by_path(&path),
                "expected test segment match: {relative}",
            );
            assert_eq!(
                ts_classify_no_target_reason(&path),
                Some(NoTargetReason::TestOrSpec),
            );
        }
    }

    #[test]
    fn ts_classifier_recognizes_jsx_component_only_tsx_file() {
        // A `.tsx` file with a JSX closing tag classifies as
        // jsx_component_only — the analyzer found zero callable targets
        // but the file is clearly a component module.
        let dir = tempfile::tempdir().expect("tempdir");
        let source = "import React from 'react';\n\
            export const Badge = ({ label }: { label: string }) => (\n\
            \x20 <span className=\"badge\">{label}</span>\n\
            );\n";
        let path = write_ts_fixture(dir.path(), "src/Badge.tsx", source);
        assert!(ts_contains_jsx_component(source));
        assert_eq!(
            ts_classify_no_target_reason(&path),
            Some(NoTargetReason::JsxComponentOnly),
        );
    }

    #[test]
    fn ts_classifier_does_not_misclassify_ts_generics_as_jsx() {
        // A plain `.ts` file using TS generics like `Array<number>` must
        // NOT classify as jsx_component_only — only `.tsx` / `.jsx`
        // files are eligible. Closing-tag form `</` also never appears
        // in TS generics, so the content check stays safe even on a
        // pathological `.tsx`.
        let dir = tempfile::tempdir().expect("tempdir");
        let source = "export type Pair<T> = readonly [T, T];\n\
             export type Numbers = Array<number>;\n";
        let path = write_ts_fixture(dir.path(), "src/generics.ts", source);
        assert!(!ts_contains_jsx_component(source));
        // Pure type-level declarations: declaration_only by content.
        assert_eq!(
            ts_classify_no_target_reason(&path),
            Some(NoTargetReason::DeclarationOnly),
        );
    }

    #[test]
    fn ts_classifier_recognizes_declaration_only_by_content() {
        // A plain `.ts` file with only types/interfaces/imports
        // classifies as declaration_only via the content fallback even
        // without the `.d.ts` extension.
        let source = "// barrel of public types\n\
            import { Brand } from './brand';\n\
            export type UserId = Brand<string, 'UserId'>;\n\
            export interface User {\n\
            \x20 id: UserId;\n\
            \x20 name: string;\n\
            }\n\
            export type { Brand };\n";
        assert!(ts_is_declaration_only(source));
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write_ts_fixture(dir.path(), "src/user.ts", source);
        assert_eq!(
            ts_classify_no_target_reason(&path),
            Some(NoTargetReason::DeclarationOnly),
        );
    }

    #[test]
    fn ts_classifier_rejects_value_definitions_as_declaration_only() {
        // `export const`, `export function`, and `class` are all
        // value-level definitions. Even when the analyzer found zero
        // targets, these files MUST NOT classify as declaration_only —
        // the classifier returns None and the caller emits Unclassified.
        for source in [
            "export const x = 1;\n",
            "export function f() { return 1; }\n",
            "export class C { m() {} }\n",
            "function bare() {}\n",
        ] {
            assert!(
                !ts_is_declaration_only(source),
                "value-level source must not classify as declaration_only: {source}",
            );
        }
    }

    #[test]
    fn ts_classifier_check_order_path_wins_over_content() {
        // A `.d.ts` file whose contents accidentally look value-level
        // still classifies as declaration_only via the path check —
        // path-based signals are checked before any content scan.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write_ts_fixture(
            dir.path(),
            "src/weird.d.ts",
            "// pathological body\nexport const sneaky = 1;\n",
        );
        assert_eq!(
            ts_classify_no_target_reason(&path),
            Some(NoTargetReason::DeclarationOnly),
        );

        // Test path wins over JSX content: a `.test.tsx` inside
        // src/__tests__ with JSX must classify as test_or_spec, not
        // jsx_component_only.
        let test_jsx = write_ts_fixture(
            dir.path(),
            "src/__tests__/Badge.test.tsx",
            "describe('Badge', () => { it('renders', () => { <Badge /> }); });\n",
        );
        assert_eq!(
            ts_classify_no_target_reason(&test_jsx),
            Some(NoTargetReason::TestOrSpec),
        );
    }

    #[test]
    fn ts_classifier_returns_none_for_empty_or_unclassifiable_files() {
        // An empty `.ts` file matches no signal — the classifier returns
        // None and the caller emits Unclassified (the str-jeen.21
        // default).
        let dir = tempfile::tempdir().expect("tempdir");
        let empty = write_ts_fixture(dir.path(), "src/empty.ts", "");
        assert_eq!(ts_classify_no_target_reason(&empty), None);

        // A `.ts` file with both value-level definitions and types
        // is intentionally unclassifiable — the analyzer should have
        // discovered the value targets, so a zero-target outcome here
        // means something else went wrong; caller's Unclassified
        // fallback surfaces it for follow-up.
        let mixed = write_ts_fixture(
            dir.path(),
            "src/mixed.ts",
            "export const x = 1;\nexport type Y = number;\n",
        );
        assert_eq!(ts_classify_no_target_reason(&mixed), None);

        // A `.tsx` file with no JSX closing tag and no declarations
        // (e.g. just a value-level const) also returns None.
        let tsx_no_jsx = write_ts_fixture(
            dir.path(),
            "src/tsx_no_jsx.tsx",
            "export const value = 42;\n",
        );
        assert_eq!(ts_classify_no_target_reason(&tsx_no_jsx), None);
    }

    #[test]
    fn ts_classifier_returns_none_on_io_error() {
        // Nonexistent path with no path-based signal: the source read
        // fails and the function returns None.
        let path = std::path::PathBuf::from("/nonexistent/dir/random.ts");
        assert_eq!(ts_classify_no_target_reason(&path), None);
    }

    #[test]
    fn ts_classifier_routed_through_classify_no_target_reason() {
        // Integration with the public dispatcher: a TS path with zero
        // targets and a TS-specific signal must yield the refined
        // variant, not the plain Unclassified fallback.
        let dir = tempfile::tempdir().expect("tempdir");
        let dts = write_ts_fixture(dir.path(), "src/api.d.ts", "export {};\n");
        assert_eq!(
            classify_no_target_reason(0, 0, crate::args::Language::TypeScript, &dts),
            Some(NoTargetReason::DeclarationOnly),
        );
        let test_path =
            write_ts_fixture(dir.path(), "src/foo.spec.ts", "describe('x', () => {});\n");
        assert_eq!(
            classify_no_target_reason(0, 0, crate::args::Language::TypeScript, &test_path),
            Some(NoTargetReason::TestOrSpec),
        );
        let badge = write_ts_fixture(
            dir.path(),
            "src/Badge.tsx",
            "export const Badge = () => <span>hi</span>;\n",
        );
        assert_eq!(
            classify_no_target_reason(0, 0, crate::args::Language::TypeScript, &badge),
            Some(NoTargetReason::JsxComponentOnly),
        );
    }

    proptest::proptest! {
        /// TS Invariant 1: `.d.ts` (and module-flavored variants) under
        /// any directory layout always classify as declaration_only,
        /// regardless of file body. Path-based signal wins over content.
        #[test]
        fn ts_dts_extension_always_classifies_as_declaration_only(
            ref dirseg in proptest::sample::select(vec![
                "src", "lib", "types", "packages/api/src", "vendor",
            ]),
            ref ext in proptest::sample::select(vec![
                ".d.ts", ".d.cts", ".d.mts",
            ]),
            ref body in "[a-zA-Z0-9 \\n_(){};:=]{0,200}",
        ) {
            let dir = tempfile::tempdir().expect("tempdir");
            let relative = format!("{dirseg}/types{ext}");
            let path = write_ts_fixture(dir.path(), &relative, body);
            proptest::prop_assert_eq!(
                ts_classify_no_target_reason(&path),
                Some(NoTargetReason::DeclarationOnly),
            );
        }

        /// TS Invariant 2: a value-level top-level definition never
        /// classifies as declaration_only via the content scan — the
        /// heuristic is conservative.
        #[test]
        fn ts_value_level_source_never_classifies_as_declaration_only(
            ref keyword in proptest::sample::select(vec![
                "export const",
                "export function",
                "export class",
                "export enum",
                "function",
                "class",
                "const",
                "let",
                "var",
            ]),
            ref tail in "[a-zA-Z0-9 ]{0,40}",
        ) {
            let source = format!("{keyword} placeholder{tail} = 1;\n");
            proptest::prop_assert!(!ts_is_declaration_only(&source));
        }
    }

    // ── Go no-target classifier (str-jeen.23) ────────────────────────────

    /// Helper: write `contents` to `dir/relative` and return the
    /// absolute path. Mirrors `write_ts_fixture` for symmetry.
    fn write_go_fixture(
        dir: &std::path::Path,
        relative: &str,
        contents: &str,
    ) -> std::path::PathBuf {
        let path = dir.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create fixture parent dir");
        }
        std::fs::write(&path, contents).expect("write fixture");
        path
    }

    #[test]
    fn go_classifier_recognizes_test_file_by_suffix() {
        // `_test.go` is Go's unambiguous testing suffix consumed by `go
        // test`. The classifier must accept the path-based signal
        // regardless of body contents.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write_go_fixture(
            dir.path(),
            "pkg/widget_test.go",
            "package widget\n\
             import \"testing\"\n\
             func TestThing(t *testing.T) { _ = t }\n",
        );
        assert!(go_is_test_file_by_path(&path));
        assert_eq!(
            go_classify_no_target_reason(&path),
            Some(NoTargetReason::TestFile),
        );
    }

    #[test]
    fn go_classifier_recognizes_generated_marker() {
        // The canonical `// Code generated ... DO NOT EDIT.` marker on
        // its own line before the package clause classifies the file
        // as `generated`.
        let dir = tempfile::tempdir().expect("tempdir");
        let source = "// Code generated by protoc-gen-go. DO NOT EDIT.\n\
                      // versions: protoc-gen-go v1.28.0\n\
                      \n\
                      package fixturepb\n\
                      \n\
                      type Empty struct{}\n";
        let path = write_go_fixture(dir.path(), "pkg/fixture.pb.go", source);
        assert!(go_is_generated_by_content(source));
        assert_eq!(
            go_classify_no_target_reason(&path),
            Some(NoTargetReason::Generated),
        );
    }

    #[test]
    fn go_classifier_ignores_generated_marker_after_package_clause() {
        // The marker must appear before the `package` clause. A line
        // matching the marker text inside a function body or a comment
        // after the package clause must NOT classify the file as
        // generated.
        let source = "package widget\n\
                      \n\
                      // Code generated by hand. DO NOT EDIT.\n\
                      func F() {}\n";
        assert!(!go_is_generated_by_content(source));
    }

    #[test]
    fn go_classifier_recognizes_receiver_method_gap() {
        // A file with one or more methods (`func (r Recv) M()`) and zero
        // free top-level functions classifies as `receiver_method_gap`
        // — the analyzer cannot synthesize an executable target without
        // a constructor for the receiver type.
        let dir = tempfile::tempdir().expect("tempdir");
        let source = "package widget\n\
                      \n\
                      type Counter struct{ n int }\n\
                      \n\
                      func (c *Counter) Inc() { c.n++ }\n\
                      func (c Counter) Value() int { return c.n }\n";
        let path = write_go_fixture(dir.path(), "pkg/counter.go", source);
        assert!(go_is_receiver_method_gap_by_content(source));
        assert_eq!(
            go_classify_no_target_reason(&path),
            Some(NoTargetReason::ReceiverMethodGap),
        );
    }

    #[test]
    fn go_classifier_rejects_receiver_gap_when_free_function_present() {
        // A file with even one free top-level `func Name(...)` MUST NOT
        // classify as `receiver_method_gap` — the analyzer should have
        // discovered a target. Conservative: return None so the caller
        // emits Unclassified rather than a wrong taxonomy slot.
        let source = "package widget\n\
                      \n\
                      type Counter struct{}\n\
                      \n\
                      func (c *Counter) Inc() {}\n\
                      func Helper() int { return 1 }\n";
        assert!(!go_is_receiver_method_gap_by_content(source));
    }

    #[test]
    fn go_classifier_check_order_path_wins_over_content() {
        // A `_test.go` file whose body is also receiver-method-gap-like
        // still classifies as `test_file` via the path check —
        // path-based signals are checked before any content scan.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write_go_fixture(
            dir.path(),
            "pkg/counter_test.go",
            "package widget\n\
             type Counter struct{}\n\
             func (c *Counter) Inc() {}\n",
        );
        assert_eq!(
            go_classify_no_target_reason(&path),
            Some(NoTargetReason::TestFile),
        );

        // A `_test.go` file containing the generated marker still
        // classifies as `test_file`, not `generated`.
        let gen_test = write_go_fixture(
            dir.path(),
            "pkg/codegen_test.go",
            "// Code generated by tool. DO NOT EDIT.\n\
             package widget\n\
             import \"testing\"\n\
             func TestX(t *testing.T) {}\n",
        );
        assert_eq!(
            go_classify_no_target_reason(&gen_test),
            Some(NoTargetReason::TestFile),
        );
    }

    #[test]
    fn go_classifier_returns_none_for_unclassifiable_files() {
        // An empty `.go` file matches no signal — the classifier
        // returns None and the caller emits Unclassified (str-jeen.21
        // default).
        let dir = tempfile::tempdir().expect("tempdir");
        let empty = write_go_fixture(dir.path(), "pkg/empty.go", "");
        assert_eq!(go_classify_no_target_reason(&empty), None);

        // A file with only declarations (no methods, no free funcs)
        // also returns None.
        let decls_only = write_go_fixture(
            dir.path(),
            "pkg/decls.go",
            "package widget\n\
             type T struct{ x int }\n\
             const K = 1\n",
        );
        assert_eq!(go_classify_no_target_reason(&decls_only), None);
    }

    #[test]
    fn go_classifier_returns_none_on_io_error() {
        // Nonexistent path with no path-based signal: the source read
        // fails and the function returns None.
        let path = std::path::PathBuf::from("/nonexistent/dir/random.go");
        assert_eq!(go_classify_no_target_reason(&path), None);
    }

    #[test]
    fn go_classifier_routed_through_classify_no_target_reason() {
        // Integration with the public dispatcher: a Go path with zero
        // targets and a Go-specific signal must yield the refined
        // variant, not the plain Unclassified fallback.
        let dir = tempfile::tempdir().expect("tempdir");

        let test_path = write_go_fixture(
            dir.path(),
            "pkg/foo_test.go",
            "package foo\nimport \"testing\"\nfunc TestA(t *testing.T) {}\n",
        );
        assert_eq!(
            classify_no_target_reason(0, 0, crate::args::Language::Go, &test_path),
            Some(NoTargetReason::TestFile),
        );

        let generated = write_go_fixture(
            dir.path(),
            "pkg/api.pb.go",
            "// Code generated by protoc-gen-go. DO NOT EDIT.\npackage api\n",
        );
        assert_eq!(
            classify_no_target_reason(0, 0, crate::args::Language::Go, &generated),
            Some(NoTargetReason::Generated),
        );

        let methods = write_go_fixture(
            dir.path(),
            "pkg/methods.go",
            "package m\n\
             type S struct{}\n\
             func (s *S) M() {}\n",
        );
        assert_eq!(
            classify_no_target_reason(0, 0, crate::args::Language::Go, &methods),
            Some(NoTargetReason::ReceiverMethodGap),
        );
    }

    proptest::proptest! {
        /// Go Invariant 1: a basename ending in `_test.go` under any
        /// directory layout always classifies as `test_file`,
        /// regardless of body. Path-based signal wins over content.
        #[test]
        fn go_test_suffix_always_classifies_as_test_file(
            ref dirseg in proptest::sample::select(vec![
                "pkg", "internal", "cmd/svc", "vendor/x", "a/b/c",
            ]),
            ref stem in "[a-z][a-z0-9_]{0,20}",
            ref body in "[a-zA-Z0-9 \\n_(){};:=]{0,200}",
        ) {
            let dir = tempfile::tempdir().expect("tempdir");
            let relative = format!("{dirseg}/{stem}_test.go");
            let path = write_go_fixture(dir.path(), &relative, body);
            proptest::prop_assert_eq!(
                go_classify_no_target_reason(&path),
                Some(NoTargetReason::TestFile),
            );
        }

        /// Go Invariant 2: a file containing any free top-level `func
        /// Name(...)` declaration (at depth 0) never classifies as
        /// `receiver_method_gap` — the heuristic is conservative.
        #[test]
        fn go_free_function_source_never_classifies_as_receiver_gap(
            ref name in "[A-Z][a-zA-Z0-9]{0,12}",
        ) {
            let source = format!(
                "package x\n\
                 type T struct{{}}\n\
                 func (t *T) M() {{}}\n\
                 func {name}() int {{ return 1 }}\n",
            );
            proptest::prop_assert!(!go_is_receiver_method_gap_by_content(&source));
        }

        // ----- str-jeen.6 classifier proptests -----

        /// str-jeen.6: every Go reason string maps to some category.
        /// Classification is total; classifier never panics.
        #[test]
        fn classify_go_build_failure_is_total(ref reason in ".{0,200}") {
            let _ = classify_go_build_failure(reason);
        }

        /// str-jeen.6: classification is stable — same input twice
        /// yields the same category.
        #[test]
        fn classify_go_build_failure_is_stable(ref reason in ".{0,200}") {
            let a = classify_go_build_failure(reason);
            let b = classify_go_build_failure(reason);
            proptest::prop_assert_eq!(a as u8, b as u8);
        }

        /// str-jeen.6: every TS reason string maps to some category.
        #[test]
        fn classify_ts_build_failure_is_total(ref reason in ".{0,200}") {
            let _ = classify_ts_build_failure(reason);
        }

        /// str-jeen.6: TS classifier is stable.
        #[test]
        fn classify_ts_build_failure_is_stable(ref reason in ".{0,200}") {
            let a = classify_ts_build_failure(reason);
            let b = classify_ts_build_failure(reason);
            proptest::prop_assert_eq!(a as u8, b as u8);
        }
    }

    // ----- str-jeen.6: new Go categories + TS classifier + rollup -----

    #[test]
    fn classify_go_constructor_arity_distinct_from_missing_import() {
        // Real wording from the Zolem audit: arity miss must NOT collapse
        // into MissingImport, even though both involve "undefined" / call
        // sites. Pin the bucket boundary.
        assert_eq!(
            classify_go_build_failure(
                "execute error: build failed: not enough arguments in call to NewSSEWriter"
            ),
            GoBuildFailureCategory::ConstructorArity,
        );
        assert_eq!(
            classify_go_build_failure("too many arguments in call to MakeThing"),
            GoBuildFailureCategory::ConstructorArity,
        );
        assert_eq!(
            classify_go_build_failure(
                "cannot use ctx (variable of type context.Context) as int value in argument to f"
            ),
            GoBuildFailureCategory::ConstructorArity,
        );
        // A pure "undefined:" with no arity hint stays in MissingImport.
        assert_eq!(
            classify_go_build_failure("undefined: pkg.X"),
            GoBuildFailureCategory::MissingImport,
        );
    }

    #[test]
    fn classify_go_build_vcs_stamping_is_distinct() {
        // Fresh-workspace failure from the Zolem 2026-05-02 evidence.
        assert_eq!(
            classify_go_build_failure("error obtaining VCS status: exit status 128"),
            GoBuildFailureCategory::BuildVcsStamping,
        );
        assert_eq!(
            classify_go_build_failure("error obtaining vcs status: not a git repository"),
            GoBuildFailureCategory::BuildVcsStamping,
        );
        // VCS message that mentions "internal" must still land in
        // BuildVcsStamping because the VCS check fires before package
        // resolution.
        assert_eq!(
            classify_go_build_failure(
                "error obtaining VCS status for internal/foo: not a git repository"
            ),
            GoBuildFailureCategory::BuildVcsStamping,
        );
    }

    #[test]
    fn aggregate_go_root_causes_includes_new_categories() {
        let entries = vec![
            entry_with_lines(
                "Ctor",
                "failed",
                Some(
                    "execute error (InstrumentationFailed): build failed: not enough arguments in call to NewSSEWriter",
                ),
                40,
            ),
            entry_with_lines(
                "Vcs",
                "failed",
                Some(
                    "execute error (InstrumentationFailed): build failed: error obtaining VCS status: exit 128",
                ),
                15,
            ),
        ];
        let breakdown = aggregate_go_root_causes_from_entries(&entries);
        assert_eq!(breakdown.constructor_arity.count, 1);
        assert_eq!(breakdown.constructor_arity.line_weight, 40);
        assert_eq!(breakdown.build_vcs_stamping.count, 1);
        assert_eq!(breakdown.build_vcs_stamping.line_weight, 15);
        let md = format_go_root_causes_md(&breakdown).expect("renders");
        assert!(md.contains("`constructor_arity`"));
        assert!(md.contains("`build_vcs_stamping`"));
        assert!(md.contains("40"));
    }

    #[test]
    fn classify_ts_build_failure_buckets_match_issue_taxonomy() {
        assert_eq!(
            classify_ts_build_failure("ReferenceError: window is not defined"),
            TsBuildFailureCategory::MissingBrowserApi,
        );
        assert_eq!(
            classify_ts_build_failure("Function `tokenizeWords` is not exported from module"),
            TsBuildFailureCategory::PrivateFunctionNotFound,
        );
        assert_eq!(
            classify_ts_build_failure("Error: Cannot find module '@app/foo'"),
            TsBuildFailureCategory::MissingDependencyOrAlias,
        );
        assert_eq!(
            classify_ts_build_failure("SyntaxError: Unexpected token '<'"),
            TsBuildFailureCategory::ParseJsxOrTypeRuntime,
        );
        assert_eq!(
            classify_ts_build_failure("unsupported parameter type: Buffer"),
            TsBuildFailureCategory::UnsupportedParamType,
        );
        // Unrecognized text drops into Other rather than misclassifying.
        assert_eq!(
            classify_ts_build_failure("kernel panic: filesystem corrupt"),
            TsBuildFailureCategory::Other,
        );
    }

    #[test]
    fn aggregate_ts_root_causes_filters_to_ts_files_and_runtime_outcomes() {
        // .ts file with one runtime failure (window) and one build failure
        // (Cannot find module). Both must contribute. A non-TS file with
        // matching reason must NOT contribute.
        let ts_summary = make_summary(
            "src/foo.ts",
            vec![
                entry_with_lines(
                    "OnClick",
                    "failed",
                    Some("ReferenceError: window is not defined"),
                    25,
                ),
                entry_with_lines(
                    "DepX",
                    "failed",
                    Some(
                        "execute error (InstrumentationFailed): build failed: Cannot find module 'lodash'",
                    ),
                    10,
                ),
            ],
        );
        let go_summary = make_summary(
            "src/bar.go",
            vec![entry_with_lines(
                "WinFn",
                "failed",
                Some("ReferenceError: window is not defined"),
                999,
            )],
        );
        let breakdown = aggregate_ts_root_causes(&[ts_summary, go_summary]);
        assert_eq!(breakdown.missing_browser_api.count, 1);
        assert_eq!(breakdown.missing_browser_api.line_weight, 25);
        assert_eq!(breakdown.missing_dependency_or_alias.count, 1);
        assert_eq!(breakdown.missing_dependency_or_alias.line_weight, 10);
        let md = format_ts_root_causes_md(&breakdown).expect("renders");
        assert!(md.contains("`missing_browser_api`"));
        assert!(md.contains("`missing_dependency_or_alias`"));
        // Empty breakdown returns None.
        assert!(format_ts_root_causes_md(&TsRootCauseBreakdown::default()).is_none());
    }

    #[test]
    fn failure_impact_rollup_carries_affected_files_and_percent() {
        // Two Go files and one TS file with mixed outcomes. Rollup must:
        // - Emit a row per non-empty (language, category) and per outcome
        //   bucket.
        // - Sort by affected_function_span_lines desc.
        // - Compute affected_files distinctly per row.
        // - Compute selected_source_lines from
        //   discovered_function_span_lines on the affected files.
        // - Compute percent_attempted_span_lines against the run-wide
        //   attempted_function_span_lines sum.
        let mut go1 = make_summary(
            "src/a.go",
            vec![
                entry_with_lines(
                    "ArityA",
                    "failed",
                    Some(
                        "execute error (InstrumentationFailed): build failed: not enough arguments in call to NewX",
                    ),
                    50,
                ),
                entry_with_lines(
                    "ArityB",
                    "failed",
                    Some(
                        "execute error (InstrumentationFailed): build failed: not enough arguments in call to NewY",
                    ),
                    30,
                ),
            ],
        );
        // discovered_function_span_lines for the file (used as
        // selected_source_lines denominator). 80 from arity functions
        // plus 20 imaginary "other discovered" lines so the file's
        // selected count exceeds the affected count.
        go1.discovered_function_span_lines = 100;
        go1.attempted_function_span_lines = 80;

        let mut go2 = make_summary(
            "src/b.go",
            vec![entry_with_lines(
                "VcsB",
                "failed",
                Some(
                    "execute error (InstrumentationFailed): build failed: error obtaining VCS status",
                ),
                15,
            )],
        );
        go2.discovered_function_span_lines = 60;
        go2.attempted_function_span_lines = 15;

        let mut ts1 = make_summary(
            "src/c.ts",
            vec![entry_with_lines(
                "WinC",
                "failed",
                Some("ReferenceError: window is not defined"),
                5,
            )],
        );
        ts1.discovered_function_span_lines = 40;
        ts1.attempted_function_span_lines = 5;

        let rows = aggregate_failure_impact(&[go1, go2, ts1]);

        // Must contain the per-language root-cause rows.
        let arity = rows
            .iter()
            .find(|r| r.language == "go" && r.category == "constructor_arity")
            .expect("constructor_arity row");
        assert_eq!(arity.affected_functions, 2);
        assert_eq!(arity.affected_files, 1);
        assert_eq!(arity.affected_function_span_lines, 80);
        assert_eq!(arity.selected_source_lines, 100);
        // 80 / (80 + 15 + 5) = 80%
        assert!((arity.percent_attempted_span_lines - 80.0).abs() < 0.01);

        let vcs = rows
            .iter()
            .find(|r| r.language == "go" && r.category == "build_vcs_stamping")
            .expect("vcs row");
        assert_eq!(vcs.affected_functions, 1);
        assert_eq!(vcs.affected_files, 1);
        assert_eq!(vcs.affected_function_span_lines, 15);
        assert_eq!(vcs.selected_source_lines, 60);

        let win = rows
            .iter()
            .find(|r| r.language == "ts" && r.category == "missing_browser_api")
            .expect("ts row");
        assert_eq!(win.affected_function_span_lines, 5);
        assert_eq!(win.selected_source_lines, 40);

        // Outcome-only rows for "any" language: a `build_failed` rollup
        // covers all three Go arity entries + the Go VCS entry + the TS
        // window entry (window is RuntimeFailed, so it lands in the
        // runtime_failed bucket, not build_failed).
        let any_build = rows
            .iter()
            .find(|r| r.language == "any" && r.category == "build_failed")
            .expect("any/build_failed row");
        // Three build failures across two files (a.go arity x2, b.go vcs).
        assert_eq!(any_build.affected_functions, 3);
        assert_eq!(any_build.affected_files, 2);
        assert_eq!(any_build.affected_function_span_lines, 95);
        // selected_source_lines == sum of discovered span lines on
        // distinct affected files: 100 (a.go) + 60 (b.go) = 160.
        assert_eq!(any_build.selected_source_lines, 160);

        // Sort: the largest span row comes first.
        assert!(
            rows[0].affected_function_span_lines
                >= rows[rows.len() - 1].affected_function_span_lines
        );

        // Markdown render.
        let md = format_failure_impact_md(&rows).expect("non-empty rollup renders");
        assert!(md.contains("Failure impact"));
        assert!(md.contains("`constructor_arity`"));
        assert!(md.contains("`missing_browser_api`"));
        assert!(md.contains("80.0%"));

        // Empty rollup returns None.
        assert!(format_failure_impact_md(&[]).is_none());
    }

    #[test]
    fn failure_impact_rollup_handles_zero_attempted_span_lines() {
        // Edge case: if no run attempted any span lines, the percent
        // denominator is undefined. Output must be 0.0, not NaN, so
        // downstream JSON consumers don't choke.
        let summary = make_summary(
            "src/empty.go",
            vec![entry_with_lines("Stub", "skipped", Some("policy"), 0)],
        );
        let rows = aggregate_failure_impact(&[summary]);
        for row in &rows {
            assert!(
                row.percent_attempted_span_lines.is_finite(),
                "percent must be finite for {row:?}"
            );
        }
    }
}
