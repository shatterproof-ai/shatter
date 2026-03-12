//! Frontier tracking for concolic exploration.
//!
//! A frontier represents a branch reached but not fully explored during
//! concolic execution. Each frontier tracks which parameters block progress
//! and how many consecutive attempts have failed to make progress (stall count).
//! The `FrontierSet` maintains a collection of frontiers per function,
//! serving them in priority order: lower depth first, then lower stall count.

use serde::{Deserialize, Serialize};

/// Stall count threshold after which a frontier is considered deeply stalled.
/// Consumers can use this to deprioritize or abandon frontiers that aren't
/// making progress despite repeated attempts.
pub const DEFAULT_MAX_STALL: u32 = 10;

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
}
