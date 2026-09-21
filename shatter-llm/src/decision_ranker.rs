//! Frontier ranker backed by a [`DecisionOracle`] (str-hjrnp.2): one choice
//! question per round, whose per-option probabilities become the external
//! frontier scores.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use async_trait::async_trait;
use shatter_core::decision::{ChoiceRequest, DecisionOracle, MAX_CRITERIA};
use shatter_core::frontier::{FrontierRanker, RankContext};

const INSTRUCTIONS: &str = "Each option is an unsolved branch of the function shown in the state: \
its source line, the predicate that must flip, how deeply it is nested, and how many \
solver attempts have already failed. Choose the branch most likely to be reachable by \
some new input AND most likely to reveal behavior not yet observed.";

#[derive(Debug)]
pub struct DecisionFrontierRanker {
    oracle: Arc<dyn DecisionOracle>,
    tokens: AtomicU32,
}

impl DecisionFrontierRanker {
    pub fn new(oracle: Arc<dyn DecisionOracle>) -> Self {
        Self {
            oracle,
            tokens: AtomicU32::new(0),
        }
    }

    /// Input tokens consumed across all rounds so far.
    pub fn tokens_used(&self) -> u32 {
        self.tokens.load(Ordering::Relaxed)
    }
}

/// Keys are `b<branch_id>`. The caller's frontier order is preserved so the
/// heuristic's top-255 survives the cardinality cap.
pub fn build_choice_request(ctx: &RankContext<'_>) -> ChoiceRequest {
    let criteria = ctx
        .frontiers
        .iter()
        .take(MAX_CRITERIA)
        .map(|f| {
            (
                format!("b{}", f.branch_id),
                format!(
                    "line {}: predicate `{}`; depth {}; {} failed attempts",
                    f.line, f.predicate, f.depth, f.stall_count
                ),
            )
        })
        .collect();
    ChoiceRequest {
        state: format!("Function `{}`:\n{}", ctx.function_name, ctx.function_source),
        instructions: INSTRUCTIONS.to_string(),
        criteria,
    }
}

#[async_trait]
impl FrontierRanker for DecisionFrontierRanker {
    fn name(&self) -> &'static str {
        "decision"
    }

    async fn rank(&self, ctx: &RankContext<'_>) -> anyhow::Result<HashMap<u32, f64>> {
        if ctx.frontiers.is_empty() {
            return Ok(HashMap::new());
        }
        let req = build_choice_request(ctx);
        let resp = self.oracle.choose(&req).await?;
        self.tokens.fetch_add(resp.input_tokens, Ordering::Relaxed);
        let mut scores = HashMap::with_capacity(resp.probabilities.len());
        for (key, p) in resp.probabilities {
            if let Some(id) = key.strip_prefix('b').and_then(|s| s.parse::<u32>().ok()) {
                scores.insert(id, p);
            }
        }
        Ok(scores)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use shatter_core::decision::MockDecisionOracle;
    use shatter_core::frontier::FrontierSummary;

    fn ctx<'a>(n: u32) -> RankContext<'a> {
        RankContext {
            function_name: "f",
            function_source: "function f(x) {}",
            round: 1,
            frontiers: (0..n)
                .map(|i| FrontierSummary {
                    branch_id: i,
                    depth: i % 3,
                    stall_count: 0,
                    line: 10 + i,
                    predicate: format!("x > {i}"),
                })
                .collect(),
        }
    }

    #[test]
    fn request_keys_map_back_to_branch_ids() {
        let req = build_choice_request(&ctx(3));
        let keys: Vec<&str> = req.criteria.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, vec!["b0", "b1", "b2"]);
        assert!(req.criteria[1].1.contains("line 11"));
        assert!(req.criteria[1].1.contains("x > 1"));
        assert!(req.state.contains("function f(x)"));
    }

    #[tokio::test]
    async fn scores_are_the_oracle_probabilities() {
        let oracle = Arc::new(MockDecisionOracle::scripted(vec![("b2".into(), 0.8)]));
        let ranker = DecisionFrontierRanker::new(oracle);
        let scores = ranker.rank(&ctx(3)).await.unwrap();
        assert!((scores[&2] - 0.8).abs() < 1e-9);
        assert!((scores[&0] - 0.1).abs() < 1e-9);
        assert_eq!(scores.len(), 3);
        assert_eq!(ranker.name(), "decision");
        assert!(!ranker.is_noop());
    }

    #[tokio::test]
    async fn empty_frontier_list_skips_the_oracle() {
        let ranker = DecisionFrontierRanker::new(Arc::new(MockDecisionOracle::uniform()));
        assert!(ranker.rank(&ctx(0)).await.unwrap().is_empty());
    }

    proptest! {
        #[test]
        fn never_exceeds_255_criteria(n in 1u32..600) {
            let req = build_choice_request(&ctx(n));
            prop_assert!(req.criteria.len() <= MAX_CRITERIA);
            prop_assert!(req.validate().is_ok());
        }
    }
}
