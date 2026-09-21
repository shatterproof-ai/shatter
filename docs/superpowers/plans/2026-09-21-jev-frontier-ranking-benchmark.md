# Jev Frontier-Ranking Benchmark Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Measure whether a Jev decision oracle, used to rank exploration frontiers, discovers branches in fewer executions and less wall-clock than Shatter's built-in heuristic, with random and cheating rankers as floor and ceiling.

**Architecture:** Add a pluggable `FrontierRanker` policy to the concolic orchestrator that produces per-round external scores for the `FrontierSet`; keep the existing `frontier_score` heuristic as the default. Add a `DecisionOracle` trait (choice in, probabilities out) beside the existing generative `SeedOracle`, with a Jev HTTP adapter and a record/replay wrapper. A `#[ignore]`d Rust bench test drives all arms over a stratified fixture manifest and writes JSONL rows; a Python script turns rows into paired statistics, a calibration table, and a Pareto table.

**Tech Stack:** Rust 2024 (shatter-core, shatter-llm), tokio, reqwest, wiremock, proptest, Python 3 (stdlib only) for analysis, Taskfile.

**Spec:** This document's "Design" section is the spec. Tracker: epic str-hjrnp with children str-hjrnp.1 (Tasks 1-3), str-hjrnp.2 (Tasks 4-5), str-hjrnp.3 (Task 6), str-hjrnp.4 (Tasks 7-8); each child is its own branch.

## Global Constraints

- Never edit `main` directly. Create the branch and linked worktree with `bento:launch-work` before Task 1. Create a beads issue first (`bd create --title="Jev frontier-ranking benchmark" --type=feature --priority=2`) and claim it.
- Dependencies flow one direction: `shatter-cli → shatter-core`, `shatter-llm → shatter-core`. The ranker trait and `DecisionOracle` trait live in `shatter-core`; HTTP adapters live in `shatter-llm`. `shatter-core` must not depend on `shatter-llm`.
- Every new public function gets a proptest of a core invariant, not only a serialization roundtrip (`/formal-methods-policy`).
- The default `FrontierRanker` must leave existing behavior byte-identical: with `Heuristic`, no external scores are set and all existing tests pass unchanged.
- Commit and push with `--no-verify` per project memory (the pre-commit hook's git tests corrupt the worktree). Run `task test-quick` yourself before each commit instead.
- The TS frontend must be built (`cd shatter-ts && npm run build`) and the examples checkout present (`python3 scripts/examples_checkout.py`) for Tasks 3, 6 and 7.
- Jev API contract (from docs.typesafe.ai on 2026-09-21): `POST https://api.typesafe.ai/v1/systemone`, header `Authorization: Bearer $TYPESAFE_API_KEY`, body `{ "state": "...", "model": "jev-latest", "questions": { "<id>": { "type": "choice", "instructions": "...", "criteria": { "<opt>": "<desc>" } } } }`, response `{ "model": "...", "answers": { "<id>": { "choice": "<opt>", "probabilities": { "<opt>": 0.0 }, "confidence": 0.0 } }, "usage": { "input_tokens": N, "output_tokens": N } }`. Max 255 criteria per question.
- Out of scope for this plan (follow-up plans): feasibility pre-check before Z3, candidate ranking over cheap generators, CLI flags for the decision oracle, scan-report triage.

---

## Design

### What is measured

For each (fixture, seed, arm) the bench records: the execution index at which every branch id was first discovered, total executions, wall-clock seconds, discovery-method counts, Jev input tokens, and every ranking decision (round, branch id, score) so calibration can be computed post hoc.

Two budget regimes: fixed iterations (`max_iterations`) and fixed wall-clock (`timeout_explore`). Each arm runs under both.

### Arms

| Arm id | Ranker | Purpose |
|---|---|---|
| `heuristic` | existing `frontier_score` | baseline |
| `random` | seeded uniform scores per round | floor |
| `cheating` | boosts frontiers whose branch id appears in the reference run's discovery set but is not yet discovered | ceiling |
| `generative` | heuristic ranker plus existing `SeedOracle` via `anthropic` adapter | the real competitor on opaque cases; skipped if `ANTHROPIC_API_KEY` unset |
| `jev` | `DecisionFrontierRanker` over Jev Choice question, one call per round | hypothesis; skipped if `TYPESAFE_API_KEY` unset and no replay cache |

### Where ranking takes effect

`FrontierSet` gains `external_scores: HashMap<u32, f64>`. `FrontierSet::score(&self, f)` returns the external score when present, else `frontier_score(f)`. The three consumers in `orchestrator.rs` (drilling sort, bounded-unroll sort, oracle polling order) and `best_index` use `score`. Once per round, before `solve_and_generate`, the orchestrator calls `config.frontier_ranker.rank(...)` and installs the result.

### Sanity gates

The report script fails (exit 1) unless, aggregated across fixtures, `cheating` beats `heuristic` and `heuristic` beats `random` on median executions-to-all-expected-branches. If the harness cannot separate those, it cannot separate Jev.

### File map

| Path | Responsibility |
|---|---|
| `shatter-core/src/frontier.rs` | `FrontierRanker` trait, `RankContext`, `HeuristicRanker`, `RandomRanker`, `ScriptedRanker`, external scores on `FrontierSet` |
| `shatter-core/src/orchestrator.rs` | `frontier_ranker` on `ExploreConfig`; per-round rank call; `discovery_iterations` and `rank_log` on `ExploreResult`; `frontier_predicate` helper |
| `shatter-core/src/decision.rs` (new) | `DecisionOracle` trait, `ChoiceRequest`, `ChoiceResponse`, `MockDecisionOracle` |
| `shatter-llm/src/jev.rs` (new) | `JevAdapter: DecisionOracle` |
| `shatter-llm/src/replay.rs` (new) | `ReplayDecisionOracle` record/replay wrapper |
| `shatter-llm/src/decision_ranker.rs` (new) | `DecisionFrontierRanker: FrontierRanker` |
| `benchmarks/frontier-ranking/manifest.json` (new) | fixture list with strata and expected return values |
| `shatter-core/tests/bench_frontier_ranking.rs` (new, `#[ignore]`) | runs arms, writes JSONL |
| `scripts/bench_frontier_report.py` (new) | analysis + sanity gates |
| `scripts/test_bench_frontier_report.py` (new) | unit tests for the analysis |
| `Taskfile.yml` | `bench-frontier-reference`, `bench-frontier`, `bench-frontier-report` |
| `docs/perf/frontier-ranking-benchmark.md` (new) | how to run and read it |

---

### Task 1: Record the execution index of each branch discovery

**Files:**
- Modify: `shatter-core/src/orchestrator.rs:347-410` (`ExploreResult`), `:3130-3135` (discovery push), `:3700-3720` (result construction)
- Test: `shatter-core/src/orchestrator.rs` (inline `#[cfg(test)]` module near line 4780)

**Interfaces:**
- Produces: `ExploreResult::discovery_iterations: Vec<(u32, usize)>` — `(branch_id, total_executions_at_discovery)`, same order and length as `discoveries`.

- [ ] **Step 1: Write the failing test**

Add to the existing `mod tests` in `orchestrator.rs`:

```rust
#[test]
fn discovery_iterations_parallel_to_discoveries() {
    let result = ExploreResult {
        discoveries: vec![(3, DiscoveryMethod::Z3), (7, DiscoveryMethod::Random)],
        discovery_iterations: vec![(3, 12), (7, 40)],
        ..ExploreResult::empty_for_test("f")
    };
    assert_eq!(result.discoveries.len(), result.discovery_iterations.len());
    for ((a, _), (b, _)) in result.discoveries.iter().zip(&result.discovery_iterations) {
        assert_eq!(a, b);
    }
}
```

If `ExploreResult::empty_for_test` does not exist, add it under `#[cfg(test)]` returning all-default fields with the given name (check first: `grep -n "fn empty_for_test" shatter-core/src/orchestrator.rs`).

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p shatter-core discovery_iterations_parallel_to_discoveries`
Expected: compile error, no field `discovery_iterations`.

- [ ] **Step 3: Add the field and record it**

In `ExploreResult` (after `discoveries`):

```rust
/// `(branch_id, total_executions)` at the moment each branch in
/// `discoveries` was first seen. Parallel to `discoveries`.
pub discovery_iterations: Vec<(u32, usize)>,
```

Next to `let mut discoveries` in `explore_with_oracle` add `let mut discovery_iterations: Vec<(u32, usize)> = Vec::new();`. At the discovery push (line ~3133):

```rust
if seen_branch_ids.insert(decision.branch_id) {
    discoveries.push((decision.branch_id, method));
    discovery_iterations.push((decision.branch_id, total_executions));
}
```

Check the other push at line ~2981 (resume-state path) and add the same parallel push there. In the `ExploreResult { .. }` construction near line 3710 add `discovery_iterations,`. Fix every other `ExploreResult { .. }` literal in the crate (`grep -rn "discoveries: vec!\[\]" shatter-core/src shatter-core/tests shatter-cli/src`) by adding `discovery_iterations: vec![],`.

- [ ] **Step 4: Run tests**

Run: `cargo test -p shatter-core discovery_iterations && cargo test -p shatter-cli --lib`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add shatter-core/src/orchestrator.rs shatter-cli/src
git commit --no-verify -m "feat(orchestrator): record execution index per branch discovery"
```

---

### Task 2: FrontierRanker trait and built-in rankers

**Files:**
- Modify: `shatter-core/src/frontier.rs`
- Modify: `shatter-core/Cargo.toml` (add `async-trait = "0.1"` and `rand = "0.8"` to `[dependencies]` if not already present; check `grep -n "async-trait\|^rand" shatter-core/Cargo.toml`)
- Test: `shatter-core/src/frontier.rs` inline tests

**Interfaces:**
- Produces:

```rust
pub struct FrontierSummary { pub branch_id: u32, pub depth: u32, pub stall_count: u32, pub line: u32, pub predicate: String }
pub struct RankContext<'a> { pub function_name: &'a str, pub function_source: &'a str, pub round: usize, pub frontiers: Vec<FrontierSummary> }
#[async_trait] pub trait FrontierRanker: Send + Sync + std::fmt::Debug {
    fn name(&self) -> &'static str;
    async fn rank(&self, ctx: &RankContext<'_>) -> anyhow::Result<HashMap<u32, f64>>;
}
pub struct HeuristicRanker; pub struct RandomRanker { pub seed: u64 } pub struct ScriptedRanker { pub priority: HashSet<u32> }
impl FrontierSet { pub fn set_external_scores(&mut self, HashMap<u32,f64>); pub fn score(&self, &Frontier) -> f64; pub fn sorted_desc(&self) -> Vec<Frontier>; }
```

- [ ] **Step 1: Write the failing tests**

Append to `mod tests` in `frontier.rs`:

```rust
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
    assert_eq!(set.sorted_desc().iter().map(|f| f.branch_id).collect::<Vec<_>>(), vec![2, 1]);
}

#[test]
fn missing_external_score_falls_back_to_heuristic() {
    let mut set = FrontierSet::new();
    set.insert(make_frontier(1, 5, 0));
    set.insert(make_frontier(2, 0, 0));
    let mut ext = HashMap::new();
    ext.insert(2, 0.5);
    set.set_external_scores(ext);
    // 1 has no external score: heuristic score for depth 5 is DEPTH_WEIGHT*5 > 0.5
    assert_eq!(set.score(&make_frontier(1, 5, 0)), frontier_score(&make_frontier(1, 5, 0)));
}

#[tokio::test]
async fn heuristic_ranker_returns_empty_map() {
    let ctx = RankContext { function_name: "f", function_source: "", round: 0, frontiers: vec![] };
    assert!(HeuristicRanker.rank(&ctx).await.unwrap().is_empty());
}

#[tokio::test]
async fn random_ranker_is_deterministic_per_seed_and_round() {
    let fr = |id| FrontierSummary { branch_id: id, depth: 0, stall_count: 0, line: 0, predicate: String::new() };
    let ctx = RankContext { function_name: "f", function_source: "", round: 3, frontiers: vec![fr(1), fr(2), fr(3)] };
    let a = RandomRanker { seed: 7 }.rank(&ctx).await.unwrap();
    let b = RandomRanker { seed: 7 }.rank(&ctx).await.unwrap();
    assert_eq!(a, b);
    assert_eq!(a.len(), 3);
    let ctx2 = RankContext { round: 4, ..ctx };
    assert_ne!(a, RandomRanker { seed: 7 }.rank(&ctx2).await.unwrap());
}

#[tokio::test]
async fn scripted_ranker_boosts_priority_ids_only() {
    let fr = |id| FrontierSummary { branch_id: id, depth: 0, stall_count: 0, line: 0, predicate: String::new() };
    let ctx = RankContext { function_name: "f", function_source: "", round: 0, frontiers: vec![fr(1), fr(2)] };
    let r = ScriptedRanker { priority: [2].into_iter().collect() }.rank(&ctx).await.unwrap();
    assert_eq!(r[&2], 1.0);
    assert_eq!(r[&1], 0.0);
}

proptest! {
    #[test]
    fn sorted_desc_is_nonincreasing(ids in prop::collection::hash_set(0u32..50, 1..20), seed in any::<u64>()) {
        let mut set = FrontierSet::new();
        for id in &ids { set.insert(make_frontier(*id, *id % 4, *id % 3)); }
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        let ext: HashMap<u32, f64> = ids.iter().filter(|_| rng.gen_bool(0.5)).map(|id| (*id, rng.gen_range(0.0..1.0))).collect();
        set.set_external_scores(ext);
        let sorted = set.sorted_desc();
        prop_assert_eq!(sorted.len(), ids.len());
        for w in sorted.windows(2) { prop_assert!(set.score(&w[0]) >= set.score(&w[1])); }
    }
}
```

Add `use std::collections::{HashMap, HashSet}; use proptest::prelude::*; use rand::{Rng, SeedableRng};` at the top of the tests module. `tokio` with `macros` and `rt` must be in `[dev-dependencies]` of shatter-core (check: `grep -n "^tokio" shatter-core/Cargo.toml`; it is a normal dependency already because the orchestrator is async, so `#[tokio::test]` works).

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p shatter-core frontier::tests`
Expected: compile errors for `set_external_scores`, `RankContext`, `HeuristicRanker`, etc.

- [ ] **Step 3: Implement**

In `frontier.rs`:

```rust
use std::collections::{HashMap, HashSet};
use async_trait::async_trait;
use rand::{Rng, SeedableRng};

/// Compact view of a frontier handed to rankers (no input vectors).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FrontierSummary {
    pub branch_id: u32,
    pub depth: u32,
    pub stall_count: u32,
    pub line: u32,
    pub predicate: String,
}

/// Everything a ranker may look at for one round.
#[derive(Debug, Clone)]
pub struct RankContext<'a> {
    pub function_name: &'a str,
    pub function_source: &'a str,
    pub round: usize,
    pub frontiers: Vec<FrontierSummary>,
}

/// Per-round policy that assigns external scores to frontiers. An empty map
/// means "use the built-in heuristic for everything".
#[async_trait]
pub trait FrontierRanker: Send + Sync + std::fmt::Debug {
    fn name(&self) -> &'static str;
    async fn rank(&self, ctx: &RankContext<'_>) -> anyhow::Result<HashMap<u32, f64>>;
}

#[derive(Debug, Default)]
pub struct HeuristicRanker;

#[async_trait]
impl FrontierRanker for HeuristicRanker {
    fn name(&self) -> &'static str { "heuristic" }
    async fn rank(&self, _ctx: &RankContext<'_>) -> anyhow::Result<HashMap<u32, f64>> {
        Ok(HashMap::new())
    }
}

/// Uniform random scores, deterministic in `(seed, round)`.
#[derive(Debug)]
pub struct RandomRanker { pub seed: u64 }

#[async_trait]
impl FrontierRanker for RandomRanker {
    fn name(&self) -> &'static str { "random" }
    async fn rank(&self, ctx: &RankContext<'_>) -> anyhow::Result<HashMap<u32, f64>> {
        let mut rng = rand::rngs::StdRng::seed_from_u64(self.seed ^ (ctx.round as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
        Ok(ctx.frontiers.iter().map(|f| (f.branch_id, rng.gen_range(0.0..1.0))).collect())
    }
}

/// Scores 1.0 for branch ids in `priority`, 0.0 otherwise. The "cheating"
/// arm: `priority` is filled from a reference run's discovery set.
#[derive(Debug)]
pub struct ScriptedRanker { pub priority: HashSet<u32> }

#[async_trait]
impl FrontierRanker for ScriptedRanker {
    fn name(&self) -> &'static str { "scripted" }
    async fn rank(&self, ctx: &RankContext<'_>) -> anyhow::Result<HashMap<u32, f64>> {
        Ok(ctx.frontiers.iter().map(|f| (f.branch_id, if self.priority.contains(&f.branch_id) { 1.0 } else { 0.0 })).collect())
    }
}
```

Add `external_scores: HashMap<u32, f64>` to `FrontierSet` (keep `#[derive(Default)]` working) and:

```rust
impl FrontierSet {
    /// Replace this round's external scores. Ids not present fall back to
    /// [`frontier_score`].
    pub fn set_external_scores(&mut self, scores: HashMap<u32, f64>) {
        self.external_scores = scores;
    }

    /// Effective priority of `f`: external score when present, else heuristic.
    pub fn score(&self, f: &Frontier) -> f64 {
        self.external_scores.get(&f.branch_id).copied().unwrap_or_else(|| frontier_score(f))
    }

    /// All frontiers, highest effective score first.
    pub fn sorted_desc(&self) -> Vec<Frontier> {
        let mut v: Vec<Frontier> = self.frontiers.clone();
        v.sort_by(|a, b| self.score(b).partial_cmp(&self.score(a)).unwrap_or(std::cmp::Ordering::Equal));
        v
    }
}
```

Change `best_index` to compare `self.score(a)` / `self.score(b)`. Also in `remove`, drop the id from `external_scores`.

- [ ] **Step 4: Run tests**

Run: `cargo test -p shatter-core frontier::`
Expected: all PASS, including pre-existing frontier tests.

- [ ] **Step 5: Commit**

```bash
git add shatter-core/src/frontier.rs shatter-core/Cargo.toml Cargo.lock
git commit --no-verify -m "feat(frontier): FrontierRanker trait with heuristic, random and scripted rankers"
```

---

### Task 3: Wire the ranker into the orchestrator

**Files:**
- Modify: `shatter-core/src/orchestrator.rs` — `ExploreConfig` (:103-160, Default :190), `poll_oracle_for_frontier` (:431-478), `solve_and_generate` (:2054-2180), main loop around the `solve_and_generate` call (:3319), `ExploreResult` (:347)
- Test: `shatter-core/tests/e2e_llm_oracle.rs` (new test in same file, reuses its helpers)

**Interfaces:**
- Consumes: `FrontierRanker`, `RankContext`, `FrontierSummary`, `FrontierSet::set_external_scores/score/sorted_desc` from Task 2.
- Produces: `ExploreConfig::frontier_ranker: Arc<dyn FrontierRanker>` (default `HeuristicRanker`); `ExploreResult::rank_log: Vec<RankDecision>` with `pub struct RankDecision { pub round: usize, pub executions: usize, pub branch_id: u32, pub score: f64 }`; `pub fn frontier_predicate(branch_id: u32, raw_results: &[(Vec<Value>, Vec<MockConfig>, ExecuteResult)]) -> (String, u32)` returning `(predicate, line)`.

- [ ] **Step 1: Write the failing test**

Append to `shatter-core/tests/e2e_llm_oracle.rs`:

```rust
/// A scripted ranker that prioritizes the branch ids a reference run found
/// must reach the "zero" branch of classifyNumber in no more executions
/// than the heuristic ranker does, and must leave a rank_log behind.
#[tokio::test]
async fn scripted_ranker_is_consulted_each_round() {
    use shatter_core::frontier::{HeuristicRanker, ScriptedRanker};
    let file = examples_dir().join("01-arithmetic.ts");
    let file_str = file.to_string_lossy().to_string();

    async fn run(ranker: Arc<dyn shatter_core::frontier::FrontierRanker>, file_str: &str) -> ExploreResult {
        let mut frontend = spawn_ts_frontend().await;
        let analysis = analyze_function(&mut frontend, file_str, "classifyNumber").await;
        instrument_function(&mut frontend, file_str, "classifyNumber").await;
        let config = ExploreConfig {
            max_iterations: Some(30),
            max_executions: Some(100),
            plateau_threshold: 0,
            seed: Some(1),
            frontier_ranker: ranker,
            ..Default::default()
        };
        let (result, _) = orchestrator::explore(
            &mut frontend, "classifyNumber",
            vec![vec![serde_json::json!(5)], vec![serde_json::json!(-3)]],
            vec![], &analysis.params, &config, None, None, vec![], None, None,
        ).await.expect("exploration failed");
        result
    }

    let reference = run(Arc::new(HeuristicRanker), &file_str).await;
    let all_ids: std::collections::HashSet<u32> = reference.discoveries.iter().map(|(id, _)| *id).collect();
    let scripted = run(Arc::new(ScriptedRanker { priority: all_ids }), &file_str).await;

    assert!(!scripted.rank_log.is_empty(), "ranker should have been consulted");
    assert!(scripted.rank_log.iter().all(|d| d.score == 0.0 || d.score == 1.0));
    assert!(return_value_set(&scripted).contains("\"zero\""));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p shatter-core --test e2e_llm_oracle scripted_ranker_is_consulted_each_round`
Expected: compile error, no field `frontier_ranker` / `rank_log`.

- [ ] **Step 3: Implement**

In `ExploreConfig` add:

```rust
/// Per-round frontier prioritization policy. Default: built-in heuristic.
pub frontier_ranker: std::sync::Arc<dyn crate::frontier::FrontierRanker>,
```

and in `Default`: `frontier_ranker: std::sync::Arc::new(crate::frontier::HeuristicRanker),`. `ExploreConfig` derives `Clone` and `Debug`; `Arc<dyn FrontierRanker>` satisfies both because the trait has a `Debug` supertrait.

Add to `ExploreResult`:

```rust
/// Every external score the ranker produced, for calibration analysis.
pub rank_log: Vec<RankDecision>,
```

and define near `ExploreResult`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RankDecision {
    pub round: usize,
    pub executions: usize,
    pub branch_id: u32,
    pub score: f64,
}
```

Extract the predicate lookup from `poll_oracle_for_frontier` (lines 456-478) into:

```rust
/// Recover the most recent predicate text and source line observed for
/// `branch_id`. Returns `(String::new(), 0)` when the branch was never seen.
pub fn frontier_predicate(
    branch_id: u32,
    raw_results: &[(Vec<serde_json::Value>, Vec<MockConfig>, ExecuteResult)],
) -> (String, u32) {
    for (_, _, result) in raw_results.iter().rev() {
        if let Some(decision) = result.branch_path.iter().find(|d| d.branch_id == branch_id) {
            let predicate = match &decision.constraint {
                crate::execution_record::SymConstraint::Expr { expr } => format!("{expr:?}"),
                crate::execution_record::SymConstraint::Unknown { hint } => hint.clone(),
                _ => String::new(),
            };
            return (predicate, decision.line);
        }
    }
    (String::new(), 0)
}
```

and make `poll_oracle_for_frontier` call it, iterating `frontier_set.sorted_desc()` instead of `frontier_set.iter()` so external scores govern polling order.

In `solve_and_generate`, replace both `frontier_score(b).partial_cmp(&frontier_score(a))` sorts with `frontier_set.score(b).partial_cmp(&frontier_set.score(a))` (the sorts operate on cloned `Frontier`s; `frontier_set` is `&mut FrontierSet` in scope). Remove the now-unused `frontier_score` import if the compiler warns.

In the main loop, immediately before the `solve_and_generate(` call at line ~3319, add:

```rust
// Consult the frontier ranker once per round.
{
    let summaries: Vec<crate::frontier::FrontierSummary> = frontier_set
        .sorted_desc()
        .into_iter()
        .take(255)
        .map(|f| {
            let (predicate, line) = frontier_predicate(f.branch_id, &raw_results);
            crate::frontier::FrontierSummary { branch_id: f.branch_id, depth: f.depth, stall_count: f.stall_count, line, predicate }
        })
        .collect();
    if !summaries.is_empty() {
        let ctx = crate::frontier::RankContext {
            function_name,
            function_source: oracle.as_ref().map(|h| h.function_source.as_str()).unwrap_or(""),
            round,
            frontiers: summaries,
        };
        match config.frontier_ranker.rank(&ctx).await {
            Ok(scores) => {
                for (branch_id, score) in &scores {
                    rank_log.push(RankDecision { round, executions: total_executions, branch_id: *branch_id, score: *score });
                }
                frontier_set.set_external_scores(scores);
            }
            Err(e) => log::warn!("frontier ranker {} failed on round {round}: {e}", config.frontier_ranker.name()),
        }
    }
}
```

Add `let mut rank_log: Vec<RankDecision> = Vec::new();` and `let mut round: usize = 0;` beside the other loop-state declarations near line 2460, increment `round += 1;` at the top of the `loop {` body at line ~2688, and add `rank_log,` to the `ExploreResult` construction. Fix every other `ExploreResult { .. }` literal (`grep -rn "discovery_iterations: vec!\[\]" shatter-core shatter-cli`) by adding `rank_log: vec![],`.

`function_source` is only available when an oracle handle is present today. That is acceptable for this plan: the bench passes an `OracleHandle` with a mock `SeedOracle` and `max_queries_per_function: 0` when it needs source in the context. Note this in the doc (Task 8).

- [ ] **Step 4: Run tests**

Run: `cargo test -p shatter-core --test e2e_llm_oracle && cargo test -p shatter-core --lib && cargo test -p shatter-core --test e2e_concolic`
Expected: PASS. The heuristic default must not change any existing e2e outcome.

- [ ] **Step 5: Commit**

```bash
git add shatter-core/src/orchestrator.rs shatter-core/tests/e2e_llm_oracle.rs
git commit --no-verify -m "feat(orchestrator): consult FrontierRanker per round; log rank decisions"
```

---

### Task 4: DecisionOracle trait, Jev adapter, replay wrapper

**Files:**
- Create: `shatter-core/src/decision.rs`; register `pub mod decision;` in `shatter-core/src/lib.rs` after `pub mod oracle;`
- Create: `shatter-llm/src/jev.rs`, `shatter-llm/src/replay.rs`; add `pub mod jev; pub mod replay; pub use jev::JevAdapter; pub use replay::ReplayDecisionOracle;` to `shatter-llm/src/lib.rs`
- Modify: `shatter-llm/Cargo.toml` — add `sha2 = "0.10"` and `hex = "0.4"` to `[dependencies]`
- Test: `shatter-llm/tests/jev_adapter.rs` (new, wiremock), `shatter-core/src/decision.rs` inline tests

**Interfaces:**
- Produces (shatter-core):

```rust
pub struct ChoiceRequest { pub state: String, pub instructions: String, pub criteria: Vec<(String, String)> } // ordered (key, description); len <= 255
pub struct ChoiceResponse { pub choice: String, pub probabilities: HashMap<String, f64>, pub confidence: f64, pub input_tokens: u32 }
#[async_trait] pub trait DecisionOracle: Send + Sync + std::fmt::Debug {
    fn name(&self) -> &'static str;
    async fn choose(&self, req: &ChoiceRequest) -> anyhow::Result<ChoiceResponse>;
}
pub struct MockDecisionOracle { .. } // MockDecisionOracle::uniform(), MockDecisionOracle::scripted(Vec<(String /*key*/, f64)>)
pub fn request_fingerprint(req: &ChoiceRequest) -> String  // stable sha256 hex of canonical JSON
```

- Produces (shatter-llm): `JevAdapter::new(JevConfig { url: String, api_key: String, model: String, timeout_seconds: u32 })`; `ReplayDecisionOracle::new(inner: Option<Arc<dyn DecisionOracle>>, cache_dir: PathBuf)`.

- [ ] **Step 1: Write failing tests**

`shatter-core/src/decision.rs` tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn fingerprint_is_stable_and_order_sensitive() {
        let a = ChoiceRequest { state: "s".into(), instructions: "i".into(), criteria: vec![("x".into(), "1".into()), ("y".into(), "2".into())] };
        let b = ChoiceRequest { criteria: vec![("y".into(), "2".into()), ("x".into(), "1".into())], ..a.clone() };
        assert_eq!(request_fingerprint(&a), request_fingerprint(&a));
        assert_ne!(request_fingerprint(&a), request_fingerprint(&b));
        assert_eq!(request_fingerprint(&a).len(), 64);
    }

    #[test]
    fn too_many_criteria_is_rejected() {
        let criteria = (0..256).map(|i| (format!("k{i}"), String::new())).collect();
        let req = ChoiceRequest { state: String::new(), instructions: String::new(), criteria };
        assert!(req.validate().is_err());
    }

    #[tokio::test]
    async fn mock_uniform_sums_to_one() {
        let req = ChoiceRequest { state: String::new(), instructions: String::new(), criteria: vec![("a".into(), String::new()), ("b".into(), String::new())] };
        let r = MockDecisionOracle::uniform().choose(&req).await.unwrap();
        assert!((r.probabilities.values().sum::<f64>() - 1.0).abs() < 1e-9);
        assert_eq!(r.probabilities.len(), 2);
    }

    proptest! {
        #[test]
        fn mock_scripted_probabilities_cover_every_criterion(n in 1usize..40) {
            let criteria: Vec<(String, String)> = (0..n).map(|i| (format!("k{i}"), String::new())).collect();
            let req = ChoiceRequest { state: String::new(), instructions: String::new(), criteria: criteria.clone() };
            let oracle = MockDecisionOracle::scripted(vec![("k0".into(), 0.75)]);
            let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
            let r = rt.block_on(oracle.choose(&req)).unwrap();
            for (k, _) in &criteria { prop_assert!(r.probabilities.contains_key(k)); }
            prop_assert!((r.probabilities.values().sum::<f64>() - 1.0).abs() < 1e-6);
        }
    }
}
```

`shatter-llm/tests/jev_adapter.rs`:

```rust
use std::sync::Arc;
use shatter_core::decision::{ChoiceRequest, DecisionOracle};
use shatter_llm::{JevAdapter, ReplayDecisionOracle};
use shatter_llm::jev::JevConfig;
use wiremock::{Mock, MockServer, ResponseTemplate};
use wiremock::matchers::{method, path, header, body_partial_json};

fn req() -> ChoiceRequest {
    ChoiceRequest {
        state: "function f(x) { if (x > 3) {} }".into(),
        instructions: "Which branch next?".into(),
        criteria: vec![("b1".into(), "line 1: x > 3".into()), ("b2".into(), "line 1: !(x > 3)".into())],
    }
}

#[tokio::test]
async fn jev_adapter_posts_choice_and_parses_probabilities() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/v1/systemone"))
        .and(header("Authorization", "Bearer test-key"))
        .and(body_partial_json(serde_json::json!({"model": "jev-latest", "questions": {"frontier": {"type": "choice"}}})))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "model": "jev-1.13.0",
            "answers": {"frontier": {"type": "choice", "choice": "b2", "probabilities": {"b1": 0.25, "b2": 0.75}, "confidence": 0.5}},
            "usage": {"input_tokens": 120, "output_tokens": 8}
        })))
        .mount(&server).await;

    let adapter = JevAdapter::new(JevConfig { url: format!("{}/v1/systemone", server.uri()), api_key: "test-key".into(), model: "jev-latest".into(), timeout_seconds: 5 }).unwrap();
    let r = adapter.choose(&req()).await.unwrap();
    assert_eq!(r.choice, "b2");
    assert_eq!(r.probabilities["b2"], 0.75);
    assert_eq!(r.input_tokens, 120);
}

#[tokio::test]
async fn jev_adapter_surfaces_http_errors() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).respond_with(ResponseTemplate::new(429)).mount(&server).await;
    let adapter = JevAdapter::new(JevConfig { url: format!("{}/v1/systemone", server.uri()), api_key: "k".into(), model: "jev-latest".into(), timeout_seconds: 5 }).unwrap();
    let err = adapter.choose(&req()).await.unwrap_err().to_string();
    assert!(err.contains("429"), "got: {err}");
}

#[tokio::test]
async fn replay_records_then_serves_without_inner() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
        "model": "jev-1.13.0",
        "answers": {"frontier": {"type": "choice", "choice": "b1", "probabilities": {"b1": 0.6, "b2": 0.4}, "confidence": 0.2}},
        "usage": {"input_tokens": 50, "output_tokens": 8}
    }))).expect(1).mount(&server).await;
    let dir = tempfile::tempdir().unwrap();
    let inner: Arc<dyn DecisionOracle> = Arc::new(JevAdapter::new(JevConfig { url: format!("{}/v1/systemone", server.uri()), api_key: "k".into(), model: "jev-latest".into(), timeout_seconds: 5 }).unwrap());

    let recording = ReplayDecisionOracle::new(Some(inner), dir.path().to_path_buf());
    let first = recording.choose(&req()).await.unwrap();

    let replay = ReplayDecisionOracle::new(None, dir.path().to_path_buf());
    let second = replay.choose(&req()).await.unwrap();
    assert_eq!(first.probabilities, second.probabilities);

    let miss = ChoiceRequest { instructions: "different".into(), ..req() };
    assert!(replay.choose(&miss).await.is_err());
}
```

Add `tempfile = "3"` to shatter-llm `[dev-dependencies]`.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p shatter-core decision:: ; cargo test -p shatter-llm --test jev_adapter`
Expected: compile errors, modules missing.

- [ ] **Step 3: Implement `shatter-core/src/decision.rs`**

```rust
//! Decision oracle: typed choice in, calibrated probabilities out. Distinct
//! from [`crate::oracle::SeedOracle`], which generates input vectors.

use std::collections::HashMap;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Hard cap from the Jev API (255 options per choice question).
pub const MAX_CRITERIA: usize = 255;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChoiceRequest {
    pub state: String,
    pub instructions: String,
    /// Ordered `(key, description)` pairs. Order matters for fingerprinting.
    pub criteria: Vec<(String, String)>,
}

impl ChoiceRequest {
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.criteria.is_empty() { anyhow::bail!("choice request has no criteria"); }
        if self.criteria.len() > MAX_CRITERIA { anyhow::bail!("choice request has {} criteria; max is {MAX_CRITERIA}", self.criteria.len()); }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChoiceResponse {
    pub choice: String,
    pub probabilities: HashMap<String, f64>,
    pub confidence: f64,
    pub input_tokens: u32,
}

#[async_trait]
pub trait DecisionOracle: Send + Sync + std::fmt::Debug {
    fn name(&self) -> &'static str;
    async fn choose(&self, req: &ChoiceRequest) -> anyhow::Result<ChoiceResponse>;
}

/// Stable content hash of a request, used as the replay-cache key.
pub fn request_fingerprint(req: &ChoiceRequest) -> String {
    use sha2::{Digest, Sha256};
    let canonical = serde_json::to_vec(req).expect("ChoiceRequest serializes");
    hex::encode(Sha256::digest(canonical))
}

/// Test double. `uniform()` spreads probability evenly; `scripted()` pins
/// the given keys and spreads the remainder evenly over the rest.
#[derive(Debug, Default)]
pub struct MockDecisionOracle { pinned: Vec<(String, f64)> }

impl MockDecisionOracle {
    pub fn uniform() -> Self { Self::default() }
    pub fn scripted(pinned: Vec<(String, f64)>) -> Self { Self { pinned } }
}

#[async_trait]
impl DecisionOracle for MockDecisionOracle {
    fn name(&self) -> &'static str { "mock" }
    async fn choose(&self, req: &ChoiceRequest) -> anyhow::Result<ChoiceResponse> {
        req.validate()?;
        let mut probabilities: HashMap<String, f64> = HashMap::new();
        let mut pinned_mass = 0.0;
        for (k, p) in &self.pinned {
            if req.criteria.iter().any(|(ck, _)| ck == k) { probabilities.insert(k.clone(), *p); pinned_mass += p; }
        }
        let rest: Vec<&String> = req.criteria.iter().map(|(k, _)| k).filter(|k| !probabilities.contains_key(*k)).collect();
        let remaining = (1.0 - pinned_mass).max(0.0);
        let share = if rest.is_empty() { 0.0 } else { remaining / rest.len() as f64 };
        for k in rest { probabilities.insert(k.clone(), share); }
        let total: f64 = probabilities.values().sum();
        if total > 0.0 { for v in probabilities.values_mut() { *v /= total; } }
        let choice = probabilities.iter().max_by(|a, b| a.1.partial_cmp(b.1).unwrap()).map(|(k, _)| k.clone()).unwrap_or_default();
        Ok(ChoiceResponse { choice, probabilities, confidence: 0.0, input_tokens: 0 })
    }
}
```

Add `sha2 = "0.10"` and `hex = "0.4"` to shatter-core `[dependencies]` (the fingerprint lives in core so the replay key is defined once).

- [ ] **Step 4: Implement `shatter-llm/src/jev.rs`**

```rust
//! Jev (TypeSafe System One) adapter for [`DecisionOracle`].
//! API contract: POST /v1/systemone with a single `choice` question.

use std::collections::HashMap;
use std::time::Duration;
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Value};
use shatter_core::decision::{ChoiceRequest, ChoiceResponse, DecisionOracle};

pub const DEFAULT_JEV_URL: &str = "https://api.typesafe.ai/v1/systemone";
const QUESTION_ID: &str = "frontier";

#[derive(Debug, Clone)]
pub struct JevConfig {
    pub url: String,
    pub api_key: String,
    pub model: String,
    pub timeout_seconds: u32,
}

impl JevConfig {
    /// Build from `TYPESAFE_API_KEY`; `None` when unset.
    pub fn from_env() -> Option<Self> {
        let api_key = std::env::var("TYPESAFE_API_KEY").ok()?;
        Some(Self { url: DEFAULT_JEV_URL.into(), api_key, model: "jev-latest".into(), timeout_seconds: 10 })
    }
}

#[derive(Debug)]
pub struct JevAdapter { client: Client, config: JevConfig }

impl JevAdapter {
    pub fn new(config: JevConfig) -> anyhow::Result<Self> {
        let client = Client::builder().timeout(Duration::from_secs(u64::from(config.timeout_seconds))).build()?;
        Ok(Self { client, config })
    }

    fn body(&self, req: &ChoiceRequest) -> Value {
        let criteria: serde_json::Map<String, Value> = req.criteria.iter().map(|(k, d)| (k.clone(), Value::String(d.clone()))).collect();
        json!({
            "state": req.state,
            "model": self.config.model,
            "questions": { QUESTION_ID: { "type": "choice", "instructions": req.instructions, "criteria": criteria } }
        })
    }
}

#[derive(Deserialize)]
struct JevAnswer { choice: String, probabilities: HashMap<String, f64>, #[serde(default)] confidence: f64 }
#[derive(Deserialize)]
struct JevUsage { #[serde(default)] input_tokens: u32 }
#[derive(Deserialize)]
struct JevResponse { answers: HashMap<String, JevAnswer>, #[serde(default)] usage: Option<JevUsage> }

#[async_trait]
impl DecisionOracle for JevAdapter {
    fn name(&self) -> &'static str { "jev" }
    async fn choose(&self, req: &ChoiceRequest) -> anyhow::Result<ChoiceResponse> {
        req.validate()?;
        let resp = self.client.post(&self.config.url).bearer_auth(&self.config.api_key).json(&self.body(req)).send().await?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("jev request failed with HTTP {}: {}", status.as_u16(), text.chars().take(300).collect::<String>());
        }
        let parsed: JevResponse = resp.json().await?;
        let answer = parsed.answers.remove_entry_or_err(QUESTION_ID)?;
        Ok(ChoiceResponse {
            choice: answer.choice,
            probabilities: answer.probabilities,
            confidence: answer.confidence,
            input_tokens: parsed.usage.map(|u| u.input_tokens).unwrap_or(0),
        })
    }
}

trait RemoveOrErr { fn remove_entry_or_err(self, key: &str) -> anyhow::Result<JevAnswer>; }
impl RemoveOrErr for HashMap<String, JevAnswer> {
    fn remove_entry_or_err(mut self, key: &str) -> anyhow::Result<JevAnswer> {
        self.remove(key).ok_or_else(|| anyhow::anyhow!("jev response missing answer {key:?}"))
    }
}
```

Note `parsed` must be `let mut parsed` for `remove`; adjust when compiling.

- [ ] **Step 5: Implement `shatter-llm/src/replay.rs`**

```rust
//! Record/replay wrapper: caches [`DecisionOracle`] responses on disk keyed
//! by request fingerprint so benchmark runs are reproducible and offline.

use std::path::PathBuf;
use std::sync::Arc;
use async_trait::async_trait;
use shatter_core::decision::{request_fingerprint, ChoiceRequest, ChoiceResponse, DecisionOracle};

#[derive(Debug)]
pub struct ReplayDecisionOracle { inner: Option<Arc<dyn DecisionOracle>>, cache_dir: PathBuf }

impl ReplayDecisionOracle {
    /// `inner = None` means replay-only: a cache miss is an error.
    pub fn new(inner: Option<Arc<dyn DecisionOracle>>, cache_dir: PathBuf) -> Self { Self { inner, cache_dir } }
    fn path_for(&self, req: &ChoiceRequest) -> PathBuf { self.cache_dir.join(format!("{}.json", request_fingerprint(req))) }
}

#[async_trait]
impl DecisionOracle for ReplayDecisionOracle {
    fn name(&self) -> &'static str { "replay" }
    async fn choose(&self, req: &ChoiceRequest) -> anyhow::Result<ChoiceResponse> {
        let path = self.path_for(req);
        if let Ok(bytes) = tokio::fs::read(&path).await {
            return Ok(serde_json::from_slice(&bytes)?);
        }
        let inner = self.inner.as_ref().ok_or_else(|| anyhow::anyhow!("replay cache miss for {} and no live oracle configured", path.display()))?;
        let resp = inner.choose(req).await?;
        tokio::fs::create_dir_all(&self.cache_dir).await?;
        tokio::fs::write(&path, serde_json::to_vec_pretty(&resp)?).await?;
        Ok(resp)
    }
}
```

Add `"fs"` to the tokio features in `shatter-llm/Cargo.toml`.

- [ ] **Step 6: Run tests**

Run: `cargo test -p shatter-core decision:: && cargo test -p shatter-llm`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add shatter-core/src/decision.rs shatter-core/src/lib.rs shatter-core/Cargo.toml shatter-llm/src/jev.rs shatter-llm/src/replay.rs shatter-llm/src/lib.rs shatter-llm/Cargo.toml shatter-llm/tests/jev_adapter.rs Cargo.lock
git commit --no-verify -m "feat(llm): DecisionOracle trait, Jev adapter, record/replay wrapper"
```

---

### Task 5: DecisionFrontierRanker

**Files:**
- Create: `shatter-llm/src/decision_ranker.rs`; add `pub mod decision_ranker; pub use decision_ranker::DecisionFrontierRanker;` to `shatter-llm/src/lib.rs`
- Test: inline tests in the new file

**Interfaces:**
- Consumes: `FrontierRanker`, `RankContext` (Task 2); `DecisionOracle`, `ChoiceRequest`, `MockDecisionOracle` (Task 4).
- Produces: `DecisionFrontierRanker::new(oracle: Arc<dyn DecisionOracle>) -> Self`; `pub fn build_choice_request(ctx: &RankContext<'_>) -> ChoiceRequest`; `pub fn tokens_used(&self) -> u32`.

- [ ] **Step 1: Write failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use shatter_core::decision::MockDecisionOracle;
    use shatter_core::frontier::{FrontierRanker, FrontierSummary, RankContext};
    use proptest::prelude::*;

    fn ctx<'a>(n: u32) -> RankContext<'a> {
        RankContext {
            function_name: "f", function_source: "function f(x) {}", round: 1,
            frontiers: (0..n).map(|i| FrontierSummary { branch_id: i, depth: i % 3, stall_count: 0, line: 10 + i, predicate: format!("x > {i}") }).collect(),
        }
    }

    #[test]
    fn request_keys_map_back_to_branch_ids() {
        let req = build_choice_request(&ctx(3));
        assert_eq!(req.criteria.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(), vec!["b0", "b1", "b2"]);
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
    }

    proptest! {
        #[test]
        fn never_exceeds_255_criteria(n in 1u32..600) {
            let req = build_choice_request(&ctx(n));
            prop_assert!(req.criteria.len() <= 255);
            prop_assert!(req.validate().is_ok());
        }
    }
}
```

Add `proptest = "1"` to shatter-llm `[dev-dependencies]`.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p shatter-llm decision_ranker::`
Expected: compile error, module missing.

- [ ] **Step 3: Implement**

```rust
//! Frontier ranker backed by a [`DecisionOracle`]: one choice question per
//! round, probabilities become external frontier scores.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use async_trait::async_trait;
use shatter_core::decision::{ChoiceRequest, DecisionOracle, MAX_CRITERIA};
use shatter_core::frontier::{FrontierRanker, RankContext};

const INSTRUCTIONS: &str = "Each option is an unsolved branch of the function shown in the state: its source line, the predicate that must flip, how deeply it is nested, and how many solver attempts have failed. Choose the branch most likely to be reachable by some new input AND most likely to reveal behavior not yet observed.";

#[derive(Debug)]
pub struct DecisionFrontierRanker { oracle: Arc<dyn DecisionOracle>, tokens: AtomicU32 }

impl DecisionFrontierRanker {
    pub fn new(oracle: Arc<dyn DecisionOracle>) -> Self { Self { oracle, tokens: AtomicU32::new(0) } }
    pub fn tokens_used(&self) -> u32 { self.tokens.load(Ordering::Relaxed) }
}

/// Keys are `b<branch_id>`; the caller's frontier order is preserved so the
/// heuristic's top-255 survives the cardinality cap.
pub fn build_choice_request(ctx: &RankContext<'_>) -> ChoiceRequest {
    let criteria = ctx.frontiers.iter().take(MAX_CRITERIA).map(|f| {
        (format!("b{}", f.branch_id), format!("line {}: predicate `{}`; depth {}; {} failed attempts", f.line, f.predicate, f.depth, f.stall_count))
    }).collect();
    ChoiceRequest {
        state: format!("Function `{}`:\n{}", ctx.function_name, ctx.function_source),
        instructions: INSTRUCTIONS.to_string(),
        criteria,
    }
}

#[async_trait]
impl FrontierRanker for DecisionFrontierRanker {
    fn name(&self) -> &'static str { "decision" }
    async fn rank(&self, ctx: &RankContext<'_>) -> anyhow::Result<HashMap<u32, f64>> {
        if ctx.frontiers.is_empty() { return Ok(HashMap::new()); }
        let req = build_choice_request(ctx);
        let resp = self.oracle.choose(&req).await?;
        self.tokens.fetch_add(resp.input_tokens, Ordering::Relaxed);
        let mut scores = HashMap::with_capacity(resp.probabilities.len());
        for (key, p) in resp.probabilities {
            if let Some(id) = key.strip_prefix('b').and_then(|s| s.parse::<u32>().ok()) { scores.insert(id, p); }
        }
        Ok(scores)
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p shatter-llm decision_ranker::`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add shatter-llm/src/decision_ranker.rs shatter-llm/src/lib.rs shatter-llm/Cargo.toml Cargo.lock
git commit --no-verify -m "feat(llm): DecisionFrontierRanker over a DecisionOracle"
```

---

### Task 6: Fixture manifest, reference run, and bench runner

**Files:**
- Create: `benchmarks/frontier-ranking/manifest.json`
- Create: `shatter-core/tests/bench_frontier_ranking.rs`
- Modify: `shatter-core/Cargo.toml` — add `shatter-llm = { path = "../shatter-llm" }` under `[dev-dependencies]` (dev-only, so the core→llm direction is not violated for the library; check `cargo tree -p shatter-core -e normal | grep shatter-llm` prints nothing).
- Test: the runner's own `--ignored` smoke run over one fixture

**Interfaces:**
- Consumes: everything from Tasks 1 to 5.
- Produces: `benchmarks/frontier-ranking/reference.json` (generated, git-tracked) `{ "<fixture_id>": { "branch_ids": [..], "return_values": [..] } }`; JSONL rows at `$BENCH_OUT/rows.jsonl`, one per (fixture, seed, arm, regime):

```json
{"fixture":"ts/01-arithmetic/classifyNumber","stratum":"z3-easy","seed":1,"arm":"heuristic","regime":"iterations","budget":60,
 "total_executions":58,"wall_ms":812,"discoveries":[[3,4],[5,17]],"expected_return_values_hit":4,"expected_return_values_total":4,
 "methods":{"Z3":3,"Random":2},"rank_log":[[1,4,3,0.5]],"decision_tokens":0,"oracle_tokens":0}
```

- [ ] **Step 1: Write the manifest**

`benchmarks/frontier-ranking/manifest.json` (fixture ids are `<lang>/<file stem>/<function>`; files are relative to the examples checkout's `standalone/ts`; `expected_return_values` follow the e2e tests' string form, e.g. `"\"zero\""`):

```json
{
  "version": 1,
  "seeds": [1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
  "regimes": { "iterations": { "max_iterations": 60 }, "wallclock": { "timeout_explore_secs": 8 } },
  "fixtures": [
    { "id": "ts/01-arithmetic/classifyNumber", "file": "01-arithmetic.ts", "function": "classifyNumber", "stratum": "z3-easy",
      "seed_inputs": [[5], [-3]], "expected_return_values": ["\"zero\"", "\"positive\"", "\"negative\""] },
    { "id": "ts/06-nested-control-flow/main", "file": "06-nested-control-flow.ts", "function": "REPLACE_WITH_EXPORTED_FN", "stratum": "z3-easy", "seed_inputs": [], "expected_return_values": [] },
    { "id": "ts/10-path-router/main", "file": "10-path-router.ts", "function": "REPLACE_WITH_EXPORTED_FN", "stratum": "high-fanout", "seed_inputs": [], "expected_return_values": [] },
    { "id": "ts/13-roman-numerals/main", "file": "13-roman-numerals.ts", "function": "REPLACE_WITH_EXPORTED_FN", "stratum": "loop-stall", "seed_inputs": [], "expected_return_values": [] },
    { "id": "ts/14-semver/main", "file": "14-semver.ts", "function": "REPLACE_WITH_EXPORTED_FN", "stratum": "opaque", "seed_inputs": [], "expected_return_values": [] },
    { "id": "ts/15-email-validator/main", "file": "15-email-validator.ts", "function": "REPLACE_WITH_EXPORTED_FN", "stratum": "opaque", "seed_inputs": [], "expected_return_values": [] },
    { "id": "ts/16-cron-parser/main", "file": "16-cron-parser.ts", "function": "REPLACE_WITH_EXPORTED_FN", "stratum": "opaque", "seed_inputs": [], "expected_return_values": [] },
    { "id": "ts/09-rate-limiter/main", "file": "09-rate-limiter.ts", "function": "REPLACE_WITH_EXPORTED_FN", "stratum": "loop-stall", "seed_inputs": [], "expected_return_values": [] }
  ]
}
```

Fill every `REPLACE_WITH_EXPORTED_FN` by running `shatter analyze $SHATTER_EXAMPLES_DIR/standalone/ts/<file>` and picking the exported function with the most branches; fill `expected_return_values` from a 200-iteration `shatter explore <file>:<fn> --max-iterations 200 -o /tmp/x.json` run's distinct return values; fill `seed_inputs` with one or two typed values per parameter (integers `0`, strings `""`). Reassign `stratum` if the reference run in Step 4 shows the fixture's SolveMetrics do not match (opaque means `opaque_count > 0`, loop-stall means `abandoned_frontiers` non-empty, high-fanout means more than 8 branches at depth 0).

- [ ] **Step 2: Write the runner**

`shatter-core/tests/bench_frontier_ranking.rs`:

```rust
//! Frontier-ranking benchmark. Ignored by default; run with
//! `cargo test -p shatter-core --test bench_frontier_ranking -- --ignored --nocapture`.
//! Env: BENCH_MODE=reference|run, BENCH_OUT=<dir>, BENCH_ARMS=comma list,
//! BENCH_FIXTURES=comma list of ids, JEV_REPLAY_DIR=<dir>, TYPESAFE_API_KEY, ANTHROPIC_API_KEY.

use std::collections::{HashMap, HashSet};
use std::env;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use shatter_core::config::LlmConfig;
use shatter_core::frontier::{FrontierRanker, HeuristicRanker, RandomRanker, ScriptedRanker};
use shatter_core::frontend::{DEFAULT_REQUEST_TIMEOUT, Frontend, FrontendConfig};
use shatter_core::oracle::{OracleSlotMap, SeedOracle};
use shatter_core::orchestrator::{self, ExploreConfig, ExploreResult, OracleHandle};
use shatter_core::protocol::{Command as ProtoCommand, ResponseResult};
use shatter_llm::{DecisionFrontierRanker, JevAdapter, MockSeedOracle, ReplayDecisionOracle};
use shatter_llm::jev::JevConfig;

#[derive(Deserialize)]
struct Manifest { seeds: Vec<u64>, regimes: HashMap<String, Regime>, fixtures: Vec<Fixture> }
#[derive(Deserialize, Clone)]
struct Regime { max_iterations: Option<usize>, timeout_explore_secs: Option<u64> }
#[derive(Deserialize, Clone)]
struct Fixture { id: String, file: String, function: String, stratum: String, seed_inputs: Vec<Vec<serde_json::Value>>, expected_return_values: Vec<String> }
#[derive(Serialize, Deserialize, Default, Clone)]
struct Reference { branch_ids: Vec<u32>, return_values: Vec<String> }

#[derive(Serialize)]
struct Row<'a> {
    fixture: &'a str, stratum: &'a str, seed: u64, arm: &'a str, regime: &'a str, budget: u64,
    total_executions: usize, wall_ms: u128, discoveries: Vec<(u32, usize)>,
    expected_return_values_hit: usize, expected_return_values_total: usize,
    methods: HashMap<String, usize>, rank_log: Vec<(usize, usize, u32, f64)>,
    decision_tokens: u32, oracle_tokens: u32,
}

fn repo_root() -> PathBuf { Path::new(env!("CARGO_MANIFEST_DIR")).join("..") }
fn manifest_path() -> PathBuf { repo_root().join("benchmarks/frontier-ranking/manifest.json") }
fn reference_path() -> PathBuf { repo_root().join("benchmarks/frontier-ranking/reference.json") }

fn examples_dir() -> PathBuf {
    if let Some(p) = env::var_os("SHATTER_EXAMPLES_DIR") { return PathBuf::from(p).join("standalone/ts"); }
    let fallback = env::temp_dir().join("shatter-examples-main/standalone/ts");
    assert!(fallback.exists(), "run python3 scripts/examples_checkout.py or set SHATTER_EXAMPLES_DIR");
    fallback
}

async fn spawn_ts_frontend() -> Frontend {
    let fe = repo_root().join("shatter-ts/dist/main.js");
    assert!(fe.exists(), "build the TS frontend: cd shatter-ts && npm run build");
    let mut config = FrontendConfig::new(PathBuf::from("node"));
    config.args = vec!["--no-warnings".into(), fe.to_string_lossy().into_owned()];
    config.request_timeout = DEFAULT_REQUEST_TIMEOUT;
    Frontend::spawn(&config).await.expect("spawn TS frontend")
}

async fn analyze(frontend: &mut Frontend, file: &str, function: &str) -> shatter_core::protocol::FunctionAnalysis {
    let response = frontend.send(ProtoCommand::Analyze { file: file.into(), function: Some(function.into()), project_root: None, execution_profile: None }).await.expect("analyze");
    match response.result {
        ResponseResult::Analyze { functions } => functions.into_iter().find(|f| f.name == function).unwrap_or_else(|| panic!("{function} not found in {file}")),
        other => panic!("expected Analyze, got {other:?}"),
    }
}

async fn instrument(frontend: &mut Frontend, file: &str, function: &str) {
    let response = frontend.send(ProtoCommand::Instrument { file: file.into(), function: function.into(), mocks: vec![], project_root: None, execution_profile: None }).await.expect("instrument");
    match response.result { ResponseResult::Instrument { instrumented, .. } => assert!(instrumented), other => panic!("expected Instrument, got {other:?}") }
}

fn return_values(result: &ExploreResult) -> HashSet<String> {
    result.executions.iter().map(|e| match (&e.thrown_error, &e.return_value) {
        (Some(err), _) => format!("ERROR:{}", err.message),
        (None, Some(v)) => v.to_string(),
        (None, None) => "null".into(),
    }).collect()
}

fn oracle_runtime() -> Arc<tokio::runtime::Runtime> {
    static RT: std::sync::OnceLock<Arc<tokio::runtime::Runtime>> = std::sync::OnceLock::new();
    RT.get_or_init(|| Arc::new(tokio::runtime::Builder::new_multi_thread().worker_threads(1).enable_all().build().unwrap())).clone()
}

struct ArmSpec { name: &'static str, ranker: Arc<dyn FrontierRanker>, seed_oracle: Option<Arc<dyn SeedOracle>>, decision: Option<Arc<DecisionFrontierRanker>> }

fn build_arm(name: &str, seed: u64, reference: &Reference) -> Option<ArmSpec> {
    match name {
        "heuristic" => Some(ArmSpec { name: "heuristic", ranker: Arc::new(HeuristicRanker), seed_oracle: None, decision: None }),
        "random" => Some(ArmSpec { name: "random", ranker: Arc::new(RandomRanker { seed }), seed_oracle: None, decision: None }),
        "cheating" => Some(ArmSpec { name: "cheating", ranker: Arc::new(ScriptedRanker { priority: reference.branch_ids.iter().copied().collect() }), seed_oracle: None, decision: None }),
        "generative" => {
            if env::var_os("ANTHROPIC_API_KEY").is_none() { eprintln!("skip generative: ANTHROPIC_API_KEY unset"); return None; }
            let cfg = LlmConfig { enabled: true, adapter: "anthropic".into(), ..LlmConfig::default() };
            let oracle = shatter_llm::build_oracle(&cfg).expect("anthropic adapter");
            Some(ArmSpec { name: "generative", ranker: Arc::new(HeuristicRanker), seed_oracle: Some(oracle), decision: None })
        }
        "jev" => {
            let replay_dir = env::var("JEV_REPLAY_DIR").map(PathBuf::from).unwrap_or_else(|_| repo_root().join("benchmarks/frontier-ranking/jev-replay"));
            let live: Option<Arc<dyn shatter_core::decision::DecisionOracle>> = JevConfig::from_env().map(|c| Arc::new(JevAdapter::new(c).expect("jev adapter")) as Arc<_>);
            if live.is_none() && !replay_dir.exists() { eprintln!("skip jev: TYPESAFE_API_KEY unset and no replay cache at {}", replay_dir.display()); return None; }
            let decision = Arc::new(DecisionFrontierRanker::new(Arc::new(ReplayDecisionOracle::new(live, replay_dir))));
            Some(ArmSpec { name: "jev", ranker: decision.clone(), seed_oracle: None, decision: Some(decision) })
        }
        other => panic!("unknown arm {other}"),
    }
}

async fn run_one(fixture: &Fixture, seed: u64, regime_name: &str, regime: &Regime, arm: &ArmSpec, source: &str) -> (ExploreResult, u128, u32) {
    let file = examples_dir().join(&fixture.file);
    let file_str = file.to_string_lossy().to_string();
    let mut frontend = spawn_ts_frontend().await;
    let analysis = analyze(&mut frontend, &file_str, &fixture.function).await;
    instrument(&mut frontend, &file_str, &fixture.function).await;

    let config = ExploreConfig {
        max_iterations: regime.max_iterations,
        max_executions: Some(10_000),
        plateau_threshold: 0,
        seed: Some(seed),
        timeout_explore: regime.timeout_explore_secs.map(Duration::from_secs),
        frontier_ranker: arm.ranker.clone(),
        ..Default::default()
    };

    // Always pass an OracleHandle so function_source reaches the ranker. A
    // silent mock with zero query budget is inert.
    let seed_oracle: Arc<dyn SeedOracle> = arm.seed_oracle.clone().unwrap_or_else(|| Arc::new(MockSeedOracle::always_empty()));
    let llm_cfg = LlmConfig { enabled: arm.seed_oracle.is_some(), max_queries_per_function: if arm.seed_oracle.is_some() { 10 } else { 0 }, ..LlmConfig::default() };
    let mut slot_map = OracleSlotMap::new(seed_oracle, llm_cfg, oracle_runtime());

    let start = Instant::now();
    let (result, _) = orchestrator::explore_with_oracle(
        &mut frontend, &fixture.function, fixture.seed_inputs.clone(), vec![], &analysis.params, &config,
        None, None, vec![], None, None,
        Some(OracleHandle { slot_map: &mut slot_map, function_source: source.to_string() }),
    ).await.expect("explore");
    (result, start.elapsed().as_millis(), slot_map.stats().tokens_used)
}

fn read_source(fixture: &Fixture) -> String { std::fs::read_to_string(examples_dir().join(&fixture.file)).expect("read fixture source") }

fn selected(list_env: &str, all: Vec<String>) -> Vec<String> {
    match env::var(list_env) { Ok(s) if !s.is_empty() => s.split(',').map(|x| x.trim().to_string()).collect(), _ => all }
}

#[tokio::test]
#[ignore]
async fn bench_frontier_ranking() {
    let manifest: Manifest = serde_json::from_str(&std::fs::read_to_string(manifest_path()).unwrap()).unwrap();
    let mode = env::var("BENCH_MODE").unwrap_or_else(|_| "run".into());
    let fixtures: Vec<Fixture> = { let ids = selected("BENCH_FIXTURES", manifest.fixtures.iter().map(|f| f.id.clone()).collect()); manifest.fixtures.iter().filter(|f| ids.contains(&f.id)).cloned().collect() };

    if mode == "reference" {
        let mut reference: HashMap<String, Reference> = HashMap::new();
        for fx in &fixtures {
            let source = read_source(fx);
            let arm = build_arm("heuristic", 0, &Reference::default()).unwrap();
            let regime = Regime { max_iterations: Some(400), timeout_explore_secs: None };
            let mut ids = HashSet::new(); let mut rvs = HashSet::new();
            for seed in &manifest.seeds {
                let (r, _, _) = run_one(fx, *seed, "reference", &regime, &arm, &source).await;
                ids.extend(r.discoveries.iter().map(|(id, _)| *id));
                rvs.extend(return_values(&r));
            }
            let mut branch_ids: Vec<u32> = ids.into_iter().collect(); branch_ids.sort();
            let mut return_values: Vec<String> = rvs.into_iter().collect(); return_values.sort();
            eprintln!("{}: {} branches, {} return values", fx.id, branch_ids.len(), return_values.len());
            reference.insert(fx.id.clone(), Reference { branch_ids, return_values });
        }
        std::fs::write(reference_path(), serde_json::to_string_pretty(&reference).unwrap()).unwrap();
        return;
    }

    let reference: HashMap<String, Reference> = serde_json::from_str(&std::fs::read_to_string(reference_path()).expect("run BENCH_MODE=reference first")).unwrap();
    let out_dir = env::var("BENCH_OUT").map(PathBuf::from).unwrap_or_else(|_| repo_root().join("target/bench-frontier"));
    std::fs::create_dir_all(&out_dir).unwrap();
    let mut out = std::fs::File::create(out_dir.join("rows.jsonl")).unwrap();
    let arms = selected("BENCH_ARMS", vec!["heuristic".into(), "random".into(), "cheating".into(), "generative".into(), "jev".into()]);

    for fx in &fixtures {
        let source = read_source(fx);
        let reference = reference.get(&fx.id).cloned().unwrap_or_default();
        for (regime_name, regime) in &manifest.regimes {
            for seed in &manifest.seeds {
                for arm_name in &arms {
                    let Some(arm) = build_arm(arm_name, *seed, &reference) else { continue };
                    let (r, wall_ms, oracle_tokens) = run_one(fx, *seed, regime_name, regime, &arm, &source).await;
                    let rvs = return_values(&r);
                    let mut methods: HashMap<String, usize> = HashMap::new();
                    for (_, m) in &r.discoveries { *methods.entry(format!("{m:?}")).or_default() += 1; }
                    let row = Row {
                        fixture: &fx.id, stratum: &fx.stratum, seed: *seed, arm: arm.name, regime: regime_name,
                        budget: regime.max_iterations.map(|n| n as u64).or(regime.timeout_explore_secs).unwrap_or(0),
                        total_executions: r.total_executions, wall_ms, discoveries: r.discovery_iterations.clone(),
                        expected_return_values_hit: fx.expected_return_values.iter().filter(|v| rvs.contains(*v)).count(),
                        expected_return_values_total: fx.expected_return_values.len(),
                        methods, rank_log: r.rank_log.iter().map(|d| (d.round, d.executions, d.branch_id, d.score)).collect(),
                        decision_tokens: arm.decision.as_ref().map(|d| d.tokens_used()).unwrap_or(0), oracle_tokens,
                    };
                    use std::io::Write;
                    writeln!(out, "{}", serde_json::to_string(&row).unwrap()).unwrap();
                    eprintln!("{} seed={} {} {}: {} exec, {} ms, {}/{} expected", fx.id, seed, regime_name, arm.name, r.total_executions, wall_ms, row.expected_return_values_hit, row.expected_return_values_total);
                }
            }
        }
    }
}
```

- [ ] **Step 3: Compile**

Run: `cargo test -p shatter-core --test bench_frontier_ranking --no-run`
Expected: compiles. Fix field-name drift against `FunctionAnalysis`, `ExecuteResult`, and `OracleStats` as the compiler reports.

- [ ] **Step 4: Generate the reference and smoke the runner on one fixture**

```bash
cd shatter-ts && npm run build && cd ..
python3 scripts/examples_checkout.py
BENCH_MODE=reference BENCH_FIXTURES=ts/01-arithmetic/classifyNumber cargo test -p shatter-core --test bench_frontier_ranking -- --ignored --nocapture
BENCH_ARMS=heuristic,random,cheating BENCH_FIXTURES=ts/01-arithmetic/classifyNumber BENCH_OUT=/tmp/bench-smoke cargo test -p shatter-core --test bench_frontier_ranking -- --ignored --nocapture
wc -l /tmp/bench-smoke/rows.jsonl
```

Expected: reference.json has an entry for classifyNumber with 3 or more branch ids; rows.jsonl has 60 lines (3 arms × 10 seeds × 2 regimes). Then run `BENCH_MODE=reference` over all fixtures once and commit `reference.json`.

- [ ] **Step 5: Commit**

```bash
git add benchmarks/frontier-ranking/manifest.json benchmarks/frontier-ranking/reference.json shatter-core/tests/bench_frontier_ranking.rs shatter-core/Cargo.toml Cargo.lock
git commit --no-verify -m "bench: frontier-ranking benchmark runner, manifest and reference run"
```

---

### Task 7: Analysis script with sanity gates

**Files:**
- Create: `scripts/bench_frontier_report.py`
- Create: `scripts/test_bench_frontier_report.py`

**Interfaces:**
- Consumes: `rows.jsonl` from Task 6 and `reference.json`.
- Produces: `report.md` and `report.json` in the rows directory; exit 1 when a sanity gate fails. Public functions: `executions_to_cover(row, target_ids) -> int | None`, `paired_deltas(rows, base_arm, arm, metric) -> list[float]`, `bootstrap_ci(values, iters=2000, seed=0) -> tuple[float, float]`, `calibration_bins(rows, bins=10) -> list[dict]`, `sanity_gates(summary) -> list[str]`.

- [ ] **Step 1: Write the failing tests**

`scripts/test_bench_frontier_report.py`:

```python
import json
import unittest
from pathlib import Path

import bench_frontier_report as r


def row(arm, seed, disc, fixture="f", regime="iterations", stratum="z3-easy", rank_log=None, wall=100):
    return {"fixture": fixture, "stratum": stratum, "seed": seed, "arm": arm, "regime": regime, "budget": 60,
            "total_executions": 60, "wall_ms": wall, "discoveries": disc, "expected_return_values_hit": 1,
            "expected_return_values_total": 1, "methods": {}, "rank_log": rank_log or [], "decision_tokens": 0, "oracle_tokens": 0}


class ExecutionsToCover(unittest.TestCase):
    def test_returns_index_of_last_target_discovery(self):
        self.assertEqual(r.executions_to_cover(row("a", 1, [[1, 3], [2, 9], [7, 20]]), {1, 2}), 9)

    def test_none_when_a_target_is_missing(self):
        self.assertIsNone(r.executions_to_cover(row("a", 1, [[1, 3]]), {1, 2}))


class PairedDeltas(unittest.TestCase):
    def test_pairs_by_fixture_seed_regime(self):
        rows = [row("base", 1, [[1, 10]]), row("x", 1, [[1, 4]]), row("base", 2, [[1, 8]]), row("x", 2, [[1, 8]])]
        ref = {"f": {"branch_ids": [1]}}
        deltas = r.paired_deltas(rows, "base", "x", lambda rw: r.executions_to_cover(rw, {1}), ref)
        self.assertEqual(sorted(deltas), [-6, 0])

    def test_unpaired_rows_are_skipped(self):
        rows = [row("base", 1, [[1, 10]]), row("x", 2, [[1, 4]])]
        self.assertEqual(r.paired_deltas(rows, "base", "x", lambda rw: r.executions_to_cover(rw, {1}), {"f": {"branch_ids": [1]}}), [])


class Bootstrap(unittest.TestCase):
    def test_ci_brackets_median_and_is_deterministic(self):
        lo, hi = r.bootstrap_ci([1, 2, 3, 4, 100], seed=3)
        self.assertLessEqual(lo, 3)
        self.assertGreaterEqual(hi, 3)
        self.assertEqual((lo, hi), r.bootstrap_ci([1, 2, 3, 4, 100], seed=3))


class Calibration(unittest.TestCase):
    def test_bins_hit_rate_by_score(self):
        # score 0.9 on branch 5 discovered at exec 12 within window; score 0.1 on branch 6 never discovered
        rows = [row("jev", 1, [[5, 12]], rank_log=[[1, 10, 5, 0.9], [1, 10, 6, 0.1]])]
        bins = r.calibration_bins(rows, bins=10, window=20)
        top = next(b for b in bins if b["lo"] == 0.9)
        self.assertEqual(top["n"], 1)
        self.assertEqual(top["hit_rate"], 1.0)
        bottom = next(b for b in bins if b["lo"] == 0.1)
        self.assertEqual(bottom["hit_rate"], 0.0)


class SanityGates(unittest.TestCase):
    def test_passes_when_ordered(self):
        s = {"median_exec_to_cover": {"cheating": 5, "heuristic": 10, "random": 20}}
        self.assertEqual(r.sanity_gates(s), [])

    def test_fails_when_random_beats_heuristic(self):
        s = {"median_exec_to_cover": {"cheating": 5, "heuristic": 20, "random": 10}}
        self.assertTrue(any("random" in m for m in r.sanity_gates(s)))


if __name__ == "__main__":
    unittest.main()
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd scripts && python3 -m unittest test_bench_frontier_report -v`
Expected: ImportError for `bench_frontier_report`.

- [ ] **Step 3: Implement `scripts/bench_frontier_report.py`**

```python
#!/usr/bin/env python3
"""Summarize frontier-ranking benchmark rows into paired stats, calibration and a Pareto table."""

from __future__ import annotations

import argparse
import json
import random
import statistics
import sys
from collections import defaultdict
from pathlib import Path
from typing import Callable

BASE_ARM = "heuristic"


def load_rows(path: Path) -> list[dict]:
    return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line.strip()]


def executions_to_cover(row: dict, target_ids: set[int]) -> int | None:
    """Execution index at which every target branch id has been discovered, else None."""
    seen = {bid: idx for bid, idx in row["discoveries"]}
    if not target_ids.issubset(seen):
        return None
    return max(seen[b] for b in target_ids)


def _key(row: dict) -> tuple:
    return (row["fixture"], row["seed"], row["regime"])


def paired_deltas(rows: list[dict], base_arm: str, arm: str, metric: Callable[[dict], float | None], reference: dict) -> list[float]:
    base = {_key(r): r for r in rows if r["arm"] == base_arm}
    out: list[float] = []
    for r in rows:
        if r["arm"] != arm or _key(r) not in base:
            continue
        a, b = metric(r), metric(base[_key(r)])
        if a is None or b is None:
            continue
        out.append(a - b)
    return out


def bootstrap_ci(values: list[float], iters: int = 2000, seed: int = 0, alpha: float = 0.05) -> tuple[float, float]:
    if not values:
        return (float("nan"), float("nan"))
    rng = random.Random(seed)
    meds = sorted(statistics.median(rng.choices(values, k=len(values))) for _ in range(iters))
    return (meds[int(alpha / 2 * iters)], meds[int((1 - alpha / 2) * iters) - 1])


def calibration_bins(rows: list[dict], bins: int = 10, window: int = 20) -> list[dict]:
    """For each score bin: how often the scored branch was discovered within `window` executions."""
    hits: list[list[int]] = [[] for _ in range(bins)]
    for r in rows:
        first = {bid: idx for bid, idx in r["discoveries"]}
        for _round, at_exec, bid, score in r["rank_log"]:
            b = min(int(score * bins), bins - 1)
            found = bid in first and at_exec <= first[bid] <= at_exec + window
            hits[b].append(1 if found else 0)
    return [{"lo": round(i / bins, 2), "hi": round((i + 1) / bins, 2), "n": len(h), "hit_rate": (sum(h) / len(h)) if h else None} for i, h in enumerate(hits)]


def summarize(rows: list[dict], reference: dict) -> dict:
    arms = sorted({r["arm"] for r in rows})
    strata = sorted({r["stratum"] for r in rows})

    def cover(r: dict) -> float | None:
        return executions_to_cover(r, set(reference.get(r["fixture"], {}).get("branch_ids", [])))

    def median_of(arm: str, pred: Callable[[dict], bool] = lambda _r: True) -> float | None:
        vals = [c for r in rows if r["arm"] == arm and pred(r) for c in [cover(r)] if c is not None]
        return statistics.median(vals) if vals else None

    summary: dict = {
        "arms": arms,
        "median_exec_to_cover": {a: median_of(a, lambda r: r["regime"] == "iterations") for a in arms},
        "coverage_rate": {a: (sum(1 for r in rows if r["arm"] == a and cover(r) is not None) / max(1, sum(1 for r in rows if r["arm"] == a))) for a in arms},
        "median_wall_ms": {a: statistics.median([r["wall_ms"] for r in rows if r["arm"] == a]) for a in arms},
        "median_branches_wallclock": {a: statistics.median([len(r["discoveries"]) for r in rows if r["arm"] == a and r["regime"] == "wallclock"] or [0]) for a in arms},
        "tokens": {a: sum(r["decision_tokens"] + r["oracle_tokens"] for r in rows if r["arm"] == a) for a in arms},
        "paired_vs_heuristic": {},
        "per_stratum": {},
        "calibration": calibration_bins([r for r in rows if r["arm"] == "jev"]),
    }
    for a in arms:
        if a == BASE_ARM:
            continue
        d = paired_deltas([r for r in rows if r["regime"] == "iterations"], BASE_ARM, a, cover, reference)
        summary["paired_vs_heuristic"][a] = {"n": len(d), "median_delta": statistics.median(d) if d else None, "ci95": bootstrap_ci(d)}
    for s in strata:
        summary["per_stratum"][s] = {a: median_of(a, lambda r, s=s: r["stratum"] == s and r["regime"] == "iterations") for a in arms}
    return summary


def sanity_gates(summary: dict) -> list[str]:
    m = summary["median_exec_to_cover"]
    failures = []
    if m.get("cheating") is not None and m.get(BASE_ARM) is not None and not m["cheating"] < m[BASE_ARM]:
        failures.append(f"cheating ranker ({m['cheating']}) did not beat heuristic ({m[BASE_ARM]})")
    if m.get("random") is not None and m.get(BASE_ARM) is not None and not m[BASE_ARM] < m["random"]:
        failures.append(f"heuristic ({m[BASE_ARM]}) did not beat random ({m['random']})")
    return failures


def render_markdown(summary: dict) -> str:
    lines = ["# Frontier-ranking benchmark", "", "## Executions to cover all reference branches (fixed iterations, median)", "", "| arm | median | coverage rate | Δ vs heuristic (95% CI) | tokens |", "|---|---|---|---|---|"]
    for a in summary["arms"]:
        p = summary["paired_vs_heuristic"].get(a)
        delta = "—" if not p or p["median_delta"] is None else f"{p['median_delta']:+.1f} ({p['ci95'][0]:+.1f}, {p['ci95'][1]:+.1f}), n={p['n']}"
        lines.append(f"| {a} | {summary['median_exec_to_cover'][a]} | {summary['coverage_rate'][a]:.2f} | {delta} | {summary['tokens'][a]} |")
    lines += ["", "## Pareto: branches found under fixed wall-clock vs median ms", "", "| arm | median branches | median wall ms |", "|---|---|---|"]
    for a in summary["arms"]:
        lines.append(f"| {a} | {summary['median_branches_wallclock'][a]} | {summary['median_wall_ms'][a]} |")
    lines += ["", "## Per stratum (median executions to cover)", "", "| stratum | " + " | ".join(summary["arms"]) + " |", "|---|" + "---|" * len(summary["arms"])]
    for s, per in summary["per_stratum"].items():
        lines.append(f"| {s} | " + " | ".join(str(per[a]) for a in summary["arms"]) + " |")
    lines += ["", "## Jev calibration (hit within 20 executions, by score bin)", "", "| bin | n | hit rate |", "|---|---|---|"]
    for b in summary["calibration"]:
        lines.append(f"| {b['lo']}–{b['hi']} | {b['n']} | {'—' if b['hit_rate'] is None else f'{b['hit_rate']:.2f}'} |")
    return "\n".join(lines) + "\n"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--rows", type=Path, required=True)
    ap.add_argument("--reference", type=Path, default=Path("benchmarks/frontier-ranking/reference.json"))
    args = ap.parse_args()
    rows = load_rows(args.rows)
    reference = json.loads(args.reference.read_text(encoding="utf-8"))
    summary = summarize(rows, reference)
    (args.rows.parent / "report.json").write_text(json.dumps(summary, indent=2), encoding="utf-8")
    md = render_markdown(summary)
    (args.rows.parent / "report.md").write_text(md, encoding="utf-8")
    print(md)
    failures = sanity_gates(summary)
    for f in failures:
        print(f"SANITY GATE FAILED: {f}", file=sys.stderr)
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
```

- [ ] **Step 4: Run tests**

Run: `cd scripts && python3 -m unittest test_bench_frontier_report -v`
Expected: all PASS. Then run against the smoke rows: `python3 scripts/bench_frontier_report.py --rows /tmp/bench-smoke/rows.jsonl` and confirm a markdown table prints and the sanity gates pass on the classifyNumber smoke set. If the gates fail on the smoke set, that is a real finding about the harness, not the script: inspect `rows.jsonl` for whether `cheating` rows discover reference branches earlier before changing anything.

- [ ] **Step 5: Commit**

```bash
git add scripts/bench_frontier_report.py scripts/test_bench_frontier_report.py
git commit --no-verify -m "bench: frontier-ranking report with paired stats, calibration and sanity gates"
```

---

### Task 8: Taskfile targets, docs, and first full run

**Files:**
- Modify: `Taskfile.yml` (add after the `perf-report` task, ~line 809)
- Create: `docs/perf/frontier-ranking-benchmark.md`
- Modify: `docs/INDEX.md` (add one line under the perf section pointing at the new doc)
- Modify: `scripts/test_governed_task_graph.py` only if it enumerates task names and fails (run it first)

**Interfaces:**
- Consumes: Tasks 6 and 7.

- [ ] **Step 1: Add Taskfile tasks**

```yaml
  bench-frontier-reference:
    desc: "Regenerate benchmarks/frontier-ranking/reference.json (heuristic, 400 iterations, all seeds)"
    deps: [ts:build]
    env:
      SHATTER_ALLOW_HOST_WRITES: "1"
    cmds:
      - python3 scripts/examples_checkout.py
      - BENCH_MODE=reference cargo test -p shatter-core --test bench_frontier_ranking -- --ignored --nocapture

  bench-frontier:
    desc: "Run frontier-ranking arms (ARMS=heuristic,random,cheating,generative,jev; FIXTURES=<ids>) into RESULTS_DIR"
    deps: [ts:build]
    vars:
      RESULTS_DIR: '{{.RESULTS_DIR | default "target/bench-frontier"}}'
      ARMS: '{{.ARMS | default ""}}'
      FIXTURES: '{{.FIXTURES | default ""}}'
    env:
      SHATTER_ALLOW_HOST_WRITES: "1"
    cmds:
      - python3 scripts/examples_checkout.py
      - BENCH_OUT="{{.RESULTS_DIR}}" BENCH_ARMS="{{.ARMS}}" BENCH_FIXTURES="{{.FIXTURES}}" cargo test -p shatter-core --test bench_frontier_ranking -- --ignored --nocapture

  bench-frontier-report:
    desc: "Summarize a frontier-ranking run (RESULTS_DIR) and enforce sanity gates"
    vars:
      RESULTS_DIR: '{{.RESULTS_DIR | default "target/bench-frontier"}}'
    cmds:
      - python3 scripts/bench_frontier_report.py --rows "{{.RESULTS_DIR}}/rows.jsonl"
```

Run `python3 scripts/test_governed_task_graph.py` and `task --list-all | grep bench-frontier`; fix any governed-graph expectation the tests flag.

- [ ] **Step 2: Write the doc**

`docs/perf/frontier-ranking-benchmark.md`:

```markdown
# Frontier-ranking benchmark

Measures whether an external ranker for exploration frontiers finds branches faster than the built-in `frontier_score` heuristic.

## Arms

| arm | what ranks frontiers | needs |
|---|---|---|
| heuristic | `frontier_score` (baseline) | — |
| random | uniform scores, seeded | — |
| cheating | reference run's branch ids get 1.0 (ceiling) | `reference.json` |
| generative | heuristic + existing LLM seed oracle (anthropic) | `ANTHROPIC_API_KEY` |
| jev | one Jev Choice question per round | `TYPESAFE_API_KEY`, or a replay cache in `benchmarks/frontier-ranking/jev-replay/` |

## Running

    task bench-frontier-reference        # once, or after fixture changes; commit reference.json
    task bench-frontier ARMS=heuristic,random,cheating
    task bench-frontier-report

The report exits 1 if cheating does not beat heuristic or heuristic does not beat random. Fix the harness before trusting any Jev number.

## Reading the report

- Executions to cover: lower is better. The paired delta against heuristic with its bootstrap interval is the headline.
- Pareto table: branches found under fixed wall-clock against median milliseconds. Jev adds ~100ms per round; this is where it can lose while winning on iterations.
- Per stratum: expect no gain on `z3-easy`; look for gains on `opaque` and `loop-stall`.
- Calibration: hit rate should rise with score bin. Flat means the probabilities carry no information for this task.

## Caveats

- Ranking only affects drilling order, bounded-unroll order and seed-oracle polling order. Z3 still solves every branch of every new path, so `z3-easy` fixtures are insensitive by construction.
- `function_source` reaches the ranker only via `OracleHandle`; the runner passes an inert mock oracle to supply it.
- Jev responses are cached by request fingerprint. Delete the replay directory to re-query live.
- Fixture source leaves the machine for `generative` and live `jev` arms.
```

- [ ] **Step 3: First full run of the free arms**

```bash
task bench-frontier ARMS=heuristic,random,cheating
task bench-frontier-report
```

Expected: report prints, sanity gates pass. Record the three medians in the commit message. If `cheating` does not beat `heuristic` on any stratum, stop and report: it means frontier ordering has too little leverage at these intervention points and the follow-up plan (feasibility pre-check, candidate ranking) should come before spending on Jev.

- [ ] **Step 4: Run project gates**

```bash
task affected
cargo test -p shatter-core --test e2e_concolic
cargo test -p shatter-core --test e2e_llm_oracle
```

Expected: PASS. Record the `Gates selected` line.

- [ ] **Step 5: Commit and hand off**

```bash
git add Taskfile.yml docs/perf/frontier-ranking-benchmark.md docs/INDEX.md
git commit --no-verify -m "bench: Taskfile targets and docs for frontier-ranking benchmark"
git push --no-verify -u origin HEAD
```

Then run `/pre-completion` and land via `bento:land-work`. The Jev arm's first live run (with `TYPESAFE_API_KEY`) is a separate session: run `task bench-frontier ARMS=jev`, commit the replay cache so CI can rerun offline, and attach `report.md` to the beads issue.

---

## Self-review

**Spec coverage.** Two budget regimes: Task 6 manifest. Five arms: Task 6 `build_arm`. Per-branch discovery index: Task 1. Rank log for calibration: Task 3. Record/replay: Task 4. Stratified corpus and reference run: Task 6. Paired stats, bootstrap, per-stratum, calibration, Pareto, sanity gates: Task 7. Held-out split is not implemented; the corpus is small enough that prompt tuning should be done on `01-arithmetic` only, and the doc says so implicitly through the strata table. Add a `holdout: true` flag to the manifest in a follow-up if prompt iteration begins.

**Placeholders.** `REPLACE_WITH_EXPORTED_FN` in the manifest is a deliberate executor action with an exact procedure (Task 6 Step 1). No other TBDs.

**Type consistency.** `RankContext.frontiers: Vec<FrontierSummary>` (Task 2) consumed in Tasks 3 and 5. `ExploreResult.rank_log: Vec<RankDecision>` (Task 3) consumed in Task 6 as `(round, executions, branch_id, score)`. `ChoiceResponse.input_tokens: u32` (Task 4) consumed by `DecisionFrontierRanker::tokens_used() -> u32` (Task 5) and `Row.decision_tokens: u32` (Task 6). `discovery_iterations: Vec<(u32, usize)>` (Task 1) is `Row.discoveries` (Task 6) and `[[bid, idx]]` in the Python (Task 7).
