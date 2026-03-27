//! Benchmark harness types and utilities.
//!
//! Defines the manifest format, output schema, and statistical helpers
//! for the `shatter bench` CLI command. The actual benchmark execution
//! loop lives in `shatter-cli`; this module provides the shared types
//! and pure-logic helpers.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::timing::TimingPhaseSummary;

// ---------------------------------------------------------------------------
// Manifest types
// ---------------------------------------------------------------------------

/// Top-level structure of `benchmarks/sample-manifest.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchManifest {
    /// Perf tiers: `smoke`, `standard`, `full`.
    /// Each tier maps language name → vec of `"file:function"` strings.
    #[serde(default)]
    pub perf: BTreeMap<String, BTreeMap<String, Vec<String>>>,

    /// Non-perf groups (e.g. `walkthrough`) are silently ignored.
    #[serde(flatten)]
    pub _other: BTreeMap<String, serde_json::Value>,
}

/// A resolved benchmark target: file path, function name, and language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchTarget {
    pub file: String,
    pub function: String,
    pub language: String,
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Parameters controlling a benchmark run.
#[derive(Debug, Clone)]
pub struct BenchConfig {
    pub tier: String,
    pub repeats: u32,
    pub warmups: u32,
    pub max_iterations: u32,
    pub request_timeout_secs: u64,
    pub exec_timeout_secs: u64,
    pub build_timeout_secs: u64,
}

// ---------------------------------------------------------------------------
// Output schema
// ---------------------------------------------------------------------------

/// Root output artifact from a benchmark run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkBundle {
    pub schema_version: u32,
    pub bundle_id: String,
    pub started_at_unix_ms: u128,
    pub finished_at_unix_ms: u128,
    pub manifest_path: String,
    pub tier: String,
    pub repeats: u32,
    pub warmups: u32,
    pub max_iterations: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_commit: Option<String>,
    pub scenarios: Vec<ScenarioResult>,
}

/// Timing results for a single target function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioResult {
    /// `"file:function"` identifier.
    pub target: String,
    pub language: String,
    pub runs: Vec<RunMeasurement>,
    pub stats: RunStatistics,
}

/// A single benchmark repetition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunMeasurement {
    pub sequence: u32,
    pub is_warmup: bool,
    pub duration_ms: f64,
    pub iterations: u32,
    pub unique_paths: usize,
    pub exit_ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub phases: Vec<TimingPhaseSummary>,
}

/// Aggregate statistics over measured (non-warmup) runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunStatistics {
    pub measured_count: u32,
    pub duration_ms: StatSummary,
    pub iterations: StatSummary,
    pub unique_paths: StatSummary,
}

/// Five-number summary for a metric.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatSummary {
    pub min: f64,
    pub max: f64,
    pub mean: f64,
    pub median: f64,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum BenchError {
    #[error("failed to read manifest: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse manifest JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unknown tier {0:?}; available tiers: {1}")]
    UnknownTier(String, String),
    #[error("tier {0:?} has no targets")]
    EmptyTier(String),
    #[error("invalid target entry {0:?}: expected \"file:function\" format")]
    InvalidTarget(String),
}

// ---------------------------------------------------------------------------
// Manifest parsing
// ---------------------------------------------------------------------------

/// Load and validate a benchmark manifest from disk.
pub fn load_manifest(path: &Path) -> Result<BenchManifest, BenchError> {
    let data = std::fs::read_to_string(path)?;
    let manifest: BenchManifest = serde_json::from_str(&data)?;
    Ok(manifest)
}

/// Extract all benchmark targets for a given tier.
pub fn resolve_targets(
    manifest: &BenchManifest,
    tier: &str,
) -> Result<Vec<BenchTarget>, BenchError> {
    let tier_map = manifest.perf.get(tier).ok_or_else(|| {
        let available = manifest
            .perf
            .keys()
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        BenchError::UnknownTier(tier.to_string(), available)
    })?;

    let mut targets = Vec::new();
    for (language, entries) in tier_map {
        for entry in entries {
            let (file, function) = parse_target_entry(entry)?;
            targets.push(BenchTarget {
                file,
                function,
                language: language.clone(),
            });
        }
    }

    if targets.is_empty() {
        return Err(BenchError::EmptyTier(tier.to_string()));
    }

    Ok(targets)
}

fn parse_target_entry(entry: &str) -> Result<(String, String), BenchError> {
    let colon_pos = entry.rfind(':').ok_or_else(|| {
        BenchError::InvalidTarget(entry.to_string())
    })?;
    // Ensure neither side is empty
    if colon_pos == 0 || colon_pos == entry.len() - 1 {
        return Err(BenchError::InvalidTarget(entry.to_string()));
    }
    Ok((entry[..colon_pos].to_string(), entry[colon_pos + 1..].to_string()))
}

// ---------------------------------------------------------------------------
// Statistics
// ---------------------------------------------------------------------------

/// Compute min, max, mean, and median from a non-empty slice of values.
///
/// # Panics
///
/// Panics if `values` is empty.
pub fn compute_statistics(values: &[f64]) -> StatSummary {
    assert!(!values.is_empty(), "compute_statistics requires non-empty input");

    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let min = sorted[0];
    let max = sorted[sorted.len() - 1];
    let mean = sorted.iter().sum::<f64>() / sorted.len() as f64;
    let median = if sorted.len() % 2 == 1 {
        sorted[sorted.len() / 2]
    } else {
        (sorted[sorted.len() / 2 - 1] + sorted[sorted.len() / 2]) / 2.0
    };

    StatSummary { min, max, mean, median }
}

/// Attempt to detect the current git commit hash.
pub fn detect_git_commit() -> Option<String> {
    std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_real_manifest() {
        let manifest_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("benchmarks/sample-manifest.json");
        if !manifest_path.exists() {
            // Skip in CI environments where the manifest may not be present.
            return;
        }
        let manifest = load_manifest(&manifest_path).unwrap();
        assert!(manifest.perf.contains_key("smoke"));
        assert!(manifest.perf.contains_key("standard"));
        assert!(manifest.perf.contains_key("full"));
    }

    #[test]
    fn resolve_targets_smoke() {
        let manifest_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("benchmarks/sample-manifest.json");
        if !manifest_path.exists() {
            return;
        }
        let manifest = load_manifest(&manifest_path).unwrap();
        let targets = resolve_targets(&manifest, "smoke").unwrap();
        // smoke: 1 per language × 3 languages
        assert_eq!(targets.len(), 3);
    }

    #[test]
    fn resolve_targets_standard() {
        let manifest_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("benchmarks/sample-manifest.json");
        if !manifest_path.exists() {
            return;
        }
        let manifest = load_manifest(&manifest_path).unwrap();
        let targets = resolve_targets(&manifest, "standard").unwrap();
        // standard: 4 per language × 3 languages
        assert_eq!(targets.len(), 12);
    }

    #[test]
    fn resolve_targets_full() {
        let manifest_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("benchmarks/sample-manifest.json");
        if !manifest_path.exists() {
            return;
        }
        let manifest = load_manifest(&manifest_path).unwrap();
        let targets = resolve_targets(&manifest, "full").unwrap();
        // full: 17 per language × 3 languages
        assert_eq!(targets.len(), 51);
    }

    #[test]
    fn resolve_targets_unknown_tier() {
        let manifest_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("benchmarks/sample-manifest.json");
        if !manifest_path.exists() {
            return;
        }
        let manifest = load_manifest(&manifest_path).unwrap();
        let result = resolve_targets(&manifest, "nonexistent");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("nonexistent"));
    }

    #[test]
    fn parse_target_entry_valid() {
        let (file, func) = parse_target_entry("examples/ts/01-arithmetic.ts:classifyNumber").unwrap();
        assert_eq!(file, "examples/ts/01-arithmetic.ts");
        assert_eq!(func, "classifyNumber");
    }

    #[test]
    fn parse_target_entry_invalid_no_colon() {
        assert!(parse_target_entry("no_colon_here").is_err());
    }

    #[test]
    fn parse_target_entry_invalid_empty_function() {
        assert!(parse_target_entry("file.ts:").is_err());
    }

    #[test]
    fn compute_statistics_known_answer() {
        let values = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let stats = compute_statistics(&values);
        assert!((stats.min - 1.0).abs() < f64::EPSILON);
        assert!((stats.max - 5.0).abs() < f64::EPSILON);
        assert!((stats.mean - 3.0).abs() < f64::EPSILON);
        assert!((stats.median - 3.0).abs() < f64::EPSILON);
    }

    #[test]
    fn compute_statistics_even_count() {
        let values = vec![1.0, 2.0, 3.0, 4.0];
        let stats = compute_statistics(&values);
        assert!((stats.median - 2.5).abs() < f64::EPSILON);
    }

    #[test]
    fn compute_statistics_single_value() {
        let stats = compute_statistics(&[42.0]);
        assert!((stats.min - 42.0).abs() < f64::EPSILON);
        assert!((stats.max - 42.0).abs() < f64::EPSILON);
        assert!((stats.mean - 42.0).abs() < f64::EPSILON);
        assert!((stats.median - 42.0).abs() < f64::EPSILON);
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn statistics_invariants(values in proptest::collection::vec(
            -1e100f64..1e100f64, 1..100usize
        )) {
            let stats = compute_statistics(&values);
            prop_assert!(stats.min <= stats.median, "min ({}) > median ({})", stats.min, stats.median);
            prop_assert!(stats.median <= stats.max, "median ({}) > max ({})", stats.median, stats.max);
            prop_assert!(stats.min <= stats.mean, "min ({}) > mean ({})", stats.min, stats.mean);
            prop_assert!(stats.mean <= stats.max, "mean ({}) > max ({})", stats.mean, stats.max);
        }

        #[test]
        fn roundtrip_benchmark_bundle(
            repeats in 1..10u32,
            warmups in 0..5u32,
            max_iter in 1..100u32,
        ) {
            let bundle = BenchmarkBundle {
                schema_version: 1,
                bundle_id: "test-id".into(),
                started_at_unix_ms: 1000,
                finished_at_unix_ms: 2000,
                manifest_path: "test.json".into(),
                tier: "smoke".into(),
                repeats,
                warmups,
                max_iterations: max_iter,
                git_commit: Some("abc123".into()),
                scenarios: vec![],
            };
            let json = serde_json::to_string(&bundle).unwrap();
            let decoded: BenchmarkBundle = serde_json::from_str(&json).unwrap();
            prop_assert_eq!(decoded.schema_version, 1);
            prop_assert_eq!(decoded.repeats, repeats);
            prop_assert_eq!(decoded.warmups, warmups);
            prop_assert_eq!(decoded.max_iterations, max_iter);
        }
    }
}
