//! Fuzzing corpus and mutation support for hybrid concolic-fuzz exploration.
//!
//! The [`Corpus`] holds inputs that discovered unique coverage paths. The
//! mutation strategy reuses [`crate::input_gen::mutate_inputs`] and
//! [`crate::input_gen::havoc_mutate_inputs`] — no new mutation code here.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use rand::Rng;
use serde_json::Value;

use crate::config::{
    self, FuzzConfig,
};
use crate::execution_record::{BranchDecision, SymConstraint};
use crate::types::ParamInfo;

/// A single entry in the fuzzing corpus.
#[derive(Debug, Clone)]
pub struct CorpusEntry {
    pub inputs: Vec<Value>,
    pub coverage_hash: u64,
    pub branch_ids: Vec<u32>,
}

/// Per-function corpus of inputs that discovered unique paths.
#[derive(Debug, Clone, Default)]
pub struct Corpus {
    entries: Vec<CorpusEntry>,
    seen_hashes: HashSet<u64>,
}

impl Corpus {
    /// Create an empty corpus.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an entry if its coverage hash has not been seen before.
    ///
    /// Returns `true` if the entry was new (and inserted), `false` if
    /// a duplicate hash was already present.
    pub fn add(&mut self, entry: CorpusEntry) -> bool {
        if self.seen_hashes.insert(entry.coverage_hash) {
            self.entries.push(entry);
            true
        } else {
            false
        }
    }

    /// Seed the corpus from orchestrator execution history.
    ///
    /// Each history tuple is `(inputs, branch_ids, coverage_hash)`. Only
    /// entries whose `branch_ids` contain `target_parent_id` are included.
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

    /// Pick a random corpus entry.
    pub fn pick(&self, rng: &mut impl Rng) -> Option<&CorpusEntry> {
        if self.entries.is_empty() {
            None
        } else {
            let idx = rng.random_range(0..self.entries.len());
            Some(&self.entries[idx])
        }
    }

    /// Number of entries in the corpus.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the corpus is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Why a fuzz phase ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FuzzTermination {
    Plateau,
    ExecutionCap,
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
pub struct FuzzPhaseResult {
    pub new_paths: Vec<FuzzDiscovery>,
    pub corpus: Corpus,
    pub stats: FuzzPhaseStats,
}

/// Bounded mutation-execution loop for coverage-guided fuzzing.
pub struct FuzzSession {
    corpus: Corpus,
    params: Vec<ParamInfo>,
    target_branch_ids: Vec<u32>,
    config: FuzzConfig,
    covered_paths: HashSet<u64>,
}

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

    /// Run the fuzz phase loop until a termination condition is met.
    ///
    /// The `execute_fn` callback takes mutated inputs and returns:
    /// - `Ok(Some((path_hash, branch_path, constraints, branch_ids)))` on successful execution
    /// - `Ok(None)` when execution failed (counts toward plateau)
    /// - `Err(e)` for fatal errors (stops fuzzing immediately)
    pub async fn run<F, Fut, E>(mut self, mut execute_fn: F) -> Result<FuzzPhaseResult, E>
    where
        F: FnMut(Vec<Value>) -> Fut,
        Fut: std::future::Future<
            Output = Result<Option<(u64, Vec<BranchDecision>, Vec<SymConstraint>, Vec<u32>)>, E>,
        >,
    {
        let plateau_threshold = self
            .config
            .plateau_threshold
            .unwrap_or(config::DEFAULT_FUZZ_PLATEAU_THRESHOLD);
        let max_executions = self
            .config
            .max_executions
            .unwrap_or(config::DEFAULT_FUZZ_MAX_EXECUTIONS);
        let timeout = Duration::from_secs(
            self.config
                .timeout_seconds
                .unwrap_or(config::DEFAULT_FUZZ_TIMEOUT_SECS) as u64,
        );

        let start = Instant::now();
        let mut executions: u32 = 0;
        let mut plateau_counter: u32 = 0;
        let mut new_paths: Vec<FuzzDiscovery> = Vec::new();
        let mut rng = rand::rng();

        let termination_reason = loop {
            // Check termination bounds before each execution.
            if plateau_counter >= plateau_threshold {
                break FuzzTermination::Plateau;
            }
            if executions >= max_executions {
                break FuzzTermination::ExecutionCap;
            }
            if start.elapsed() >= timeout {
                break FuzzTermination::Timeout;
            }

            // Pick a parent from the corpus and mutate.
            let parent_inputs = match self.corpus.pick(&mut rng) {
                Some(entry) => entry.inputs.clone(),
                None => break FuzzTermination::Plateau, // empty corpus, nothing to do
            };

            let mutated = crate::input_gen::havoc_mutate_inputs(
                &parent_inputs,
                &self.params,
                1.0,
                &[],
                &mut rng,
            );

            // Execute the mutated inputs.
            let result = execute_fn(mutated.clone()).await?;
            executions += 1;

            match result {
                Some((path_hash, branch_path, constraints, branch_ids)) => {
                    if self.covered_paths.insert(path_hash) {
                        // New path discovered.
                        plateau_counter = 0;
                        new_paths.push(FuzzDiscovery {
                            path_hash,
                            branch_path,
                            constraints,
                            inputs: mutated.clone(),
                        });
                        self.corpus.add(CorpusEntry {
                            inputs: mutated,
                            coverage_hash: path_hash,
                            branch_ids,
                        });
                    } else {
                        plateau_counter += 1;
                    }
                }
                None => {
                    // Execution failed — count toward plateau.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corpus_add_deduplicates_by_coverage_hash() {
        let mut corpus = Corpus::new();

        let e1 = CorpusEntry {
            inputs: vec![Value::from(1)],
            coverage_hash: 42,
            branch_ids: vec![1],
        };
        let e2 = CorpusEntry {
            inputs: vec![Value::from(2)],
            coverage_hash: 42,
            branch_ids: vec![2],
        };

        assert!(corpus.add(e1));
        assert!(!corpus.add(e2));
        assert_eq!(corpus.len(), 1);
    }

    #[test]
    fn corpus_seed_from_history_filters_by_parent_branch() {
        let mut corpus = Corpus::new();
        let target = 5;

        let history = vec![
            (vec![Value::from(1)], vec![5, 10], 100),
            (vec![Value::from(2)], vec![3, 7], 200),  // no target
            (vec![Value::from(3)], vec![5, 20], 300),
        ];

        corpus.seed_from_history(&history, target);
        assert_eq!(corpus.len(), 2);
    }

    #[test]
    fn corpus_pick_returns_none_when_empty() {
        let corpus = Corpus::new();
        let mut rng = rand::rng();
        assert!(corpus.pick(&mut rng).is_none());
    }

    mod proptests {
        use proptest::prelude::*;
        use crate::input_gen;
        use crate::test_arbitraries::arb_param_info;

        proptest! {
            #[test]
            fn mutated_inputs_preserve_vector_length(
                params in proptest::collection::vec(arb_param_info(), 1..=8),
            ) {
                let mut rng = rand::rng();
                let inputs: Vec<serde_json::Value> = params
                    .iter()
                    .map(|p| input_gen::generate_random_value(&p.typ, &mut rng, None))
                    .collect();

                let mutated = input_gen::mutate_inputs(
                    &inputs,
                    &params,
                    1.0,
                    &[],
                    &mut rng,
                );
                prop_assert_eq!(mutated.len(), inputs.len());
            }
        }
    }

    mod fuzz_session_tests {
        use super::*;
        use std::collections::HashSet;
        use std::sync::atomic::{AtomicU64, Ordering};
        use crate::config::FuzzConfig;
        use crate::types::{ParamInfo, TypeInfo};

        fn make_config(plateau: u32, max_exec: u32, timeout: u32) -> FuzzConfig {
            FuzzConfig {
                plateau_threshold: Some(plateau),
                max_executions: Some(max_exec),
                timeout_seconds: Some(timeout),
                max_attempts: Some(10),
            }
        }

        fn make_params() -> Vec<ParamInfo> {
            vec![ParamInfo {
                name: "x".into(),
                typ: TypeInfo::Int,
                type_name: None,
            }]
        }

        fn seed_corpus(hash: u64) -> Corpus {
            let mut corpus = Corpus::new();
            corpus.add(CorpusEntry {
                inputs: vec![Value::from(1)],
                coverage_hash: hash,
                branch_ids: vec![1],
            });
            corpus
        }

        #[tokio::test]
        async fn fuzz_session_terminates_on_plateau() {
            let corpus = seed_corpus(100);
            let mut covered = HashSet::new();
            covered.insert(100); // seed hash already covered

            let session = FuzzSession::new(
                corpus,
                make_params(),
                vec![1],
                make_config(3, 1000, 60),
                covered,
            );

            // Always return the same path_hash (already covered) -> plateau.
            let result = session
                .run(|_inputs| async {
                    Ok::<_, std::convert::Infallible>(Some((100, vec![], vec![], vec![1])))
                })
                .await
                .unwrap();

            assert_eq!(result.stats.termination_reason, FuzzTermination::Plateau);
            assert_eq!(result.stats.executions, 3);
            assert_eq!(result.stats.new_paths_found, 0);
        }

        #[tokio::test]
        async fn fuzz_session_terminates_on_execution_cap() {
            let corpus = seed_corpus(100);
            let covered = HashSet::new(); // seed hash NOT pre-covered, so first exec is new too

            let counter = AtomicU64::new(200);

            let session = FuzzSession::new(
                corpus,
                make_params(),
                vec![1],
                make_config(1000, 5, 60), // high plateau, cap at 5
                covered,
            );

            let result = session
                .run(|_inputs| {
                    let hash = counter.fetch_add(1, Ordering::Relaxed);
                    async move {
                        Ok::<_, std::convert::Infallible>(Some((hash, vec![], vec![], vec![1])))
                    }
                })
                .await
                .unwrap();

            assert_eq!(
                result.stats.termination_reason,
                FuzzTermination::ExecutionCap
            );
            assert_eq!(result.stats.executions, 5);
            assert_eq!(result.stats.new_paths_found, 5);
        }

        #[tokio::test]
        async fn fuzz_session_adds_discoveries_to_corpus() {
            let corpus = seed_corpus(100);
            let covered = HashSet::new();

            let counter = AtomicU64::new(300);

            let session = FuzzSession::new(
                corpus,
                make_params(),
                vec![1],
                make_config(1000, 3, 60), // cap at 3 executions
                covered,
            );

            let result = session
                .run(|_inputs| {
                    let hash = counter.fetch_add(1, Ordering::Relaxed);
                    async move {
                        Ok::<_, std::convert::Infallible>(Some((hash, vec![], vec![], vec![1])))
                    }
                })
                .await
                .unwrap();

            // 1 seed + 3 discoveries = 4
            assert_eq!(result.corpus.len(), 4);
            assert_eq!(result.stats.new_paths_found, 3);
        }
    }
}
