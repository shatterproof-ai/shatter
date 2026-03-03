//! Data structures for the cross-function interesting input pool.
//!
//! Values discovered during exploration of one function are pooled and reused
//! as seeds for other functions with matching parameter types. Entry identity
//! is the `(ty, value)` pair — behaviors accumulate across functions.

use serde::{Deserialize, Serialize};

use crate::types::TypeInfo;

/// How severe the behavior triggered by an input was.
///
/// Ordered low-to-high so that [`Ord`] gives natural severity comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Novel path exercised by very few inputs, no error.
    RarePath = 1,
    /// Thrown error with an application-defined exception type.
    HandledError = 2,
    /// Thrown error with a runtime error type (TypeError, panic, etc.).
    UnhandledError = 3,
    /// Frontend process died, timed out, or protocol error.
    Crash = 4,
}

/// A single behavior observed when running a particular input against a function.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BehaviorObservation {
    /// Fully qualified function identifier.
    pub function: String,
    /// Branch point that was exercised.
    pub branch_id: u32,
    /// Severity of the observed behavior.
    pub severity: Severity,
}

/// Grouping key for deduplication and eviction decisions.
///
/// Two observations with the same `BehaviorSig` are considered redundant
/// witnesses to the same behavior.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BehaviorSig {
    /// Fully qualified function identifier.
    pub function_id: String,
    /// Branch point that was exercised.
    pub branch_id: u32,
    /// Severity of the observed behavior.
    pub severity: Severity,
}

impl From<&BehaviorObservation> for BehaviorSig {
    fn from(obs: &BehaviorObservation) -> Self {
        Self {
            function_id: obs.function.clone(),
            branch_id: obs.branch_id,
            severity: obs.severity,
        }
    }
}

/// A single entry in the interesting input pool.
///
/// Identity is the `(ty, value)` pair. When the same value is observed to
/// trigger interesting behavior in a different function, its `behaviors`
/// vector grows rather than creating a duplicate entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PoolEntry {
    /// The concrete input value (JSON-encoded).
    pub value: serde_json::Value,
    /// Type of the value, used for matching against function parameters.
    #[serde(rename = "type")]
    pub ty: TypeInfo,
    /// All interesting behaviors this value has triggered across functions.
    pub behaviors: Vec<BehaviorObservation>,
    /// Epoch at which this entry was first added to the pool.
    pub discovered_epoch: u64,
    /// Most recent epoch at which this entry triggered a new behavior.
    pub last_hit_epoch: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_ordering() {
        assert!(Severity::RarePath < Severity::HandledError);
        assert!(Severity::HandledError < Severity::UnhandledError);
        assert!(Severity::UnhandledError < Severity::Crash);
    }

    #[test]
    fn behavior_sig_from_observation() {
        let obs = BehaviorObservation {
            function: "myModule.foo".into(),
            branch_id: 3,
            severity: Severity::UnhandledError,
        };
        let sig = BehaviorSig::from(&obs);
        assert_eq!(sig.function_id, "myModule.foo");
        assert_eq!(sig.branch_id, 3);
        assert_eq!(sig.severity, Severity::UnhandledError);
    }

    #[test]
    fn pool_entry_serde_round_trip() {
        let entry = PoolEntry {
            value: serde_json::json!(42),
            ty: TypeInfo::Int,
            behaviors: vec![BehaviorObservation {
                function: "mod.bar".into(),
                branch_id: 1,
                severity: Severity::Crash,
            }],
            discovered_epoch: 0,
            last_hit_epoch: 1,
        };
        let json = serde_json::to_string(&entry).expect("serialize");
        let back: PoolEntry = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(entry, back);
    }

    #[test]
    fn behavior_sig_hash_equality() {
        use std::collections::HashSet;
        let sig1 = BehaviorSig {
            function_id: "f".into(),
            branch_id: 1,
            severity: Severity::RarePath,
        };
        let sig2 = sig1.clone();
        let mut set = HashSet::new();
        set.insert(sig1);
        assert!(set.contains(&sig2));
    }
}
