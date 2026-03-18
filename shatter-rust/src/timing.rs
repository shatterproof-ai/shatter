use std::collections::BTreeMap;
use std::time::Instant;

use crate::protocol::{TimingPhaseSummary, TimingSummary};

struct ActivePhase {
    phase_path: String,
    start: Instant,
    child_ms: f64,
}

/// Lightweight per-request timing collector for the Rust frontend.
#[derive(Default)]
pub struct TimingCollector {
    active: Vec<ActivePhase>,
    phases: BTreeMap<String, TimingPhaseSummary>,
}

impl TimingCollector {
    /// Record a phase while executing `f`, allowing nested timing in the same collector.
    pub fn record<T>(&mut self, phase_path: &str, f: impl FnOnce(&mut Self) -> T) -> T {
        self.active.push(ActivePhase {
            phase_path: phase_path.to_string(),
            start: Instant::now(),
            child_ms: 0.0,
        });
        let result = f(self);
        self.finish(phase_path);
        result
    }

    /// Convert the aggregated phase data into the shared protocol summary.
    pub fn summary(&self) -> Option<TimingSummary> {
        if self.phases.is_empty() {
            return None;
        }

        Some(TimingSummary {
            phases: self.phases.values().cloned().collect(),
        })
    }

    fn finish(&mut self, phase_path: &str) {
        let phase = self.active.pop().expect("timing phase stack underflow");
        assert_eq!(
            phase.phase_path, phase_path,
            "timing phase stack mismatch for {phase_path}"
        );

        let total_ms = phase.start.elapsed().as_secs_f64() * 1000.0;
        let self_ms = (total_ms - phase.child_ms).max(0.0);

        self.phases
            .entry(phase.phase_path)
            .and_modify(|existing| {
                existing.total_ms += total_ms;
                existing.self_ms += self_ms;
                existing.count += 1;
            })
            .or_insert_with(|| TimingPhaseSummary {
                phase_path: phase_path.to_string(),
                total_ms,
                self_ms,
                count: 1,
                attributes: BTreeMap::new(),
            });

        if let Some(parent) = self.active.last_mut() {
            parent.child_ms += total_ms;
        }
    }
}
