//! Cumulative state across progressive batch runs.
//!
//! When a scan uses `--batch 0`, then `--batch 1`, etc., this module persists
//! per-batch summaries and cumulative coverage metrics so the user can track
//! overall progress across separate CLI invocations. The state file is separate
//! from the crash-recovery [`ScanCheckpoint`](crate::checkpoint::ScanCheckpoint)
//! which tracks progress within a single run.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::coverage_metrics::CoverageMetrics;
use crate::scan_orchestrator::ParallelScanResult;

/// Format version for forward compatibility.
const BATCH_STATE_VERSION: &str = "1";

/// Errors during batch state I/O.
#[derive(Debug, thiserror::Error)]
pub enum BatchStateError {
    #[error("batch state I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("batch state parse error: {0}")]
    Parse(#[from] serde_json::Error),
}

/// Summary of a single completed batch.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BatchSummary {
    /// Batch index (e.g. 0, 1, 2).
    pub batch_index: usize,
    /// Functions explored in this batch.
    pub functions_explored: Vec<String>,
    /// Aggregated coverage metrics for this batch.
    pub metrics: CoverageMetrics,
    /// Number of functions skipped in this batch.
    pub skipped_count: usize,
    /// Unix timestamp of completion.
    pub timestamp: u64,
}

impl BatchSummary {
    /// Build from a completed parallel scan result.
    pub fn from_scan_result(batch_index: usize, result: &ParallelScanResult) -> Self {
        let functions_explored: Vec<String> = result
            .function_results
            .iter()
            .map(|fr| fr.function_name.clone())
            .collect();

        let mut metrics = CoverageMetrics::default();
        for fr in &result.function_results {
            metrics.merge(&fr.coverage_metrics);
        }

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        BatchSummary {
            batch_index,
            functions_explored,
            metrics,
            skipped_count: result.skipped.len(),
            timestamp,
        }
    }
}

/// Cumulative state across progressive batch runs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BatchState {
    /// Format version.
    pub version: String,
    /// Scan ID (SHA256 of sorted file paths) — validates batches belong to same scan.
    pub scan_id: String,
    /// Per-batch summaries, keyed by batch index.
    pub batches: HashMap<usize, BatchSummary>,
    /// Cumulative coverage metrics (sum across all batches).
    pub cumulative_metrics: CoverageMetrics,
    /// Total functions across the entire scan scope (not just explored).
    pub total_scope_functions: usize,
}

impl BatchState {
    pub fn new(scan_id: String, total_scope_functions: usize) -> Self {
        Self {
            version: BATCH_STATE_VERSION.to_string(),
            scan_id,
            batches: HashMap::new(),
            cumulative_metrics: CoverageMetrics::default(),
            total_scope_functions,
        }
    }

    /// Load from disk. Returns `Ok(None)` if the file does not exist.
    pub fn load(path: &Path) -> Result<Option<Self>, BatchStateError> {
        match std::fs::read_to_string(path) {
            Ok(contents) => {
                let state: BatchState = serde_json::from_str(&contents)?;
                Ok(Some(state))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(BatchStateError::Io(e)),
        }
    }

    /// Atomic save (write to temp file, then rename).
    pub fn save(&self, path: &Path) -> Result<(), BatchStateError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        let tmp_path = path.with_extension("batch-state.tmp");
        std::fs::write(&tmp_path, json)?;
        std::fs::rename(&tmp_path, path)?;
        Ok(())
    }

    /// Record a completed batch and recompute cumulative metrics.
    pub fn record_batch(&mut self, summary: BatchSummary) {
        self.batches.insert(summary.batch_index, summary);
        self.recompute_cumulative();
    }

    fn recompute_cumulative(&mut self) {
        let mut cumulative = CoverageMetrics::default();
        for summary in self.batches.values() {
            cumulative.merge(&summary.metrics);
        }
        self.cumulative_metrics = cumulative;
    }

    /// Sorted list of completed batch indices.
    pub fn completed_batches(&self) -> Vec<usize> {
        let mut indices: Vec<usize> = self.batches.keys().copied().collect();
        indices.sort_unstable();
        indices
    }

    /// Total functions explored across all batches.
    pub fn total_functions_explored(&self) -> usize {
        self.batches
            .values()
            .map(|b| b.functions_explored.len())
            .sum()
    }

    /// Overall coverage percentage across all batches.
    pub fn cumulative_coverage_pct(&self) -> f64 {
        let m = &self.cumulative_metrics;
        if m.total_branches == 0 {
            return 0.0;
        }
        let covered = m.z3_solved + m.random_found + m.user_provided;
        covered as f64 / m.total_branches as f64 * 100.0
    }
}

/// Format the cumulative batch progress section for terminal output.
pub fn format_cumulative_batch_section(state: &BatchState, current_batch: usize) -> String {
    let mut out = String::new();

    let completed = state.completed_batches();
    out.push_str(&format!(
        "\n--- Cumulative progress (batch {current_batch}) ---\n"
    ));
    out.push_str(&format!(
        "  Batches completed: {} ({})\n",
        completed.len(),
        completed
            .iter()
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(", "),
    ));
    out.push_str(&format!(
        "  Functions explored: {}/{}\n",
        state.total_functions_explored(),
        state.total_scope_functions,
    ));
    let cum = &state.cumulative_metrics;
    let covered = cum.z3_solved + cum.random_found + cum.user_provided;
    out.push_str(&format!(
        "  Branches: {} covered / {} total ({:.1}%)\n",
        covered,
        cum.total_branches,
        state.cumulative_coverage_pct(),
    ));
    out.push_str(&format!(
        "    Z3: {}, Random: {}, User: {}, Uncovered: {}\n",
        cum.z3_solved, cum.random_found, cum.user_provided, cum.uncovered,
    ));

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_summary(index: usize, functions: &[&str], branches: usize, z3: usize) -> BatchSummary {
        BatchSummary {
            batch_index: index,
            functions_explored: functions.iter().map(|s| s.to_string()).collect(),
            metrics: CoverageMetrics {
                total_branches: branches,
                z3_solved: z3,
                random_found: 0,
                user_provided: 0,
                uncovered: branches.saturating_sub(z3),
                symexpr_count: branches,
                unknown_count: 0,
            },
            skipped_count: 0,
            timestamp: 1000,
        }
    }

    #[test]
    fn round_trip_save_load() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("batch-state.json");

        let mut state = BatchState::new("scan123".to_string(), 20);
        state.record_batch(make_summary(0, &["foo", "bar"], 10, 6));

        state.save(&path).expect("save");
        let loaded = BatchState::load(&path).expect("load").expect("exists");
        assert_eq!(state, loaded);
    }

    #[test]
    fn load_missing_returns_none() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("nonexistent.json");
        let loaded = BatchState::load(&path).expect("load");
        assert!(loaded.is_none());
    }

    #[test]
    fn record_batch_accumulates_metrics() {
        let mut state = BatchState::new("scan1".to_string(), 30);
        state.record_batch(make_summary(0, &["a", "b"], 10, 4));
        state.record_batch(make_summary(1, &["c"], 8, 3));

        assert_eq!(state.cumulative_metrics.total_branches, 18);
        assert_eq!(state.cumulative_metrics.z3_solved, 7);
        assert_eq!(state.cumulative_metrics.uncovered, 11);
        assert_eq!(state.total_functions_explored(), 3);
    }

    #[test]
    fn completed_batches_sorted() {
        let mut state = BatchState::new("s".to_string(), 10);
        state.record_batch(make_summary(2, &["c"], 3, 1));
        state.record_batch(make_summary(0, &["a"], 4, 2));
        state.record_batch(make_summary(1, &["b"], 3, 1));

        assert_eq!(state.completed_batches(), vec![0, 1, 2]);
    }

    #[test]
    fn cumulative_coverage_pct_zero_branches() {
        let state = BatchState::new("s".to_string(), 5);
        assert_eq!(state.cumulative_coverage_pct(), 0.0);
    }

    #[test]
    fn cumulative_coverage_pct_computed() {
        let mut state = BatchState::new("s".to_string(), 10);
        state.record_batch(make_summary(0, &["a"], 10, 7));
        assert!((state.cumulative_coverage_pct() - 70.0).abs() < 0.01);
    }

    #[test]
    fn re_running_same_batch_replaces_previous() {
        let mut state = BatchState::new("s".to_string(), 10);
        state.record_batch(make_summary(0, &["a"], 10, 4));
        assert_eq!(state.cumulative_metrics.z3_solved, 4);

        // Re-run batch 0 with better results
        state.record_batch(make_summary(0, &["a"], 10, 7));
        assert_eq!(state.cumulative_metrics.z3_solved, 7);
        assert_eq!(state.batches.len(), 1);
    }

    #[test]
    fn save_creates_parent_directories() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("nested").join("deep").join("batch-state.json");

        let state = BatchState::new("s".to_string(), 5);
        state.save(&path).expect("save");
        assert!(path.exists());
    }

    #[test]
    fn format_cumulative_section_includes_key_info() {
        let mut state = BatchState::new("s".to_string(), 20);
        state.record_batch(make_summary(0, &["a", "b"], 10, 6));
        state.record_batch(make_summary(1, &["c"], 5, 3));

        let output = format_cumulative_batch_section(&state, 1);
        assert!(output.contains("batch 1"));
        assert!(output.contains("Batches completed: 2"));
        assert!(output.contains("0, 1"));
        assert!(output.contains("Functions explored: 3/20"));
        assert!(output.contains("9 covered / 15 total"));
    }
}
