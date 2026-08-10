//! Population management and selection for genetic search.
//!
//! Manages a fixed-size collection of [`Individual`]s through tournament
//! selection, elitism, crossover, and mutation to evolve inputs that maximize
//! branch coverage fitness.

use std::collections::HashSet;

use rand::Rng;
use serde_json::Value;

use crate::behavior::BehaviorMap;
use crate::input_gen::{
    ValueSource, crossover_inputs_with_sources, generate_random_inputs, mutate_inputs_with_sources,
};
use crate::orchestrator::{FrontendCapabilities, hash_branch_path};
use crate::types::ParamInfo;

/// Fraction of the population carried unchanged into the next generation.
const ELITISM_FRACTION: f64 = 0.10;

/// Number of individuals competing in each tournament selection round.
const TOURNAMENT_SIZE: usize = 3;

/// A single candidate in the genetic population.
#[derive(Debug, Clone)]
pub struct Individual {
    /// Function input arguments (one `Value` per parameter).
    pub inputs: Vec<Value>,
    /// Fitness score assigned externally after execution. Higher is better.
    pub fitness: f64,
    /// Hash of the branch path taken during execution. 0 if not yet executed.
    pub path_hash: u64,
}

/// Fixed-size population of candidate inputs for genetic search.
#[derive(Debug, Clone)]
pub struct Population {
    individuals: Vec<Individual>,
    generation: u32,
}

impl Population {
    /// Create a new population seeded from existing behaviors and filled with
    /// random inputs to reach `population_size`.
    ///
    /// Behaviors from the `BehaviorMap` provide known-good inputs from prior
    /// exploration. Remaining slots are filled with random inputs matching the
    /// parameter types.
    pub fn new(
        behavior_map: &BehaviorMap,
        params: &[ParamInfo],
        population_size: u32,
        rng: &mut impl Rng,
        caps: Option<&FrontendCapabilities>,
    ) -> Self {
        let target = population_size as usize;
        let mut individuals = Vec::with_capacity(target);

        // Seed from existing behaviors (known-good inputs from prior exploration).
        for behavior in &behavior_map.behaviors {
            if individuals.len() >= target {
                break;
            }
            individuals.push(Individual {
                inputs: behavior.input_args.clone(),
                fitness: 0.0,
                path_hash: hash_branch_path(&behavior.branch_path),
            });
        }

        // Fill remaining slots with random inputs.
        while individuals.len() < target {
            individuals.push(Individual {
                inputs: generate_random_inputs(params, rng, caps),
                fitness: 0.0,
                path_hash: 0,
            });
        }

        Self {
            individuals,
            generation: 0,
        }
    }

    /// Produce the next generation via elitism + tournament selection +
    /// crossover + mutation. Mutates the population in place.
    ///
    /// The caller is responsible for assigning fitness scores (via
    /// [`individuals_mut`](Self::individuals_mut)) before calling this method.
    /// `sources` pins custom-generator/extractor parameter slots so their
    /// native-replay markers survive crossover and mutation (str-6cdp). Pass an
    /// empty slice when no generators are configured.
    pub fn evolve(
        &mut self,
        params: &[ParamInfo],
        sources: &[ValueSource],
        mutation_rate: f64,
        crossover_rate: f64,
        dictionary: &[&str],
        rng: &mut impl Rng,
    ) {
        let pop_size = self.individuals.len();
        if pop_size == 0 {
            return;
        }

        // Sort by fitness descending.
        self.individuals.sort_by(|a, b| {
            b.fitness
                .partial_cmp(&a.fitness)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Elitism: carry top 10% (at least 1) unchanged.
        let elite_count = ((pop_size as f64 * ELITISM_FRACTION).ceil() as usize)
            .max(1)
            .min(pop_size);
        let mut next_gen: Vec<Individual> = self.individuals[..elite_count].to_vec();

        // Fill remaining slots via tournament selection → crossover → mutation.
        while next_gen.len() < pop_size {
            let parent_a = self.tournament_select(rng);
            let parent_b = self.tournament_select(rng);

            let (child_a_inputs, child_b_inputs) = crossover_inputs_with_sources(
                &parent_a.inputs,
                &parent_b.inputs,
                params,
                sources,
                crossover_rate,
                rng,
            );

            let mutated_a = mutate_inputs_with_sources(
                &child_a_inputs,
                params,
                sources,
                mutation_rate,
                dictionary,
                rng,
            );
            next_gen.push(Individual {
                inputs: mutated_a,
                fitness: 0.0,
                path_hash: 0,
            });

            if next_gen.len() < pop_size {
                let mutated_b = mutate_inputs_with_sources(
                    &child_b_inputs,
                    params,
                    sources,
                    mutation_rate,
                    dictionary,
                    rng,
                );
                next_gen.push(Individual {
                    inputs: mutated_b,
                    fitness: 0.0,
                    path_hash: 0,
                });
            }
        }

        self.individuals = next_gen;
        self.generation += 1;
    }

    /// Tournament selection: pick the best of `TOURNAMENT_SIZE` random individuals.
    fn tournament_select(&self, rng: &mut impl Rng) -> &Individual {
        let len = self.individuals.len();
        let mut best = &self.individuals[rng.random_range(0..len)];
        for _ in 1..TOURNAMENT_SIZE {
            let candidate = &self.individuals[rng.random_range(0..len)];
            if candidate.fitness > best.fitness {
                best = candidate;
            }
        }
        best
    }

    /// Return the individual with the highest fitness, or `None` if empty.
    pub fn best(&self) -> Option<&Individual> {
        self.individuals.iter().max_by(|a, b| {
            a.fitness
                .partial_cmp(&b.fitness)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    }

    /// Fraction of unique path hashes in the population (0.0–1.0).
    ///
    /// Measures behavioral diversity: near 1.0 means most individuals explore
    /// distinct execution paths; near 0.0 means convergence.
    pub fn diversity(&self) -> f64 {
        if self.individuals.is_empty() {
            return 0.0;
        }
        let unique: HashSet<u64> = self.individuals.iter().map(|i| i.path_hash).collect();
        unique.len() as f64 / self.individuals.len() as f64
    }

    /// Read-only access to all individuals.
    pub fn individuals(&self) -> &[Individual] {
        &self.individuals
    }

    /// Mutable access to all individuals (for updating fitness after execution).
    pub fn individuals_mut(&mut self) -> &mut [Individual] {
        &mut self.individuals
    }

    /// Current generation number (0-based, incremented by each `evolve` call).
    pub fn generation(&self) -> u32 {
        self.generation
    }

    /// Number of individuals in the population.
    pub fn len(&self) -> usize {
        self.individuals.len()
    }

    /// Whether the population is empty.
    pub fn is_empty(&self) -> bool {
        self.individuals.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behavior::{Behavior, BehaviorMap};
    use crate::execution_record::{BranchDecision, SymConstraint};
    use crate::types::TypeInfo;
    use rand::SeedableRng;
    use rand::rngs::StdRng;
    use serde_json::json;

    fn make_params() -> Vec<ParamInfo> {
        vec![ParamInfo {
            name: "x".into(),
            typ: TypeInfo::Int { int_width: None, int_signed: None },
            type_name: None,
        }]
    }

    fn make_behavior(id: u32, input: Value, branch_id: u32, taken: bool) -> Behavior {
        Behavior {
            id,
            input_args: vec![input],
            return_value: None,
            thrown_error: None,
            branch_path: vec![BranchDecision {
                branch_id,
                line: 1,
                taken,
                constraint: SymConstraint::Unknown {
                    hint: "test".into(),
                },
                conditions: None,
            }],
            side_effects: vec![],
            dependency_trace: None,
            mock_values: vec![],
        }
    }

    fn make_behavior_map(behaviors: Vec<Behavior>) -> BehaviorMap {
        BehaviorMap {
            function_id: "test_fn".into(),
            behaviors,
            fingerprint: None,
            nondeterministic_fields: vec![],
        }
    }

    fn empty_behavior_map() -> BehaviorMap {
        make_behavior_map(vec![])
    }

    #[test]
    fn new_seeds_from_behaviors() {
        let mut rng = StdRng::seed_from_u64(42);
        let behaviors = vec![
            make_behavior(0, json!(1), 10, true),
            make_behavior(1, json!(2), 20, false),
            make_behavior(2, json!(3), 30, true),
        ];
        let bmap = make_behavior_map(behaviors);
        let pop = Population::new(&bmap, &make_params(), 5, &mut rng, None);

        assert_eq!(pop.len(), 5);
        // First 3 should be seeded from behaviors.
        assert_eq!(pop.individuals()[0].inputs, vec![json!(1)]);
        assert_eq!(pop.individuals()[1].inputs, vec![json!(2)]);
        assert_eq!(pop.individuals()[2].inputs, vec![json!(3)]);
        // Seeded individuals have non-zero path_hash.
        assert_ne!(pop.individuals()[0].path_hash, 0);
    }

    #[test]
    fn new_all_random_when_no_behaviors() {
        let mut rng = StdRng::seed_from_u64(42);
        let pop = Population::new(&empty_behavior_map(), &make_params(), 10, &mut rng, None);
        assert_eq!(pop.len(), 10);
        assert_eq!(pop.generation(), 0);
    }

    #[test]
    fn new_zero_size() {
        let mut rng = StdRng::seed_from_u64(42);
        let pop = Population::new(&empty_behavior_map(), &make_params(), 0, &mut rng, None);
        assert!(pop.is_empty());
        assert_eq!(pop.diversity(), 0.0);
        assert!(pop.best().is_none());
    }

    #[test]
    fn best_returns_highest_fitness() {
        let mut rng = StdRng::seed_from_u64(42);
        let mut pop = Population::new(&empty_behavior_map(), &make_params(), 5, &mut rng, None);
        for (i, ind) in pop.individuals_mut().iter_mut().enumerate() {
            ind.fitness = i as f64 * 0.1;
        }
        let best = pop.best().expect("non-empty population");
        assert!((best.fitness - 0.4).abs() < f64::EPSILON);
    }

    #[test]
    fn diversity_all_unique() {
        let mut rng = StdRng::seed_from_u64(42);
        let mut pop = Population::new(&empty_behavior_map(), &make_params(), 4, &mut rng, None);
        for (i, ind) in pop.individuals_mut().iter_mut().enumerate() {
            ind.path_hash = (i + 1) as u64; // all distinct, all non-zero
        }
        assert!((pop.diversity() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn diversity_all_same() {
        let mut rng = StdRng::seed_from_u64(42);
        let mut pop = Population::new(&empty_behavior_map(), &make_params(), 4, &mut rng, None);
        for ind in pop.individuals_mut() {
            ind.path_hash = 999;
        }
        assert!((pop.diversity() - 0.25).abs() < f64::EPSILON);
    }

    #[test]
    fn evolve_preserves_size() {
        let mut rng = StdRng::seed_from_u64(42);
        let mut pop = Population::new(&empty_behavior_map(), &make_params(), 10, &mut rng, None);
        for ind in pop.individuals_mut() {
            ind.fitness = rng.random_range(0.0..1.0_f64);
        }
        pop.evolve(&make_params(), &[], 0.3, 0.7, &[], &mut rng);
        assert_eq!(pop.len(), 10);
        assert_eq!(pop.generation(), 1);
    }

    #[test]
    fn evolve_preserves_elites() {
        let mut rng = StdRng::seed_from_u64(42);
        let mut pop = Population::new(&empty_behavior_map(), &make_params(), 20, &mut rng, None);
        for (i, ind) in pop.individuals_mut().iter_mut().enumerate() {
            ind.fitness = i as f64;
        }
        let best_inputs = pop.best().expect("non-empty").inputs.clone();
        pop.evolve(&make_params(), &[], 0.3, 0.7, &[], &mut rng);
        let has_elite = pop.individuals().iter().any(|i| i.inputs == best_inputs);
        assert!(has_elite, "best individual should survive via elitism");
    }

    #[test]
    fn evolve_on_empty_population() {
        let mut pop = Population {
            individuals: vec![],
            generation: 0,
        };
        let mut rng = StdRng::seed_from_u64(42);
        pop.evolve(&make_params(), &[], 0.3, 0.7, &[], &mut rng);
        assert!(pop.is_empty());
        assert_eq!(pop.generation(), 0); // no increment on empty
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use crate::behavior::BehaviorMap;
    use crate::types::TypeInfo;
    use proptest::prelude::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    fn simple_params() -> Vec<ParamInfo> {
        vec![ParamInfo {
            name: "x".into(),
            typ: TypeInfo::Int { int_width: None, int_signed: None },
            type_name: None,
        }]
    }

    fn empty_bmap() -> BehaviorMap {
        BehaviorMap {
            function_id: "f".into(),
            behaviors: vec![],
            fingerprint: None,
            nondeterministic_fields: vec![],
        }
    }

    proptest! {
        #[test]
        fn evolve_preserves_population_size(
            pop_size in 2u32..50,
            seed in any::<u64>(),
        ) {
            let mut rng = StdRng::seed_from_u64(seed);
            let params = simple_params();
            let mut pop = Population::new(&empty_bmap(), &params, pop_size, &mut rng, None);
            for ind in pop.individuals_mut() {
                ind.fitness = rng.random_range(0.0..1.0_f64);
            }
            pop.evolve(&params, &[], 0.3, 0.7, &[], &mut rng);
            prop_assert_eq!(pop.len(), pop_size as usize);
        }

        #[test]
        fn diversity_in_zero_one(
            pop_size in 1u32..50,
            seed in any::<u64>(),
        ) {
            let mut rng = StdRng::seed_from_u64(seed);
            let pop = Population::new(&empty_bmap(), &simple_params(), pop_size, &mut rng, None);
            let d = pop.diversity();
            prop_assert!((0.0..=1.0).contains(&d), "diversity={d} out of range");
        }

        #[test]
        fn best_is_max_fitness(
            pop_size in 1u32..30,
            seed in any::<u64>(),
        ) {
            let mut rng = StdRng::seed_from_u64(seed);
            let mut pop = Population::new(&empty_bmap(), &simple_params(), pop_size, &mut rng, None);
            for ind in pop.individuals_mut() {
                ind.fitness = rng.random_range(0.0..1.0_f64);
            }
            let best = pop.best().unwrap();
            for ind in pop.individuals() {
                prop_assert!(best.fitness >= ind.fitness);
            }
        }

        #[test]
        fn elitism_preserves_best(
            pop_size in 3u32..30,
            seed in any::<u64>(),
        ) {
            let mut rng = StdRng::seed_from_u64(seed);
            let params = simple_params();
            let mut pop = Population::new(&empty_bmap(), &params, pop_size, &mut rng, None);
            for ind in pop.individuals_mut() {
                ind.fitness = rng.random_range(0.0..1.0_f64);
            }
            let best_inputs = pop.best().unwrap().inputs.clone();
            pop.evolve(&params, &[], 0.3, 0.7, &[], &mut rng);
            let has_elite = pop.individuals().iter().any(|i| i.inputs == best_inputs);
            prop_assert!(has_elite, "elite individual lost after evolve");
        }

        #[test]
        fn generation_increments(
            n_evolves in 1usize..10,
            seed in any::<u64>(),
        ) {
            let mut rng = StdRng::seed_from_u64(seed);
            let params = simple_params();
            let mut pop = Population::new(&empty_bmap(), &params, 10, &mut rng, None);
            for _ in 0..n_evolves {
                for ind in pop.individuals_mut() {
                    ind.fitness = rng.random_range(0.0..1.0_f64);
                }
                pop.evolve(&params, &[], 0.3, 0.7, &[], &mut rng);
            }
            prop_assert_eq!(pop.generation(), n_evolves as u32);
        }
    }
}
