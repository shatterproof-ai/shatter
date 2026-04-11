//! MC/DC (Modified Condition/Decision Coverage) data model.
//!
//! Tracks per-condition outcomes within compound decisions to determine
//! whether each condition independently affects the overall decision outcome.
//! MC/DC requires finding "independence pairs" for every condition: two
//! observations where condition i has opposite values, all other non-masked
//! conditions have the same values, and the decision outcome differs.

use std::collections::HashMap;

use crate::execution_record::ConditionOutcome;

/// A single observed truth row for a compound decision.
#[derive(Debug, Clone)]
pub struct McdcObservation {
    /// Truth values for each condition (None = masked by short-circuit).
    pub condition_values: Vec<Option<bool>>,
    /// The overall decision outcome for this observation.
    pub decision_outcome: bool,
}

/// MC/DC coverage tracking for a single compound decision.
#[derive(Debug, Clone)]
pub struct DecisionMcdc {
    /// Branch ID of the parent decision.
    pub branch_id: u32,
    /// Number of leaf conditions in this decision.
    pub num_conditions: usize,
    /// Observed truth rows.
    pub observations: Vec<McdcObservation>,
    /// Which conditions have independence pairs satisfied.
    /// Entry i is true once an independence pair for condition i is found.
    pub independent: Vec<bool>,
}

impl DecisionMcdc {
    fn new(branch_id: u32, num_conditions: usize) -> Self {
        Self {
            branch_id,
            num_conditions,
            observations: Vec::new(),
            independent: vec![false; num_conditions],
        }
    }

    /// Update the `independent` vec by searching for unique-cause masking
    /// MC/DC pairs among all observations recorded so far.
    ///
    /// For each condition i that is not yet independent, we search for two
    /// observations where:
    /// - Condition i has opposite concrete values (true vs false, neither masked).
    /// - Every other non-masked condition has the same value in both observations.
    /// - The overall decision outcome differs.
    pub fn check_independence(&mut self) {
        for i in 0..self.num_conditions {
            if self.independent[i] {
                continue;
            }
            'pair: for a in 0..self.observations.len() {
                for b in (a + 1)..self.observations.len() {
                    let obs_a = &self.observations[a];
                    let obs_b = &self.observations[b];

                    // Condition i must be observed in both rows with opposite values.
                    let val_a = match obs_a.condition_values.get(i).copied().flatten() {
                        Some(v) => v,
                        None => continue,
                    };
                    let val_b = match obs_b.condition_values.get(i).copied().flatten() {
                        Some(v) => v,
                        None => continue,
                    };
                    if val_a == val_b {
                        continue;
                    }

                    // Decision outcome must differ.
                    if obs_a.decision_outcome == obs_b.decision_outcome {
                        continue;
                    }

                    // All other non-masked conditions must agree between the two rows.
                    let mut all_others_agree = true;
                    for j in 0..self.num_conditions {
                        if j == i {
                            continue;
                        }
                        // If either row has this condition masked, skip it (masked
                        // conditions cannot violate the independence requirement).
                        let cond_a = obs_a.condition_values.get(j).copied().flatten();
                        let cond_b = obs_b.condition_values.get(j).copied().flatten();
                        match (cond_a, cond_b) {
                            (Some(ca), Some(cb)) if ca != cb => {
                                all_others_agree = false;
                                break;
                            }
                            _ => {}
                        }
                    }

                    if all_others_agree {
                        self.independent[i] = true;
                        break 'pair;
                    }
                }
            }
        }
    }
}

/// MC/DC state for an entire function, tracking all compound decisions.
#[derive(Debug, Clone, Default)]
pub struct McdcTable {
    /// Per-decision MC/DC tracking. Key is branch_id.
    pub decisions: HashMap<u32, DecisionMcdc>,
}

impl McdcTable {
    /// Record an observation row for a compound decision.
    ///
    /// If this is the first observation for `branch_id`, a new `DecisionMcdc`
    /// is initialized using the number of conditions in `conditions`. The
    /// independence check is run after each new observation.
    pub fn record_observation(
        &mut self,
        branch_id: u32,
        conditions: &[ConditionOutcome],
        decision_outcome: bool,
    ) {
        let num_conditions = conditions.len();
        let entry = self
            .decisions
            .entry(branch_id)
            .or_insert_with(|| DecisionMcdc::new(branch_id, num_conditions));

        // Build the truth row, placing each condition at its declared index.
        let mut row: Vec<Option<bool>> = vec![None; num_conditions.max(entry.num_conditions)];
        for co in conditions {
            let idx = co.condition_index as usize;
            if idx < row.len() {
                row[idx] = if co.masked { None } else { co.value };
            }
        }

        entry.observations.push(McdcObservation {
            condition_values: row,
            decision_outcome,
        });

        entry.check_independence();
    }

    /// Returns true when every condition in every decision has an independence pair.
    pub fn is_complete(&self) -> bool {
        self.decisions
            .values()
            .all(|d| d.independent.iter().all(|&v| v))
    }

    /// Returns `(total_conditions, independent_conditions, opaque_conditions)`.
    ///
    /// `opaque_conditions` counts conditions that appear in at least one observation
    /// but were always masked (never had a concrete value), making independence
    /// verification impossible.
    pub fn summary(&self) -> (usize, usize, usize) {
        let mut total = 0usize;
        let mut independent = 0usize;
        let mut opaque = 0usize;

        for decision in self.decisions.values() {
            total += decision.num_conditions;
            for i in 0..decision.num_conditions {
                if decision.independent[i] {
                    independent += 1;
                } else {
                    // A condition is opaque if it was always masked across all observations.
                    let always_masked = decision.observations.iter().all(|obs| {
                        obs.condition_values
                            .get(i)
                            .copied()
                            .map(|v| v.is_none())
                            .unwrap_or(true)
                    });
                    if always_masked && !decision.observations.is_empty() {
                        opaque += 1;
                    }
                }
            }
        }

        (total, independent, opaque)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution_record::{ConditionOutcome, SymConstraint};

    fn make_condition(index: u32, value: Option<bool>, masked: bool) -> ConditionOutcome {
        ConditionOutcome {
            condition_index: index,
            value,
            masked,
            constraint: SymConstraint::Unknown {
                hint: String::new(),
            },
        }
    }

    // --- Basic observation recording ---

    #[test]
    fn new_table_is_empty_and_incomplete() {
        let table = McdcTable::default();
        assert!(table.decisions.is_empty());
        // Empty table: vacuously complete (no decisions to satisfy).
        assert!(table.is_complete());
        assert_eq!(table.summary(), (0, 0, 0));
    }

    #[test]
    fn single_observation_does_not_satisfy_independence() {
        let mut table = McdcTable::default();
        let conditions = vec![
            make_condition(0, Some(true), false),
            make_condition(1, Some(false), false),
        ];
        table.record_observation(0, &conditions, true);

        assert!(!table.is_complete());
        let (total, independent, _opaque) = table.summary();
        assert_eq!(total, 2);
        assert_eq!(independent, 0);
    }

    // --- Independence detection ---

    #[test]
    fn two_observations_with_one_condition_differing_satisfies_independence() {
        // Decision: A && B
        // Obs 1: A=T, B=T → outcome=T
        // Obs 2: A=F, B=T → outcome=F
        // Condition 0 (A) has independence pair (obs1, obs2): A flips, B stays, outcome flips.
        // Condition 1 (B) still needs a pair.
        let mut table = McdcTable::default();
        table.record_observation(
            0,
            &[
                make_condition(0, Some(true), false),
                make_condition(1, Some(true), false),
            ],
            true,
        );
        table.record_observation(
            0,
            &[
                make_condition(0, Some(false), false),
                make_condition(1, Some(true), false),
            ],
            false,
        );

        let (total, independent, _) = table.summary();
        assert_eq!(total, 2);
        assert_eq!(independent, 1);

        let decision = &table.decisions[&0];
        assert!(decision.independent[0], "condition 0 should be independent");
        assert!(!decision.independent[1], "condition 1 not yet independent");
    }

    #[test]
    fn full_mcdc_for_two_condition_and() {
        // A && B: three observations satisfy MC/DC for both conditions.
        // Obs 1: A=T, B=T → T  (baseline)
        // Obs 2: A=F, B=T → F  (independence pair for A)
        // Obs 3: A=T, B=F → F  (independence pair for B)
        let mut table = McdcTable::default();
        table.record_observation(
            1,
            &[
                make_condition(0, Some(true), false),
                make_condition(1, Some(true), false),
            ],
            true,
        );
        table.record_observation(
            1,
            &[
                make_condition(0, Some(false), false),
                make_condition(1, Some(true), false),
            ],
            false,
        );
        table.record_observation(
            1,
            &[
                make_condition(0, Some(true), false),
                make_condition(1, Some(false), false),
            ],
            false,
        );

        assert!(
            table.is_complete(),
            "all conditions should have independence pairs"
        );
        let (total, independent, opaque) = table.summary();
        assert_eq!(total, 2);
        assert_eq!(independent, 2);
        assert_eq!(opaque, 0);
    }

    #[test]
    fn masked_condition_counted_as_opaque_when_always_masked() {
        // Condition 1 is always masked → opaque.
        let mut table = McdcTable::default();
        table.record_observation(
            2,
            &[
                make_condition(0, Some(true), false),
                make_condition(1, None, true),
            ],
            true,
        );
        table.record_observation(
            2,
            &[
                make_condition(0, Some(false), false),
                make_condition(1, None, true),
            ],
            false,
        );

        let (total, independent, opaque) = table.summary();
        assert_eq!(total, 2);
        assert_eq!(independent, 1); // condition 0 has a pair
        assert_eq!(opaque, 1); // condition 1 always masked
    }

    // --- Monotonicity property: independence never decreases ---

    #[test]
    fn independence_is_monotonic() {
        // Adding more observations should never reduce the independent count.
        let mut table = McdcTable::default();
        let mut prev_independent = 0usize;

        type ConditionSpec = (u32, Option<bool>, bool);
        let observations: &[(&[ConditionSpec], bool)] = &[
            (&[(0, Some(true), false), (1, Some(true), false)], true),
            (&[(0, Some(false), false), (1, Some(true), false)], false),
            (&[(0, Some(true), false), (1, Some(false), false)], false),
            (&[(0, Some(false), false), (1, Some(false), false)], false),
        ];

        for (conds, outcome) in observations {
            let condition_outcomes: Vec<ConditionOutcome> = conds
                .iter()
                .map(|(idx, val, masked)| make_condition(*idx, *val, *masked))
                .collect();
            table.record_observation(0, &condition_outcomes, *outcome);
            let (_, independent, _) = table.summary();
            assert!(
                independent >= prev_independent,
                "independence count decreased from {prev_independent} to {independent}"
            );
            prev_independent = independent;
        }
    }

    // --- Multiple decisions in a single table ---

    #[test]
    fn multiple_decisions_tracked_independently() {
        let mut table = McdcTable::default();

        // Decision branch_id=0: achieve independence for condition 0.
        table.record_observation(
            0,
            &[
                make_condition(0, Some(true), false),
                make_condition(1, Some(true), false),
            ],
            true,
        );
        table.record_observation(
            0,
            &[
                make_condition(0, Some(false), false),
                make_condition(1, Some(true), false),
            ],
            false,
        );

        // Decision branch_id=1: only one observation so far.
        table.record_observation(1, &[make_condition(0, Some(true), false)], true);

        assert!(!table.is_complete(), "branch 1 is not complete yet");
        assert_eq!(table.decisions.len(), 2);
    }

    // --- Summary correctness ---

    #[test]
    fn summary_aggregates_across_decisions() {
        let mut table = McdcTable::default();

        // Decision 0: 2 conditions, both independent after 3 observations.
        table.record_observation(
            0,
            &[
                make_condition(0, Some(true), false),
                make_condition(1, Some(true), false),
            ],
            true,
        );
        table.record_observation(
            0,
            &[
                make_condition(0, Some(false), false),
                make_condition(1, Some(true), false),
            ],
            false,
        );
        table.record_observation(
            0,
            &[
                make_condition(0, Some(true), false),
                make_condition(1, Some(false), false),
            ],
            false,
        );

        // Decision 1: 1 condition, not yet independent.
        table.record_observation(1, &[make_condition(0, Some(true), false)], true);

        let (total, independent, opaque) = table.summary();
        assert_eq!(total, 3); // 2 from decision 0 + 1 from decision 1
        assert_eq!(independent, 2); // only decision 0's conditions
        assert_eq!(opaque, 0);
    }

    // --- Proptest: monotonicity invariant ---

    #[cfg(test)]
    mod proptest_tests {
        use super::*;
        use proptest::prelude::*;

        fn arb_observation(
            num_conditions: usize,
        ) -> impl Strategy<Value = (Vec<ConditionOutcome>, bool)> {
            let conds = (0..num_conditions)
                .map(|i| {
                    prop::bool::ANY.prop_flat_map(move |masked| {
                        if masked {
                            Just(make_condition(i as u32, None, true)).boxed()
                        } else {
                            prop::bool::ANY
                                .prop_map(move |v| make_condition(i as u32, Some(v), false))
                                .boxed()
                        }
                    })
                })
                .collect::<Vec<_>>();
            (conds, prop::bool::ANY)
        }

        proptest! {
            #[test]
            fn independence_count_never_decreases(
                observations in prop::collection::vec(
                    arb_observation(2),
                    1..=10,
                )
            ) {
                let mut table = McdcTable::default();
                let mut prev_independent = 0usize;

                for (conds, outcome) in observations {
                    table.record_observation(0, &conds, outcome);
                    let (_, independent, _) = table.summary();
                    prop_assert!(
                        independent >= prev_independent,
                        "independence decreased: {} -> {}",
                        prev_independent,
                        independent
                    );
                    prev_independent = independent;
                }
            }

            #[test]
            fn summary_total_equals_sum_of_decision_conditions(
                branch_ids in prop::collection::vec(0u32..5, 1..=8),
                num_conds in 1usize..=3,
            ) {
                let mut table = McdcTable::default();
                for bid in &branch_ids {
                    let conds: Vec<ConditionOutcome> = (0..num_conds)
                        .map(|i| make_condition(i as u32, Some(true), false))
                        .collect();
                    table.record_observation(*bid, &conds, true);
                }
                let (total, independent, opaque) = table.summary();
                let expected_total: usize = table.decisions.values().map(|d| d.num_conditions).sum();
                prop_assert_eq!(total, expected_total);
                prop_assert!(independent + opaque <= total);
            }
        }
    }
}
