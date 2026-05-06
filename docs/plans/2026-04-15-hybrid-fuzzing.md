# Hybrid Concolic + Coverage-Guided Fuzzing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a phase-separated hybrid fuzzer that activates automatically during concolic exploration when Z3 stalls on opaque constraints.

**Architecture:** New `fuzzer.rs` module in `shatter-core` containing `Corpus`, `Mutator`, and `FuzzSession`. The orchestrator gains fuzz-phase trigger logic, attempt tracking, and `FuzzConfig`. Mutation reuses existing `input_gen::havoc_mutate_inputs`. `DiscoveryMethod::Fuzzed` provides attribution.

**Tech Stack:** Rust (shatter-core), serde_json, rand, proptest

**Spec:** `docs/specs/2026-04-15-hybrid-fuzzing-design.md`

---

### Task 1: Add `DiscoveryMethod::Fuzzed` and metrics plumbing

Smallest self-contained change — just the new enum variant and metrics bucket. Everything else builds on this.

**Files:**
- Modify: `shatter-core/src/coverage_metrics.rs:20-33` (enum), `:37-46` (MethodPercentages), `:69-87` (CoverageMetrics), `:138-145` (match arm)
- Modify: `shatter-core/src/orchestrator.rs:1747-1754` (attribution match)
- Modify: `shatter-core/src/test_arbitraries.rs:409` (arbitrary impl — already lists Fuzzed for InputSource, no change needed there)

- [ ] **Step 1: Write the failing test**

Add a test in `coverage_metrics.rs` that uses the new variant:

```rust
#[test]
fn fuzzed_discovery_counts_toward_fuzz_found() {
    let discoveries = vec![
        (0, DiscoveryMethod::Z3),
        (1, DiscoveryMethod::Fuzzed),
        (2, DiscoveryMethod::Fuzzed),
        (3, DiscoveryMethod::Random),
    ];
    let metrics = CoverageMetrics::from_exploration(5, &discoveries, &[]);
    assert_eq!(metrics.z3_solved, 1);
    assert_eq!(metrics.fuzz_found, 2);
    assert_eq!(metrics.random_found, 1);
    let pct = metrics.percentages();
    assert!((pct.fuzz_pct - 40.0).abs() < f64::EPSILON);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p shatter-core fuzzed_discovery_counts_toward_fuzz_found`
Expected: FAIL — `DiscoveryMethod::Fuzzed` does not exist, `metrics.fuzz_found` does not exist

- [ ] **Step 3: Add `DiscoveryMethod::Fuzzed` variant**

In `shatter-core/src/coverage_metrics.rs`, add after `McdcTarget` (line 32):

```rust
    /// Found by coverage-guided fuzzing on an opaque-constraint frontier.
    Fuzzed,
```

- [ ] **Step 4: Add `fuzz_found` to `CoverageMetrics` and `fuzz_pct` to `MethodPercentages`**

In `CoverageMetrics` struct (line 69), add field:

```rust
    /// Branches first discovered by coverage-guided fuzzing.
    pub fuzz_found: usize,
```

In `MethodPercentages` struct (line 37), add field:

```rust
    /// Percentage of branches found by coverage-guided fuzzing.
    pub fuzz_pct: f64,
```

- [ ] **Step 5: Update the `from_exploration` match arm**

In `from_exploration` (line 138), add a `fuzz_found` counter and update the match:

```rust
let mut fuzz_found = 0usize;

for (_, method) in discoveries {
    match method {
        DiscoveryMethod::Z3 | DiscoveryMethod::McdcTarget => z3_solved += 1,
        DiscoveryMethod::Fuzzed => fuzz_found += 1,
        DiscoveryMethod::Random
        | DiscoveryMethod::Drilled
        | DiscoveryMethod::BoundarySearch => random_found += 1,
        DiscoveryMethod::UserProvided => user_provided += 1,
    }
}
```

Update `covered` calculation to include `fuzz_found`:

```rust
let covered = z3_solved + random_found + user_provided + fuzz_found;
```

Store `fuzz_found` in the struct. Update `percentages()` to compute `fuzz_pct` the same way as other percentages.

- [ ] **Step 6: Update orchestrator attribution match**

In `orchestrator.rs` line 1753, change:

```rust
InputSource::Seed | InputSource::Fuzzed => DiscoveryMethod::Random,
```

to:

```rust
InputSource::Seed => DiscoveryMethod::Random,
InputSource::Fuzzed => DiscoveryMethod::Fuzzed,
```

- [ ] **Step 7: Fix any remaining compile errors**

Grep for exhaustive match arms on `DiscoveryMethod` and add `Fuzzed` where needed. Check serialization tests — the serde roundtrip test at line ~700 lists all variants.

- [ ] **Step 8: Run test to verify it passes**

Run: `cargo test -p shatter-core fuzzed_discovery_counts_toward_fuzz_found`
Expected: PASS

Run: `cargo test -p shatter-core coverage_metrics`
Expected: All coverage_metrics tests PASS

- [ ] **Step 9: Commit**

```bash
git add shatter-core/src/coverage_metrics.rs shatter-core/src/orchestrator.rs
git commit -m "str-e0uy: add DiscoveryMethod::Fuzzed and fuzz_found metrics"
```

---

### Task 2: Add `FuzzConfig` to config pipeline

Wire fuzz budget parameters through the config system so the orchestrator can read them.

**Files:**
- Modify: `shatter-core/src/config.rs` (add `FuzzConfig` struct, add `fuzz` field to `DefaultsConfig`, `FunctionConfig`, `ResolvedFunctionConfig`)
- Modify: `shatter-core/src/orchestrator.rs:99-141` (add `fuzz: FuzzConfig` to `ExploreConfig`)

- [ ] **Step 1: Write the failing test**

In `config.rs` tests, add a YAML roundtrip test:

```rust
#[test]
fn fuzz_config_yaml_roundtrip() {
    let yaml = "\
defaults:
  fuzz:
    plateau_threshold: 100
    max_executions: 2000
    timeout_seconds: 60
    max_attempts: 5
";
    let config: ProjectConfig = serde_yaml::from_str(yaml).unwrap();
    let fuzz = config.defaults.fuzz.unwrap();
    assert_eq!(fuzz.plateau_threshold, Some(100));
    assert_eq!(fuzz.max_executions, Some(2000));
    assert_eq!(fuzz.timeout_seconds, Some(60));
    assert_eq!(fuzz.max_attempts, Some(5));
}

#[test]
fn fuzz_config_defaults_when_absent() {
    let yaml = "defaults: {}\n";
    let config: ProjectConfig = serde_yaml::from_str(yaml).unwrap();
    assert!(config.defaults.fuzz.is_none());
    let resolved_fuzz = FuzzConfig::default();
    assert_eq!(resolved_fuzz.plateau_threshold, Some(50));
    assert_eq!(resolved_fuzz.max_executions, Some(1000));
    assert_eq!(resolved_fuzz.timeout_seconds, Some(30));
    assert_eq!(resolved_fuzz.max_attempts, Some(3));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p shatter-core fuzz_config_yaml`
Expected: FAIL — `FuzzConfig` does not exist

- [ ] **Step 3: Define `FuzzConfig` struct in `config.rs`**

```rust
/// Configuration for the hybrid coverage-guided fuzzing phase.
///
/// The fuzzer activates automatically during concolic exploration when Z3
/// stalls on opaque constraints. These settings control the budget per
/// fuzz phase.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct FuzzConfig {
    /// Consecutive no-new-path executions before ending a fuzz phase.
    #[serde(default = "FuzzConfig::default_plateau_threshold")]
    pub plateau_threshold: Option<u32>,

    /// Maximum total executions per fuzz phase.
    #[serde(default = "FuzzConfig::default_max_executions")]
    pub max_executions: Option<u32>,

    /// Wall-clock timeout in seconds per fuzz phase.
    #[serde(default = "FuzzConfig::default_timeout_seconds")]
    pub timeout_seconds: Option<u32>,

    /// Maximum fuzz attempts per branch before giving up (bounded mode).
    /// `None` means unlimited (indefinite mode).
    #[serde(default = "FuzzConfig::default_max_attempts")]
    pub max_attempts: Option<u32>,
}

/// Default fuzz plateau threshold.
pub const DEFAULT_FUZZ_PLATEAU_THRESHOLD: u32 = 50;
/// Default fuzz max executions per phase.
pub const DEFAULT_FUZZ_MAX_EXECUTIONS: u32 = 1000;
/// Default fuzz timeout in seconds per phase.
pub const DEFAULT_FUZZ_TIMEOUT_SECS: u32 = 30;
/// Default max fuzz attempts per branch.
pub const DEFAULT_FUZZ_MAX_ATTEMPTS: u32 = 3;

impl FuzzConfig {
    fn default_plateau_threshold() -> Option<u32> {
        Some(DEFAULT_FUZZ_PLATEAU_THRESHOLD)
    }
    fn default_max_executions() -> Option<u32> {
        Some(DEFAULT_FUZZ_MAX_EXECUTIONS)
    }
    fn default_timeout_seconds() -> Option<u32> {
        Some(DEFAULT_FUZZ_TIMEOUT_SECS)
    }
    fn default_max_attempts() -> Option<u32> {
        Some(DEFAULT_FUZZ_MAX_ATTEMPTS)
    }
}

impl Default for FuzzConfig {
    fn default() -> Self {
        Self {
            plateau_threshold: Self::default_plateau_threshold(),
            max_executions: Self::default_max_executions(),
            timeout_seconds: Self::default_timeout_seconds(),
            max_attempts: Self::default_max_attempts(),
        }
    }
}
```

- [ ] **Step 4: Add `fuzz` field to `DefaultsConfig`, `FunctionConfig`, and `ResolvedFunctionConfig`**

In `DefaultsConfig` and `FunctionConfig`:

```rust
    #[serde(default)]
    pub fuzz: Option<FuzzConfig>,
```

In `ResolvedFunctionConfig`:

```rust
    pub fuzz: FuzzConfig,
```

Update the resolution logic (near line 1077) to resolve `fuzz` the same way `exploration` is resolved: function > defaults > built-in default.

- [ ] **Step 5: Add `fuzz: FuzzConfig` to `ExploreConfig` in `orchestrator.rs`**

In the `ExploreConfig` struct (line 99):

```rust
    /// Configuration for the hybrid fuzzing phase.
    pub fuzz: crate::config::FuzzConfig,
```

Update all `ExploreConfig { ... }` construction sites in shatter-cli (explore.rs line 1981, bench.rs line 246, and any others found by `cargo build`) to include `fuzz: resolved.fuzz.clone()` or `fuzz: FuzzConfig::default()`.

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p shatter-core fuzz_config`
Expected: PASS

Run: `cargo build -p shatter-cli`
Expected: Compiles without errors

- [ ] **Step 7: Commit**

```bash
git add shatter-core/src/config.rs shatter-core/src/orchestrator.rs shatter-cli/src/commands/
git commit -m "str-e0uy: add FuzzConfig to config pipeline and ExploreConfig"
```

---

### Task 3: Implement `Corpus` and `Mutator` in `fuzzer.rs`

The core fuzzing components. `Corpus` stores interesting inputs; `Mutator` wraps existing `input_gen` mutation functions with corpus-backed parent selection.

**Files:**
- Create: `shatter-core/src/fuzzer.rs`
- Modify: `shatter-core/src/lib.rs` (add `pub mod fuzzer;`)

- [ ] **Step 1: Write failing tests for `Corpus`**

Create `shatter-core/src/fuzzer.rs` with tests at the bottom:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn corpus_add_deduplicates_by_coverage_hash() {
        let mut corpus = Corpus::new();
        let entry1 = CorpusEntry {
            inputs: vec![json!(1)],
            coverage_hash: 42,
            branch_ids: vec![1, 2],
        };
        let entry2 = CorpusEntry {
            inputs: vec![json!(2)],
            coverage_hash: 42, // same hash
            branch_ids: vec![1, 2],
        };
        assert!(corpus.add(entry1));
        assert!(!corpus.add(entry2)); // duplicate
        assert_eq!(corpus.len(), 1);
    }

    #[test]
    fn corpus_seed_from_history_filters_by_parent_branch() {
        let mut corpus = Corpus::new();
        let target_parent_id = 5;
        let history = vec![
            (vec![json!("a")], vec![3, 5, 7], 100), // has parent branch 5
            (vec![json!("b")], vec![1, 2, 3], 200), // does not
            (vec![json!("c")], vec![5, 8], 300),     // has parent branch 5
        ];
        corpus.seed_from_history(&history, target_parent_id);
        assert_eq!(corpus.len(), 2);
    }

    #[test]
    fn corpus_pick_returns_none_when_empty() {
        let corpus = Corpus::new();
        let mut rng = rand::rng();
        assert!(corpus.pick(&mut rng).is_none());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p shatter-core fuzzer::tests`
Expected: FAIL — module does not exist

- [ ] **Step 3: Add `pub mod fuzzer;` to `lib.rs`**

Insert alphabetically (between `frontier` and `genetic`):

```rust
pub mod fuzzer;
```

- [ ] **Step 4: Implement `Corpus`**

```rust
//! Hybrid coverage-guided fuzzer for opaque-constraint frontiers.
//!
//! Activated by the orchestrator when Z3 stalls on `Unknown` constraints.
//! Runs a bounded mutation-execution loop using the existing `input_gen`
//! mutation functions and the standard `execute` protocol message.

use std::collections::HashSet;

use rand::Rng;
use serde_json::Value;

/// A single entry in the fuzzing corpus.
#[derive(Debug, Clone)]
pub struct CorpusEntry {
    /// Concrete parameter values.
    pub inputs: Vec<Value>,
    /// Path hash from execution (for dedup).
    pub coverage_hash: u64,
    /// Branch IDs this input exercised.
    pub branch_ids: Vec<u32>,
}

/// Per-function corpus of inputs that discovered unique paths.
#[derive(Debug, Clone, Default)]
pub struct Corpus {
    entries: Vec<CorpusEntry>,
    seen_hashes: HashSet<u64>,
}

impl Corpus {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an entry. Returns `true` if it was new (not a duplicate hash).
    pub fn add(&mut self, entry: CorpusEntry) -> bool {
        if self.seen_hashes.insert(entry.coverage_hash) {
            self.entries.push(entry);
            true
        } else {
            false
        }
    }

    /// Seed from orchestrator execution history. Includes inputs whose
    /// `branch_ids` contain `target_parent_id` (the last known branch
    /// before the opaque decision point).
    pub fn seed_from_history(
        &mut self,
        history: &[(Vec<Value>, Vec<u32>, u64)],
        target_parent_id: u32,
    ) {
        for (inputs, branch_ids, hash) in history {
            if branch_ids.contains(&target_parent_id) {
                self.add(CorpusEntry {
                    inputs: inputs.clone(),
                    coverage_hash: *hash,
                    branch_ids: branch_ids.clone(),
                });
            }
        }
    }

    /// Pick a random corpus entry as mutation parent. Returns `None` if empty.
    pub fn pick(&self, rng: &mut impl Rng) -> Option<&CorpusEntry> {
        if self.entries.is_empty() {
            None
        } else {
            let idx = rng.random_range(0..self.entries.len());
            Some(&self.entries[idx])
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
```

- [ ] **Step 5: Run Corpus tests to verify they pass**

Run: `cargo test -p shatter-core fuzzer::tests`
Expected: PASS

- [ ] **Step 6: Write a proptest for mutation integration**

```rust
#[cfg(test)]
mod tests {
    // ... existing tests ...

    use proptest::prelude::*;

    proptest! {
        #[test]
        fn mutated_inputs_preserve_vector_length(
            seed_len in 1usize..5,
        ) {
            use crate::types::{ParamInfo, TypeInfo};
            use crate::input_gen;

            let params: Vec<ParamInfo> = (0..seed_len)
                .map(|i| ParamInfo {
                    name: format!("p{i}"),
                    typ: TypeInfo::Int,
                    optional: false,
                })
                .collect();
            let inputs: Vec<Value> = (0..seed_len)
                .map(|i| serde_json::json!(i))
                .collect();
            let mut rng = rand::rng();
            let mutated = input_gen::mutate_inputs(
                &inputs, &params, 1.0, &[], &mut rng,
            );
            prop_assert_eq!(mutated.len(), inputs.len());
        }
    }
}
```

- [ ] **Step 7: Run proptest to verify it passes**

Run: `cargo test -p shatter-core fuzzer::tests::mutated_inputs`
Expected: PASS

- [ ] **Step 8: Commit**

```bash
git add shatter-core/src/fuzzer.rs shatter-core/src/lib.rs
git commit -m "str-e0uy: add Corpus and mutation integration in fuzzer module"
```

---

### Task 4: Implement `FuzzSession` — the bounded mutation-execution loop

This is the core of the fuzzer: takes a corpus, runs mutations through the frontend, tracks coverage, and returns results.

**Files:**
- Modify: `shatter-core/src/fuzzer.rs`

- [ ] **Step 1: Define the FuzzSession types**

Add to `fuzzer.rs`:

```rust
use std::time::{Duration, Instant};

use crate::config::FuzzConfig;
use crate::execution_record::BranchDecision;
use crate::protocol::SymConstraint;
use crate::types::ParamInfo;

/// Why a fuzz phase ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FuzzTermination {
    /// Consecutive no-new-path executions reached `fuzz_plateau_threshold`.
    Plateau,
    /// Total executions reached `fuzz_max_executions`.
    ExecutionCap,
    /// Wall-clock timeout reached.
    Timeout,
}

/// Statistics for a completed fuzz phase.
#[derive(Debug, Clone)]
pub struct FuzzPhaseStats {
    pub executions: u32,
    pub new_paths_found: u32,
    pub corpus_size: usize,
    pub termination_reason: FuzzTermination,
    pub target_branch_ids: Vec<u32>,
}

/// A new path discovered during fuzzing.
#[derive(Debug, Clone)]
pub struct FuzzDiscovery {
    pub path_hash: u64,
    pub branch_path: Vec<BranchDecision>,
    pub constraints: Vec<SymConstraint>,
    pub inputs: Vec<Value>,
}

/// Result of a completed fuzz phase.
#[derive(Debug)]
pub struct FuzzPhaseResult {
    pub new_paths: Vec<FuzzDiscovery>,
    pub corpus: Corpus,
    pub stats: FuzzPhaseStats,
}

/// A fuzz session: bounded mutation-execution loop.
///
/// The session does not own the frontend communication — the caller
/// (orchestrator) provides an `execute` callback that sends the inputs
/// to the frontend and returns the execution result.
pub struct FuzzSession {
    corpus: Corpus,
    params: Vec<ParamInfo>,
    target_branch_ids: Vec<u32>,
    config: FuzzConfig,
    covered_paths: HashSet<u64>,
}
```

- [ ] **Step 2: Implement `FuzzSession::new` and `FuzzSession::run`**

```rust
impl FuzzSession {
    pub fn new(
        corpus: Corpus,
        params: Vec<ParamInfo>,
        target_branch_ids: Vec<u32>,
        config: FuzzConfig,
        covered_paths: HashSet<u64>,
    ) -> Self {
        Self {
            corpus,
            params,
            target_branch_ids,
            config,
            covered_paths,
        }
    }

    /// Run the fuzz phase. Calls `execute_fn` for each mutated input.
    ///
    /// `execute_fn` takes input values and returns:
    /// - `Ok(Some((path_hash, branch_path, constraints)))` on successful execution
    /// - `Ok(None)` if execution failed (error, timeout, etc.)
    /// - `Err(e)` on fatal communication failure (stop fuzzing)
    pub async fn run<F, E>(
        mut self,
        mut execute_fn: F,
    ) -> Result<FuzzPhaseResult, E>
    where
        F: FnMut(Vec<Value>) -> std::pin::Pin<
            Box<dyn std::future::Future<
                Output = Result<
                    Option<(u64, Vec<BranchDecision>, Vec<SymConstraint>, Vec<u32>)>,
                    E,
                >,
            > + '_>,
        >,
    {
        let plateau_threshold = self.config.plateau_threshold
            .unwrap_or(crate::config::DEFAULT_FUZZ_PLATEAU_THRESHOLD);
        let max_executions = self.config.max_executions
            .unwrap_or(crate::config::DEFAULT_FUZZ_MAX_EXECUTIONS);
        let timeout = Duration::from_secs(
            self.config.timeout_seconds
                .unwrap_or(crate::config::DEFAULT_FUZZ_TIMEOUT_SECS) as u64,
        );

        let start = Instant::now();
        let mut executions: u32 = 0;
        let mut plateau_counter: u32 = 0;
        let mut new_paths = Vec::new();
        let mut rng = rand::rng();
        let dictionary: Vec<&str> = Vec::new();

        let termination_reason = loop {
            // Check termination bounds.
            if plateau_counter >= plateau_threshold {
                break FuzzTermination::Plateau;
            }
            if executions >= max_executions {
                break FuzzTermination::ExecutionCap;
            }
            if start.elapsed() >= timeout {
                break FuzzTermination::Timeout;
            }

            // Pick a parent and mutate.
            let parent = match self.corpus.pick(&mut rng) {
                Some(entry) => entry.inputs.clone(),
                None => break FuzzTermination::Plateau, // empty corpus, nothing to do
            };
            let mutated = crate::input_gen::havoc_mutate_inputs(
                &parent,
                &self.params,
                1.0, // mutate all params
                &dictionary,
                &mut rng,
            );

            executions += 1;

            // Execute via callback.
            let result = execute_fn(mutated.clone()).await?;

            match result {
                Some((path_hash, branch_path, constraints, branch_ids)) => {
                    if self.covered_paths.insert(path_hash) {
                        // New path discovered.
                        plateau_counter = 0;
                        self.corpus.add(CorpusEntry {
                            inputs: mutated.clone(),
                            coverage_hash: path_hash,
                            branch_ids: branch_ids.clone(),
                        });
                        new_paths.push(FuzzDiscovery {
                            path_hash,
                            branch_path,
                            constraints,
                            inputs: mutated,
                        });
                    } else {
                        plateau_counter += 1;
                    }
                }
                None => {
                    // Execution failed (error, timeout) — count toward plateau.
                    plateau_counter += 1;
                }
            }
        };

        Ok(FuzzPhaseResult {
            stats: FuzzPhaseStats {
                executions,
                new_paths_found: new_paths.len() as u32,
                corpus_size: self.corpus.len(),
                termination_reason,
                target_branch_ids: self.target_branch_ids.clone(),
            },
            new_paths,
            corpus: self.corpus,
        })
    }
}
```

- [ ] **Step 3: Write unit tests for FuzzSession termination**

```rust
#[cfg(test)]
mod tests {
    // ... existing tests ...

    #[tokio::test]
    async fn fuzz_session_terminates_on_plateau() {
        let mut corpus = Corpus::new();
        corpus.add(CorpusEntry {
            inputs: vec![json!(1)],
            coverage_hash: 1,
            branch_ids: vec![1],
        });
        let config = FuzzConfig {
            plateau_threshold: Some(3),
            max_executions: Some(1000),
            timeout_seconds: Some(60),
            max_attempts: Some(3),
        };
        let params = vec![ParamInfo {
            name: "x".into(),
            typ: crate::types::TypeInfo::Int,
            optional: false,
        }];
        let covered: HashSet<u64> = HashSet::new();

        let session = FuzzSession::new(
            corpus, params, vec![5], config, covered,
        );

        // Execute callback always returns the same known path.
        let result = session.run(|_inputs| {
            Box::pin(async {
                Ok::<_, std::convert::Infallible>(Some((
                    1u64, // same hash every time
                    vec![],
                    vec![],
                    vec![1u32],
                )))
            })
        }).await.unwrap();

        assert_eq!(result.stats.termination_reason, FuzzTermination::Plateau);
        assert_eq!(result.stats.executions, 3); // plateau_threshold = 3
        assert_eq!(result.stats.new_paths_found, 0);
    }

    #[tokio::test]
    async fn fuzz_session_terminates_on_execution_cap() {
        let mut corpus = Corpus::new();
        corpus.add(CorpusEntry {
            inputs: vec![json!(1)],
            coverage_hash: 1,
            branch_ids: vec![1],
        });
        let config = FuzzConfig {
            plateau_threshold: Some(1000), // high — won't trigger
            max_executions: Some(5),
            timeout_seconds: Some(60),
            max_attempts: Some(3),
        };
        let params = vec![ParamInfo {
            name: "x".into(),
            typ: crate::types::TypeInfo::Int,
            optional: false,
        }];

        let session = FuzzSession::new(
            corpus, params, vec![5], config, HashSet::new(),
        );

        let counter = std::sync::atomic::AtomicU64::new(100);
        let result = session.run(|_inputs| {
            let hash = counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Box::pin(async move {
                Ok::<_, std::convert::Infallible>(Some((
                    hash,
                    vec![],
                    vec![],
                    vec![1u32],
                )))
            })
        }).await.unwrap();

        assert_eq!(result.stats.termination_reason, FuzzTermination::ExecutionCap);
        assert_eq!(result.stats.executions, 5);
        assert_eq!(result.stats.new_paths_found, 5); // every exec is unique
    }

    #[tokio::test]
    async fn fuzz_session_adds_discoveries_to_corpus() {
        let mut corpus = Corpus::new();
        corpus.add(CorpusEntry {
            inputs: vec![json!(0)],
            coverage_hash: 0,
            branch_ids: vec![1],
        });
        let config = FuzzConfig {
            plateau_threshold: Some(1000),
            max_executions: Some(3),
            timeout_seconds: Some(60),
            max_attempts: Some(3),
        };
        let params = vec![ParamInfo {
            name: "x".into(),
            typ: crate::types::TypeInfo::Int,
            optional: false,
        }];

        let session = FuzzSession::new(
            corpus, params, vec![5], config, HashSet::new(),
        );

        let counter = std::sync::atomic::AtomicU64::new(10);
        let result = session.run(|_inputs| {
            let hash = counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Box::pin(async move {
                Ok::<_, std::convert::Infallible>(Some((
                    hash, vec![], vec![], vec![1u32],
                )))
            })
        }).await.unwrap();

        assert_eq!(result.stats.new_paths_found, 3);
        // Corpus should contain original seed + 3 discoveries = 4
        assert_eq!(result.corpus.len(), 4);
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p shatter-core fuzzer::tests`
Expected: All PASS

- [ ] **Step 5: Commit**

```bash
git add shatter-core/src/fuzzer.rs
git commit -m "str-e0uy: implement FuzzSession with bounded mutation-execution loop"
```

---

### Task 5: Wire fuzz phase into the orchestrator

The orchestrator gains: fuzz attempt tracking, trigger logic, and fuzz phase entry/exit within the main exploration loop.

**Files:**
- Modify: `shatter-core/src/orchestrator.rs`

This is the most complex task. The changes integrate into the existing explore loop.

- [ ] **Step 1: Add fuzz state types to orchestrator**

Near the `ExploreState` struct (line 338), add:

```rust
/// Tracks fuzz attempt state for a single branch.
#[derive(Debug, Clone)]
pub struct FuzzAttemptState {
    /// How many fuzz phases have targeted this branch.
    pub count: u32,
    /// `covered_paths.len()` at the time of the last attempt.
    pub coverage_at_last_attempt: usize,
}
```

- [ ] **Step 2: Write failing test for fuzz trigger logic**

Add a test that validates the trigger conditions:

```rust
#[cfg(test)]
mod fuzz_trigger_tests {
    use super::*;

    #[test]
    fn branch_eligible_for_fuzzing_when_fresh() {
        let attempts: HashMap<u32, FuzzAttemptState> = HashMap::new();
        let max_attempts = Some(3u32);
        let current_coverage = 10usize;
        assert!(is_fuzz_eligible(
            5, &attempts, max_attempts, current_coverage,
        ));
    }

    #[test]
    fn branch_ineligible_after_max_attempts_no_coverage_growth() {
        let mut attempts = HashMap::new();
        attempts.insert(5, FuzzAttemptState {
            count: 3,
            coverage_at_last_attempt: 10,
        });
        assert!(!is_fuzz_eligible(
            5, &attempts, Some(3), 10, // same coverage
        ));
    }

    #[test]
    fn branch_eligible_after_max_attempts_with_coverage_growth() {
        let mut attempts = HashMap::new();
        attempts.insert(5, FuzzAttemptState {
            count: 3,
            coverage_at_last_attempt: 10,
        });
        assert!(is_fuzz_eligible(
            5, &attempts, Some(3), 15, // coverage grew
        ));
    }

    #[test]
    fn branch_always_eligible_in_indefinite_mode() {
        let mut attempts = HashMap::new();
        attempts.insert(5, FuzzAttemptState {
            count: 100,
            coverage_at_last_attempt: 50,
        });
        // max_attempts = None means indefinite, but still gated on coverage growth
        assert!(!is_fuzz_eligible(
            5, &attempts, None, 50, // no growth
        ));
        assert!(is_fuzz_eligible(
            5, &attempts, None, 51, // growth
        ));
    }
}
```

- [ ] **Step 3: Implement `is_fuzz_eligible` helper**

```rust
/// Check whether a branch is eligible for a fuzz attempt.
///
/// Eligible when: never fuzzed before, OR under the attempt cap, OR
/// coverage has grown since the last attempt (context changed).
fn is_fuzz_eligible(
    branch_id: u32,
    attempts: &HashMap<u32, FuzzAttemptState>,
    max_attempts: Option<u32>,
    current_coverage: usize,
) -> bool {
    match attempts.get(&branch_id) {
        None => true,
        Some(state) => {
            // Coverage grew since last attempt — always retry.
            if current_coverage > state.coverage_at_last_attempt {
                return true;
            }
            // Under the cap (bounded mode).
            match max_attempts {
                Some(max) => state.count < max,
                None => false, // indefinite but no coverage growth — skip
            }
        }
    }
}
```

- [ ] **Step 4: Run trigger tests to verify they pass**

Run: `cargo test -p shatter-core fuzz_trigger_tests`
Expected: All PASS

- [ ] **Step 5: Implement `collect_fuzz_targets` helper**

This function inspects the uncovered frontier for branches with `Unknown` constraints that are eligible for fuzzing:

```rust
/// Collect branches eligible for fuzzing from the uncovered frontier.
///
/// Returns branch IDs that have `Unknown` constraints and pass the
/// retry eligibility check.
fn collect_fuzz_targets(
    frontier: &[/* the frontier type used in orchestrator */],
    fuzz_attempts: &HashMap<u32, FuzzAttemptState>,
    max_attempts: Option<u32>,
    current_coverage: usize,
) -> Vec<u32> {
    frontier
        .iter()
        .filter(|f| {
            // Has Unknown constraint (opaque).
            f.has_unknown_constraint()
                && is_fuzz_eligible(
                    f.branch_id,
                    fuzz_attempts,
                    max_attempts,
                    current_coverage,
                )
        })
        .map(|f| f.branch_id)
        .collect()
}
```

Note: the exact frontier type and `has_unknown_constraint()` method depend on the existing frontier representation in the orchestrator. The implementer should read the frontier structures (look for `target_branches`, `FrontierEntry`, or similar near the main loop) and adapt accordingly.

- [ ] **Step 6: Add fuzz phase entry point in the main exploration loop**

In the main loop (near line 1735 where `CoveragePlateau` is detected), before returning the termination reason, check for fuzz targets:

```rust
// Existing: plateau detected.
if config.plateau_threshold > 0 && plateau_counter >= config.plateau_threshold {
    // NEW: check for fuzz-eligible opaque branches before terminating.
    let fuzz_targets = collect_fuzz_targets(
        &frontier, &fuzz_attempts, config.fuzz.max_attempts, covered_paths.len(),
    );
    if !fuzz_targets.is_empty() {
        log::info!(
            "Coverage plateau — entering fuzz phase targeting {} opaque branch(es)",
            fuzz_targets.len(),
        );

        // Build corpus from execution history.
        let mut corpus = fuzz_corpus.take().unwrap_or_default();
        // Seed from inputs that reached near target branches.
        // (Use the existing observation history to find parent branches.)

        let session = crate::fuzzer::FuzzSession::new(
            corpus,
            param_infos.to_vec(),
            fuzz_targets.clone(),
            config.fuzz.clone(),
            covered_paths.clone(),
        );

        // Run the fuzz phase using the same frontend execute path.
        let fuzz_result = session.run(|inputs| {
            // Wrap the existing observe_one / execute logic.
            // The implementer should extract the execute-and-hash logic
            // into a reusable closure or helper.
            Box::pin(async { /* ... */ })
        }).await?;

        // Integrate results.
        for discovery in &fuzz_result.new_paths {
            covered_paths.insert(discovery.path_hash);
            for decision in &discovery.branch_path {
                if seen_branch_ids.insert(decision.branch_id) {
                    discoveries.push((decision.branch_id, DiscoveryMethod::Fuzzed));
                }
            }
        }

        // Update attempt tracking.
        for branch_id in &fuzz_targets {
            let state = fuzz_attempts.entry(*branch_id).or_insert(FuzzAttemptState {
                count: 0,
                coverage_at_last_attempt: 0,
            });
            state.count += 1;
            state.coverage_at_last_attempt = covered_paths.len();
        }

        // Preserve corpus for future phases.
        fuzz_corpus = Some(fuzz_result.corpus);

        // Reset plateau and continue concolic.
        plateau_counter = 0;
        log::info!(
            "Fuzz phase complete: {} new path(s) from {} executions ({:?})",
            fuzz_result.stats.new_paths_found,
            fuzz_result.stats.executions,
            fuzz_result.stats.termination_reason,
        );
        continue; // resume concolic loop
    }

    // No fuzz targets — terminate as before.
    return Ok(ObserveOneResult::Terminated(TerminationReason::CoveragePlateau));
}
```

**Important implementation notes for the implementer:**
- The `execute_fn` callback in `FuzzSession::run` must call the same frontend execute path used by `observe_one`. Extract the execute + hash logic into a helper or closure that both `observe_one` and the fuzz session can use.
- `fuzz_corpus` is a new `Option<Corpus>` local variable initialized to `None` at the top of the explore function.
- `fuzz_attempts` is a new `HashMap<u32, FuzzAttemptState>` local variable initialized to `HashMap::new()`.
- The plateau check in `observe_one` (line 546) must be updated to call this new logic instead of returning `Terminated` directly. The cleanest approach is to return a new variant (e.g., `ObserveOneResult::PossibleFuzzPhase`) and handle it in the main loop, or move the plateau check into the main loop body.

- [ ] **Step 7: Run `cargo test -p shatter-core` to verify nothing is broken**

Run: `cargo test -p shatter-core`
Expected: All existing tests PASS

- [ ] **Step 8: Commit**

```bash
git add shatter-core/src/orchestrator.rs
git commit -m "str-e0uy: wire fuzz phase trigger and execution into orchestrator"
```

---

### Task 6: Integration test — fuzz phase activates on opaque constraints

Verify the full integration: orchestrator → fuzz phase → new path discovery → resume concolic.

**Files:**
- Modify: `shatter-core/src/orchestrator.rs` (add integration test) or create a new test file

- [ ] **Step 1: Write integration test**

This test needs a mock frontend that returns `Unknown` constraints for specific branches, causing Z3 to fail and the fuzzer to activate. The test verifies:
1. The orchestrator enters a fuzz phase (visible in log output or returned stats)
2. The fuzzer discovers at least one new path
3. The discovery is attributed as `DiscoveryMethod::Fuzzed`

```rust
#[tokio::test]
async fn fuzz_phase_activates_on_unknown_constraints() {
    // Set up a mock frontend that:
    // - Returns 2 branches on the initial seed input
    // - Branch 1: Expr constraint (Z3 can solve)
    // - Branch 2: Unknown constraint (opaque — triggers fuzz)
    // - Returns a new unique path when inputs are mutated
    //   past a threshold (simulating the fuzzer cracking the opaque branch)

    // Use ExploreConfig with a low plateau threshold to trigger quickly.
    let config = ExploreConfig {
        max_iterations: Some(50),
        max_executions: Some(200),
        plateau_threshold: 5,
        fuzz: FuzzConfig {
            plateau_threshold: Some(10),
            max_executions: Some(100),
            timeout_seconds: Some(10),
            max_attempts: Some(3),
        },
        // ... other fields with defaults ...
    };

    // Run explore with the mock frontend.
    // Verify the result contains Fuzzed discoveries.
    let result = explore(/* ... */).await.unwrap();

    let fuzz_discoveries: Vec<_> = result
        .discoveries
        .iter()
        .filter(|(_, method)| *method == DiscoveryMethod::Fuzzed)
        .collect();
    assert!(
        !fuzz_discoveries.is_empty(),
        "Expected at least one Fuzzed discovery, got none"
    );
}
```

The exact test setup depends on how mocked frontends work in the existing test suite. The implementer should find and follow the pattern from existing orchestrator integration tests (look for `#[tokio::test]` blocks that call `explore()` with mock responses).

- [ ] **Step 2: Run integration test**

Run: `cargo test -p shatter-core fuzz_phase_activates`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add shatter-core/src/orchestrator.rs
git commit -m "str-e0uy: add integration test for fuzz phase activation"
```

---

### Task 7: CLI explore output includes fuzz metrics

The explore report should show `Fuzzed` in the method breakdown when relevant.

**Files:**
- Modify: `shatter-cli/src/commands/explore.rs` (update report formatting if needed)

- [ ] **Step 1: Check existing report formatting**

Read the explore report formatting code. Search for where `MethodPercentages` or `CoverageMetrics` is printed. The new `fuzz_pct` / `fuzz_found` fields may already appear if the report iterates over all fields, or may need explicit handling.

- [ ] **Step 2: Add `fuzz_pct` to the method breakdown display**

If the report uses explicit field access (likely), add the fuzz line:

```rust
if metrics.fuzz_found > 0 {
    // Add fuzz percentage to the method breakdown output.
    // Follow the pattern used for z3_pct, random_pct, etc.
}
```

- [ ] **Step 3: Run the quick test tier**

Run: `task test-quick`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add shatter-cli/src/commands/explore.rs
git commit -m "str-e0uy: show fuzz discovery metrics in explore report"
```

---

### Task 8: E2E test and final quality gates

Verify the full pipeline with a real frontend, then run all required gates.

**Files:**
- Modify or create: E2E test in `shatter-core/tests/` or `tests/`

- [ ] **Step 1: Add E2E test with opaque branch**

Create a test TypeScript function with an opaque branch that Z3 cannot solve:

```typescript
// examples/ts/opaque-branch.ts
export function opaqueGuard(x: string): string {
  // Simple "opaque" check: length comparison that symbolic execution
  // may struggle with after mutations stack up.
  if (x.length === 7 && x[0] === 'z') {
    return "rare-branch";
  }
  if (x.length > 3) {
    return "medium";
  }
  return "short";
}
```

Add an E2E test that runs `shatter explore` on this function with `--concolic` and verifies the output includes fuzz-discovered paths (or at minimum, that the explore completes without error and covers more branches than pure concolic would).

- [ ] **Step 2: Run E2E test**

Run: `cargo test --test e2e_concolic`
Expected: PASS (including new test case)

- [ ] **Step 3: Run standard test tier**

Run: `task test-standard`
Expected: PASS

- [ ] **Step 4: Run full check**

Run: `task check`
Expected: PASS

- [ ] **Step 5: Commit any test fixtures**

```bash
git add examples/ts/opaque-branch.ts shatter-core/tests/
git commit -m "str-e0uy: add E2E test for hybrid fuzzing on opaque branch"
```

- [ ] **Step 6: Run pre-completion**

Invoke `/pre-completion` to verify all quality gates pass.
