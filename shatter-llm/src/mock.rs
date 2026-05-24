//! Trait-level mock [`SeedOracle`] for unit tests. No HTTP, no real LLM.

use std::collections::HashMap;

use async_trait::async_trait;
use shatter_core::oracle::{InputVector, OracleContext, OracleResponse, SeedOracle};

#[derive(Debug, Clone, Copy)]
enum Mode {
    Scripted,
    AlwaysFail,
    AlwaysEmpty,
}

/// Test double for [`SeedOracle`]. Use [`MockSeedOracle::scripted`] to script
/// responses per condition predicate, [`MockSeedOracle::always_fail`] to
/// exercise the error path, and [`MockSeedOracle::always_empty`] for the
/// drop/dedup path.
pub struct MockSeedOracle {
    mode: Mode,
    scripted: HashMap<String, Vec<InputVector>>,
}

impl MockSeedOracle {
    /// Script responses keyed by `condition.predicate`. The slot map uses
    /// [`ConditionId`](shatter_core::oracle::ConditionId) for routing but the
    /// oracle itself receives the full [`OracleContext`]; matching on the
    /// predicate string keeps tests close to the prompt the LLM would see.
    pub fn scripted(entries: Vec<(String, Vec<InputVector>)>) -> Self {
        let mut scripted = HashMap::new();
        for (k, v) in entries {
            scripted.insert(k, v);
        }
        Self {
            mode: Mode::Scripted,
            scripted,
        }
    }

    /// Every `query()` returns `Err` — exercises the orchestrator's
    /// exhausted/error path.
    pub fn always_fail() -> Self {
        Self {
            mode: Mode::AlwaysFail,
            scripted: HashMap::new(),
        }
    }

    /// Every `query()` returns `Ok` with no candidates — exercises the
    /// drop/dedup path.
    pub fn always_empty() -> Self {
        Self {
            mode: Mode::AlwaysEmpty,
            scripted: HashMap::new(),
        }
    }
}

#[async_trait]
impl SeedOracle for MockSeedOracle {
    async fn query(&self, ctx: OracleContext) -> anyhow::Result<OracleResponse> {
        match self.mode {
            Mode::AlwaysFail => Err(anyhow::anyhow!("mock oracle: always_fail")),
            Mode::AlwaysEmpty => Ok(OracleResponse {
                candidates: Vec::new(),
                tokens_used: 0,
            }),
            Mode::Scripted => {
                let candidates = self
                    .scripted
                    .get(&ctx.condition.predicate)
                    .cloned()
                    .unwrap_or_default();
                Ok(OracleResponse {
                    candidates,
                    tokens_used: 0,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use shatter_core::oracle::FailedCondition;

    fn ctx(predicate: &str) -> OracleContext {
        OracleContext {
            function_source: String::new(),
            param_types: Vec::new(),
            condition: FailedCondition {
                predicate: predicate.to_string(),
                location: "t:1".to_string(),
            },
            attempted: Vec::new(),
        }
    }

    #[tokio::test]
    async fn always_fail_returns_err() {
        let oracle = MockSeedOracle::always_fail();
        for _ in 0..3 {
            let r = oracle.query(ctx("anything")).await;
            assert!(r.is_err());
        }
    }

    #[tokio::test]
    async fn always_empty_returns_ok_empty() {
        let oracle = MockSeedOracle::always_empty();
        let r = oracle.query(ctx("p")).await.unwrap();
        assert!(r.candidates.is_empty());
        assert_eq!(r.tokens_used, 0);
    }

    #[tokio::test]
    async fn scripted_routes_by_predicate() {
        let oracle = MockSeedOracle::scripted(vec![
            ("x > 10".to_string(), vec![vec![json!(11)], vec![json!(99)]]),
            ("y == 0".to_string(), vec![vec![json!(0)]]),
        ]);
        let r = oracle.query(ctx("x > 10")).await.unwrap();
        assert_eq!(r.candidates.len(), 2);
        let r = oracle.query(ctx("y == 0")).await.unwrap();
        assert_eq!(r.candidates, vec![vec![json!(0)]]);
        let r = oracle.query(ctx("unmapped")).await.unwrap();
        assert!(r.candidates.is_empty());
    }
}
