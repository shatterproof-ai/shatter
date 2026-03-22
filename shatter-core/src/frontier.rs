//! Frontier tracking for concolic exploration.
//!
//! A frontier represents a branch reached but not fully explored during
//! concolic execution. Each frontier tracks which parameters block progress
//! and how many consecutive attempts have failed to make progress (stall count).
//! The `FrontierSet` maintains a collection of frontiers per function,
//! serving them in priority order: lower depth first, then lower stall count.

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

/// Priority-ordered collection of frontiers for a single function.
///
/// Frontiers are served lowest-depth first (shallowest branches are explored
/// before deeper ones), with stall count as a tiebreaker (less-stalled
/// frontiers are preferred). The set is typically small (< 100 entries),
/// so linear scans are acceptable.
#[derive(Debug, Clone, Default)]
pub struct FrontierSet {
    frontiers: Vec<Frontier>,
}

impl FrontierSet {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a frontier. If a frontier with the same `branch_id` already
    /// exists, it is replaced.
    pub fn insert(&mut self, frontier: Frontier) {
        self.remove(frontier.branch_id);
        self.frontiers.push(frontier);
    }

    /// Remove and return the highest-priority frontier (lowest depth, then
    /// lowest stall count).
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

    /// Index of the highest-priority frontier.
    /// Caller must ensure `self.frontiers` is non-empty.
    fn best_index(&self) -> usize {
        self.frontiers
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| frontier_priority(a, b))
            .map(|(i, _)| i)
            .expect("best_index called on non-empty FrontierSet")
    }
}

/// Compare two frontiers for priority ordering.
///
/// Lower depth wins first. Then higher rarity_boost wins (rare branches
/// get explored sooner). Finally lower stall count breaks ties.
fn frontier_priority(a: &Frontier, b: &Frontier) -> std::cmp::Ordering {
    a.depth
        .cmp(&b.depth)
        .then(b.rarity_boost.partial_cmp(&a.rarity_boost).unwrap_or(std::cmp::Ordering::Equal))
        .then(a.stall_count.cmp(&b.stall_count))
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn make_frontier_with_rarity(branch_id: u32, depth: u32, stall_count: u32, rarity_boost: f64) -> Frontier {
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
    fn priority_lower_depth_first() {
        let mut set = FrontierSet::new();
        set.insert(make_frontier(1, 5, 0));
        set.insert(make_frontier(2, 2, 0));
        set.insert(make_frontier(3, 8, 0));

        let f = set.pop_highest_priority().unwrap();
        assert_eq!(f.branch_id, 2, "shallowest branch should be popped first");

        let f = set.pop_highest_priority().unwrap();
        assert_eq!(f.branch_id, 1);

        let f = set.pop_highest_priority().unwrap();
        assert_eq!(f.branch_id, 3);
    }

    #[test]
    fn priority_stall_count_breaks_ties() {
        let mut set = FrontierSet::new();
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
    fn depth_still_wins_over_rarity_boost() {
        let mut set = FrontierSet::new();
        set.insert(make_frontier_with_rarity(1, 5, 0, 1.0)); // deep but very rare
        set.insert(make_frontier_with_rarity(2, 1, 0, 0.1)); // shallow but common

        let f = set.pop_highest_priority().unwrap();
        assert_eq!(f.branch_id, 2, "shallower depth should win even with lower rarity");
    }

    #[test]
    fn zero_rarity_boost_preserves_original_order() {
        let mut set = FrontierSet::new();
        set.insert(make_frontier_with_rarity(1, 3, 5, 0.0));
        set.insert(make_frontier_with_rarity(2, 3, 1, 0.0));

        let f = set.pop_highest_priority().unwrap();
        assert_eq!(f.branch_id, 2, "lower stall count wins when rarity is equal");
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
        set.insert(make_frontier(1, 0, 3));  // below threshold
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
            .prop_map(|(branch_id, depth, blocking_params, stall_count, rarity_boost)| {
                Frontier {
                    branch_id,
                    depth,
                    blocking_params,
                    best_prefix: vec![],
                    stall_count,
                    rarity_boost,
                }
            })
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
    }
}
