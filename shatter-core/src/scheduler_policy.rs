//! Scheduler policy: centralized overlap rules for scan exploration.
//!
//! A [`SchedulerPolicy`] determines which exploration tasks may run
//! concurrently and how many workers are needed. All scheduling eligibility
//! decisions flow through this type so they can evolve (e.g., toward
//! profiling-driven adaptive modes) without touching the orchestration loop.

/// Controls which exploration tasks may overlap during a scan.
///
/// Policy is consulted by the scan orchestrator before spawning workers.
/// All overlap decisions should go through [`SchedulerPolicy::may_overlap`]
/// and [`SchedulerPolicy::effective_workers`] rather than being determined ad hoc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SchedulerPolicy {
    /// Execute one function at a time, in strict dependency order.
    ///
    /// This is the conservative baseline: no two functions ever run
    /// concurrently. Useful when resource contention, determinism, or
    /// debuggability matters more than throughput.
    Serial,

    /// Functions within the same topological layer may run concurrently.
    ///
    /// Functions in different layers are always serialized (each layer
    /// completes before the next starts), because later layers depend on
    /// behavior maps produced by earlier layers.
    #[default]
    LayerParallel,
}

impl SchedulerPolicy {
    /// Returns `true` if two functions (identified by their layer index) are
    /// eligible to run concurrently under this policy.
    ///
    /// - Under [`Serial`][SchedulerPolicy::Serial]: always `false`.
    /// - Under [`LayerParallel`][SchedulerPolicy::LayerParallel]: `true` only
    ///   when both functions are in the same topological layer (`layer_a == layer_b`).
    pub fn may_overlap(&self, layer_a: usize, layer_b: usize) -> bool {
        match self {
            Self::Serial => false,
            Self::LayerParallel => layer_a == layer_b,
        }
    }

    /// Returns the number of workers that should be spawned for this policy.
    ///
    /// Under [`Serial`][SchedulerPolicy::Serial] this is always `1`, regardless
    /// of the user-configured value, enforcing sequential execution through the
    /// single-worker pool.
    ///
    /// Under [`LayerParallel`][SchedulerPolicy::LayerParallel] the configured
    /// count is returned unchanged (caller is responsible for clamping to ≥ 1).
    pub fn effective_workers(&self, configured: usize) -> usize {
        match self {
            Self::Serial => 1,
            Self::LayerParallel => configured,
        }
    }
}

impl std::fmt::Display for SchedulerPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Serial => write!(f, "serial"),
            Self::LayerParallel => write!(f, "layer-parallel"),
        }
    }
}

impl std::str::FromStr for SchedulerPolicy {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "serial" => Ok(Self::Serial),
            "layer-parallel" => Ok(Self::LayerParallel),
            other => Err(format!(
                "unknown scheduler policy {:?}; expected \"serial\" or \"layer-parallel\"",
                other
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serial_never_overlaps() {
        assert!(!SchedulerPolicy::Serial.may_overlap(0, 0));
        assert!(!SchedulerPolicy::Serial.may_overlap(0, 1));
        assert!(!SchedulerPolicy::Serial.may_overlap(2, 2));
    }

    #[test]
    fn layer_parallel_same_layer_overlaps() {
        assert!(SchedulerPolicy::LayerParallel.may_overlap(0, 0));
        assert!(SchedulerPolicy::LayerParallel.may_overlap(3, 3));
    }

    #[test]
    fn layer_parallel_different_layers_no_overlap() {
        assert!(!SchedulerPolicy::LayerParallel.may_overlap(0, 1));
        assert!(!SchedulerPolicy::LayerParallel.may_overlap(1, 2));
        assert!(!SchedulerPolicy::LayerParallel.may_overlap(2, 0));
    }

    #[test]
    fn serial_effective_workers_always_one() {
        assert_eq!(SchedulerPolicy::Serial.effective_workers(0), 1);
        assert_eq!(SchedulerPolicy::Serial.effective_workers(1), 1);
        assert_eq!(SchedulerPolicy::Serial.effective_workers(8), 1);
        assert_eq!(SchedulerPolicy::Serial.effective_workers(100), 1);
    }

    #[test]
    fn layer_parallel_effective_workers_passes_through() {
        assert_eq!(SchedulerPolicy::LayerParallel.effective_workers(0), 0);
        assert_eq!(SchedulerPolicy::LayerParallel.effective_workers(1), 1);
        assert_eq!(SchedulerPolicy::LayerParallel.effective_workers(4), 4);
        assert_eq!(SchedulerPolicy::LayerParallel.effective_workers(16), 16);
    }

    #[test]
    fn default_is_layer_parallel() {
        assert_eq!(SchedulerPolicy::default(), SchedulerPolicy::LayerParallel);
    }

    #[test]
    fn display_round_trip() {
        for policy in [SchedulerPolicy::Serial, SchedulerPolicy::LayerParallel] {
            let s = policy.to_string();
            let parsed: SchedulerPolicy = s.parse().expect("should parse");
            assert_eq!(parsed, policy);
        }
    }

    #[test]
    fn from_str_invalid_returns_error() {
        assert!("parallel".parse::<SchedulerPolicy>().is_err());
        assert!("".parse::<SchedulerPolicy>().is_err());
        assert!("SERIAL".parse::<SchedulerPolicy>().is_err());
    }
}
