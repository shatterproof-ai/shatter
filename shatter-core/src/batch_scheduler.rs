//! Rank-ordered batch scheduler for multi-function exploration.
//!
//! When exploring many functions, the scheduler assigns each function a
//! fixed-size iteration budget (one "batch") before moving to the next.
//! Functions that aren't fully explored after their batch are re-enqueued
//! along with a caller-supplied `rank` that describes how productive the
//! most recent batch was. The next [`BatchScheduler::next_batch`] call
//! picks whichever queued entry has the highest rank, breaking ties by
//! earliest insertion order. The same function may therefore be chosen
//! again back-to-back if it still ranks highest; once its rank drops to
//! tie with its peers, standard FIFO round-robin resumes.
//!
//! With initial ranks at 0 and callers that always pass `rank: 0`, the
//! scheduler degenerates to a strict round-robin (ties keep insertion
//! order) — the str-b2my.6 round-robin semantics are preserved as the
//! rank-0 special case.
//!
//! The batch size is an internal tuning parameter — it is not exposed
//! through the user-facing CLI.

use std::collections::{HashMap, VecDeque};

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
    /// Caller-supplied ranking score for this outcome. Higher ranks are
    /// picked first on the next [`BatchScheduler::next_batch`] call; ties
    /// fall back to insertion order (FIFO). The scheduler never inspects
    /// the magnitude of the score — the caller owns the metric. Typical
    /// use: "number of new branches discovered in this batch" so that a
    /// function on a discovery streak continues to be scheduled while
    /// functions that stop producing new work fall behind.
    pub rank: i64,
}

/// Internal queue entry tracking per-function state.
#[derive(Debug, Clone)]
struct Entry {
    task_index: usize,
    /// Remaining iteration budget. `None` means unbounded.
    remaining: Option<u32>,
    batches_completed: u32,
    /// Current ranking score — set on insertion (initial 0) and replaced
    /// on every re-enqueue via [`BatchScheduler::record_outcome`]. Used
    /// by [`BatchScheduler::next_batch`] to select the highest-ranked
    /// pending entry.
    rank: i64,
}

/// Rank-ordered batch scheduler.
///
/// Assigns fixed-size iteration batches to functions. Callers may request
/// multiple batches concurrently (one per function) and record their
/// outcomes in any order; non-exhausted functions are re-enqueued with
/// the outcome's rank and the next pick is whichever queued entry has
/// the highest rank (FIFO tie-break). When every outcome is reported
/// with `rank: 0` the scheduler collapses to a strict round-robin.
#[derive(Debug)]
pub struct BatchScheduler {
    queue: VecDeque<Entry>,
    batch_size: u32,
    /// Entries popped by [`next_batch`] but not yet resolved via
    /// [`record_outcome`], keyed by `task_index`.
    in_flight: HashMap<usize, Entry>,
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
                rank: 0,
            })
            .collect();
        Self {
            queue,
            batch_size,
            in_flight: HashMap::new(),
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
                rank: 0,
            })
            .collect();
        Self {
            queue,
            batch_size,
            in_flight: HashMap::new(),
        }
    }

    /// Pick the highest-ranked pending function and return its batch
    /// configuration.
    ///
    /// Returns `None` when the queue is empty. Selection semantics:
    ///
    /// 1. Entries with zero remaining budget are dropped from the queue
    ///    before selection — they will never be scheduled again.
    /// 2. Among the surviving entries, the one with the largest `rank`
    ///    is chosen. Ties resolve to the entry with the earliest queue
    ///    position (FIFO), so a stream of equal ranks produces a strict
    ///    round-robin.
    ///
    /// Unlike a strict single-slot scheduler, multiple calls without
    /// intervening [`record_outcome`] calls are allowed: each returned
    /// batch is tracked in the in-flight set until the matching outcome
    /// is recorded. The scheduler never returns the same `task_index`
    /// twice while it is in flight, because entries are only re-added
    /// via [`record_outcome`].
    pub fn next_batch(&mut self) -> Option<BatchConfig> {
        // Drop any entries whose budget has been spent. They are inert
        // but still occupy a queue slot until we compact them out.
        self.queue.retain(|e| e.remaining != Some(0));

        // Linear scan for the highest-ranked entry. `>` (strictly greater)
        // keeps the earliest index on ties, yielding stable FIFO tie-break.
        let best = self.queue.iter().enumerate().fold(
            None,
            |acc: Option<(usize, i64)>, (i, e)| match acc {
                None => Some((i, e.rank)),
                Some((_, best_rank)) if e.rank > best_rank => Some((i, e.rank)),
                Some(prev) => Some(prev),
            },
        )?;

        let entry = self.queue.remove(best.0)?;
        let batch_iters = match entry.remaining {
            Some(r) => r.min(self.batch_size),
            None => self.batch_size,
        };
        let config = BatchConfig {
            task_index: entry.task_index,
            batch_size: batch_iters,
            batch_number: entry.batches_completed,
        };
        self.in_flight.insert(entry.task_index, entry);
        Some(config)
    }

    /// Record the outcome of an in-flight batch.
    ///
    /// If the function is not exhausted, it is re-enqueued at the tail
    /// of the queue with its remaining budget reduced by `iterations_used`
    /// and its stored rank replaced by `outcome.rank`. The next call to
    /// [`next_batch`] will re-select based on the updated rank, so the
    /// same function may be picked again back-to-back if it still ranks
    /// highest.
    ///
    /// # Panics
    ///
    /// Panics if `outcome.task_index` does not correspond to an in-flight
    /// batch (i.e., `next_batch` was not called for this index, or the
    /// outcome was already recorded).
    pub fn record_outcome(&mut self, outcome: BatchOutcome) {
        let mut entry = self
            .in_flight
            .remove(&outcome.task_index)
            .expect("record_outcome called for a task_index that is not in flight");

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

        // Replace the stored rank with the outcome's. Rank is not
        // accumulated across batches: each batch reports its own score
        // and the scheduler uses only the latest signal.
        entry.rank = outcome.rank;
        self.queue.push_back(entry);
    }

    /// Returns `true` when all functions have been exhausted and there is
    /// no batch in flight.
    pub fn is_complete(&self) -> bool {
        self.in_flight.is_empty() && self.queue.is_empty()
    }

    /// Number of functions still in the queue (excludes in-flight batches).
    pub fn pending_count(&self) -> usize {
        self.queue.len()
    }

    /// Number of batches currently in flight.
    pub fn in_flight_count(&self) -> usize {
        self.in_flight.len()
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
            rank: 0,
        });
        assert!(s.next_batch().is_none());
        assert!(s.is_complete());
    }

    #[test]
    fn reenqueue_path_interleaves_two_functions() {
        // Narrow test of the re-enqueue state machine requested in str-b2my.6
        // review: the single behavior separating round-robin batching from
        // "one batch per function" degenerate mode is that a task reported
        // with `exhausted: false` must be re-queued and must NOT starve the
        // other tasks. Two unbounded functions, batch=50:
        //
        //   pop → A (task 0)
        //   record_outcome(A, exhausted: false)
        //   pop → B (task 1)   // A must not monopolise; B runs next
        //   record_outcome(B, exhausted: true)   // B done
        //   pop → A            // A re-queued and returns now that B is gone
        //   record_outcome(A, exhausted: true)
        //   pop → None
        let mut s = BatchScheduler::new(2, None, 50);

        let b1 = s.next_batch().expect("first batch");
        assert_eq!(b1.task_index, 0, "first batch should be task 0");
        assert_eq!(b1.batch_number, 0);
        s.record_outcome(BatchOutcome {
            task_index: 0,
            iterations_used: 50,
            exhausted: false,
            rank: 0,
        });

        let b2 = s.next_batch().expect("second batch");
        assert_eq!(
            b2.task_index, 1,
            "task 0 must yield to task 1 after exhausted:false"
        );
        s.record_outcome(BatchOutcome {
            task_index: 1,
            iterations_used: 50,
            exhausted: true,
            rank: 0,
        });

        let b3 = s.next_batch().expect("task 0 should be re-enqueued");
        assert_eq!(b3.task_index, 0, "re-enqueued task 0 must reappear");
        assert_eq!(b3.batch_number, 1, "batch_number advances on re-enqueue");
        s.record_outcome(BatchOutcome {
            task_index: 0,
            iterations_used: 50,
            exhausted: true,
            rank: 0,
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
                rank: 0,
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
                rank: 0,
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
            rank: 0,
        });

        let b2 = s.next_batch().unwrap();
        assert_eq!(b2.batch_size, 30);
        assert_eq!(b2.batch_number, 1);
        s.record_outcome(BatchOutcome {
            task_index: 0,
            iterations_used: 30,
            exhausted: false, // caller says not exhausted, but budget is now 0
            rank: 0,
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
                rank: 0,
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
                rank: 0,
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
            rank: 0,
        });
        // Remaining = 100 - 20 = 80. Next batch = min(80, 50) = 50.
        let b2 = s.next_batch().unwrap();
        assert_eq!(b2.batch_size, 50);
        assert!(!s.is_complete());

        s.record_outcome(BatchOutcome {
            task_index: 0,
            iterations_used: 50,
            exhausted: true,
            rank: 0,
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
            rank: 0,
        });
        assert_eq!(s.pending_count(), 2); // exhausted, not re-enqueued
    }

    #[test]
    fn concurrent_in_flight_batches() {
        let mut s = BatchScheduler::new(3, Some(100), 50);

        let b0 = s.next_batch().unwrap();
        let b1 = s.next_batch().unwrap();
        let b2 = s.next_batch().unwrap();
        assert_eq!(b0.task_index, 0);
        assert_eq!(b1.task_index, 1);
        assert_eq!(b2.task_index, 2);
        assert_eq!(s.in_flight_count(), 3);
        assert_eq!(s.pending_count(), 0);
        assert!(s.next_batch().is_none());

        // Record in reverse order — non-sequential completion is allowed.
        s.record_outcome(BatchOutcome {
            task_index: 2,
            iterations_used: 50,
            exhausted: false,
            rank: 0,
        });
        s.record_outcome(BatchOutcome {
            task_index: 0,
            iterations_used: 50,
            exhausted: true,
            rank: 0,
        });
        s.record_outcome(BatchOutcome {
            task_index: 1,
            iterations_used: 50,
            exhausted: false,
            rank: 0,
        });

        assert_eq!(s.in_flight_count(), 0);
        // 0 was exhausted; 1 and 2 re-enqueued. Round order: 2 then 1.
        let next = s.next_batch().unwrap();
        assert_eq!(next.task_index, 2);
    }

    #[test]
    fn higher_rank_wins_next_pick() {
        // str-b2my.7 regression: after a batch finishes, the scheduler
        // must re-rank the queue and pick the highest-scored entry next,
        // which may be the same task back-to-back rather than the next
        // round-robin slot. Two unbounded tasks, batch=50.
        let mut s = BatchScheduler::new(2, None, 50);

        // Both start at rank 0 → FIFO → task 0 wins the first pick.
        let b = s.next_batch().expect("first batch");
        assert_eq!(b.task_index, 0);

        // Task 0 reports a highly productive batch. It re-enters the
        // queue with rank 5; task 1 is still at rank 0.
        s.record_outcome(BatchOutcome {
            task_index: 0,
            iterations_used: 50,
            exhausted: false,
            rank: 5,
        });

        // Next pick: task 0 again, because its stored rank (5) beats
        // task 1's initial rank (0) — even though task 1 has never run.
        let b = s.next_batch().expect("second batch");
        assert_eq!(
            b.task_index, 0,
            "rank-5 task 0 should be picked back-to-back over rank-0 task 1"
        );
        assert_eq!(
            b.batch_number, 1,
            "back-to-back picks still advance batch_number"
        );

        // Task 0's streak ends — rank drops to 0. Task 1 now ties at 0
        // and wins via FIFO tie-break (earliest queue position).
        s.record_outcome(BatchOutcome {
            task_index: 0,
            iterations_used: 50,
            exhausted: false,
            rank: 0,
        });
        let b = s.next_batch().expect("third batch");
        assert_eq!(
            b.task_index, 1,
            "after task 0's rank drops to 0, FIFO tie-break yields to task 1"
        );

        // Task 1 converges early — dropped. Task 0 is the only survivor.
        s.record_outcome(BatchOutcome {
            task_index: 1,
            iterations_used: 50,
            exhausted: true,
            rank: 0,
        });
        let b = s.next_batch().expect("fourth batch");
        assert_eq!(b.task_index, 0);
        s.record_outcome(BatchOutcome {
            task_index: 0,
            iterations_used: 50,
            exhausted: true,
            rank: 0,
        });
        assert!(s.next_batch().is_none());
        assert!(s.is_complete());
    }

    #[test]
    fn rerank_replaces_stored_rank_not_accumulates() {
        // A task's stored rank must be the *latest* outcome's rank, not
        // a running sum. Otherwise a task that scored highly once would
        // starve its peers forever even after becoming unproductive.
        let mut s = BatchScheduler::new(2, None, 50);

        // Pop both tasks so neither sits in the queue during priming.
        let a = s.next_batch().unwrap();
        let b = s.next_batch().unwrap();
        assert_eq!(a.task_index, 0);
        assert_eq!(b.task_index, 1);

        // Prime task 0 to a very high rank, task 1 to a moderate rank.
        s.record_outcome(BatchOutcome {
            task_index: 0,
            iterations_used: 50,
            exhausted: false,
            rank: 100,
        });
        s.record_outcome(BatchOutcome {
            task_index: 1,
            iterations_used: 50,
            exhausted: false,
            rank: 10,
        });

        // Highest-rank task 0 wins the next pick.
        let pick = s.next_batch().unwrap();
        assert_eq!(pick.task_index, 0);

        // Task 0 reports a low-rank outcome this time. The stored rank
        // must drop from 100 to 3 — NOT stay at 100 and not accumulate.
        s.record_outcome(BatchOutcome {
            task_index: 0,
            iterations_used: 50,
            exhausted: false,
            rank: 3,
        });

        // Task 1 (still rank 10 in the queue) should now outrank task 0
        // (rank 3). If rank accumulated, task 0 would be at 103 and win.
        let pick = s.next_batch().unwrap();
        assert_eq!(
            pick.task_index, 1,
            "rank must be replaced on re-enqueue, not accumulated"
        );
    }

    #[test]
    #[should_panic(expected = "not in flight")]
    fn record_outcome_panics_for_unknown_task() {
        let mut s = BatchScheduler::new(1, Some(100), 50);
        s.record_outcome(BatchOutcome {
            task_index: 0,
            iterations_used: 10,
            exhausted: false,
            rank: 0,
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
                rank: 0,
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
                rank: 0,
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
                rank: 0,
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
                rank: 0,
                });
                rounds += 1;
                if rounds > max_rounds * 3 {
                    break; // safety net for unbounded
                }
            }
        }

        /// After every task has recorded at least one outcome with an
        /// assigned rank, the next pick is always a task whose stored
        /// rank equals the maximum rank currently in the queue.
        #[test]
        fn highest_rank_is_always_picked_next(
            ranks in proptest::collection::vec(-50_i64..=50, 2..=6),
        ) {
            let task_count = ranks.len();
            let mut scheduler = BatchScheduler::new(task_count, None, 100);

            // First round: pop every task in FIFO order and record each
            // with its assigned rank. No exhaustion — all are re-queued.
            let mut first_round = Vec::new();
            for _ in 0..task_count {
                let c = scheduler.next_batch().unwrap();
                first_round.push(c.task_index);
            }
            for ti in &first_round {
                scheduler.record_outcome(BatchOutcome {
                    task_index: *ti,
                    iterations_used: 100,
                    exhausted: false,
                    rank: ranks[*ti],
                });
            }

            let max_rank = *ranks.iter().max().unwrap();
            let picked = scheduler.next_batch().unwrap();
            prop_assert_eq!(
                ranks[picked.task_index],
                max_rank,
                "next pick must be a task tied with the maximum rank"
            );
        }
    }
}
