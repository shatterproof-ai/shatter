//! Data model for nondeterminism detection.
//!
//! Presence in the nondeterministic field list means "we have evidence
//! this is nondeterministic." Absence does NOT assert determinism.

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::protocol::ExecuteResult;

/// How nondeterminism was detected for a field or parameter.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum NondeterminismEvidence {
    /// Explicitly declared by the user (e.g., config or annotation).
    UserDeclared,
    /// Different outputs observed for the same input within a single run.
    ObservedWithinRun,
    /// Different outputs observed for the same input across separate runs.
    ObservedAcrossRuns,
    /// Matched a known nondeterministic API pattern (e.g., `Date.now()`, `Math.random()`).
    PatternMatch { pattern: String },
    /// Name heuristic suggests nondeterminism (e.g., "timestamp", "random", "uuid").
    NameHeuristic { matched_name: String },
    /// Value matches a slow nondeterminism pattern: deterministic within a run but
    /// likely to vary across runs (dates, near-current timestamps, monotonic counters).
    SlowPattern { pattern_type: String },
}

/// Confidence that a field is nondeterministic, based on accumulated evidence.
///
/// Ordered low-to-high so that [`Ord`] gives natural confidence comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    Low,
    Medium,
    High,
}

/// A parameter or field identified as potentially nondeterministic.
///
/// The `evidence` vector accumulates over time — multiple detection methods
/// may independently flag the same field, increasing confidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NondeterministicField {
    /// Path to the field (e.g., "param0", "param1.timestamp", "return.id").
    pub field_path: String,
    /// Evidence supporting the nondeterminism classification.
    pub evidence: Vec<NondeterminismEvidence>,
    /// Overall confidence derived from the evidence.
    pub confidence: Confidence,
}

/// Similarity score above which two values likely differ only by nondeterministic fields.
pub const SIMILARITY_HIGH_THRESHOLD: f64 = 0.9;

/// Similarity score below which two values likely represent genuinely different behavior.
pub const SIMILARITY_LOW_THRESHOLD: f64 = 0.5;

/// Result of comparing two JSON values structurally.
#[derive(Debug, Clone, PartialEq)]
pub struct SimilarityResult {
    /// Fraction of matching leaves (0.0–1.0).
    pub score: f64,
    /// Dot-separated paths of leaves that differ between the two values.
    pub changed_paths: Vec<String>,
    /// Total number of leaf comparisons performed.
    pub total_leaves: usize,
}

/// Compare two JSON values leaf-by-leaf, returning similarity as a fraction.
///
/// Objects are compared by union of keys (missing keys count as mismatches).
/// Arrays are compared element-by-element up to the longer length.
/// Primitives use exact equality. Type mismatches at any node count as
/// a single leaf mismatch.
pub fn structural_similarity(a: &Value, b: &Value) -> SimilarityResult {
    let mut matched: usize = 0;
    let mut total: usize = 0;
    let mut changed_paths: Vec<String> = Vec::new();

    collect_diff(a, b, "", &mut matched, &mut total, &mut changed_paths);

    let score = if total == 0 { 1.0 } else { matched as f64 / total as f64 };

    SimilarityResult {
        score,
        changed_paths,
        total_leaves: total,
    }
}

fn collect_diff(
    a: &Value,
    b: &Value,
    prefix: &str,
    matched: &mut usize,
    total: &mut usize,
    changed: &mut Vec<String>,
) {
    match (a, b) {
        (Value::Object(ma), Value::Object(mb)) => {
            if ma.is_empty() && mb.is_empty() {
                // Two empty objects are identical — one matching leaf.
                *total += 1;
                *matched += 1;
                return;
            }
            let mut keys: Vec<&String> = ma.keys().chain(mb.keys()).collect();
            keys.sort();
            keys.dedup();
            for key in keys {
                let child_path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                match (ma.get(key), mb.get(key)) {
                    (Some(va), Some(vb)) => {
                        collect_diff(va, vb, &child_path, matched, total, changed);
                    }
                    _ => {
                        // Key missing on one side — count leaves from present side.
                        let present = ma.get(key).or_else(|| mb.get(key)).unwrap();
                        let leaf_count = count_leaves(present);
                        *total += leaf_count;
                        changed.push(child_path);
                    }
                }
            }
        }
        (Value::Array(aa), Value::Array(ab)) => {
            if aa.is_empty() && ab.is_empty() {
                *total += 1;
                *matched += 1;
                return;
            }
            let max_len = aa.len().max(ab.len());
            for i in 0..max_len {
                let child_path = if prefix.is_empty() {
                    format!("[{i}]")
                } else {
                    format!("{prefix}[{i}]")
                };
                match (aa.get(i), ab.get(i)) {
                    (Some(va), Some(vb)) => {
                        collect_diff(va, vb, &child_path, matched, total, changed);
                    }
                    (Some(present), None) | (None, Some(present)) => {
                        let leaf_count = count_leaves(present);
                        *total += leaf_count;
                        changed.push(child_path);
                    }
                    (None, None) => unreachable!(),
                }
            }
        }
        // Both are leaves (or type mismatch at this level).
        _ => {
            *total += 1;
            if a == b {
                *matched += 1;
            } else {
                changed.push(prefix.to_string());
            }
        }
    }
}

/// Count the number of leaf values in a JSON tree.
fn count_leaves(v: &Value) -> usize {
    match v {
        Value::Object(m) if !m.is_empty() => m.values().map(count_leaves).sum(),
        Value::Array(a) if !a.is_empty() => a.iter().map(count_leaves).sum(),
        _ => 1,
    }
}

// --- Name-based heuristics for nondeterminism detection ---

/// How a [`NamePattern`] matches against a field name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchKind {
    /// Field name ends with the pattern (case-insensitive).
    Suffix,
    /// Field name contains the pattern anywhere (case-insensitive).
    Substring,
}

/// A pattern that suggests a field is nondeterministic based on its name.
#[derive(Debug, Clone, PartialEq)]
pub struct NamePattern {
    pub pattern: &'static str,
    pub match_kind: MatchKind,
    /// Base confidence (0.0–1.0) before input-echo adjustment.
    pub confidence: f64,
}

/// Confidence reduction factor when a field name matches an input parameter name,
/// suggesting the value is echoed rather than generated.
const INPUT_ECHO_DISCOUNT: f64 = 0.5;

/// Default patterns ordered by confidence descending. First match wins,
/// so more specific patterns (higher confidence) come first.
pub const NAME_PATTERNS: &[NamePattern] = &[
    NamePattern { pattern: "uuid",      match_kind: MatchKind::Suffix,    confidence: 0.95 },
    NamePattern { pattern: "token",     match_kind: MatchKind::Suffix,    confidence: 0.90 },
    NamePattern { pattern: "nonce",     match_kind: MatchKind::Suffix,    confidence: 0.90 },
    NamePattern { pattern: "random",    match_kind: MatchKind::Substring, confidence: 0.85 },
    NamePattern { pattern: "timestamp", match_kind: MatchKind::Suffix,    confidence: 0.80 },
    NamePattern { pattern: "_at",       match_kind: MatchKind::Suffix,    confidence: 0.80 },
    NamePattern { pattern: "date",      match_kind: MatchKind::Suffix,    confidence: 0.70 },
    NamePattern { pattern: "hostname",  match_kind: MatchKind::Suffix,    confidence: 0.65 },
    NamePattern { pattern: "pid",       match_kind: MatchKind::Suffix,    confidence: 0.60 },
    NamePattern { pattern: "id",        match_kind: MatchKind::Suffix,    confidence: 0.60 },
];

/// Result of a successful name-heuristic match.
#[derive(Debug, Clone, PartialEq)]
pub struct NameHeuristicResult {
    /// The pattern string that matched.
    pub matched_pattern: &'static str,
    /// Confidence after input-echo adjustment (0.0–1.0).
    pub confidence: f64,
}

/// Check whether `field_name` matches any known nondeterministic name pattern.
///
/// For dot-separated paths (e.g. `"return.requestId"`), only the last segment
/// is matched. When the last segment case-insensitively equals any entry in
/// `input_param_names`, confidence is halved (the field likely echoes an input).
pub fn check_name_heuristics(
    field_name: &str,
    input_param_names: &[&str],
) -> Option<NameHeuristicResult> {
    let segment = field_name.rsplit('.').next().unwrap_or(field_name);
    let lower = segment.to_ascii_lowercase();

    for pat in NAME_PATTERNS {
        let pat_lower = pat.pattern.to_ascii_lowercase();
        let matched = match pat.match_kind {
            MatchKind::Suffix => lower.ends_with(&pat_lower) && lower.len() > pat_lower.len(),
            MatchKind::Substring => lower.contains(&pat_lower),
        };

        if matched {
            let is_echo = input_param_names.iter().any(|p| {
                p.eq_ignore_ascii_case(segment)
            });
            let confidence = if is_echo {
                pat.confidence * INPUT_ECHO_DISCOUNT
            } else {
                pat.confidence
            };
            return Some(NameHeuristicResult {
                matched_pattern: pat.pattern,
                confidence,
            });
        }
    }
    None
}

// --- Value-pattern heuristics ---

/// A matched value pattern indicating likely nondeterminism.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValuePatternMatch {
    /// Human-readable pattern name (e.g., "uuid_v4", "iso8601_datetime").
    pub pattern_name: String,
    /// Confidence that this pattern indicates nondeterminism.
    pub confidence: Confidence,
}

/// Pattern name constants — used in both production code and tests.
pub const PATTERN_UUID_V4: &str = "uuid_v4";
pub const PATTERN_ISO8601_DATETIME: &str = "iso8601_datetime";
pub const PATTERN_UNIX_TIMESTAMP_S: &str = "unix_timestamp_seconds";
pub const PATTERN_UNIX_TIMESTAMP_MS: &str = "unix_timestamp_millis";
pub const PATTERN_JWT: &str = "jwt_token";
pub const PATTERN_SHA256_HEX: &str = "sha256_hex";
pub const PATTERN_RANDOM_HEX: &str = "random_hex";
pub const PATTERN_DATE_ONLY: &str = "date_only";
pub const PATTERN_LOCALE_DATE: &str = "locale_date";
pub const PATTERN_MONOTONIC_COUNTER: &str = "monotonic_counter";
pub const PATTERN_ENV_VALUE: &str = "environment_value";

/// Epoch boundaries for unix timestamp detection (2020-01-01 to 2030-01-01).
const UNIX_TS_MIN_S: i64 = 1_577_836_800;
const UNIX_TS_MAX_S: i64 = 1_893_456_000;
const UNIX_TS_MIN_MS: i64 = UNIX_TS_MIN_S * 1_000;
const UNIX_TS_MAX_MS: i64 = UNIX_TS_MAX_S * 1_000;

/// Minimum length for random hex string detection.
const RANDOM_HEX_MIN_LEN: usize = 32;

/// Exact length of a SHA-256 hex digest.
const SHA256_HEX_LEN: usize = 64;

static RE_UUID_V4: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$")
        .expect("uuid_v4 regex")
});

static RE_ISO8601: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}").expect("iso8601 regex")
});

static RE_JWT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^eyJ[A-Za-z0-9_-]+\.eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+$").expect("jwt regex")
});

static RE_HEX_LOWER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^[0-9a-f]+$").expect("hex regex")
});

/// ISO date without time component (e.g. "2026-03-06").
static RE_DATE_ISO_ONLY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\d{4}-\d{2}-\d{2}$").expect("date_iso regex")
});

/// US-style date format MM/DD/YYYY.
static RE_DATE_US: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\d{2}/\d{2}/\d{4}$").expect("date_us regex")
});

/// English locale date (e.g. "March 6, 2026" or "6 March 2026").
static RE_DATE_LOCALE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(?:(?:January|February|March|April|May|June|July|August|September|October|November|December)\s+\d{1,2},?\s+\d{4}|\d{1,2}\s+(?:January|February|March|April|May|June|July|August|September|October|November|December)\s+\d{4})$")
        .expect("date_locale regex")
});

/// Maximum step between consecutive values to qualify as a monotonic counter.
const MONOTONIC_COUNTER_MAX_STEP: i64 = 100;

/// Maximum string length for environment value heuristic (hostnames, PIDs).
const ENV_VALUE_MAX_STRING_LEN: usize = 64;

/// Field name patterns that suggest environment-dependent values.
const ENV_FIELD_PATTERNS: &[&str] = &["host", "hostname", "pid", "ppid", "port"];

/// Check a JSON value against known nondeterministic value patterns.
///
/// For strings: tests UUID v4, ISO 8601 datetime, JWT, SHA-256 hex, and random hex.
/// For numbers: tests unix timestamp ranges (seconds and milliseconds).
/// SHA-256 (64 hex chars) is preferred over generic random hex when both match.
pub fn check_value_patterns(value: &Value) -> Vec<ValuePatternMatch> {
    match value {
        Value::String(s) => check_string_patterns(s),
        Value::Number(n) => check_number_patterns(n),
        _ => Vec::new(),
    }
}

fn check_string_patterns(s: &str) -> Vec<ValuePatternMatch> {
    let mut matches = Vec::new();

    if RE_UUID_V4.is_match(s) {
        matches.push(ValuePatternMatch {
            pattern_name: PATTERN_UUID_V4.into(),
            confidence: Confidence::High,
        });
    }

    if RE_ISO8601.is_match(s) {
        matches.push(ValuePatternMatch {
            pattern_name: PATTERN_ISO8601_DATETIME.into(),
            confidence: Confidence::High,
        });
    }

    if RE_JWT.is_match(s) {
        matches.push(ValuePatternMatch {
            pattern_name: PATTERN_JWT.into(),
            confidence: Confidence::High,
        });
    }

    // Date-only patterns (slow nondeterminism — stable within a run, vary across days).
    if RE_DATE_ISO_ONLY.is_match(s) || RE_DATE_US.is_match(s) {
        matches.push(ValuePatternMatch {
            pattern_name: PATTERN_DATE_ONLY.into(),
            confidence: Confidence::Medium,
        });
    }

    if RE_DATE_LOCALE.is_match(s) {
        matches.push(ValuePatternMatch {
            pattern_name: PATTERN_LOCALE_DATE.into(),
            confidence: Confidence::Medium,
        });
    }

    // Hex string patterns: prefer SHA-256 (specific) over generic random hex.
    if s.len() >= RANDOM_HEX_MIN_LEN && RE_HEX_LOWER.is_match(s) {
        if s.len() == SHA256_HEX_LEN {
            matches.push(ValuePatternMatch {
                pattern_name: PATTERN_SHA256_HEX.into(),
                confidence: Confidence::Medium,
            });
        } else {
            matches.push(ValuePatternMatch {
                pattern_name: PATTERN_RANDOM_HEX.into(),
                confidence: Confidence::Medium,
            });
        }
    }

    matches
}

fn check_number_patterns(n: &serde_json::Number) -> Vec<ValuePatternMatch> {
    let mut matches = Vec::new();

    if let Some(v) = n.as_i64() {
        if (UNIX_TS_MIN_S..=UNIX_TS_MAX_S).contains(&v) {
            matches.push(ValuePatternMatch {
                pattern_name: PATTERN_UNIX_TIMESTAMP_S.into(),
                confidence: Confidence::Medium,
            });
        } else if (UNIX_TS_MIN_MS..=UNIX_TS_MAX_MS).contains(&v) {
            matches.push(ValuePatternMatch {
                pattern_name: PATTERN_UNIX_TIMESTAMP_MS.into(),
                confidence: Confidence::Medium,
            });
        }
    }

    matches
}

// --- Within-run re-execution sampling ---

/// Number of inputs to sample from each equivalence class for re-execution.
pub const NONDETERMINISM_SAMPLES_PER_CLASS: usize = 2;

/// Number of times to re-execute each sampled input to detect nondeterminism.
pub const NONDETERMINISM_REEXECUTION_COUNT: usize = 3;

/// Field path used when the outcome type itself varies (one execution returns,
/// another throws, or vice versa).
pub const FIELD_PATH_OUTCOME: &str = "<outcome>";

/// Field path used when thrown error type or message varies across re-executions.
pub const FIELD_PATH_THROWN_ERROR: &str = "thrown_error";

/// Result of the within-run nondeterminism detection phase.
#[derive(Debug, Clone, Default)]
pub struct ReexecutionReport {
    /// Fields identified as nondeterministic via re-execution sampling.
    pub nondeterministic_fields: Vec<NondeterministicField>,
    /// Number of distinct inputs that were re-executed.
    pub inputs_sampled: usize,
    /// Total number of re-executions performed.
    pub reexecutions_performed: usize,
}

/// Compare an original execution with its re-executions to detect nondeterministic fields.
///
/// Each entry in `samples` is `(original_result, re_execution_results)` for the same input.
/// Returns a report listing fields whose values varied across re-executions, with
/// confidence based on how consistently the variation appeared.
pub fn detect_within_run_nondeterminism(
    samples: &[(ExecuteResult, Vec<ExecuteResult>)],
) -> ReexecutionReport {
    if samples.is_empty() {
        return ReexecutionReport::default();
    }

    // Track (field_path → (times_changed, total_comparisons)) across all samples.
    let mut field_stats: HashMap<String, (usize, usize)> = HashMap::new();
    let total_samples = samples.len();
    let mut total_reexecutions = 0usize;

    for (original, reexecutions) in samples {
        total_reexecutions += reexecutions.len();

        for reexec in reexecutions {
            // Check outcome type mismatch (return vs throw).
            let orig_returns = original.return_value.is_some() || original.thrown_error.is_none();
            let reexec_returns = reexec.return_value.is_some() || reexec.thrown_error.is_none();

            if orig_returns != reexec_returns {
                // One returns normally, the other throws (or vice versa).
                let entry = field_stats.entry(FIELD_PATH_OUTCOME.to_string()).or_insert((0, 0));
                entry.0 += 1;
                entry.1 += 1;
                continue;
            }

            // Both returned normally — compare return values.
            if let (Some(orig_val), Some(reexec_val)) =
                (&original.return_value, &reexec.return_value)
            {
                let sim = structural_similarity(orig_val, reexec_val);
                for path in &sim.changed_paths {
                    let field_path = if path.is_empty() {
                        "return".to_string()
                    } else {
                        format!("return.{path}")
                    };
                    let entry = field_stats.entry(field_path).or_insert((0, 0));
                    entry.0 += 1;
                    entry.1 += 1;
                }
                // Count non-changed comparisons too.
                if sim.changed_paths.is_empty() {
                    // No changes — record that this comparison was clean.
                    // We don't add to field_stats since no paths changed.
                }
            }

            // Both threw — compare error type and message.
            if let (Some(orig_err), Some(reexec_err)) =
                (&original.thrown_error, &reexec.thrown_error)
            && (orig_err.error_type != reexec_err.error_type
                    || orig_err.message != reexec_err.message)
            {
                let entry = field_stats
                    .entry(FIELD_PATH_THROWN_ERROR.to_string())
                    .or_insert((0, 0));
                entry.0 += 1;
                entry.1 += 1;
            }
        }
    }

    // Convert field stats to NondeterministicField entries with confidence.
    let mut fields: Vec<NondeterministicField> = field_stats
        .into_iter()
        .map(|(field_path, (times_changed, _))| {
            // Confidence based on how consistently the field varied:
            // - Changed in every re-execution of every sample → High
            // - Changed in majority of re-executions → Medium
            // - Changed in at least one → Low
            let total_comparisons = total_reexecutions;
            let ratio = if total_comparisons > 0 {
                times_changed as f64 / total_comparisons as f64
            } else {
                0.0
            };

            let confidence = if ratio >= 1.0 - f64::EPSILON {
                Confidence::High
            } else if ratio >= 0.5 {
                Confidence::Medium
            } else {
                Confidence::Low
            };

            NondeterministicField {
                field_path,
                evidence: vec![NondeterminismEvidence::ObservedWithinRun],
                confidence,
            }
        })
        .collect();

    // Sort by field_path for deterministic output.
    fields.sort_by(|a, b| a.field_path.cmp(&b.field_path));

    ReexecutionReport {
        nondeterministic_fields: fields,
        inputs_sampled: total_samples,
        reexecutions_performed: total_reexecutions,
    }
}

/// Select up to `max_samples` input indices from an equivalence class's inputs.
///
/// Picks the first (canonical) input, then the one with the most different JSON
/// serialization length to maximize diversity.
pub fn select_sample_indices(
    inputs: &[Vec<serde_json::Value>],
    max_samples: usize,
) -> Vec<usize> {
    if inputs.is_empty() || max_samples == 0 {
        return vec![];
    }

    let mut selected = vec![0usize];
    if max_samples >= 2 && inputs.len() >= 2 {
        let canonical_len: usize = inputs[0]
            .iter()
            .map(|v| serde_json::to_string(v).unwrap_or_default().len())
            .sum();

        let mut best_idx = 1;
        let mut best_diff = 0usize;
        for (i, inp) in inputs.iter().enumerate().skip(1) {
            let len: usize = inp
                .iter()
                .map(|v| serde_json::to_string(v).unwrap_or_default().len())
                .sum();
            let diff = len.abs_diff(canonical_len);
            if diff > best_diff {
                best_diff = diff;
                best_idx = i;
            }
        }
        selected.push(best_idx);
    }

    selected.truncate(max_samples);
    selected
}

// --- Cross-run slow nondeterminism detection ---

/// Detect slow nondeterminism patterns by comparing the same field's values across runs.
///
/// Checks for monotonic counters (strictly increasing integers with bounded steps).
/// Requires at least 2 values; confidence is `Low` for 2 values, `Medium` for 3+.
pub fn check_cross_run_patterns(values: &[Value]) -> Vec<ValuePatternMatch> {
    let mut matches = Vec::new();

    // Extract integer values for monotonic counter check.
    let ints: Vec<i64> = values.iter().filter_map(|v| v.as_i64()).collect();

    if ints.len() >= 2 && ints.len() == values.len() {
        let is_monotonic = ints.windows(2).all(|w| {
            let step = w[1] - w[0];
            step > 0 && step <= MONOTONIC_COUNTER_MAX_STEP
        });

        if is_monotonic {
            let confidence = if ints.len() >= 3 {
                Confidence::Medium
            } else {
                Confidence::Low
            };
            matches.push(ValuePatternMatch {
                pattern_name: PATTERN_MONOTONIC_COUNTER.into(),
                confidence,
            });
        }
    }

    matches
}

/// Detect environment-dependent values by combining field name with value shape.
///
/// Returns a match when the field name contains a known environment pattern
/// (e.g. "hostname", "pid") AND the value is a short string or small integer.
pub fn check_env_value_heuristic(
    field_name: &str,
    value: &Value,
) -> Option<ValuePatternMatch> {
    let segment = field_name.rsplit('.').next().unwrap_or(field_name);
    let lower = segment.to_ascii_lowercase();

    let name_matches = ENV_FIELD_PATTERNS.iter().any(|pat| lower.contains(pat));
    if !name_matches {
        return None;
    }

    let value_plausible = match value {
        Value::String(s) => !s.is_empty() && s.len() <= ENV_VALUE_MAX_STRING_LEN,
        Value::Number(n) => n.as_i64().is_some_and(|v| v >= 0),
        _ => false,
    };

    if value_plausible {
        Some(ValuePatternMatch {
            pattern_name: PATTERN_ENV_VALUE.into(),
            confidence: Confidence::Low,
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn construct_nondeterministic_field() {
        let field = NondeterministicField {
            field_path: "param0.timestamp".into(),
            evidence: vec![NondeterminismEvidence::ObservedAcrossRuns],
            confidence: Confidence::Medium,
        };
        assert_eq!(field.field_path, "param0.timestamp");
        assert_eq!(field.evidence.len(), 1);
        assert_eq!(field.confidence, Confidence::Medium);
    }

    #[test]
    fn serialize_deserialize_round_trip() {
        let field = NondeterministicField {
            field_path: "return.id".into(),
            evidence: vec![
                NondeterminismEvidence::PatternMatch {
                    pattern: "Math.random()".into(),
                },
                NondeterminismEvidence::NameHeuristic {
                    matched_name: "random".into(),
                },
            ],
            confidence: Confidence::High,
        };

        let json = serde_json::to_string(&field).expect("serialize");
        let restored: NondeterministicField =
            serde_json::from_str(&json).expect("deserialize");
        assert_eq!(field, restored);
    }

    #[test]
    fn evidence_variants_round_trip() {
        let variants = vec![
            NondeterminismEvidence::UserDeclared,
            NondeterminismEvidence::ObservedWithinRun,
            NondeterminismEvidence::ObservedAcrossRuns,
            NondeterminismEvidence::PatternMatch {
                pattern: "Date.now()".into(),
            },
            NondeterminismEvidence::NameHeuristic {
                matched_name: "uuid".into(),
            },
            NondeterminismEvidence::SlowPattern {
                pattern_type: PATTERN_DATE_ONLY.into(),
            },
        ];

        for variant in &variants {
            let json = serde_json::to_string(variant).expect("serialize");
            let restored: NondeterminismEvidence =
                serde_json::from_str(&json).expect("deserialize");
            assert_eq!(*variant, restored);
        }
    }

    #[test]
    fn confidence_ordering() {
        assert!(Confidence::Low < Confidence::Medium);
        assert!(Confidence::Medium < Confidence::High);
    }

    // --- structural_similarity tests ---

    #[test]
    fn similarity_identical_primitives() {
        let r = structural_similarity(&json!(42), &json!(42));
        assert_eq!(r.score, 1.0);
        assert!(r.changed_paths.is_empty());
        assert_eq!(r.total_leaves, 1);
    }

    #[test]
    fn similarity_different_primitives() {
        let r = structural_similarity(&json!(42), &json!("hello"));
        assert_eq!(r.score, 0.0);
        assert_eq!(r.changed_paths, vec![""]);
    }

    #[test]
    fn similarity_identical_objects() {
        let a = json!({"a": 1, "b": 2});
        let r = structural_similarity(&a, &a.clone());
        assert_eq!(r.score, 1.0);
        assert!(r.changed_paths.is_empty());
    }

    #[test]
    fn similarity_one_field_changed() {
        let a = json!({"a": 1, "b": 2, "c": 3});
        let b = json!({"a": 1, "b": 99, "c": 3});
        let r = structural_similarity(&a, &b);
        let expected = 2.0 / 3.0;
        assert!((r.score - expected).abs() < 1e-10);
        assert_eq!(r.changed_paths, vec!["b"]);
        assert_eq!(r.total_leaves, 3);
    }

    #[test]
    fn similarity_nested_objects() {
        let a = json!({"a": {"x": 1, "y": 2}});
        let b = json!({"a": {"x": 1, "y": 99}});
        let r = structural_similarity(&a, &b);
        assert_eq!(r.score, 0.5);
        assert_eq!(r.changed_paths, vec!["a.y"]);
    }

    #[test]
    fn similarity_completely_different_objects() {
        let a = json!({"a": 1});
        let b = json!({"b": 2});
        let r = structural_similarity(&a, &b);
        assert_eq!(r.score, 0.0);
        assert_eq!(r.total_leaves, 2);
    }

    #[test]
    fn similarity_array_same_length() {
        let a = json!([1, 2, 3]);
        let b = json!([1, 2, 4]);
        let r = structural_similarity(&a, &b);
        let expected = 2.0 / 3.0;
        assert!((r.score - expected).abs() < 1e-10);
        assert_eq!(r.changed_paths, vec!["[2]"]);
    }

    #[test]
    fn similarity_array_different_length() {
        let a = json!([1, 2]);
        let b = json!([1, 2, 3]);
        let r = structural_similarity(&a, &b);
        let expected = 2.0 / 3.0;
        assert!((r.score - expected).abs() < 1e-10);
        assert_eq!(r.changed_paths, vec!["[2]"]);
    }

    #[test]
    fn similarity_type_mismatch() {
        let r = structural_similarity(&json!("hello"), &json!(42));
        assert_eq!(r.score, 0.0);
        assert_eq!(r.total_leaves, 1);
    }

    #[test]
    fn similarity_null_handling() {
        let r = structural_similarity(&json!(null), &json!(null));
        assert_eq!(r.score, 1.0);

        let r2 = structural_similarity(&json!(null), &json!(42));
        assert_eq!(r2.score, 0.0);
    }

    #[test]
    fn similarity_empty_objects() {
        let r = structural_similarity(&json!({}), &json!({}));
        assert_eq!(r.score, 1.0);
        assert_eq!(r.total_leaves, 1);
    }

    #[test]
    fn similarity_empty_arrays() {
        let r = structural_similarity(&json!([]), &json!([]));
        assert_eq!(r.score, 1.0);
        assert_eq!(r.total_leaves, 1);
    }

    #[test]
    fn similarity_high_threshold_large_object() {
        // 10 fields, 1 changed → 0.9, which meets the threshold.
        let a = json!({"a":1,"b":2,"c":3,"d":4,"e":5,"f":6,"g":7,"h":8,"i":9,"j":10});
        let b = json!({"a":1,"b":2,"c":3,"d":4,"e":5,"f":6,"g":7,"h":8,"i":9,"j":99});
        let r = structural_similarity(&a, &b);
        assert!(r.score >= SIMILARITY_HIGH_THRESHOLD);
    }

    #[test]
    fn similarity_low_threshold_mostly_different() {
        let a = json!({"a":1,"b":2,"c":3,"d":4});
        let b = json!({"a":99,"b":98,"c":97,"d":4});
        let r = structural_similarity(&a, &b);
        assert!(r.score < SIMILARITY_LOW_THRESHOLD);
    }

    #[test]
    fn similarity_deeply_nested() {
        let a = json!({"l1": {"l2": {"l3": {"val": 1, "stable": true}}}});
        let b = json!({"l1": {"l2": {"l3": {"val": 2, "stable": true}}}});
        let r = structural_similarity(&a, &b);
        assert_eq!(r.score, 0.5);
        assert_eq!(r.changed_paths, vec!["l1.l2.l3.val"]);
    }

    #[test]
    fn similarity_mixed_array_and_object() {
        let a = json!({"items": [1, 2], "name": "test"});
        let b = json!({"items": [1, 3], "name": "test"});
        let r = structural_similarity(&a, &b);
        // 3 leaves: items[0]=match, items[1]=diff, name=match → 2/3
        let expected = 2.0 / 3.0;
        assert!((r.score - expected).abs() < 1e-10);
        assert_eq!(r.changed_paths, vec!["items[1]"]);
    }

    #[test]
    fn similarity_object_vs_primitive() {
        let a = json!({"a": 1});
        let b = json!(42);
        let r = structural_similarity(&a, &b);
        assert_eq!(r.score, 0.0);
        assert_eq!(r.total_leaves, 1);
    }

    #[test]
    fn similarity_missing_key_with_nested_value() {
        // Missing key has a nested object with 2 leaves — both count as mismatches.
        let a = json!({"x": 1, "y": {"p": 1, "q": 2}});
        let b = json!({"x": 1});
        let r = structural_similarity(&a, &b);
        // total: x(1) + y.p(1) + y.q(1) = 3, matched: 1
        assert!((r.score - 1.0 / 3.0).abs() < 1e-10);
        assert_eq!(r.changed_paths, vec!["y"]);
    }

    #[test]
    fn multiple_evidence_accumulates() {
        let mut field = NondeterministicField {
            field_path: "param0".into(),
            evidence: vec![NondeterminismEvidence::NameHeuristic {
                matched_name: "timestamp".into(),
            }],
            confidence: Confidence::Low,
        };

        field
            .evidence
            .push(NondeterminismEvidence::ObservedAcrossRuns);
        field.confidence = Confidence::High;

        assert_eq!(field.evidence.len(), 2);
        assert_eq!(field.confidence, Confidence::High);
    }

    // --- name heuristic tests ---

    #[test]
    fn name_heuristic_uuid_suffix() {
        let r = check_name_heuristics("requestUuid", &[]).unwrap();
        assert_eq!(r.matched_pattern, "uuid");
        assert!((r.confidence - 0.95).abs() < 1e-10);
    }

    #[test]
    fn name_heuristic_token_suffix() {
        let r = check_name_heuristics("authToken", &[]).unwrap();
        assert_eq!(r.matched_pattern, "token");
        assert!((r.confidence - 0.90).abs() < 1e-10);
    }

    #[test]
    fn name_heuristic_nonce_suffix() {
        let r = check_name_heuristics("sessionNonce", &[]).unwrap();
        assert_eq!(r.matched_pattern, "nonce");
        assert!((r.confidence - 0.90).abs() < 1e-10);
    }

    #[test]
    fn name_heuristic_random_substring() {
        let r = check_name_heuristics("myRandomValue", &[]).unwrap();
        assert_eq!(r.matched_pattern, "random");
        assert!((r.confidence - 0.85).abs() < 1e-10);
    }

    #[test]
    fn name_heuristic_timestamp_suffix() {
        let r = check_name_heuristics("createdTimestamp", &[]).unwrap();
        assert_eq!(r.matched_pattern, "timestamp");
        assert!((r.confidence - 0.80).abs() < 1e-10);
    }

    #[test]
    fn name_heuristic_at_suffix() {
        let r = check_name_heuristics("updated_at", &[]).unwrap();
        assert_eq!(r.matched_pattern, "_at");
        assert!((r.confidence - 0.80).abs() < 1e-10);
    }

    #[test]
    fn name_heuristic_id_suffix() {
        let r = check_name_heuristics("requestId", &[]).unwrap();
        assert_eq!(r.matched_pattern, "id");
        assert!((r.confidence - 0.60).abs() < 1e-10);
    }

    #[test]
    fn name_heuristic_case_insensitive() {
        let r = check_name_heuristics("RequestUUID", &[]).unwrap();
        assert_eq!(r.matched_pattern, "uuid");

        let r2 = check_name_heuristics("SESSION_TOKEN", &[]).unwrap();
        assert_eq!(r2.matched_pattern, "token");
    }

    #[test]
    fn name_heuristic_dot_path_uses_last_segment() {
        let r = check_name_heuristics("return.response.requestId", &[]).unwrap();
        assert_eq!(r.matched_pattern, "id");

        let r2 = check_name_heuristics("param0.authToken", &[]).unwrap();
        assert_eq!(r2.matched_pattern, "token");
    }

    #[test]
    fn name_heuristic_input_echo_reduces_confidence() {
        let r = check_name_heuristics("requestId", &["requestId"]).unwrap();
        assert_eq!(r.matched_pattern, "id");
        assert!((r.confidence - 0.60 * INPUT_ECHO_DISCOUNT).abs() < 1e-10);
    }

    #[test]
    fn name_heuristic_input_echo_case_insensitive() {
        let r = check_name_heuristics("AuthToken", &["authtoken"]).unwrap();
        assert!((r.confidence - 0.90 * INPUT_ECHO_DISCOUNT).abs() < 1e-10);
    }

    #[test]
    fn name_heuristic_no_match() {
        assert!(check_name_heuristics("name", &[]).is_none());
        assert!(check_name_heuristics("count", &[]).is_none());
        assert!(check_name_heuristics("email", &[]).is_none());
    }

    #[test]
    fn name_heuristic_suffix_requires_prefix() {
        // "id" alone equals the pattern — no prefix chars, so suffix match fails.
        assert!(check_name_heuristics("id", &[]).is_none());
        // "uuid" ends with "id" and has prefix chars, so it matches "id".
        let r = check_name_heuristics("uuid", &[]).unwrap();
        assert_eq!(r.matched_pattern, "id");
    }

    #[test]
    fn name_heuristic_suffix_no_false_positive_on_interior() {
        // "video" ends with "id" + "eo", not with "id" — but "avid" does NOT
        // end with "id" in a meaningful way. Let's test "video" doesn't match.
        assert!(check_name_heuristics("video", &[]).is_none());
    }

    #[test]
    fn name_heuristic_highest_confidence_wins() {
        // "randomUuid" matches both "random" (substring, 0.85) and "uuid" (suffix, 0.95).
        // "uuid" comes first in the table, so it wins.
        let r = check_name_heuristics("randomUuid", &[]).unwrap();
        assert_eq!(r.matched_pattern, "uuid");
        assert!((r.confidence - 0.95).abs() < 1e-10);
    }

    #[test]
    fn name_heuristic_pattern_table_ordered_by_confidence() {
        for window in NAME_PATTERNS.windows(2) {
            assert!(
                window[0].confidence >= window[1].confidence,
                "NAME_PATTERNS not ordered by confidence: {} ({}) before {} ({})",
                window[0].pattern, window[0].confidence,
                window[1].pattern, window[1].confidence,
            );
        }
    }

    // --- value pattern heuristic tests ---

    #[test]
    fn pattern_uuid_v4() {
        let v = json!("550e8400-e29b-41d4-a716-446655440000");
        let matches = check_value_patterns(&v);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].pattern_name, PATTERN_UUID_V4);
        assert_eq!(matches[0].confidence, Confidence::High);
    }

    #[test]
    fn pattern_uuid_v4_uppercase() {
        let v = json!("550E8400-E29B-41D4-A716-446655440000");
        let matches = check_value_patterns(&v);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].pattern_name, PATTERN_UUID_V4);
    }

    #[test]
    fn pattern_uuid_v4_rejects_v1() {
        // Version nibble is 1, not 4.
        let v = json!("550e8400-e29b-11d4-a716-446655440000");
        let matches = check_value_patterns(&v);
        assert!(
            matches.iter().all(|m| m.pattern_name != PATTERN_UUID_V4),
            "v1 UUID should not match uuid_v4 pattern"
        );
    }

    #[test]
    fn pattern_iso8601_datetime() {
        let v = json!("2026-03-05T14:30:00Z");
        let matches = check_value_patterns(&v);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].pattern_name, PATTERN_ISO8601_DATETIME);
        assert_eq!(matches[0].confidence, Confidence::High);
    }

    #[test]
    fn pattern_iso8601_with_offset() {
        let v = json!("2026-03-05T14:30:00+05:30");
        let matches = check_value_patterns(&v);
        assert!(matches.iter().any(|m| m.pattern_name == PATTERN_ISO8601_DATETIME));
    }

    #[test]
    fn pattern_iso8601_rejects_date_only() {
        let v = json!("2026-03-05");
        let matches = check_value_patterns(&v);
        assert!(matches.iter().all(|m| m.pattern_name != PATTERN_ISO8601_DATETIME));
    }

    #[test]
    fn pattern_jwt_token() {
        // Real JWT structure: header.payload.signature (all base64url).
        let v = json!("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U");
        let matches = check_value_patterns(&v);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].pattern_name, PATTERN_JWT);
        assert_eq!(matches[0].confidence, Confidence::High);
    }

    #[test]
    fn pattern_jwt_rejects_non_jwt() {
        let v = json!("not.a.jwt");
        let matches = check_value_patterns(&v);
        assert!(matches.iter().all(|m| m.pattern_name != PATTERN_JWT));
    }

    #[test]
    fn pattern_sha256_hex() {
        let v = json!("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        let matches = check_value_patterns(&v);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].pattern_name, PATTERN_SHA256_HEX);
        assert_eq!(matches[0].confidence, Confidence::Medium);
    }

    #[test]
    fn pattern_random_hex_long() {
        // 48 hex chars — not SHA-256 length, so classified as random hex.
        let v = json!("a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6");
        let matches = check_value_patterns(&v);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].pattern_name, PATTERN_RANDOM_HEX);
        assert_eq!(matches[0].confidence, Confidence::Medium);
    }

    #[test]
    fn pattern_hex_too_short() {
        // 16 hex chars — below RANDOM_HEX_MIN_LEN threshold.
        let v = json!("a1b2c3d4e5f6a1b2");
        let matches = check_value_patterns(&v);
        assert!(matches.is_empty());
    }

    #[test]
    fn pattern_unix_timestamp_seconds() {
        // 2026-01-01 ~ 1767225600
        let v = json!(1_767_225_600_i64);
        let matches = check_value_patterns(&v);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].pattern_name, PATTERN_UNIX_TIMESTAMP_S);
        assert_eq!(matches[0].confidence, Confidence::Medium);
    }

    #[test]
    fn pattern_unix_timestamp_millis() {
        let v = json!(1_767_225_600_000_i64);
        let matches = check_value_patterns(&v);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].pattern_name, PATTERN_UNIX_TIMESTAMP_MS);
    }

    #[test]
    fn pattern_number_outside_timestamp_range() {
        let v = json!(42);
        let matches = check_value_patterns(&v);
        assert!(matches.is_empty());
    }

    #[test]
    fn pattern_null_and_bool() {
        assert!(check_value_patterns(&json!(null)).is_empty());
        assert!(check_value_patterns(&json!(true)).is_empty());
    }

    #[test]
    fn pattern_empty_string() {
        assert!(check_value_patterns(&json!("")).is_empty());
    }

    #[test]
    fn pattern_non_hex_string() {
        // Contains 'g' which is not hex.
        let v = json!("g1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4");
        let matches = check_value_patterns(&v);
        assert!(matches.iter().all(|m| m.pattern_name != PATTERN_RANDOM_HEX));
    }

    #[test]
    fn pattern_match_serialization_round_trip() {
        let m = ValuePatternMatch {
            pattern_name: PATTERN_UUID_V4.into(),
            confidence: Confidence::High,
        };
        let json_str = serde_json::to_string(&m).expect("serialize");
        let restored: ValuePatternMatch = serde_json::from_str(&json_str).expect("deserialize");
        assert_eq!(m, restored);
    }

    #[test]
    fn pattern_object_and_array_return_empty() {
        assert!(check_value_patterns(&json!({"id": "abc"})).is_empty());
        assert!(check_value_patterns(&json!([1, 2, 3])).is_empty());
    }

    // --- re-execution nondeterminism detection tests ---

    use crate::execution_record::ErrorInfo;
    use crate::protocol::PerformanceMetrics;

    fn make_exec_result(return_value: Option<Value>) -> ExecuteResult {
        ExecuteResult {
            return_value,
            thrown_error: None,
            branch_path: vec![],
            lines_executed: vec![],
            calls_to_external: vec![],
            path_constraints: vec![],
            scope_events: vec![],
            side_effects: vec![],
            performance: PerformanceMetrics::default(),
            capture_truncation: None, discovered_dependencies: vec![], connection_failures: vec![], runtime_crypto_boundaries: vec![],
        }
    }

    fn make_error_result(error_type: &str, message: &str) -> ExecuteResult {
        ExecuteResult {
            return_value: None,
            thrown_error: Some(ErrorInfo {
                error_type: error_type.to_string(),
                message: message.to_string(),
                stack: None,
                error_category: None,
            }),
            branch_path: vec![],
            lines_executed: vec![],
            calls_to_external: vec![],
            path_constraints: vec![],
            scope_events: vec![],
            side_effects: vec![],
            performance: PerformanceMetrics::default(),
            capture_truncation: None, discovered_dependencies: vec![], connection_failures: vec![], runtime_crypto_boundaries: vec![],
        }
    }

    #[test]
    fn reexec_all_deterministic() {
        let original = make_exec_result(Some(json!({"a": 1, "b": 2})));
        let reexecs = vec![
            make_exec_result(Some(json!({"a": 1, "b": 2}))),
            make_exec_result(Some(json!({"a": 1, "b": 2}))),
            make_exec_result(Some(json!({"a": 1, "b": 2}))),
        ];
        assert_eq!(reexecs.len(), NONDETERMINISM_REEXECUTION_COUNT);

        let report = detect_within_run_nondeterminism(&[(original, reexecs)]);
        assert!(report.nondeterministic_fields.is_empty());
        assert_eq!(report.inputs_sampled, 1);
        assert_eq!(report.reexecutions_performed, NONDETERMINISM_REEXECUTION_COUNT);
    }

    #[test]
    fn reexec_single_field_nondeterministic() {
        let original = make_exec_result(Some(json!({"a": 1, "b": 2})));
        let reexecs = vec![
            make_exec_result(Some(json!({"a": 1, "b": 99}))),
            make_exec_result(Some(json!({"a": 1, "b": 77}))),
            make_exec_result(Some(json!({"a": 1, "b": 55}))),
        ];

        let report = detect_within_run_nondeterminism(&[(original, reexecs)]);
        assert_eq!(report.nondeterministic_fields.len(), 1);
        assert_eq!(report.nondeterministic_fields[0].field_path, "return.b");
        assert_eq!(
            report.nondeterministic_fields[0].evidence,
            vec![NondeterminismEvidence::ObservedWithinRun]
        );
        // Changed in all 3 re-executions → High confidence.
        assert_eq!(report.nondeterministic_fields[0].confidence, Confidence::High);
    }

    #[test]
    fn reexec_fully_nondeterministic() {
        let original = make_exec_result(Some(json!({"a": 1, "b": 2})));
        let reexecs = vec![
            make_exec_result(Some(json!({"a": 10, "b": 20}))),
            make_exec_result(Some(json!({"a": 100, "b": 200}))),
            make_exec_result(Some(json!({"a": 1000, "b": 2000}))),
        ];

        let report = detect_within_run_nondeterminism(&[(original, reexecs)]);
        assert_eq!(report.nondeterministic_fields.len(), 2);
        let paths: Vec<&str> = report
            .nondeterministic_fields
            .iter()
            .map(|f| f.field_path.as_str())
            .collect();
        assert!(paths.contains(&"return.a"));
        assert!(paths.contains(&"return.b"));
    }

    #[test]
    fn reexec_error_type_variation() {
        let original = make_error_result("TypeError", "x is not a function");
        let reexecs = vec![
            make_error_result("TypeError", "x is not a function"),
            make_error_result("RangeError", "out of bounds"),
            make_error_result("TypeError", "x is not a function"),
        ];

        let report = detect_within_run_nondeterminism(&[(original, reexecs)]);
        assert_eq!(report.nondeterministic_fields.len(), 1);
        assert_eq!(
            report.nondeterministic_fields[0].field_path,
            FIELD_PATH_THROWN_ERROR
        );
    }

    #[test]
    fn reexec_outcome_mismatch() {
        // Original returns a value, re-execution throws.
        let original = make_exec_result(Some(json!(42)));
        let reexecs = vec![
            make_exec_result(Some(json!(42))),
            make_error_result("Error", "oops"),
            make_exec_result(Some(json!(42))),
        ];

        let report = detect_within_run_nondeterminism(&[(original, reexecs)]);
        assert_eq!(report.nondeterministic_fields.len(), 1);
        assert_eq!(
            report.nondeterministic_fields[0].field_path,
            FIELD_PATH_OUTCOME
        );
    }

    #[test]
    fn reexec_confidence_levels() {
        // Field changes in 1 of 3 re-executions → Low.
        let original = make_exec_result(Some(json!({"x": 1})));
        let reexecs = vec![
            make_exec_result(Some(json!({"x": 1}))),
            make_exec_result(Some(json!({"x": 99}))),
            make_exec_result(Some(json!({"x": 1}))),
        ];

        let report = detect_within_run_nondeterminism(&[(original, reexecs)]);
        assert_eq!(report.nondeterministic_fields.len(), 1);
        assert_eq!(report.nondeterministic_fields[0].confidence, Confidence::Low);
    }

    #[test]
    fn reexec_confidence_medium() {
        // Field changes in 2 of 3 re-executions → Medium (ratio 2/3 ≥ 0.5).
        let original = make_exec_result(Some(json!({"x": 1})));
        let reexecs = vec![
            make_exec_result(Some(json!({"x": 99}))),
            make_exec_result(Some(json!({"x": 77}))),
            make_exec_result(Some(json!({"x": 1}))),
        ];

        let report = detect_within_run_nondeterminism(&[(original, reexecs)]);
        assert_eq!(report.nondeterministic_fields.len(), 1);
        assert_eq!(
            report.nondeterministic_fields[0].confidence,
            Confidence::Medium
        );
    }

    #[test]
    fn reexec_multiple_samples_aggregate() {
        // Two different inputs both show "return.b" varying → merged into one field.
        let sample1 = (
            make_exec_result(Some(json!({"a": 1, "b": 2}))),
            vec![make_exec_result(Some(json!({"a": 1, "b": 99})))],
        );
        let sample2 = (
            make_exec_result(Some(json!({"a": 10, "b": 20}))),
            vec![make_exec_result(Some(json!({"a": 10, "b": 88})))],
        );

        let report = detect_within_run_nondeterminism(&[sample1, sample2]);
        assert_eq!(report.nondeterministic_fields.len(), 1);
        assert_eq!(report.nondeterministic_fields[0].field_path, "return.b");
        assert_eq!(report.inputs_sampled, NONDETERMINISM_SAMPLES_PER_CLASS);
        // Both re-executions showed change → 2/2 = High.
        assert_eq!(report.nondeterministic_fields[0].confidence, Confidence::High);
    }

    #[test]
    fn reexec_both_return_none_is_deterministic() {
        // Void functions with no errors.
        let original = make_exec_result(None);
        let reexecs = vec![
            make_exec_result(None),
            make_exec_result(None),
            make_exec_result(None),
        ];

        let report = detect_within_run_nondeterminism(&[(original, reexecs)]);
        assert!(report.nondeterministic_fields.is_empty());
    }

    #[test]
    fn reexec_empty_samples() {
        let report = detect_within_run_nondeterminism(&[]);
        assert!(report.nondeterministic_fields.is_empty());
        assert_eq!(report.inputs_sampled, 0);
        assert_eq!(report.reexecutions_performed, 0);
    }

    #[test]
    fn reexec_primitive_return_value_changes() {
        // Non-object return value that changes.
        let original = make_exec_result(Some(json!(42)));
        let reexecs = vec![
            make_exec_result(Some(json!(99))),
            make_exec_result(Some(json!(77))),
            make_exec_result(Some(json!(55))),
        ];

        let report = detect_within_run_nondeterminism(&[(original, reexecs)]);
        assert_eq!(report.nondeterministic_fields.len(), 1);
        // Primitive return value has empty changed_path → mapped to "return".
        assert_eq!(report.nondeterministic_fields[0].field_path, "return");
    }

    // --- select_sample_indices tests ---

    #[test]
    fn select_samples_empty_inputs() {
        assert!(select_sample_indices(&[], NONDETERMINISM_SAMPLES_PER_CLASS).is_empty());
    }

    #[test]
    fn select_samples_single_input() {
        let inputs = vec![vec![json!(1)]];
        let indices = select_sample_indices(&inputs, NONDETERMINISM_SAMPLES_PER_CLASS);
        assert_eq!(indices, vec![0]);
    }

    #[test]
    fn select_samples_picks_most_different() {
        let inputs = vec![
            vec![json!(1)],
            vec![json!(2)],
            vec![json!("a very long string that differs significantly in length")],
        ];
        let indices = select_sample_indices(&inputs, NONDETERMINISM_SAMPLES_PER_CLASS);
        assert_eq!(indices.len(), NONDETERMINISM_SAMPLES_PER_CLASS);
        assert_eq!(indices[0], 0);
        // Index 2 has the most different JSON length.
        assert_eq!(indices[1], 2);
    }

    // --- slow nondeterminism: date pattern tests ---

    #[test]
    fn pattern_date_iso_only() {
        let v = json!("2026-03-06");
        let matches = check_value_patterns(&v);
        assert!(matches.iter().any(|m| m.pattern_name == PATTERN_DATE_ONLY));
        assert!(matches.iter().any(|m| m.confidence == Confidence::Medium));
    }

    #[test]
    fn pattern_date_us_format() {
        let v = json!("03/06/2026");
        let matches = check_value_patterns(&v);
        assert!(matches.iter().any(|m| m.pattern_name == PATTERN_DATE_ONLY));
    }

    #[test]
    fn pattern_date_locale_month_first() {
        let v = json!("March 6, 2026");
        let matches = check_value_patterns(&v);
        assert!(matches.iter().any(|m| m.pattern_name == PATTERN_LOCALE_DATE));
    }

    #[test]
    fn pattern_date_locale_day_first() {
        let v = json!("6 March 2026");
        let matches = check_value_patterns(&v);
        assert!(matches.iter().any(|m| m.pattern_name == PATTERN_LOCALE_DATE));
    }

    #[test]
    fn pattern_date_locale_case_insensitive() {
        let v = json!("JANUARY 1, 2026");
        let matches = check_value_patterns(&v);
        assert!(matches.iter().any(|m| m.pattern_name == PATTERN_LOCALE_DATE));
    }

    #[test]
    fn pattern_date_rejects_partial() {
        // Not a complete date pattern.
        assert!(check_value_patterns(&json!("2026-03")).is_empty());
        assert!(check_value_patterns(&json!("March")).is_empty());
        assert!(check_value_patterns(&json!("hello world")).is_empty());
    }

    #[test]
    fn pattern_iso8601_still_works_separately() {
        // Full ISO 8601 datetime should match iso8601_datetime, NOT date_only.
        let v = json!("2026-03-06T12:00:00Z");
        let matches = check_value_patterns(&v);
        assert!(matches.iter().any(|m| m.pattern_name == PATTERN_ISO8601_DATETIME));
        assert!(matches.iter().all(|m| m.pattern_name != PATTERN_DATE_ONLY));
    }

    // --- slow nondeterminism: monotonic counter tests ---

    #[test]
    fn cross_run_monotonic_counter_three_values() {
        let values = vec![json!(1), json!(2), json!(3)];
        let matches = check_cross_run_patterns(&values);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].pattern_name, PATTERN_MONOTONIC_COUNTER);
        assert_eq!(matches[0].confidence, Confidence::Medium);
    }

    #[test]
    fn cross_run_monotonic_counter_two_values_low_confidence() {
        let values = vec![json!(100), json!(101)];
        let matches = check_cross_run_patterns(&values);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].pattern_name, PATTERN_MONOTONIC_COUNTER);
        assert_eq!(matches[0].confidence, Confidence::Low);
    }

    #[test]
    fn cross_run_monotonic_counter_large_step_within_bound() {
        let values = vec![json!(1), json!(50), json!(100)];
        let matches = check_cross_run_patterns(&values);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].pattern_name, PATTERN_MONOTONIC_COUNTER);
    }

    #[test]
    fn cross_run_monotonic_counter_step_too_large() {
        let values = vec![json!(1), json!(200), json!(400)];
        let matches = check_cross_run_patterns(&values);
        assert!(matches.is_empty());
    }

    #[test]
    fn cross_run_not_monotonic_decreasing() {
        let values = vec![json!(3), json!(2), json!(1)];
        let matches = check_cross_run_patterns(&values);
        assert!(matches.is_empty());
    }

    #[test]
    fn cross_run_not_monotonic_equal() {
        let values = vec![json!(5), json!(5), json!(5)];
        let matches = check_cross_run_patterns(&values);
        assert!(matches.is_empty());
    }

    #[test]
    fn cross_run_mixed_types_no_match() {
        let values = vec![json!(1), json!("two"), json!(3)];
        let matches = check_cross_run_patterns(&values);
        assert!(matches.is_empty());
    }

    #[test]
    fn cross_run_single_value_no_match() {
        let values = vec![json!(42)];
        let matches = check_cross_run_patterns(&values);
        assert!(matches.is_empty());
    }

    #[test]
    fn cross_run_empty_no_match() {
        let matches = check_cross_run_patterns(&[]);
        assert!(matches.is_empty());
    }

    // --- slow nondeterminism: environment value heuristic tests ---

    #[test]
    fn env_heuristic_hostname_string() {
        let m = check_env_value_heuristic("serverHostname", &json!("web-prod-01"));
        assert!(m.is_some());
        let m = m.unwrap();
        assert_eq!(m.pattern_name, PATTERN_ENV_VALUE);
        assert_eq!(m.confidence, Confidence::Low);
    }

    #[test]
    fn env_heuristic_pid_integer() {
        let m = check_env_value_heuristic("processPid", &json!(12345));
        assert!(m.is_some());
        assert_eq!(m.unwrap().pattern_name, PATTERN_ENV_VALUE);
    }

    #[test]
    fn env_heuristic_dot_path() {
        let m = check_env_value_heuristic("config.host", &json!("localhost"));
        assert!(m.is_some());
    }

    #[test]
    fn env_heuristic_unrelated_field() {
        assert!(check_env_value_heuristic("userName", &json!("alice")).is_none());
    }

    #[test]
    fn env_heuristic_empty_string_rejected() {
        assert!(check_env_value_heuristic("hostname", &json!("")).is_none());
    }

    #[test]
    fn env_heuristic_long_string_rejected() {
        let long = "a".repeat(ENV_VALUE_MAX_STRING_LEN + 1);
        assert!(check_env_value_heuristic("hostname", &json!(long)).is_none());
    }

    #[test]
    fn env_heuristic_negative_pid_rejected() {
        assert!(check_env_value_heuristic("pid", &json!(-1)).is_none());
    }

    #[test]
    fn env_heuristic_bool_rejected() {
        assert!(check_env_value_heuristic("hostname", &json!(true)).is_none());
    }

    // --- slow nondeterminism: name heuristic additions ---

    #[test]
    fn name_heuristic_date_suffix() {
        let r = check_name_heuristics("createdDate", &[]).unwrap();
        assert_eq!(r.matched_pattern, "date");
        assert!((r.confidence - 0.70).abs() < 1e-10);
    }

    #[test]
    fn name_heuristic_hostname_suffix() {
        let r = check_name_heuristics("serverHostname", &[]).unwrap();
        assert_eq!(r.matched_pattern, "hostname");
        assert!((r.confidence - 0.65).abs() < 1e-10);
    }

    #[test]
    fn name_heuristic_pid_suffix() {
        let r = check_name_heuristics("processPid", &[]).unwrap();
        assert_eq!(r.matched_pattern, "pid");
        assert!((r.confidence - 0.60).abs() < 1e-10);
    }

    #[test]
    fn name_heuristic_pid_bare_matches_id() {
        // "pid" doesn't match "pid" suffix (no prefix chars), but matches "id" suffix.
        let r = check_name_heuristics("pid", &[]).unwrap();
        assert_eq!(r.matched_pattern, "id");
    }

    // --- SlowPattern evidence serialization ---

    #[test]
    fn slow_pattern_evidence_round_trip() {
        let evidence = NondeterminismEvidence::SlowPattern {
            pattern_type: PATTERN_MONOTONIC_COUNTER.into(),
        };
        let json_str = serde_json::to_string(&evidence).expect("serialize");
        let restored: NondeterminismEvidence =
            serde_json::from_str(&json_str).expect("deserialize");
        assert_eq!(evidence, restored);
    }
}
