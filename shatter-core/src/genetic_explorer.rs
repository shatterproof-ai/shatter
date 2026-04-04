//! Genetic algorithm exploration loop.
//!
//! Drives a [`Population`] of candidate inputs through repeated execution,
//! fitness scoring, and evolution to discover branch coverage that random
//! and concolic exploration missed.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use rand::rngs::StdRng;
use rand::SeedableRng;

use crate::behavior::Behavior;
use crate::config::GeneticConfig;
use crate::coverage_metrics::TargetBranch;
use crate::execution_record::BranchDecision;
use crate::frontend::{Frontend, FrontendError};
use crate::genetic::Population;
use crate::genetic_fitness::{score, FitnessContext, FitnessWeights};
use crate::orchestrator::hash_branch_path;
use crate::protocol::{Command, ExecuteResult, ResponseResult};
use crate::types::ParamInfo;

/// Result of a genetic exploration session.
#[derive(Debug)]
pub struct GeneticResult {
    /// Behaviors discovered for previously-unsolved target branches.
    pub discoveries: Vec<Behavior>,
    /// Number of generations completed.
    pub generations_run: u32,
    /// Total number of individual executions performed.
    pub total_executions: usize,
    /// Number of target branches solved (newly covered).
    pub targets_solved: usize,
}

/// Errors that can occur during genetic exploration.
#[derive(Debug, thiserror::Error)]
pub enum GeneticExploreError {
    #[error("frontend error: {0}")]
    Frontend(#[from] FrontendError),
}

/// Run the genetic exploration loop.
///
/// Initializes a population from `seed_inputs`, then iterates through
/// generations — executing each individual via the frontend, scoring
/// fitness against `targets`, persisting discoveries, and evolving the
/// population — until the generation limit, timeout, or full target
/// coverage is reached.
pub async fn genetic_explore(
    frontend: &mut Frontend,
    function_name: &str,
    seed_inputs: Vec<Vec<serde_json::Value>>,
    targets: Vec<TargetBranch>,
    params: &[ParamInfo],
    config: &GeneticConfig,
) -> Result<GeneticResult, GeneticExploreError> {
    let deadline = Instant::now() + Duration::from_secs(u64::from(config.timeout_secs));
    let mut rng = StdRng::from_os_rng();

    // Build a minimal BehaviorMap to seed the population from prior inputs.
    let seed_behavior_map = seed_behavior_map(function_name, &seed_inputs);
    let mut population = Population::new(
        &seed_behavior_map,
        params,
        config.population_size,
        &mut rng,
        None,
    );

    // Track which target branch IDs remain unsolved.
    let mut remaining_targets: HashSet<u32> = targets.iter().map(|t| t.branch_id).collect();
    let initial_target_count = remaining_targets.len();

    let mut fitness_context = FitnessContext::new();
    let fitness_weights = FitnessWeights::default();

    let mut discoveries: Vec<Behavior> = Vec::new();
    let mut total_executions: usize = 0;
    let mut generations_run: u32 = 0;
    let mut next_behavior_id: u32 = 0;

    for _gen in 0..config.max_generations {
        if remaining_targets.is_empty() {
            break;
        }
        if Instant::now() >= deadline {
            break;
        }

        // Evaluate every individual in the population.
        for individual in population.individuals_mut() {
            if Instant::now() >= deadline {
                break;
            }

            let result = execute_individual(
                frontend,
                function_name,
                &individual.inputs,
            )
            .await?;

            total_executions += 1;

            // Score fitness.
            let breakdown = score(
                &result,
                &remaining_targets,
                &mut fitness_context,
                &fitness_weights,
                None,
            );
            individual.fitness = breakdown.total;
            individual.path_hash = hash_branch_path(&result.branch_path);

            // Check for newly covered target branches.
            let new_hits = find_new_target_hits(&result.branch_path, &remaining_targets);
            if !new_hits.is_empty() {
                for &branch_id in &new_hits {
                    remaining_targets.remove(&branch_id);
                }

                discoveries.push(Behavior {
                    id: next_behavior_id,
                    input_args: individual.inputs.clone(),
                    return_value: result.return_value.clone(),
                    thrown_error: result.thrown_error.clone(),
                    branch_path: result.branch_path.clone(),
                    side_effects: result.side_effects.clone(),
                    dependency_trace: None,
                    mock_values: vec![],
                });
                next_behavior_id += 1;
            }
        }

        generations_run += 1;

        // Log progress.
        let solved = initial_target_count - remaining_targets.len();
        eprintln!(
            "[genetic] gen={} best_fitness={:.3} diversity={:.2} targets={}/{} executions={}",
            generations_run,
            population.best().map_or(0.0, |b| b.fitness),
            population.diversity(),
            solved,
            initial_target_count,
            total_executions,
        );

        if remaining_targets.is_empty() {
            break;
        }
        if Instant::now() >= deadline {
            break;
        }

        // Evolve to next generation.
        population.evolve(
            params,
            config.mutation_rate,
            config.crossover_rate,
            &[],
            &mut rng,
        );
    }

    let targets_solved = initial_target_count - remaining_targets.len();

    Ok(GeneticResult {
        discoveries,
        generations_run,
        total_executions,
        targets_solved,
    })
}

/// Execute a single individual's inputs via the frontend.
async fn execute_individual(
    frontend: &mut Frontend,
    function_name: &str,
    inputs: &[serde_json::Value],
) -> Result<ExecuteResult, FrontendError> {
    let response = frontend
        .send(Command::Execute {
            function: function_name.into(),
            inputs: inputs.to_vec(),
            mocks: vec![],
            setup_context: None,
            capture: true,
            prepare_id: None,
            execution_profile: None,
        })
        .await?;

    match response.result {
        ResponseResult::Execute(result) => Ok(*result),
        ResponseResult::Error {
            code,
            message,
            details,
        } => Err(FrontendError::Protocol {
            code,
            message,
            details,
        }),
        other => Err(FrontendError::Protocol {
            code: crate::protocol::ErrorCode::InvalidRequest,
            message: format!("unexpected execute response: {other:?}"),
            details: None,
        }),
    }
}

/// Find target branch IDs that appear in the execution's branch path.
fn find_new_target_hits(
    branch_path: &[BranchDecision],
    remaining_targets: &HashSet<u32>,
) -> Vec<u32> {
    let mut hits = Vec::new();
    let mut seen = HashSet::new();
    for decision in branch_path {
        if remaining_targets.contains(&decision.branch_id) && seen.insert(decision.branch_id) {
            hits.push(decision.branch_id);
        }
    }
    hits
}

/// Build a minimal `BehaviorMap` from seed inputs to initialize the population.
fn seed_behavior_map(
    function_id: &str,
    seed_inputs: &[Vec<serde_json::Value>],
) -> crate::behavior::BehaviorMap {
    let behaviors = seed_inputs
        .iter()
        .enumerate()
        .map(|(i, inputs)| Behavior {
            id: i as u32,
            input_args: inputs.clone(),
            return_value: None,
            thrown_error: None,
            branch_path: vec![],
            side_effects: vec![],
            dependency_trace: None,
            mock_values: vec![],
        })
        .collect();

    crate::behavior::BehaviorMap {
        function_id: function_id.into(),
        behaviors,
        fingerprint: None,
        nondeterministic_fields: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution_record::SymConstraint;

    fn make_branch(branch_id: u32, taken: bool) -> BranchDecision {
        BranchDecision {
            branch_id,
            line: branch_id * 10,
            taken,
            constraint: SymConstraint::Unknown {
                hint: "test".into(),
            },
            conditions: None,
        }
    }

    #[test]
    fn find_new_target_hits_finds_matching_branches() {
        let targets: HashSet<u32> = [1, 3, 5].into_iter().collect();
        let path = vec![make_branch(1, true), make_branch(2, true), make_branch(3, false)];
        let hits = find_new_target_hits(&path, &targets);
        assert_eq!(hits, vec![1, 3]);
    }

    #[test]
    fn find_new_target_hits_no_duplicates() {
        let targets: HashSet<u32> = [1].into_iter().collect();
        let path = vec![make_branch(1, true), make_branch(1, false)];
        let hits = find_new_target_hits(&path, &targets);
        assert_eq!(hits, vec![1]);
    }

    #[test]
    fn find_new_target_hits_empty_when_no_match() {
        let targets: HashSet<u32> = [10, 20].into_iter().collect();
        let path = vec![make_branch(1, true), make_branch(2, true)];
        let hits = find_new_target_hits(&path, &targets);
        assert!(hits.is_empty());
    }

    #[test]
    fn find_new_target_hits_empty_path() {
        let targets: HashSet<u32> = [1].into_iter().collect();
        let hits = find_new_target_hits(&[], &targets);
        assert!(hits.is_empty());
    }

    #[test]
    fn find_new_target_hits_empty_targets() {
        let path = vec![make_branch(1, true)];
        let hits = find_new_target_hits(&path, &HashSet::new());
        assert!(hits.is_empty());
    }

    #[test]
    fn seed_behavior_map_creates_behaviors_from_inputs() {
        let seeds = vec![vec![serde_json::json!(1)], vec![serde_json::json!("hello")]];
        let bmap = seed_behavior_map("test_fn", &seeds);
        assert_eq!(bmap.function_id, "test_fn");
        assert_eq!(bmap.behaviors.len(), 2);
        assert_eq!(bmap.behaviors[0].input_args, vec![serde_json::json!(1)]);
        assert_eq!(bmap.behaviors[1].input_args, vec![serde_json::json!("hello")]);
    }

    #[test]
    fn seed_behavior_map_empty_seeds() {
        let bmap = seed_behavior_map("f", &[]);
        assert!(bmap.behaviors.is_empty());
    }

    #[test]
    fn genetic_result_fields() {
        let result = GeneticResult {
            discoveries: vec![],
            generations_run: 5,
            total_executions: 250,
            targets_solved: 3,
        };
        assert_eq!(result.generations_run, 5);
        assert_eq!(result.total_executions, 250);
        assert_eq!(result.targets_solved, 3);
        assert!(result.discoveries.is_empty());
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use crate::execution_record::SymConstraint;
    use proptest::prelude::*;

    fn arb_branch_decision() -> impl Strategy<Value = BranchDecision> {
        (0u32..100, any::<bool>()).prop_map(|(id, taken)| BranchDecision {
            branch_id: id,
            line: id * 10,
            taken,
            constraint: SymConstraint::Unknown {
                hint: "test".into(),
            },
            conditions: None,
        })
    }

    proptest! {
        #[test]
        fn hits_are_subset_of_targets(
            target_ids in proptest::collection::hash_set(0u32..50, 0..20),
            path in proptest::collection::vec(arb_branch_decision(), 0..50),
        ) {
            let hits = find_new_target_hits(&path, &target_ids);
            for hit in &hits {
                prop_assert!(target_ids.contains(hit), "hit {hit} not in targets");
            }
        }

        #[test]
        fn hits_have_no_duplicates(
            target_ids in proptest::collection::hash_set(0u32..50, 0..20),
            path in proptest::collection::vec(arb_branch_decision(), 0..50),
        ) {
            let hits = find_new_target_hits(&path, &target_ids);
            let unique: HashSet<u32> = hits.iter().copied().collect();
            prop_assert_eq!(hits.len(), unique.len(), "duplicate hits found");
        }

        #[test]
        fn hits_are_subset_of_path_branch_ids(
            target_ids in proptest::collection::hash_set(0u32..50, 0..20),
            path in proptest::collection::vec(arb_branch_decision(), 0..50),
        ) {
            let path_ids: HashSet<u32> = path.iter().map(|d| d.branch_id).collect();
            let hits = find_new_target_hits(&path, &target_ids);
            for hit in &hits {
                prop_assert!(path_ids.contains(hit), "hit {hit} not in path");
            }
        }

        #[test]
        fn seed_behavior_map_preserves_count(
            n_seeds in 0usize..20,
        ) {
            let seeds: Vec<Vec<serde_json::Value>> = (0..n_seeds)
                .map(|i| vec![serde_json::json!(i)])
                .collect();
            let bmap = seed_behavior_map("f", &seeds);
            prop_assert_eq!(bmap.behaviors.len(), n_seeds);
        }

        #[test]
        fn seed_behavior_map_ids_are_sequential(
            n_seeds in 1usize..20,
        ) {
            let seeds: Vec<Vec<serde_json::Value>> = (0..n_seeds)
                .map(|i| vec![serde_json::json!(i)])
                .collect();
            let bmap = seed_behavior_map("f", &seeds);
            for (i, b) in bmap.behaviors.iter().enumerate() {
                prop_assert_eq!(b.id, i as u32);
            }
        }
    }
}
