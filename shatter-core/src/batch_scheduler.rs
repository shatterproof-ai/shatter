//! Round-robin batch scheduler for multi-function exploration.
//!
//! When exploring many functions, the scheduler assigns each function a
//! fixed-size iteration budget (one "batch") before moving to the next.
//! Functions that aren't fully explored after their batch are re-enqueued
//! at the tail, producing a round-robin traversal.
//!
//! The batch size is an internal tuning parameter — it is not exposed
//! through the user-facing CLI.

use std::collections::VecDeque;

/// Default number of iterations per batch.
pub const DEFAULT_BATCH_SIZE: u32 = 50;

/// Configuration for one batch of exploration, returned by
/// [`BatchScheduler::next_batch`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchConfig {
    /// Index into the caller's original task list.
    pub task_index: usize,
    /// Maximum iterations to run in this batch.
    pub batch_size: u32,
    /// How many batches this function has already completed (0-indexed).
    pub batch_number: u32,
}

/// Outcome of one batch, passed to [`BatchScheduler::record_outcome`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchOutcome {
    /// Index matching the [`BatchConfig::task_index`] that produced this outcome.
    pub task_index: usize,
    /// Actual iterations consumed (may be less than the batch budget).
    pub iterations_used: u32,
    /// True when the function is fully explored or its total budget is spent.
    /// The scheduler will not re-enqueue exhausted functions.
    pub exhausted: bool,
}

/// Internal queue entry tracking per-function state.
#[derive(Debug, Clone)]
struct Entry {
    task_index: usize,
    /// Remaining iteration budget. `None` means unbounded.
    remaining: Option<u32>,
    batches_completed: u32,
}

/// Round-robin batch scheduler.
///
/// Assigns fixed-size iteration batches to functions one at a time.
/// After each batch the caller reports an outcome; non-exhausted functions
/// are re-enqueued at the tail for another round.
#[derive(Debug)]
pub struct BatchScheduler {
    queue: VecDeque<Entry>,
    batch_size: u32,
    /// The entry currently being explored (popped by `next_batch`,
    /// consumed by `record_outcome`).
    active: Option<Entry>,
}

impl BatchScheduler {
    /// Create a scheduler for `task_count` functions.
    ///
    /// `per_function_budget` is the total iteration budget each function
    /// may consume across all its batches. `None` means unbounded — the
    /// function will keep being re-enqueued until explicitly marked
    /// exhausted by the caller.
    ///
    /// `batch_size` is the maximum iterations per batch.
    pub fn new(task_count: usize, per_function_budget: Option<u32>, batch_size: u32) -> Self {
        let queue = (0..task_count)
            .map(|i| Entry {
                task_index: i,
                remaining: per_function_budget,
                batches_completed: 0,
            })
            .collect();
        Self {
            queue,
            batch_size,
            active: None,
        }
    }

    /// Create a scheduler where each function has its own iteration budget.
    ///
    /// `budgets[i]` is the total budget for task `i`. `None` means unbounded.
    pub fn with_individual_budgets(budgets: &[Option<u32>], batch_size: u32) -> Self {
        let queue = budgets
            .iter()
            .enumerate()
            .map(|(i, &budget)| Entry {
                task_index: i,
                remaining: budget,
                batches_completed: 0,
            })
            .collect();
        Self {
            queue,
            batch_size,
            active: None,
        }
    }

    /// Pop the next function and return its batch configuration.
    ///
    /// Returns `None` when the queue is empty (all functions exhausted).
    ///
    /// # Panics
    ///
    /// Panics if called while a previous batch has not been recorded via
    /// [`record_outcome`](Self::record_outcome).
    pub fn next_batch(&mut self) -> Option<BatchConfig> {
        assert!(
            self.active.is_none(),
            "previous batch must be recorded before requesting the next"
        );

        // Skip entries that have zero remaining budget.
        while let Some(entry) = self.queue.pop_front() {
            if entry.remaining == Some(0) {
                continue;
            }
            let batch_iters = match entry.remaining {
                Some(r) => r.min(self.batch_size),
                None => self.batch_size,
            };
            let config = BatchConfig {
                task_index: entry.task_index,
                batch_size: batch_iters,
                batch_number: entry.batches_completed,
            };
            self.active = Some(entry);
            return Some(config);
        }
        None
    }

    /// Record the outcome of the current batch.
    ///
    /// If the function is not exhausted, it is re-enqueued at the tail
    /// with its remaining budget reduced by `iterations_used`.
    ///
    /// # Panics
    ///
    /// Panics if no batch is active (i.e., `next_batch` was not called
    /// or the outcome was already recorded).
    pub fn record_outcome(&mut self, outcome: BatchOutcome) {
        let mut entry = self
            .active
            .take()
            .expect("record_outcome called without an active batch");
        debug_assert_eq!(entry.task_index, outcome.task_index);

        entry.batches_completed += 1;

        if outcome.exhausted {
            return; // drop entry — function is done
        }

        // Deduct used iterations from the remaining budget.
        if let Some(ref mut r) = entry.remaining {
            *r = r.saturating_sub(outcome.iterations_used);
            if *r == 0 {
                return; // budget spent — don't re-enqueue
            }
        }

        self.queue.push_back(entry);
    }

    /// Returns `true` when all functions have been exhausted and there is
    /// no active batch in flight.
    pub fn is_complete(&self) -> bool {
        self.active.is_none() && self.queue.is_empty()
    }

    /// Number of functions still in the queue (excludes the active batch).
    pub fn pending_count(&self) -> usize {
        self.queue.len()
    }

    /// Configured batch size.
    pub fn batch_size(&self) -> u32 {
        self.batch_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_scheduler_returns_none() {
        let mut s = BatchScheduler::new(0, Some(100), 50);
        assert!(s.next_batch().is_none());
        assert!(s.is_complete());
    }

    #[test]
    fn single_function_single_batch() {
        let mut s = BatchScheduler::new(1, Some(30), 50);
        let b = s.next_batch().unwrap();
        assert_eq!(b.task_index, 0);
        // Budget (30) < batch_size (50), so batch_size is clamped.
        assert_eq!(b.batch_size, 30);
        assert_eq!(b.batch_number, 0);

        s.record_outcome(BatchOutcome {
            task_index: 0,
            iterations_used: 30,
            exhausted: true,
        });
        assert!(s.next_batch().is_none());
        assert!(s.is_complete());
    }

    #[test]
    fn round_robin_ordering() {
        let mut s = BatchScheduler::new(3, Some(100), 50);
        let mut order = Vec::new();

        // First round: 0, 1, 2
        for _ in 0..3 {
            let b = s.next_batch().unwrap();
            order.push(b.task_index);
            s.record_outcome(BatchOutcome {
                task_index: b.task_index,
                iterations_used: 50,
                exhausted: false,
            });
        }
        assert_eq!(order, vec![0, 1, 2]);

        // Second round: 0, 1, 2 again
        order.clear();
        for _ in 0..3 {
            let b = s.next_batch().unwrap();
            order.push(b.task_index);
            s.record_outcome(BatchOutcome {
                task_index: b.task_index,
                iterations_used: 50,
                exhausted: true, // exhaust all on second round
            });
        }
        assert_eq!(order, vec![0, 1, 2]);
        assert!(s.is_complete());
    }

    #[test]
    fn budget_exhaustion_removes_function() {
        // Budget = 80, batch = 50. First batch uses 50, leaving 30.
        // Second batch gets 30 (clamped), then budget is 0.
        let mut s = BatchScheduler::new(1, Some(80), 50);

        let b1 = s.next_batch().unwrap();
        assert_eq!(b1.batch_size, 50);
        s.record_outcome(BatchOutcome {
            task_index: 0,
            iterations_used: 50,
            exhausted: false,
        });

        let b2 = s.next_batch().unwrap();
        assert_eq!(b2.batch_size, 30);
        assert_eq!(b2.batch_number, 1);
        s.record_outcome(BatchOutcome {
            task_index: 0,
            iterations_used: 30,
            exhausted: false, // caller says not exhausted, but budget is now 0
        });

        // Budget spent — scheduler should not re-enqueue.
        assert!(s.next_batch().is_none());
        assert!(s.is_complete());
    }

    #[test]
    fn unbounded_budget_keeps_cycling() {
        let mut s = BatchScheduler::new(2, None, 10);

        // Run 6 batches — should cycle: 0,1,0,1,0,1
        let mut order = Vec::new();
        for i in 0..6 {
            let b = s.next_batch().unwrap();
            order.push(b.task_index);
            s.record_outcome(BatchOutcome {
                task_index: b.task_index,
                iterations_used: 10,
                // Exhaust both on the 3rd round.
                exhausted: i >= 4,
            });
        }
        assert_eq!(order, vec![0, 1, 0, 1, 0, 1]);
        assert!(s.is_complete());
    }

    #[test]
    fn batch_number_increments() {
        let mut s = BatchScheduler::new(1, None, 10);
        for expected in 0..5 {
            let b = s.next_batch().unwrap();
            assert_eq!(b.batch_number, expected);
            s.record_outcome(BatchOutcome {
                task_index: 0,
                iterations_used: 10,
                exhausted: expected == 4,
            });
        }
    }

    #[test]
    fn partial_iteration_use() {
        // Budget = 100, batch = 50, but function only uses 20 per batch.
        let mut s = BatchScheduler::new(1, Some(100), 50);

        let b = s.next_batch().unwrap();
        assert_eq!(b.batch_size, 50);
        s.record_outcome(BatchOutcome {
            task_index: 0,
            iterations_used: 20,
            exhausted: false,
        });
        // Remaining = 100 - 20 = 80. Next batch = min(80, 50) = 50.
        let b2 = s.next_batch().unwrap();
        assert_eq!(b2.batch_size, 50);
        assert!(!s.is_complete());

        s.record_outcome(BatchOutcome {
            task_index: 0,
            iterations_used: 50,
            exhausted: true,
        });
        assert!(s.is_complete());
    }

    #[test]
    fn pending_count_tracks_queue() {
        let mut s = BatchScheduler::new(3, Some(50), 50);
        assert_eq!(s.pending_count(), 3);

        let b = s.next_batch().unwrap();
        assert_eq!(s.pending_count(), 2); // one is active, not pending

        s.record_outcome(BatchOutcome {
            task_index: b.task_index,
            iterations_used: 50,
            exhausted: true,
        });
        assert_eq!(s.pending_count(), 2); // exhausted, not re-enqueued
    }

    #[test]
    #[should_panic(expected = "previous batch must be recorded")]
    fn next_batch_panics_without_recording() {
        let mut s = BatchScheduler::new(2, Some(100), 50);
        let _ = s.next_batch().unwrap();
        let _ = s.next_batch(); // should panic
    }

    #[test]
    #[should_panic(expected = "record_outcome called without an active batch")]
    fn record_outcome_panics_without_active() {
        let mut s = BatchScheduler::new(1, Some(100), 50);
        s.record_outcome(BatchOutcome {
            task_index: 0,
            iterations_used: 10,
            exhausted: false,
        });
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// Total iterations assigned across all batches never exceeds the
        /// sum of per-function budgets.
        #[test]
        fn total_iterations_within_budget(
            task_count in 1_usize..10,
            budget in 1_u32..500,
            batch_size in 1_u32..200,
        ) {
            let mut scheduler = BatchScheduler::new(task_count, Some(budget), batch_size);
            let mut total_assigned: u64 = 0;

            while let Some(config) = scheduler.next_batch() {
                total_assigned += config.batch_size as u64;
                scheduler.record_outcome(BatchOutcome {
                    task_index: config.task_index,
                    iterations_used: config.batch_size,
                    exhausted: false,
                });
            }

            let max_total = task_count as u64 * budget as u64;
            prop_assert!(
                total_assigned <= max_total,
                "assigned {} > max {} (tasks={}, budget={}, batch={})",
                total_assigned, max_total, task_count, budget, batch_size,
            );
        }

        /// Every task index appears at least once in the batch sequence.
        #[test]
        fn every_task_appears(
            task_count in 1_usize..10,
            budget in 1_u32..500,
            batch_size in 1_u32..200,
        ) {
            let mut scheduler = BatchScheduler::new(task_count, Some(budget), batch_size);
            let mut seen = std::collections::HashSet::new();

            while let Some(config) = scheduler.next_batch() {
                seen.insert(config.task_index);
                scheduler.record_outcome(BatchOutcome {
                    task_index: config.task_index,
                    iterations_used: config.batch_size,
                    exhausted: false,
                });
            }

            for i in 0..task_count {
                prop_assert!(seen.contains(&i), "task {} never scheduled", i);
            }
        }

        /// Batch sizes never exceed the configured batch_size.
        #[test]
        fn batch_size_capped(
            task_count in 1_usize..10,
            budget in 1_u32..500,
            batch_size in 1_u32..200,
        ) {
            let mut scheduler = BatchScheduler::new(task_count, Some(budget), batch_size);

            while let Some(config) = scheduler.next_batch() {
                prop_assert!(
                    config.batch_size <= batch_size,
                    "batch_size {} > cap {}",
                    config.batch_size, batch_size,
                );
                scheduler.record_outcome(BatchOutcome {
                    task_index: config.task_index,
                    iterations_used: config.batch_size,
                    exhausted: false,
                });
            }
        }

        /// No task appears after being marked exhausted.
        #[test]
        fn no_appearance_after_exhaustion(
            task_count in 1_usize..5,
            batch_size in 1_u32..50,
        ) {
            let mut scheduler = BatchScheduler::new(task_count, None, batch_size);
            let mut exhausted_set = std::collections::HashSet::new();
            let max_rounds = task_count * 4;
            let mut rounds = 0;

            while let Some(config) = scheduler.next_batch() {
                prop_assert!(
                    !exhausted_set.contains(&config.task_index),
                    "task {} appeared after exhaustion",
                    config.task_index,
                );
                // Exhaust tasks randomly based on batch_number.
                let exhaust = config.batch_number >= 2;
                if exhaust {
                    exhausted_set.insert(config.task_index);
                }
                scheduler.record_outcome(BatchOutcome {
                    task_index: config.task_index,
                    iterations_used: batch_size,
                    exhausted: exhaust,
                });
                rounds += 1;
                if rounds > max_rounds * 3 {
                    break; // safety net for unbounded
                }
            }
        }
    }
}
