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

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn arb_phase_path() -> impl Strategy<Value = String> {
        prop::string::string_regex("[a-z][a-z0-9_]*(\\.[a-z][a-z0-9_]*){0,3}")
            .expect("valid regex")
    }

    fn arb_timing_phase_summary() -> impl Strategy<Value = TimingPhaseSummary> {
        (
            arb_phase_path(),
            0.0f64..1e6,
            0.0f64..1e6,
            1u64..1000,
            prop::collection::btree_map("[a-z]{1,8}", "[a-z0-9]{1,16}", 0..3),
        )
            .prop_map(|(phase_path, total_ms, self_ms, count, attributes)| {
                TimingPhaseSummary {
                    phase_path,
                    total_ms,
                    self_ms: self_ms.min(total_ms),
                    count,
                    attributes,
                }
            })
    }

    fn arb_timing_summary() -> impl Strategy<Value = TimingSummary> {
        prop::collection::vec(arb_timing_phase_summary(), 0..5).prop_map(|phases| {
            // Deduplicate by phase_path (BTreeMap behavior in real collector)
            let mut seen = BTreeMap::new();
            for p in phases {
                seen.entry(p.phase_path.clone()).or_insert(p);
            }
            TimingSummary {
                phases: seen.into_values().collect(),
            }
        })
    }

    /// Compare two f64 values allowing for JSON roundtrip precision loss.
    fn f64_eq_json(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-10 * a.abs().max(b.abs()).max(1.0)
    }

    fn timing_phase_eq(a: &TimingPhaseSummary, b: &TimingPhaseSummary) -> bool {
        a.phase_path == b.phase_path
            && f64_eq_json(a.total_ms, b.total_ms)
            && f64_eq_json(a.self_ms, b.self_ms)
            && a.count == b.count
            && a.attributes == b.attributes
    }

    proptest! {
        #[test]
        fn timing_phase_summary_roundtrip(phase in arb_timing_phase_summary()) {
            let json = serde_json::to_string(&phase).expect("serialize");
            let deserialized: TimingPhaseSummary =
                serde_json::from_str(&json).expect("deserialize");
            prop_assert!(timing_phase_eq(&phase, &deserialized),
                "roundtrip mismatch: {:?} vs {:?}", phase, deserialized);
        }

        #[test]
        fn timing_summary_roundtrip(summary in arb_timing_summary()) {
            let json = serde_json::to_string(&summary).expect("serialize");
            let deserialized: TimingSummary =
                serde_json::from_str(&json).expect("deserialize");
            prop_assert_eq!(summary.phases.len(), deserialized.phases.len());
            for (a, b) in summary.phases.iter().zip(deserialized.phases.iter()) {
                prop_assert!(timing_phase_eq(a, b),
                    "phase roundtrip mismatch: {:?} vs {:?}", a, b);
            }
        }

        #[test]
        fn self_ms_never_exceeds_total_ms(phase in arb_timing_phase_summary()) {
            prop_assert!(phase.self_ms <= phase.total_ms);
        }

        #[test]
        fn count_aggregation(n in 1usize..10) {
            let mut collector = TimingCollector::default();
            for _ in 0..n {
                collector.record("repeat.phase", |_| ());
            }
            let summary = collector.summary().expect("non-empty");
            let phase = &summary.phases[0];
            prop_assert_eq!(phase.count, n as u64);
            prop_assert_eq!(&phase.phase_path, "repeat.phase");
        }
    }

    #[test]
    fn empty_collector_returns_none() {
        let collector = TimingCollector::default();
        assert!(collector.summary().is_none());
    }

    #[test]
    fn nested_timing_parent_self_excludes_child() {
        let mut collector = TimingCollector::default();
        collector.record("parent", |c| {
            c.record("child", |_| {
                std::thread::sleep(std::time::Duration::from_millis(5));
            });
        });
        let summary = collector.summary().expect("non-empty");
        let parent = summary.phases.iter().find(|p| p.phase_path == "parent").unwrap();
        let child = summary.phases.iter().find(|p| p.phase_path == "child").unwrap();
        assert!(parent.total_ms >= child.total_ms);
        assert!(parent.self_ms < parent.total_ms);
    }
}
