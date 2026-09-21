//! Frontier tracking for concolic exploration.
//!
//! A frontier represents a branch reached but not fully explored during
//! concolic execution. Each frontier tracks which parameters block progress
//! and how many consecutive attempts have failed to make progress (stall count).
//! The `FrontierSet` maintains a collection of frontiers per function,
//! serving them in score order: deeper, less-stalled, constraint-rich frontiers
//! are explored first.

use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};

/// Stall count threshold after which a frontier is abandoned.
///
/// When a frontier accumulates this many consecutive failed solve/drill attempts
/// without discovering a new path, it is removed from the active worklist.
/// The value 10 balances thoroughness (giving Z3 and drilling enough attempts
/// to find a solution) against efficiency (not wasting budget on contradictory
/// or unsolvable constraints). Empirically, frontiers that don't yield progress
/// within 10 attempts rarely do so with more.
pub const FRONTIER_STALL_THRESHOLD: u32 = 10;

/// Weight for depth component in frontier scoring. Deeper branches are harder
/// to reach via random exploration, so the concolic engine should prioritize them.
const DEPTH_WEIGHT: f64 = 1.0;

/// Weight for stall penalty. A fully-fresh frontier (stall_count=0) gets the
/// full bonus; as stall_count approaches FRONTIER_STALL_THRESHOLD the bonus
/// decays to zero. Value of 2.0 means the stall component ranges [0.0, 2.0].
const STALL_DECAY: f64 = 2.0;

/// Fixed boost for frontiers where the blocking parameter has a known symbolic
/// constraint (`blocking_params` is non-empty). Solver-guided search on these
/// frontiers is significantly more efficient than blind mutations, so the
/// boost is the largest single weight.
const CONSTRAINT_BOOST: f64 = 3.0;

/// Weight for the profile-guided rarity boost (0.0–1.0). Rare branches are
/// less likely to be discovered by random exploration, so they deserve a
/// secondary boost. Lower than CONSTRAINT_BOOST because rarity alone doesn't
/// guarantee solver tractability.
const RARITY_WEIGHT: f64 = 1.5;

/// Compute the priority score for a frontier. Higher score = higher priority.
///
/// ```text
/// score = DEPTH_WEIGHT * depth
///       + STALL_DECAY * (1.0 - stall_count / FRONTIER_STALL_THRESHOLD).clamp(0)
///       + CONSTRAINT_BOOST * (1 if blocking_params non-empty, else 0)
///       + RARITY_WEIGHT * rarity_boost
/// ```
pub fn frontier_score(f: &Frontier) -> f64 {
    let depth_component = DEPTH_WEIGHT * f.depth as f64;
    let stall_ratio = f.stall_count as f64 / FRONTIER_STALL_THRESHOLD as f64;
    let stall_component = STALL_DECAY * (1.0 - stall_ratio).max(0.0);
    let constraint_component = if f.blocking_params.is_empty() {
        0.0
    } else {
        CONSTRAINT_BOOST
    };
    let rarity_component = RARITY_WEIGHT * f.rarity_boost;

    depth_component + stall_component + constraint_component + rarity_component
}

/// A single exploration frontier — a branch reached but not yet solved.
///
/// `blocking_params` lists the parameter indices (into the function's `ParamInfo`
/// vec) that appear in the branch's symbolic constraint, identified via
/// `sym_expr::extract_param_names`. `best_prefix` holds the input vector from
/// the execution that reached this branch deepest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Frontier {
    /// Branch identifier, unique within the function scope.
    pub branch_id: u32,
    /// Nesting depth of this branch in the control-flow graph (0 = top-level).
    pub depth: u32,
    /// Indices of parameters that influence this branch's condition.
    pub blocking_params: Vec<usize>,
    /// Input vector from the best execution reaching this branch.
    pub best_prefix: Vec<serde_json::Value>,
    /// Consecutive failed attempts to solve past this branch.
    pub stall_count: u32,
    /// Profile-guided rarity boost (0.0 = no boost, 1.0 = maximum priority).
    /// Set from [`BranchProfile::rarity()`] when a profile is available.
    #[serde(default)]
    pub rarity_boost: f64,
}

/// Score-ordered collection of frontiers for a single function.
///
/// Frontiers are served by [`frontier_score`]: deeper, less-stalled, and
/// constraint-rich frontiers are explored first. The set is typically small
/// (< 100 entries), so linear scans are acceptable.
#[derive(Debug, Clone, Default)]
pub struct FrontierSet {
    frontiers: Vec<Frontier>,
    /// Per-round scores installed by a [`FrontierRanker`]. Ids absent from
    /// the map fall back to [`frontier_score`]. Cleared entries are dropped
    /// together with their frontier.
    external_scores: HashMap<u32, f64>,
}

impl FrontierSet {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a frontier. If a frontier with the same `branch_id` already
    /// exists, it is replaced.
    pub fn insert(&mut self, frontier: Frontier) {
        // Keep any external score installed this round: re-inserting a
        // frontier refreshes its prefix/depth, not the ranker's opinion.
        if let Some(pos) = self
            .frontiers
            .iter()
            .position(|f| f.branch_id == frontier.branch_id)
        {
            self.frontiers.swap_remove(pos);
        }
        self.frontiers.push(frontier);
    }

    /// Remove and return the highest-scoring frontier.
    pub fn pop_highest_priority(&mut self) -> Option<Frontier> {
        if self.frontiers.is_empty() {
            return None;
        }
        let idx = self.best_index();
        Some(self.frontiers.swap_remove(idx))
    }

    /// View the highest-priority frontier without removing it.
    pub fn peek(&self) -> Option<&Frontier> {
        if self.frontiers.is_empty() {
            return None;
        }
        let idx = self.best_index();
        Some(&self.frontiers[idx])
    }

    /// Increment the stall count for the frontier with the given `branch_id`.
    /// Returns `true` if the frontier was found and updated.
    pub fn increment_stall(&mut self, branch_id: u32) -> bool {
        if let Some(f) = self.frontiers.iter_mut().find(|f| f.branch_id == branch_id) {
            f.stall_count = f.stall_count.saturating_add(1);
            true
        } else {
            false
        }
    }

    /// Remove the frontier with the given `branch_id` (e.g., after solving it).
    /// Returns the removed frontier, if any.
    pub fn remove(&mut self, branch_id: u32) -> Option<Frontier> {
        self.external_scores.remove(&branch_id);
        if let Some(pos) = self.frontiers.iter().position(|f| f.branch_id == branch_id) {
            Some(self.frontiers.swap_remove(pos))
        } else {
            None
        }
    }

    pub fn len(&self) -> usize {
        self.frontiers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.frontiers.is_empty()
    }

    /// Reset the stall count to zero for the frontier with the given `branch_id`.
    /// Returns `true` if the frontier was found and reset.
    ///
    /// Call this when a new path is discovered through a frontier that was
    /// previously stalling — the progress proves it wasn't truly stuck.
    pub fn reset_stall(&mut self, branch_id: u32) -> bool {
        if let Some(f) = self.frontiers.iter_mut().find(|f| f.branch_id == branch_id) {
            f.stall_count = 0;
            true
        } else {
            false
        }
    }

    /// Remove and return all frontiers with `stall_count >= threshold`.
    ///
    /// These frontiers have failed to produce new paths over many consecutive
    /// iterations and are considered abandoned. The returned vec can be used
    /// for diagnostics and budget reallocation.
    pub fn abandon_stalled(&mut self, threshold: u32) -> Vec<Frontier> {
        let mut abandoned = Vec::new();
        let mut i = 0;
        while i < self.frontiers.len() {
            if self.frontiers[i].stall_count >= threshold {
                abandoned.push(self.frontiers.swap_remove(i));
                // Don't increment i — swap_remove moved the last element here
            } else {
                i += 1;
            }
        }
        abandoned
    }

    /// Iterate over all frontiers in arbitrary order.
    pub fn iter(&self) -> impl Iterator<Item = &Frontier> {
        self.frontiers.iter()
    }

    /// Replace this round's external scores (str-hjrnp.1). Ids not present
    /// fall back to [`frontier_score`]; an empty map restores pure heuristic
    /// ordering.
    ///
    /// External and heuristic scores are compared on one axis, and the
    /// heuristic ranges roughly `0..7.5` while rankers emit `0..1`, so a
    /// partial map lets heuristic-scored frontiers outrank externally
    /// scored ones. Rankers should score every frontier they are given.
    pub fn set_external_scores(&mut self, scores: HashMap<u32, f64>) {
        self.external_scores = scores;
    }

    /// `true` when a ranker installed at least one score this round.
    pub fn has_external_scores(&self) -> bool {
        !self.external_scores.is_empty()
    }

    /// Effective priority of `f`: the external score when one was installed
    /// for its branch id, else [`frontier_score`].
    pub fn score(&self, f: &Frontier) -> f64 {
        self.external_scores
            .get(&f.branch_id)
            .copied()
            .unwrap_or_else(|| frontier_score(f))
    }

    /// All frontiers, highest effective score first. Ties keep insertion
    /// order (stable sort).
    pub fn sorted_desc(&self) -> Vec<Frontier> {
        let mut v: Vec<Frontier> = self.frontiers.clone();
        v.sort_by(|a, b| {
            self.score(b)
                .partial_cmp(&self.score(a))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        v
    }

    /// Index of the highest-scoring frontier.
    /// Caller must ensure `self.frontiers` is non-empty.
    fn best_index(&self) -> usize {
        self.frontiers
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| {
                self.score(a)
                    .partial_cmp(&self.score(b))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(i, _)| i)
            .expect("best_index called on non-empty FrontierSet")
    }
}

/// Compact view of a frontier handed to a [`FrontierRanker`] (no input
/// vectors, so it is cheap to clone and safe to send off-machine).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FrontierSummary {
    pub branch_id: u32,
    pub depth: u32,
    pub stall_count: u32,
    /// Source line of the branch, `0` when never observed.
    pub line: u32,
    /// Predicate text recovered from the branch's symbolic constraint, or
    /// the opaque hint; empty when unknown.
    pub predicate: String,
}

/// Everything a ranker may look at for one exploration round.
#[derive(Debug, Clone)]
pub struct RankContext<'a> {
    pub function_name: &'a str,
    /// Source of the function under exploration; empty when the caller
    /// could not supply it.
    pub function_source: &'a str,
    /// 1-based round counter of the orchestrator loop.
    pub round: usize,
    /// Frontiers in the orchestrator's current effective order, capped by
    /// the caller.
    pub frontiers: Vec<FrontierSummary>,
}

/// Per-round policy that assigns external scores to frontiers
/// (str-hjrnp.1). Returning an empty map means "use the built-in heuristic
/// for everything"; ids missing from a non-empty map fall back to the
/// heuristic individually.
#[async_trait]
pub trait FrontierRanker: Send + Sync + std::fmt::Debug {
    fn name(&self) -> &'static str;

    /// `true` when `rank` always returns an empty map, so the orchestrator
    /// can skip building a [`RankContext`] entirely. Only the built-in
    /// heuristic should override this.
    fn is_noop(&self) -> bool {
        false
    }

    async fn rank(&self, ctx: &RankContext<'_>) -> anyhow::Result<HashMap<u32, f64>>;
}

/// Default policy: no external scores, so [`frontier_score`] governs.
#[derive(Debug, Default, Clone, Copy)]
pub struct HeuristicRanker;

#[async_trait]
impl FrontierRanker for HeuristicRanker {
    fn name(&self) -> &'static str {
        "heuristic"
    }

    fn is_noop(&self) -> bool {
        true
    }

    async fn rank(&self, _ctx: &RankContext<'_>) -> anyhow::Result<HashMap<u32, f64>> {
        Ok(HashMap::new())
    }
}

/// Uniform random scores in `[0, 1)`, deterministic in `(seed, round)`. The
/// benchmark floor.
#[derive(Debug, Clone, Copy)]
pub struct RandomRanker {
    pub seed: u64,
}

#[async_trait]
impl FrontierRanker for RandomRanker {
    fn name(&self) -> &'static str {
        "random"
    }

    async fn rank(&self, ctx: &RankContext<'_>) -> anyhow::Result<HashMap<u32, f64>> {
        let round_salt = (ctx.round as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        let mut rng = rand::rngs::StdRng::seed_from_u64(self.seed ^ round_salt);
        Ok(ctx
            .frontiers
            .iter()
            .map(|f| (f.branch_id, rng.random_range(0.0..1.0)))
            .collect())
    }
}

/// Scores `1.0` for branch ids in `priority` and `0.0` otherwise. The
/// benchmark ceiling: `priority` is filled from a reference run's discovery
/// set, so this ranker knows which frontiers actually lead somewhere.
#[derive(Debug, Clone)]
pub struct ScriptedRanker {
    pub priority: HashSet<u32>,
}

#[async_trait]
impl FrontierRanker for ScriptedRanker {
    fn name(&self) -> &'static str {
        "scripted"
    }

    async fn rank(&self, ctx: &RankContext<'_>) -> anyhow::Result<HashMap<u32, f64>> {
        Ok(ctx
            .frontiers
            .iter()
            .map(|f| {
                let score = if self.priority.contains(&f.branch_id) { 1.0 } else { 0.0 };
                (f.branch_id, score)
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn make_frontier(branch_id: u32, depth: u32, stall_count: u32) -> Frontier {
        Frontier {
            branch_id,
            depth,
            blocking_params: vec![],
            best_prefix: vec![],
            stall_count,
            rarity_boost: 0.0,
        }
    }

    fn make_frontier_with_rarity(
        branch_id: u32,
        depth: u32,
        stall_count: u32,
        rarity_boost: f64,
    ) -> Frontier {
        Frontier {
            branch_id,
            depth,
            blocking_params: vec![],
            best_prefix: vec![],
            stall_count,
            rarity_boost,
        }
    }

    #[test]
    fn empty_set_returns_none() {
        let mut set = FrontierSet::new();
        assert!(set.is_empty());
        assert_eq!(set.len(), 0);
        assert!(set.peek().is_none());
        assert!(set.pop_highest_priority().is_none());
    }

    #[test]
    fn single_insert_and_pop() {
        let mut set = FrontierSet::new();
        set.insert(make_frontier(1, 3, 0));
        assert_eq!(set.len(), 1);

        let f = set.pop_highest_priority().unwrap();
        assert_eq!(f.branch_id, 1);
        assert!(set.is_empty());
    }

    #[test]
    fn priority_deeper_first() {
        let mut set = FrontierSet::new();
        set.insert(make_frontier(1, 5, 0));
        set.insert(make_frontier(2, 2, 0));
        set.insert(make_frontier(3, 8, 0));

        let f = set.pop_highest_priority().unwrap();
        assert_eq!(f.branch_id, 3, "deepest branch should be popped first");

        let f = set.pop_highest_priority().unwrap();
        assert_eq!(f.branch_id, 1);

        let f = set.pop_highest_priority().unwrap();
        assert_eq!(f.branch_id, 2);
    }

    #[test]
    fn stall_count_breaks_ties_at_equal_depth() {
        let mut set = FrontierSet::new();
        // All depth=3, scores differ only by stall component:
        // stall=1: 3.0 + 2.0*(1-0.1) = 4.8
        // stall=3: 3.0 + 2.0*(1-0.3) = 4.4
        // stall=5: 3.0 + 2.0*(1-0.5) = 4.0
        set.insert(make_frontier(1, 3, 5));
        set.insert(make_frontier(2, 3, 1));
        set.insert(make_frontier(3, 3, 3));

        let f = set.pop_highest_priority().unwrap();
        assert_eq!(f.branch_id, 2, "lowest stall count wins at equal depth");

        let f = set.pop_highest_priority().unwrap();
        assert_eq!(f.branch_id, 3);
    }

    #[test]
    fn duplicate_branch_id_replaces() {
        let mut set = FrontierSet::new();
        set.insert(make_frontier(1, 5, 0));
        set.insert(make_frontier(1, 2, 3));

        assert_eq!(set.len(), 1);
        let f = set.peek().unwrap();
        assert_eq!(f.depth, 2, "second insert should replace the first");
    }

    #[test]
    fn increment_stall_updates_count() {
        let mut set = FrontierSet::new();
        set.insert(make_frontier(1, 3, 0));

        assert!(set.increment_stall(1));
        assert_eq!(set.peek().unwrap().stall_count, 1);

        assert!(set.increment_stall(1));
        assert_eq!(set.peek().unwrap().stall_count, 2);
    }

    #[test]
    fn increment_stall_missing_returns_false() {
        let mut set = FrontierSet::new();
        assert!(!set.increment_stall(99));
    }

    #[test]
    fn remove_by_branch_id() {
        let mut set = FrontierSet::new();
        set.insert(make_frontier(1, 3, 0));
        set.insert(make_frontier(2, 5, 0));

        let removed = set.remove(1);
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().branch_id, 1);
        assert_eq!(set.len(), 1);

        assert!(set.remove(1).is_none());
    }

    #[test]
    fn iter_yields_all_frontiers() {
        let mut set = FrontierSet::new();
        set.insert(make_frontier(1, 3, 0));
        set.insert(make_frontier(2, 5, 0));
        set.insert(make_frontier(3, 1, 0));

        let ids: Vec<u32> = set.iter().map(|f| f.branch_id).collect();
        assert_eq!(ids.len(), 3);
        assert!(ids.contains(&1));
        assert!(ids.contains(&2));
        assert!(ids.contains(&3));
    }

    #[test]
    fn peek_matches_pop() {
        let mut set = FrontierSet::new();
        set.insert(make_frontier(1, 5, 0));
        set.insert(make_frontier(2, 2, 0));

        let peeked_id = set.peek().unwrap().branch_id;
        let popped_id = set.pop_highest_priority().unwrap().branch_id;
        assert_eq!(peeked_id, popped_id);
    }

    #[test]
    fn blocking_params_and_best_prefix_preserved() {
        let mut set = FrontierSet::new();
        set.insert(Frontier {
            branch_id: 1,
            depth: 0,
            blocking_params: vec![0, 2],
            best_prefix: vec![serde_json::json!(42), serde_json::json!("hello")],
            stall_count: 0,
            rarity_boost: 0.0,
        });

        let f = set.peek().unwrap();
        assert_eq!(f.blocking_params, vec![0, 2]);
        assert_eq!(f.best_prefix.len(), 2);
    }

    #[test]
    fn stall_count_saturates() {
        let mut set = FrontierSet::new();
        set.insert(make_frontier(1, 0, u32::MAX));
        assert!(set.increment_stall(1));
        assert_eq!(set.peek().unwrap().stall_count, u32::MAX);
    }

    #[test]
    fn rarity_boost_breaks_depth_ties() {
        let mut set = FrontierSet::new();
        set.insert(make_frontier_with_rarity(1, 3, 0, 0.2));
        set.insert(make_frontier_with_rarity(2, 3, 0, 0.8));
        set.insert(make_frontier_with_rarity(3, 3, 0, 0.5));

        let f = set.pop_highest_priority().unwrap();
        assert_eq!(f.branch_id, 2, "highest rarity_boost wins at equal depth");

        let f = set.pop_highest_priority().unwrap();
        assert_eq!(f.branch_id, 3);
    }

    #[test]
    fn depth_dominates_rarity() {
        let mut set = FrontierSet::new();
        // depth=5, rarity=0.0: score = 5.0 + 2.0 + 0.0 + 0.0 = 7.0
        set.insert(make_frontier_with_rarity(1, 5, 0, 0.0));
        // depth=1, rarity=1.0: score = 1.0 + 2.0 + 0.0 + 1.5 = 4.5
        set.insert(make_frontier_with_rarity(2, 1, 0, 1.0));

        let f = set.pop_highest_priority().unwrap();
        assert_eq!(f.branch_id, 1, "deeper branch wins even with lower rarity");
    }

    #[test]
    fn zero_rarity_boost_preserves_original_order() {
        let mut set = FrontierSet::new();
        set.insert(make_frontier_with_rarity(1, 3, 5, 0.0));
        set.insert(make_frontier_with_rarity(2, 3, 1, 0.0));

        let f = set.pop_highest_priority().unwrap();
        assert_eq!(
            f.branch_id, 2,
            "lower stall count wins when rarity is equal"
        );
    }

    #[test]
    fn reset_stall_resets_count() {
        let mut set = FrontierSet::new();
        set.insert(make_frontier(1, 3, 7));
        assert!(set.reset_stall(1));
        assert_eq!(set.peek().unwrap().stall_count, 0);
    }

    #[test]
    fn reset_stall_missing_returns_false() {
        let mut set = FrontierSet::new();
        assert!(!set.reset_stall(99));
    }

    #[test]
    fn abandon_stalled_empty_set() {
        let mut set = FrontierSet::new();
        let abandoned = set.abandon_stalled(5);
        assert!(abandoned.is_empty());
    }

    #[test]
    fn abandon_stalled_none_above_threshold() {
        let mut set = FrontierSet::new();
        set.insert(make_frontier(1, 0, 2));
        set.insert(make_frontier(2, 1, 4));
        let abandoned = set.abandon_stalled(5);
        assert!(abandoned.is_empty());
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn abandon_stalled_removes_correct_frontiers() {
        let mut set = FrontierSet::new();
        set.insert(make_frontier(1, 0, 3)); // below threshold
        set.insert(make_frontier(2, 1, 10)); // at threshold
        set.insert(make_frontier(3, 2, 15)); // above threshold

        let abandoned = set.abandon_stalled(10);
        assert_eq!(abandoned.len(), 2);
        let abandoned_ids: Vec<u32> = abandoned.iter().map(|f| f.branch_id).collect();
        assert!(abandoned_ids.contains(&2));
        assert!(abandoned_ids.contains(&3));
        assert_eq!(set.len(), 1);
        assert_eq!(set.peek().unwrap().branch_id, 1);
    }

    #[test]
    fn abandon_stalled_preserves_frontier_data() {
        let mut set = FrontierSet::new();
        set.insert(Frontier {
            branch_id: 1,
            depth: 5,
            blocking_params: vec![0, 2],
            best_prefix: vec![serde_json::json!(42)],
            stall_count: 10,
            rarity_boost: 0.7,
        });

        let abandoned = set.abandon_stalled(10);
        assert_eq!(abandoned.len(), 1);
        let f = &abandoned[0];
        assert_eq!(f.branch_id, 1);
        assert_eq!(f.depth, 5);
        assert_eq!(f.blocking_params, vec![0, 2]);
        assert_eq!(f.stall_count, 10);
    }

    #[test]
    fn constraint_boost_raises_priority() {
        let mut set = FrontierSet::new();
        // Same depth, stall, rarity — only blocking_params differs
        set.insert(Frontier {
            branch_id: 1,
            depth: 3,
            blocking_params: vec![], // no constraint info
            best_prefix: vec![],
            stall_count: 0,
            rarity_boost: 0.0,
        });
        set.insert(Frontier {
            branch_id: 2,
            depth: 3,
            blocking_params: vec![0], // has constraint info
            best_prefix: vec![],
            stall_count: 0,
            rarity_boost: 0.0,
        });

        let f = set.pop_highest_priority().unwrap();
        assert_eq!(f.branch_id, 2, "constrained frontier should pop first");
    }

    #[test]
    fn score_monotonicity_depth() {
        // Higher depth = higher score, all else equal
        let shallow = make_frontier(1, 2, 0);
        let deep = make_frontier(2, 10, 0);
        assert!(
            frontier_score(&deep) > frontier_score(&shallow),
            "deeper frontier should score higher"
        );
    }

    #[test]
    fn score_clamps_at_threshold() {
        // stall_count at/above threshold: stall component should be 0, not negative
        let at_threshold = make_frontier(1, 3, FRONTIER_STALL_THRESHOLD);
        let above_threshold = make_frontier(2, 3, FRONTIER_STALL_THRESHOLD + 5);

        let score_at = frontier_score(&at_threshold);
        let score_above = frontier_score(&above_threshold);

        // Both should have stall component = 0, so equal scores
        assert!(
            (score_at - score_above).abs() < f64::EPSILON,
            "stall component should clamp at 0"
        );
    }

    #[test]
    fn score_components_documented() {
        // Verify the scoring formula matches documented weights
        let f = Frontier {
            branch_id: 1,
            depth: 5,
            blocking_params: vec![0],
            best_prefix: vec![],
            stall_count: 2,
            rarity_boost: 0.6,
        };
        let score = frontier_score(&f);
        // depth: 1.0 * 5 = 5.0
        // stall: 2.0 * (1.0 - 2/10) = 2.0 * 0.8 = 1.6
        // constraint: 3.0 (non-empty blocking_params)
        // rarity: 1.5 * 0.6 = 0.9
        let expected = 5.0 + 1.6 + 3.0 + 0.9;
        assert!(
            (score - expected).abs() < f64::EPSILON,
            "score {score} != expected {expected}"
        );
    }

    fn summary(branch_id: u32) -> FrontierSummary {
        FrontierSummary {
            branch_id,
            depth: 0,
            stall_count: 0,
            line: 0,
            predicate: String::new(),
        }
    }

    #[test]
    fn external_scores_override_heuristic() {
        let mut set = FrontierSet::new();
        set.insert(make_frontier(1, 5, 0)); // heuristically best (deepest)
        set.insert(make_frontier(2, 0, 0));
        assert_eq!(set.peek().unwrap().branch_id, 1);

        let mut ext = HashMap::new();
        ext.insert(2, 0.9);
        ext.insert(1, 0.1);
        set.set_external_scores(ext);
        assert_eq!(set.peek().unwrap().branch_id, 2);
        let order: Vec<u32> = set.sorted_desc().iter().map(|f| f.branch_id).collect();
        assert_eq!(order, vec![2, 1]);
    }

    #[test]
    fn missing_external_score_falls_back_to_heuristic() {
        let mut set = FrontierSet::new();
        set.insert(make_frontier(1, 5, 0));
        set.insert(make_frontier(2, 0, 0));
        let mut ext = HashMap::new();
        ext.insert(2, 0.5);
        set.set_external_scores(ext);
        let deep = make_frontier(1, 5, 0);
        assert_eq!(set.score(&deep), frontier_score(&deep));
        assert_eq!(set.score(&make_frontier(2, 0, 0)), 0.5);
    }

    #[test]
    fn empty_external_map_restores_heuristic_order() {
        let mut set = FrontierSet::new();
        set.insert(make_frontier(1, 5, 0));
        set.insert(make_frontier(2, 0, 0));
        let mut ext = HashMap::new();
        ext.insert(1, 0.1);
        ext.insert(2, 0.9);
        set.set_external_scores(ext);
        assert_eq!(set.peek().unwrap().branch_id, 2);
        set.set_external_scores(HashMap::new());
        assert_eq!(set.peek().unwrap().branch_id, 1);
    }

    #[test]
    fn insert_preserves_external_score() {
        let mut set = FrontierSet::new();
        set.insert(make_frontier(1, 0, 0));
        let mut ext = HashMap::new();
        ext.insert(1, 0.9);
        set.set_external_scores(ext);
        assert!(set.has_external_scores());
        set.insert(make_frontier(1, 3, 2)); // refreshed depth/stall, same id
        assert_eq!(set.score(&make_frontier(1, 3, 2)), 0.9);
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn heuristic_ranker_is_noop_and_others_are_not() {
        assert!(HeuristicRanker.is_noop());
        assert!(!RandomRanker { seed: 1 }.is_noop());
        assert!(!ScriptedRanker { priority: HashSet::new() }.is_noop());
    }

    #[test]
    fn remove_drops_external_score() {
        let mut set = FrontierSet::new();
        set.insert(make_frontier(1, 0, 0));
        let mut ext = HashMap::new();
        ext.insert(1, 0.9);
        set.set_external_scores(ext);
        set.remove(1);
        set.insert(make_frontier(1, 0, 0));
        assert_eq!(
            set.score(&make_frontier(1, 0, 0)),
            frontier_score(&make_frontier(1, 0, 0))
        );
    }

    #[tokio::test]
    async fn heuristic_ranker_returns_empty_map() {
        let ctx = RankContext {
            function_name: "f",
            function_source: "",
            round: 1,
            frontiers: vec![summary(1), summary(2)],
        };
        assert!(HeuristicRanker.rank(&ctx).await.unwrap().is_empty());
        assert_eq!(HeuristicRanker.name(), "heuristic");
    }

    #[tokio::test]
    async fn random_ranker_is_deterministic_per_seed_and_round() {
        let ctx = RankContext {
            function_name: "f",
            function_source: "",
            round: 3,
            frontiers: vec![summary(1), summary(2), summary(3)],
        };
        let a = RandomRanker { seed: 7 }.rank(&ctx).await.unwrap();
        let b = RandomRanker { seed: 7 }.rank(&ctx).await.unwrap();
        assert_eq!(a, b);
        assert_eq!(a.len(), 3);
        assert!(a.values().all(|v| (0.0..1.0).contains(v)));
        let ctx_next = RankContext { round: 4, ..ctx.clone() };
        assert_ne!(a, RandomRanker { seed: 7 }.rank(&ctx_next).await.unwrap());
        assert_ne!(a, RandomRanker { seed: 8 }.rank(&ctx).await.unwrap());
    }

    #[tokio::test]
    async fn scripted_ranker_boosts_priority_ids_only() {
        let ctx = RankContext {
            function_name: "f",
            function_source: "",
            round: 1,
            frontiers: vec![summary(1), summary(2)],
        };
        let ranker = ScriptedRanker {
            priority: [2].into_iter().collect(),
        };
        let r = ranker.rank(&ctx).await.unwrap();
        assert_eq!(r[&2], 1.0);
        assert_eq!(r[&1], 0.0);
    }

    proptest! {
        #[test]
        fn sorted_desc_is_nonincreasing_under_score(
            ids in prop::collection::hash_set(0u32..50, 1..20),
            seed in any::<u64>(),
        ) {
            let mut set = FrontierSet::new();
            for id in &ids {
                set.insert(make_frontier(*id, *id % 4, *id % 3));
            }
            let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
            let mut ext: HashMap<u32, f64> = HashMap::new();
            for id in &ids {
                if rng.random_bool(0.5) {
                    ext.insert(*id, rng.random_range(0.0..1.0));
                }
            }
            set.set_external_scores(ext);
            let sorted = set.sorted_desc();
            prop_assert_eq!(sorted.len(), ids.len());
            for w in sorted.windows(2) {
                prop_assert!(set.score(&w[0]) >= set.score(&w[1]));
            }
            // peek agrees with sorted_desc's head.
            prop_assert_eq!(set.score(set.peek().unwrap()), set.score(&sorted[0]));
        }
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    fn arb_frontier() -> impl Strategy<Value = Frontier> {
        (
            0..1000u32,
            0..20u32,
            prop::collection::vec(0..10usize, 0..5),
            0..50u32,
            0.0..=1.0f64,
        )
            .prop_map(
                |(branch_id, depth, blocking_params, stall_count, rarity_boost)| Frontier {
                    branch_id,
                    depth,
                    blocking_params,
                    best_prefix: vec![],
                    stall_count,
                    rarity_boost,
                },
            )
    }

    proptest! {
        #[test]
        fn abandon_stalled_removes_only_above_threshold(
            frontiers in prop::collection::vec(arb_frontier(), 0..20),
            threshold in 1..30u32,
        ) {
            let mut set = FrontierSet::new();
            for f in frontiers {
                set.insert(f);
            }
            let original_len = set.len();
            let abandoned = set.abandon_stalled(threshold);

            // All abandoned frontiers must have stall_count >= threshold
            for f in &abandoned {
                prop_assert!(f.stall_count >= threshold,
                    "abandoned frontier {} has stall_count {} < threshold {}",
                    f.branch_id, f.stall_count, threshold);
            }

            // No remaining frontier should have stall_count >= threshold
            for f in set.iter() {
                prop_assert!(f.stall_count < threshold,
                    "remaining frontier {} has stall_count {} >= threshold {}",
                    f.branch_id, f.stall_count, threshold);
            }

            // Counts must add up
            prop_assert_eq!(set.len() + abandoned.len(), original_len);
        }

        #[test]
        fn reset_stall_sets_to_zero(
            branch_id in 0..100u32,
            initial_stall in 0..1000u32,
        ) {
            let mut set = FrontierSet::new();
            set.insert(Frontier {
                branch_id,
                depth: 0,
                blocking_params: vec![],
                best_prefix: vec![],
                stall_count: initial_stall,
                rarity_boost: 0.0,
            });
            set.reset_stall(branch_id);
            let f = set.peek().unwrap();
            prop_assert_eq!(f.stall_count, 0);
        }

        #[test]
        fn stall_increment_then_abandon_at_threshold(
            threshold in 1..20u32,
        ) {
            let mut set = FrontierSet::new();
            set.insert(Frontier {
                branch_id: 1,
                depth: 0,
                blocking_params: vec![],
                best_prefix: vec![],
                stall_count: 0,
                rarity_boost: 0.0,
            });

            // Increment stall count up to threshold
            for _ in 0..threshold {
                set.increment_stall(1);
            }

            let abandoned = set.abandon_stalled(threshold);
            prop_assert_eq!(abandoned.len(), 1);
            prop_assert_eq!(abandoned[0].stall_count, threshold);
            prop_assert!(set.is_empty());
        }

        #[test]
        fn score_is_finite(f in arb_frontier()) {
            let score = frontier_score(&f);
            prop_assert!(score.is_finite(), "score must be finite, got {}", score);
        }

        #[test]
        fn deeper_frontier_scores_higher_all_else_equal(
            depth_a in 0..20u32,
            depth_b in 0..20u32,
            stall in 0..10u32,
            blocking_params in prop::collection::vec(0..10usize, 0..5),
            rarity in 0.0..=1.0f64,
        ) {
            let fa = Frontier {
                branch_id: 1, depth: depth_a, stall_count: stall,
                blocking_params: blocking_params.clone(), best_prefix: vec![],
                rarity_boost: rarity,
            };
            let fb = Frontier {
                branch_id: 2, depth: depth_b, stall_count: stall,
                blocking_params, best_prefix: vec![],
                rarity_boost: rarity,
            };
            if depth_a > depth_b {
                prop_assert!(frontier_score(&fa) > frontier_score(&fb));
            } else if depth_a < depth_b {
                prop_assert!(frontier_score(&fa) < frontier_score(&fb));
            } else {
                prop_assert!((frontier_score(&fa) - frontier_score(&fb)).abs() < f64::EPSILON);
            }
        }

        #[test]
        fn pop_returns_max_score(
            frontiers in prop::collection::vec(arb_frontier(), 1..20),
        ) {
            let mut set = FrontierSet::new();
            for f in frontiers {
                set.insert(f);
            }
            let popped = set.pop_highest_priority().unwrap();
            let popped_score = frontier_score(&popped);
            for remaining in set.iter() {
                prop_assert!(
                    popped_score >= frontier_score(remaining) - f64::EPSILON,
                    "popped score {} < remaining score {}",
                    popped_score, frontier_score(remaining)
                );
            }
        }

        #[test]
        fn constraint_boost_is_positive(
            depth in 0..20u32,
            stall in 0..10u32,
            rarity in 0.0..=1.0f64,
        ) {
            let without = Frontier {
                branch_id: 1, depth, stall_count: stall,
                blocking_params: vec![], best_prefix: vec![],
                rarity_boost: rarity,
            };
            let with = Frontier {
                branch_id: 2, depth, stall_count: stall,
                blocking_params: vec![0], best_prefix: vec![],
                rarity_boost: rarity,
            };
            prop_assert!(frontier_score(&with) > frontier_score(&without));
        }
    }
}
