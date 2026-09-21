//! Decision oracle (str-hjrnp.2): a typed choice question in, calibrated
//! per-option probabilities out. Distinct from [`crate::oracle::SeedOracle`],
//! which *generates* candidate input vectors; a decision oracle only picks
//! among options the caller enumerates.

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Hard cap on options per choice question (the Jev API limit).
pub const MAX_CRITERIA: usize = 255;

/// One choice question: free-text `state`, an instruction, and an ordered
/// list of `(key, description)` options.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChoiceRequest {
    pub state: String,
    pub instructions: String,
    /// Ordered `(key, description)` pairs. Order is part of the request
    /// identity (see [`request_fingerprint`]).
    pub criteria: Vec<(String, String)>,
}

impl ChoiceRequest {
    /// Reject empty or oversized option lists before any network call.
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.criteria.is_empty() {
            anyhow::bail!("choice request has no criteria");
        }
        if self.criteria.len() > MAX_CRITERIA {
            anyhow::bail!(
                "choice request has {} criteria; max is {MAX_CRITERIA}",
                self.criteria.len()
            );
        }
        Ok(())
    }
}

/// The oracle's answer: the chosen key, a probability per key, a
/// confidence in `[0, 1]`, and the input tokens the call cost.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChoiceResponse {
    pub choice: String,
    pub probabilities: HashMap<String, f64>,
    pub confidence: f64,
    pub input_tokens: u32,
}

/// Adapter trait for decision backends (Jev, mocks, replay caches).
#[async_trait]
pub trait DecisionOracle: Send + Sync + std::fmt::Debug {
    fn name(&self) -> &'static str;
    async fn choose(&self, req: &ChoiceRequest) -> anyhow::Result<ChoiceResponse>;
}

/// Stable content hash of a request (sha256 hex of its canonical JSON).
/// Used as the replay-cache key, so it is order-sensitive on `criteria`.
pub fn request_fingerprint(req: &ChoiceRequest) -> String {
    use sha2::{Digest, Sha256};
    let canonical = serde_json::to_vec(req).expect("ChoiceRequest serializes");
    hex::encode(Sha256::digest(canonical))
}

/// Test double. [`MockDecisionOracle::uniform`] spreads probability evenly
/// over the request's criteria; [`MockDecisionOracle::scripted`] pins the
/// listed keys and spreads the remainder evenly over the rest. Either way
/// every criterion gets a probability and they sum to 1.
#[derive(Debug, Default, Clone)]
pub struct MockDecisionOracle {
    pinned: Vec<(String, f64)>,
}

impl MockDecisionOracle {
    pub fn uniform() -> Self {
        Self::default()
    }

    pub fn scripted(pinned: Vec<(String, f64)>) -> Self {
        Self { pinned }
    }
}

#[async_trait]
impl DecisionOracle for MockDecisionOracle {
    fn name(&self) -> &'static str {
        "mock"
    }

    async fn choose(&self, req: &ChoiceRequest) -> anyhow::Result<ChoiceResponse> {
        req.validate()?;
        let mut probabilities: HashMap<String, f64> = HashMap::new();
        let mut pinned_mass = 0.0;
        for (k, p) in &self.pinned {
            if req.criteria.iter().any(|(ck, _)| ck == k) {
                probabilities.insert(k.clone(), *p);
                pinned_mass += p;
            }
        }
        let rest: Vec<&String> = req
            .criteria
            .iter()
            .map(|(k, _)| k)
            .filter(|k| !probabilities.contains_key(*k))
            .collect();
        let remaining = (1.0 - pinned_mass).max(0.0);
        let share = if rest.is_empty() {
            0.0
        } else {
            remaining / rest.len() as f64
        };
        for k in rest {
            probabilities.insert(k.clone(), share);
        }
        let total: f64 = probabilities.values().sum();
        if total > 0.0 {
            for v in probabilities.values_mut() {
                *v /= total;
            }
        }
        let choice = probabilities
            .iter()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(k, _)| k.clone())
            .unwrap_or_default();
        Ok(ChoiceResponse {
            choice,
            probabilities,
            confidence: 0.0,
            input_tokens: 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn req(keys: &[&str]) -> ChoiceRequest {
        ChoiceRequest {
            state: "s".into(),
            instructions: "i".into(),
            criteria: keys.iter().map(|k| (k.to_string(), String::new())).collect(),
        }
    }

    #[test]
    fn fingerprint_is_stable_and_order_sensitive() {
        let a = req(&["x", "y"]);
        let b = req(&["y", "x"]);
        assert_eq!(request_fingerprint(&a), request_fingerprint(&a));
        assert_ne!(request_fingerprint(&a), request_fingerprint(&b));
        assert_eq!(request_fingerprint(&a).len(), 64);
    }

    #[test]
    fn empty_and_oversized_criteria_are_rejected() {
        assert!(req(&[]).validate().is_err());
        let keys: Vec<String> = (0..256).map(|i| format!("k{i}")).collect();
        let refs: Vec<&str> = keys.iter().map(String::as_str).collect();
        assert!(req(&refs).validate().is_err());
        let ok: Vec<&str> = refs[..255].to_vec();
        assert!(req(&ok).validate().is_ok());
    }

    #[tokio::test]
    async fn mock_uniform_sums_to_one() {
        let r = MockDecisionOracle::uniform().choose(&req(&["a", "b"])).await.unwrap();
        assert!((r.probabilities.values().sum::<f64>() - 1.0).abs() < 1e-9);
        assert_eq!(r.probabilities.len(), 2);
        assert!((r.probabilities["a"] - 0.5).abs() < 1e-9);
    }

    #[tokio::test]
    async fn mock_scripted_pins_and_picks_max() {
        let oracle = MockDecisionOracle::scripted(vec![("b".into(), 0.8)]);
        let r = oracle.choose(&req(&["a", "b", "c"])).await.unwrap();
        assert_eq!(r.choice, "b");
        assert!((r.probabilities["b"] - 0.8).abs() < 1e-9);
        assert!((r.probabilities["a"] - 0.1).abs() < 1e-9);
    }

    proptest! {
        #[test]
        fn mock_scripted_probabilities_cover_every_criterion(n in 1usize..40, pin in 0.0f64..1.0) {
            let keys: Vec<String> = (0..n).map(|i| format!("k{i}")).collect();
            let refs: Vec<&str> = keys.iter().map(String::as_str).collect();
            let r = req(&refs);
            let oracle = MockDecisionOracle::scripted(vec![("k0".into(), pin)]);
            let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
            let resp = rt.block_on(oracle.choose(&r)).unwrap();
            for k in &keys {
                prop_assert!(resp.probabilities.contains_key(k));
            }
            prop_assert!((resp.probabilities.values().sum::<f64>() - 1.0).abs() < 1e-6);
            prop_assert!(keys.contains(&resp.choice));
        }
    }
}
