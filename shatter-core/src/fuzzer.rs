//! Fuzzing corpus and mutation support for hybrid concolic-fuzz exploration.
//!
//! The [`Corpus`] holds inputs that discovered unique coverage paths. The
//! mutation strategy reuses [`crate::input_gen::mutate_inputs`] and
//! [`crate::input_gen::havoc_mutate_inputs`] — no new mutation code here.

use std::collections::HashSet;

use rand::Rng;
use serde_json::Value;

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
}
