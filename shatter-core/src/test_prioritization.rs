//! Greedy submodular test case prioritization.
//!
//! Orders tests to maximize marginal coverage per unit of execution time.
//! The algorithm is a standard greedy approximation for submodular set cover:
//! at each step, pick the test with the highest ratio of *new lines covered*
//! to *execution time*, mark those lines covered, and repeat.
//!
//! Supports optional change-recency weighting (recent git commits boost file
//! importance) and time-budget truncation (`--budget`).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::test_impact::CoverageMap;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Default time budget when none is specified (0 means unlimited).
pub const DEFAULT_BUDGET_SECS: u64 = 0;

/// Half-life in days for recency weighting decay.
/// Files modified within this many days receive ≥50% of max weight.
pub const RECENCY_HALF_LIFE_DAYS: f64 = 14.0;

/// Minimum recency weight (floor) so old files are never completely ignored.
pub const MIN_RECENCY_WEIGHT: f64 = 0.1;

/// Seconds per day (used for recency decay calculation).
const SECS_PER_DAY: f64 = 86400.0;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors from test prioritization.
#[derive(Debug, thiserror::Error)]
pub enum PrioritizeError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid budget format: {0}")]
    InvalidBudget(String),
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// A test case with its coverage footprint and timing metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TestCase {
    /// Unique test identifier (e.g. "crate::module::test_fn" or "shatter-ts::all").
    pub id: String,
    /// Source files this test covers (relative paths).
    pub covered_files: Vec<String>,
    /// Recorded execution duration (or estimate). Zero means unknown.
    pub duration: Duration,
}

/// A prioritized test case with its computed score.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RankedTest {
    pub test: TestCase,
    /// Marginal weighted coverage at the time this test was selected.
    pub marginal_coverage: f64,
    /// Score = marginal_coverage / duration_secs at selection time.
    pub score: f64,
    /// Cumulative duration of all tests up to and including this one.
    pub cumulative_duration: Duration,
}

/// Configuration for the prioritization algorithm.
#[derive(Debug, Clone)]
pub struct PrioritizeConfig {
    /// Maximum total test execution time. Zero means unlimited.
    pub budget: Duration,
    /// Enable change-recency weighting from git log.
    pub use_recency: bool,
}

impl Default for PrioritizeConfig {
    fn default() -> Self {
        Self {
            budget: Duration::ZERO,
            use_recency: false,
        }
    }
}

/// Result of a prioritization run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrioritizeResult {
    /// Tests in priority order (highest value first).
    pub ordered: Vec<RankedTest>,
    /// Tests excluded because they exceeded the budget.
    pub excluded: Vec<TestCase>,
    /// Total duration of the ordered (included) tests.
    pub total_duration: Duration,
    /// The budget that was applied (zero if unlimited).
    pub budget: Duration,
}

// ---------------------------------------------------------------------------
// Budget parsing
// ---------------------------------------------------------------------------

/// Parse a budget string like "10s", "2m", "1m30s", or plain seconds "30".
pub fn parse_budget(s: &str) -> Result<Duration, PrioritizeError> {
    let s = s.trim();
    if s.is_empty() {
        return Err(PrioritizeError::InvalidBudget(
            "empty budget string".to_string(),
        ));
    }

    // Try plain integer (seconds)
    if let Ok(secs) = s.parse::<u64>() {
        return Ok(Duration::from_secs(secs));
    }

    let mut total_secs: u64 = 0;
    let mut current_num = String::new();

    for ch in s.chars() {
        if ch.is_ascii_digit() || ch == '.' {
            current_num.push(ch);
        } else {
            let n: f64 = current_num
                .parse()
                .map_err(|_| PrioritizeError::InvalidBudget(format!("invalid number in '{s}'")))?;
            current_num.clear();
            match ch {
                's' => total_secs += n as u64,
                'm' => total_secs += (n * 60.0) as u64,
                'h' => total_secs += (n * 3600.0) as u64,
                _ => {
                    return Err(PrioritizeError::InvalidBudget(format!(
                        "unknown unit '{ch}' in '{s}'"
                    )));
                }
            }
        }
    }

    // Trailing digits without a unit → seconds
    if !current_num.is_empty() {
        let n: f64 = current_num
            .parse()
            .map_err(|_| PrioritizeError::InvalidBudget(format!("invalid number in '{s}'")))?;
        total_secs += n as u64;
    }

    if total_secs == 0 {
        return Err(PrioritizeError::InvalidBudget(format!(
            "budget '{s}' evaluates to zero"
        )));
    }

    Ok(Duration::from_secs(total_secs))
}

// ---------------------------------------------------------------------------
// Recency weighting
// ---------------------------------------------------------------------------

/// Per-file weight based on how recently it was modified in git.
pub type RecencyWeights = HashMap<String, f64>;

/// Query git log for the last-modified timestamp of each file and compute
/// exponential decay weights. Files modified recently get weight ~1.0,
/// older files decay toward `MIN_RECENCY_WEIGHT`.
pub fn compute_recency_weights(
    project_root: &Path,
    files: &[String],
) -> Result<RecencyWeights, PrioritizeError> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);

    let mut weights = HashMap::new();

    for file in files {
        let age_days = git_file_age_days(project_root, file, now);
        let weight = recency_decay(age_days);
        weights.insert(file.clone(), weight);
    }

    Ok(weights)
}

/// Get the age in days of a file's most recent git commit.
/// Returns a large value if the file has no git history.
fn git_file_age_days(project_root: &Path, file: &str, now_epoch: f64) -> f64 {
    let output = Command::new("git")
        .args(["log", "-1", "--format=%ct", "--", file])
        .current_dir(project_root)
        .output();

    match output {
        Ok(o) if o.status.success() => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            let timestamp: f64 = stdout.trim().parse().unwrap_or(0.0);
            if timestamp > 0.0 {
                (now_epoch - timestamp) / SECS_PER_DAY
            } else {
                // No commits found for this file — treat as very old
                365.0
            }
        }
        _ => 365.0,
    }
}

/// Exponential decay: weight = max(MIN_RECENCY_WEIGHT, 2^(-age/half_life)).
fn recency_decay(age_days: f64) -> f64 {
    let raw = 2.0_f64.powf(-age_days / RECENCY_HALF_LIFE_DAYS);
    raw.max(MIN_RECENCY_WEIGHT)
}

// ---------------------------------------------------------------------------
// Core algorithm: greedy submodular maximization
// ---------------------------------------------------------------------------

/// Build `TestCase` entries from a coverage map, using recorded durations
/// or a default estimate.
pub fn test_cases_from_coverage_map(
    coverage_map: &CoverageMap,
    recorded_durations: &BTreeMap<String, Duration>,
) -> Vec<TestCase> {
    let default_duration = Duration::from_secs(1);

    coverage_map
        .data
        .entries
        .iter()
        .map(|(test_id, entry)| {
            let covered_files: Vec<String> = entry.files.keys().cloned().collect();
            let duration = recorded_durations
                .get(test_id)
                .copied()
                .unwrap_or(default_duration);
            TestCase {
                id: test_id.clone(),
                covered_files,
                duration,
            }
        })
        .collect()
}

/// Greedy submodular maximization for test prioritization.
///
/// At each step, selects the test with the highest ratio of
/// (weighted new lines covered) / (execution time), adds it to the
/// result, marks its files as covered, and repeats.
///
/// If `budget` is non-zero, stops when adding the next test would
/// exceed the time budget.
pub fn prioritize(
    tests: &[TestCase],
    config: &PrioritizeConfig,
    recency_weights: Option<&RecencyWeights>,
) -> PrioritizeResult {
    // Dedup by test ID, keeping the first occurrence.
    let mut seen_ids = HashSet::new();
    let deduped: Vec<&TestCase> = tests.iter().filter(|t| seen_ids.insert(&t.id)).collect();

    let n = deduped.len();
    let mut selected: Vec<RankedTest> = Vec::with_capacity(n);
    let mut excluded: Vec<TestCase> = Vec::new();
    let mut covered: HashSet<String> = HashSet::new();
    let mut remaining: Vec<usize> = (0..n).collect();
    let mut cumulative = Duration::ZERO;

    while !remaining.is_empty() {
        let mut best_idx: Option<usize> = None;
        let mut best_score: f64 = -1.0;
        let mut best_marginal: f64 = 0.0;

        for &ri in &remaining {
            let test = deduped[ri];
            let marginal = marginal_coverage(&test.covered_files, &covered, recency_weights);
            let duration_secs = test.duration.as_secs_f64().max(0.001); // avoid div-by-zero
            let score = marginal / duration_secs;

            if score > best_score {
                best_score = score;
                best_idx = Some(ri);
                best_marginal = marginal;
            }
        }

        let Some(chosen_idx) = best_idx else {
            break;
        };

        // If the best test adds zero marginal coverage, we're done
        // (all remaining tests are redundant).
        if best_marginal <= 0.0 {
            for &ri in &remaining {
                excluded.push(deduped[ri].clone());
            }
            break;
        }

        let chosen = deduped[chosen_idx];

        // Budget check: would adding this test exceed the budget?
        if !config.budget.is_zero() {
            let new_cumulative = cumulative + chosen.duration;
            if new_cumulative > config.budget {
                // This test doesn't fit — exclude it and continue
                // looking for smaller tests that might fit.
                remaining.retain(|&i| i != chosen_idx);
                excluded.push(chosen.clone());
                continue;
            }
        }

        // Accept this test
        cumulative += chosen.duration;
        for file in &chosen.covered_files {
            covered.insert(file.clone());
        }

        selected.push(RankedTest {
            test: chosen.clone(),
            marginal_coverage: best_marginal,
            score: best_score,
            cumulative_duration: cumulative,
        });

        remaining.retain(|&i| i != chosen_idx);
    }

    PrioritizeResult {
        ordered: selected,
        excluded,
        total_duration: cumulative,
        budget: config.budget,
    }
}

/// Compute weighted marginal coverage: sum of weights for files not yet covered.
fn marginal_coverage(
    files: &[String],
    covered: &HashSet<String>,
    recency_weights: Option<&RecencyWeights>,
) -> f64 {
    let mut total = 0.0;
    for file in files {
        if !covered.contains(file) {
            let weight = recency_weights
                .and_then(|w| w.get(file))
                .copied()
                .unwrap_or(1.0);
            total += weight;
        }
    }
    total
}

// ---------------------------------------------------------------------------
// Pipeline composition: TIA + prioritization
// ---------------------------------------------------------------------------

/// Run the full TIA → prioritize pipeline:
/// 1. Load coverage map
/// 2. Query affected tests for changed files
/// 3. Build test cases from affected tests
/// 4. Optionally compute recency weights
/// 5. Prioritize with greedy submodular maximization
pub fn prioritize_affected(
    coverage_map: &CoverageMap,
    affected_tests: &[String],
    recorded_durations: &BTreeMap<String, Duration>,
    config: &PrioritizeConfig,
    project_root: Option<&Path>,
) -> Result<PrioritizeResult, PrioritizeError> {
    // Build test cases from the subset of affected tests
    let all_cases = test_cases_from_coverage_map(coverage_map, recorded_durations);
    let affected_set: HashSet<&str> = affected_tests.iter().map(|s| s.as_str()).collect();
    let cases: Vec<TestCase> = all_cases
        .into_iter()
        .filter(|tc| affected_set.contains(tc.id.as_str()))
        .collect();

    // Optionally compute recency weights
    let recency = if config.use_recency {
        if let Some(root) = project_root {
            let all_files: Vec<String> = cases
                .iter()
                .flat_map(|tc| tc.covered_files.iter().cloned())
                .collect::<HashSet<_>>()
                .into_iter()
                .collect();
            Some(compute_recency_weights(root, &all_files)?)
        } else {
            None
        }
    } else {
        None
    };

    Ok(prioritize(&cases, config, recency.as_ref()))
}

// ---------------------------------------------------------------------------
// Display
// ---------------------------------------------------------------------------

/// Format a prioritization result for human-readable CLI output.
pub fn format_prioritize_report(result: &PrioritizeResult, use_color: bool) -> String {
    let mut out = String::new();

    let header = format!(
        "{} test(s) prioritized, total {:.1}s",
        result.ordered.len(),
        result.total_duration.as_secs_f64(),
    );
    if use_color {
        out.push_str(&format!("\x1b[1m{header}\x1b[0m\n"));
    } else {
        out.push_str(&format!("{header}\n"));
    }

    if !result.budget.is_zero() {
        out.push_str(&format!("Budget: {:.0}s\n", result.budget.as_secs_f64()));
    }
    out.push('\n');

    for (i, ranked) in result.ordered.iter().enumerate() {
        let idx = i + 1;
        let id = &ranked.test.id;
        let dur = ranked.test.duration.as_secs_f64();
        let files = ranked.test.covered_files.len();
        let marginal = ranked.marginal_coverage;
        let cum = ranked.cumulative_duration.as_secs_f64();
        out.push_str(&format!(
            "  {idx:>3}. {id}  ({dur:.1}s, {files} file(s), marginal={marginal:.2}, cum={cum:.1}s)\n"
        ));
    }

    if !result.excluded.is_empty() {
        out.push('\n');
        let excl_header = format!("{} test(s) excluded:", result.excluded.len());
        if use_color {
            out.push_str(&format!("\x1b[33m{excl_header}\x1b[0m\n"));
        } else {
            out.push_str(&format!("{excl_header}\n"));
        }
        for tc in &result.excluded {
            out.push_str(&format!(
                "  - {} ({:.1}s)\n",
                tc.id,
                tc.duration.as_secs_f64()
            ));
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

    fn make_test(id: &str, files: &[&str], secs: u64) -> TestCase {
        TestCase {
            id: id.to_string(),
            covered_files: files.iter().map(|s| s.to_string()).collect(),
            duration: Duration::from_secs(secs),
        }
    }

    #[test]
    fn prioritize_empty_input() {
        let config = PrioritizeConfig::default();
        let result = prioritize(&[], &config, None);
        assert!(result.ordered.is_empty());
        assert!(result.excluded.is_empty());
        assert_eq!(result.total_duration, Duration::ZERO);
    }

    #[test]
    fn prioritize_single_test() {
        let tests = vec![make_test("t1", &["a.rs", "b.rs"], 2)];
        let config = PrioritizeConfig::default();
        let result = prioritize(&tests, &config, None);
        assert_eq!(result.ordered.len(), 1);
        assert_eq!(result.ordered[0].test.id, "t1");
        assert_eq!(result.ordered[0].marginal_coverage, 2.0);
    }

    #[test]
    fn greedy_picks_highest_ratio_first() {
        let tests = vec![
            // t1: covers 2 files in 10s → ratio 0.2
            make_test("t1", &["a.rs", "b.rs"], 10),
            // t2: covers 1 file in 1s → ratio 1.0
            make_test("t2", &["c.rs"], 1),
        ];
        let config = PrioritizeConfig::default();
        let result = prioritize(&tests, &config, None);

        assert_eq!(result.ordered.len(), 2);
        // t2 should be picked first (higher ratio)
        assert_eq!(result.ordered[0].test.id, "t2");
        assert_eq!(result.ordered[1].test.id, "t1");
    }

    #[test]
    fn marginal_coverage_decreases() {
        let tests = vec![
            make_test("t1", &["a.rs", "b.rs"], 1),
            make_test("t2", &["a.rs", "c.rs"], 1),
        ];
        let config = PrioritizeConfig::default();
        let result = prioritize(&tests, &config, None);

        // Both have ratio 2.0 initially, so t1 is picked first (first in list).
        // After t1, t2 covers only c.rs (a.rs already covered) → marginal 1.0.
        assert_eq!(result.ordered.len(), 2);
        assert_eq!(result.ordered[0].marginal_coverage, 2.0);
        assert_eq!(result.ordered[1].marginal_coverage, 1.0);
    }

    #[test]
    fn redundant_test_excluded() {
        let tests = vec![
            make_test("t1", &["a.rs", "b.rs"], 1),
            make_test("t2", &["a.rs", "b.rs"], 1), // fully redundant
        ];
        let config = PrioritizeConfig::default();
        let result = prioritize(&tests, &config, None);

        // t2 adds zero marginal coverage → excluded
        assert_eq!(result.ordered.len(), 1);
        assert_eq!(result.excluded.len(), 1);
        assert_eq!(result.excluded[0].id, "t2");
    }

    #[test]
    fn budget_excludes_over_limit() {
        let tests = vec![
            make_test("t1", &["a.rs"], 3),
            make_test("t2", &["b.rs"], 3),
            make_test("t3", &["c.rs"], 3),
        ];
        let config = PrioritizeConfig {
            budget: Duration::from_secs(7),
            use_recency: false,
        };
        let result = prioritize(&tests, &config, None);

        // Budget=7s, each test 3s → can fit 2 tests (6s), third excluded
        assert_eq!(result.ordered.len(), 2);
        assert_eq!(result.excluded.len(), 1);
        assert!(result.total_duration <= Duration::from_secs(7));
    }

    #[test]
    fn budget_prefers_smaller_when_large_exceeds() {
        let tests = vec![
            // Highest coverage but too big for remaining budget
            make_test("big", &["a.rs", "b.rs", "c.rs", "d.rs"], 10),
            make_test("small1", &["a.rs"], 2),
            make_test("small2", &["b.rs"], 2),
        ];
        let config = PrioritizeConfig {
            budget: Duration::from_secs(5),
            use_recency: false,
        };
        let result = prioritize(&tests, &config, None);

        // big doesn't fit in 5s budget, small1+small2 do (4s)
        assert!(result.ordered.iter().all(|r| r.test.id != "big"));
        assert!(result.excluded.iter().any(|t| t.id == "big"));
    }

    #[test]
    fn recency_weights_boost_recent_files() {
        let tests = vec![
            make_test("t_old", &["old.rs"], 1),
            make_test("t_new", &["new.rs"], 1),
        ];
        let mut weights = RecencyWeights::new();
        weights.insert("old.rs".to_string(), 0.1);
        weights.insert("new.rs".to_string(), 1.0);

        let config = PrioritizeConfig::default();
        let result = prioritize(&tests, &config, Some(&weights));

        // t_new should be first (higher weighted coverage)
        assert_eq!(result.ordered[0].test.id, "t_new");
    }

    #[test]
    fn parse_budget_seconds() {
        assert_eq!(parse_budget("10").unwrap(), Duration::from_secs(10));
        assert_eq!(parse_budget("10s").unwrap(), Duration::from_secs(10));
    }

    #[test]
    fn parse_budget_minutes() {
        assert_eq!(parse_budget("2m").unwrap(), Duration::from_secs(120));
    }

    #[test]
    fn parse_budget_combined() {
        assert_eq!(parse_budget("1m30s").unwrap(), Duration::from_secs(90));
    }

    #[test]
    fn parse_budget_hours() {
        assert_eq!(parse_budget("1h").unwrap(), Duration::from_secs(3600));
    }

    #[test]
    fn parse_budget_invalid() {
        assert!(parse_budget("").is_err());
        assert!(parse_budget("0s").is_err());
        assert!(parse_budget("abc").is_err());
    }

    #[test]
    fn recency_decay_recent_file() {
        let weight = recency_decay(0.0);
        assert!((weight - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn recency_decay_half_life() {
        let weight = recency_decay(RECENCY_HALF_LIFE_DAYS);
        assert!((weight - 0.5).abs() < 0.01);
    }

    #[test]
    fn recency_decay_very_old() {
        let weight = recency_decay(365.0);
        assert_eq!(weight, MIN_RECENCY_WEIGHT);
    }

    #[test]
    fn test_cases_from_coverage_map_basic() {
        let map = CoverageMap::empty();
        let durations = BTreeMap::new();
        let cases = test_cases_from_coverage_map(&map, &durations);
        assert!(cases.is_empty());
    }

    #[test]
    fn format_report_no_budget() {
        let result = PrioritizeResult {
            ordered: vec![RankedTest {
                test: make_test("t1", &["a.rs"], 2),
                marginal_coverage: 1.0,
                score: 0.5,
                cumulative_duration: Duration::from_secs(2),
            }],
            excluded: vec![],
            total_duration: Duration::from_secs(2),
            budget: Duration::ZERO,
        };
        let report = format_prioritize_report(&result, false);
        assert!(report.contains("1 test(s) prioritized"));
        assert!(report.contains("t1"));
        assert!(!report.contains("Budget:"));
    }

    #[test]
    fn format_report_with_budget_and_excluded() {
        let result = PrioritizeResult {
            ordered: vec![],
            excluded: vec![make_test("t_big", &["x.rs"], 100)],
            total_duration: Duration::ZERO,
            budget: Duration::from_secs(5),
        };
        let report = format_prioritize_report(&result, false);
        assert!(report.contains("Budget: 5s"));
        assert!(report.contains("1 test(s) excluded"));
        assert!(report.contains("t_big"));
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    fn arb_test_case() -> impl Strategy<Value = TestCase> {
        (
            "[a-z_]{3,15}",
            prop::collection::vec("[a-z/]{1,15}\\.rs", 0..=8),
            1..=30u64,
        )
            .prop_map(|(id, files, secs)| TestCase {
                id,
                covered_files: files,
                duration: Duration::from_secs(secs),
            })
    }

    proptest! {
        /// Total duration of ordered tests never exceeds budget (when set).
        #[test]
        fn budget_respected(
            tests in prop::collection::vec(arb_test_case(), 1..=20),
            budget_secs in 1..=120u64,
        ) {
            let config = PrioritizeConfig {
                budget: Duration::from_secs(budget_secs),
                use_recency: false,
            };
            let result = prioritize(&tests, &config, None);
            prop_assert!(
                result.total_duration <= config.budget,
                "total {}s > budget {}s",
                result.total_duration.as_secs(),
                config.budget.as_secs()
            );
        }

        /// Every unique test ID in the input appears in either ordered or excluded.
        #[test]
        fn partition_complete(
            tests in prop::collection::vec(arb_test_case(), 0..=15),
        ) {
            let config = PrioritizeConfig::default();
            let result = prioritize(&tests, &config, None);
            let unique_ids: HashSet<_> = tests.iter().map(|t| &t.id).collect();
            let total = result.ordered.len() + result.excluded.len();
            prop_assert_eq!(total, unique_ids.len());
        }

        /// Ordered list has no duplicate test IDs.
        #[test]
        fn no_duplicates_in_ordered(
            tests in prop::collection::vec(arb_test_case(), 0..=15),
        ) {
            let config = PrioritizeConfig::default();
            let result = prioritize(&tests, &config, None);
            let ids: HashSet<_> = result.ordered.iter().map(|r| &r.test.id).collect();
            prop_assert_eq!(ids.len(), result.ordered.len());
        }

        /// Cumulative duration is monotonically increasing.
        #[test]
        fn cumulative_monotonic(
            tests in prop::collection::vec(arb_test_case(), 1..=15),
        ) {
            let config = PrioritizeConfig::default();
            let result = prioritize(&tests, &config, None);
            for window in result.ordered.windows(2) {
                prop_assert!(
                    window[1].cumulative_duration >= window[0].cumulative_duration,
                    "cumulative duration not monotonic"
                );
            }
        }

        /// Without recency weights, all files count equally (weight 1.0).
        #[test]
        fn no_recency_uniform_weight(
            files in prop::collection::vec("[a-z]{1,8}\\.rs", 1..=5),
        ) {
            let mut covered = HashSet::new();
            let marginal = marginal_coverage(&files, &covered, None);
            prop_assert!((marginal - files.len() as f64).abs() < f64::EPSILON);

            // After covering all files, marginal should be 0
            for f in &files {
                covered.insert(f.clone());
            }
            let marginal2 = marginal_coverage(&files, &covered, None);
            prop_assert!((marginal2 - 0.0).abs() < f64::EPSILON);
        }

        /// Recency decay is monotonically non-increasing with age.
        #[test]
        fn recency_decay_monotonic(age1 in 0.0..1000.0f64, age2 in 0.0..1000.0f64) {
            let (younger, older) = if age1 <= age2 { (age1, age2) } else { (age2, age1) };
            prop_assert!(recency_decay(younger) >= recency_decay(older));
        }

        /// Budget parse roundtrip for simple second values.
        #[test]
        fn budget_parse_secs(secs in 1..=10000u64) {
            let parsed = parse_budget(&format!("{secs}s")).unwrap();
            prop_assert_eq!(parsed, Duration::from_secs(secs));
        }
    }
}
