//! Rank-ordered batch scheduler with per-function worker leasing.
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
//! **Worker leasing (str-b2my.12):** Each function is leased to at most
//! one worker at a time. A call to [`BatchScheduler::next_batch`] acquires
//! the lease; [`BatchScheduler::record_outcome`] releases it. If
//! [`BatchScheduler::enqueue`] is called for a function that is currently
//! leased (in-flight) or already queued, the request is deferred or merged
//! rather than panicking. Deferred work is drained automatically when the
//! lease is released, ensuring that targets discovered for an active
//! function wait until that function is released and requeued.
//!
//! With initial ranks at 0 and callers that always pass `rank: 0`, the
//! scheduler degenerates to a strict round-robin (ties keep insertion
//! order) — the str-b2my.6 round-robin semantics are preserved as the
//! rank-0 special case.
//!
//! The batch size is an internal tuning parameter — it is not exposed
//! through the user-facing CLI.

use std::collections::{HashMap, VecDeque};

use serde::{Deserialize, Serialize};

use crate::coverage_metrics::CoverageMetrics;

/// Default number of iterations per batch.
pub const DEFAULT_BATCH_SIZE: u32 = 50;

/// Cooldown penalty applied to a function's effective rank immediately
/// after it completes a batch (str-b2my.8).
pub const COOLDOWN_PENALTY: i64 = 3;

/// Per-streak-count penalty applied when a function completes consecutive
/// batches without producing new coverage (str-b2my.9). The total attempt
/// penalty is `miss_streak * MISS_PENALTY_PER_STREAK`, so a function that
/// has missed 3 batches in a row pays an effective rank penalty of 6.
pub const MISS_PENALTY_PER_STREAK: i64 = 2;

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

/// Integer-only coverage counts suitable for scheduler ranking.
///
/// Extracted from [`CoverageMetrics`] but limited to integer fields
/// so the type can derive `Eq` (required by [`BatchOutcome`]).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoverageCounts {
    /// Total number of branch points in the function.
    pub total_branches: usize,
    /// Branches covered by any method (`z3_solved + random_found + user_provided`).
    pub covered: usize,
    /// Branches discovered by Z3 constraint solving.
    pub z3_solved: usize,
    /// Branches discovered by random/boundary generation.
    pub random_found: usize,
    /// Branches discovered via user-provided inputs.
    pub user_provided: usize,
    /// Branches that remain uncovered.
    pub uncovered: usize,
}

impl From<&CoverageMetrics> for CoverageCounts {
    fn from(m: &CoverageMetrics) -> Self {
        Self {
            total_branches: m.total_branches,
            covered: m.z3_solved + m.random_found + m.user_provided,
            z3_solved: m.z3_solved,
            random_found: m.random_found,
            user_provided: m.user_provided,
            uncovered: m.uncovered,
        }
    }
}

/// Generator-agnostic summary of work performed in a single batch.
///
/// Attached to [`BatchOutcome`] so the global scheduler can rank
/// functions without depending on per-generator internals.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerBatchSummary {
    /// Total executions performed in this batch.
    pub executions_run: u32,
    /// Branch coverage snapshot before this batch started.
    pub coverage_before: CoverageCounts,
    /// Branch coverage snapshot after this batch completed.
    pub coverage_after: CoverageCounts,
    /// Number of branches that remain uncovered after this batch.
    pub uncovered_remaining: usize,
    /// Number of new equivalence classes (unique branch paths) discovered
    /// in this batch.
    pub new_classes: usize,
    /// Number of new behaviors retained in the behavior map after
    /// deduplication (delta from prior batches, or total if first batch).
    pub new_retained_inputs: usize,
    /// Number of executions that resulted in a thrown error.
    pub failures: usize,
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
    /// Optional structured summary of the batch's work. `None` for
    /// error/timeout/respawn-failure outcomes where no exploration ran.
    pub summary: Option<WorkerBatchSummary>,
}

/// Result of an [`BatchScheduler::enqueue`] call, indicating how the
/// request was handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnqueueResult {
    /// Function was new — added to the queue tail with rank 0.
    Queued,
    /// Function is currently leased (in-flight). The budget was stored
    /// in the deferred map and will be applied when the lease is released
    /// via [`BatchScheduler::record_outcome`].
    Deferred,
    /// Function was already in the queue. The budget was merged into the
    /// existing queue entry.
    Merged,
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

/// Rank-ordered batch scheduler with per-function worker leasing.
///
/// Assigns fixed-size iteration batches to functions. Callers may request
/// multiple batches concurrently (one per function) and record their
/// outcomes in any order; non-exhausted functions are re-enqueued with
/// the outcome's rank and the next pick is whichever queued entry has
/// the highest rank (FIFO tie-break). When every outcome is reported
/// with `rank: 0` the scheduler collapses to a strict round-robin.
///
/// Each in-flight batch constitutes a *lease* on that function: no other
/// worker can receive a batch for the same function until the lease is
/// released via [`BatchScheduler::record_outcome`]. If new work is
/// enqueued for a leased function, it is deferred and drained
/// automatically on release.
#[derive(Debug)]
pub struct BatchScheduler {
    queue: VecDeque<Entry>,
    batch_size: u32,
    /// Entries popped by [`next_batch`] but not yet resolved via
    /// [`record_outcome`], keyed by `task_index`.
    in_flight: HashMap<usize, Entry>,
    /// Budgets enqueued for functions that are currently leased
    /// (in-flight). Drained by [`record_outcome`] when the lease is
    /// released. `None` means unbounded; `Some(n)` is additive across
    /// repeated deferrals.
    deferred: HashMap<usize, Option<u32>>,
    /// Per-function cooldown penalty (str-b2my.8). Set to [`COOLDOWN_PENALTY`]
    /// on batch completion; decays by 1 per subsequent batch completion.
    cooldown: HashMap<usize, i64>,
    /// Per-function consecutive no-progress batch count (str-b2my.9).
    /// Incremented when a batch produces no new coverage; reset on progress
    /// or deferred re-enqueue. The effective penalty applied to rank is
    /// `miss_streak * MISS_PENALTY_PER_STREAK`.
    miss_streak: HashMap<usize, u32>,
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
            deferred: HashMap::new(),
            cooldown: HashMap::new(),
            miss_streak: HashMap::new(),
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
            deferred: HashMap::new(),
            cooldown: HashMap::new(),
            miss_streak: HashMap::new(),
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

        // Linear scan for the highest effective-ranked entry. Effective rank
        // is `stored_rank - cooldown_penalty - attempt_penalty` (str-b2my.8,
        // str-b2my.9). `>` (strictly greater) keeps the earliest index on
        // ties, yielding stable FIFO tie-break.
        let best = self.queue.iter().enumerate().fold(
            None,
            |acc: Option<(usize, i64)>, (i, e)| {
                let cooldown = self.cooldown.get(&e.task_index).copied().unwrap_or(0);
                let attempt = self.miss_streak.get(&e.task_index).copied().unwrap_or(0)
                    as i64
                    * MISS_PENALTY_PER_STREAK;
                let effective = e.rank - cooldown - attempt;
                match acc {
                    None => Some((i, effective)),
                    Some((_, best_rank)) if effective > best_rank => Some((i, effective)),
                    Some(prev) => Some(prev),
                }
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

        // Deduct used iterations from the remaining budget.
        if let Some(ref mut r) = entry.remaining {
            *r = r.saturating_sub(outcome.iterations_used);
        }

        // Check for deferred work enqueued while this function was leased.
        let deferred_budget = self.deferred.remove(&outcome.task_index);

        // Determine whether the function should be re-enqueued. Without
        // deferred work, the original exhaustion/budget logic applies.
        // With deferred work, the function is always re-enqueued — deferred
        // targets override exhaustion because the caller signalled that new
        // work exists for this function.
        let should_drop = if let Some(def_budget) = deferred_budget {
            // Merge deferred budget into remaining.
            entry.remaining = merge_budgets(entry.remaining, def_budget);
            // Reset batch counter — the deferred work is logically a fresh
            // scheduling request for this function.
            entry.batches_completed = 0;
            // Deferred re-enqueue is a fresh arrival — clear cooldown
            // and miss streak.
            self.cooldown.remove(&outcome.task_index);
            self.miss_streak.remove(&outcome.task_index);
            false
        } else if outcome.exhausted {
            true
        } else {
            // Not exhausted, no deferred — check budget.
            entry.remaining == Some(0)
        };

        // --- Cooldown decay (str-b2my.8) ---
        // Every completed batch ticks all existing cooldowns down by 1,
        // promoting breadth-first exploration across functions.
        self.cooldown.retain(|_, penalty| {
            *penalty -= 1;
            *penalty > 0
        });

        if should_drop {
            self.cooldown.remove(&outcome.task_index);
            self.miss_streak.remove(&outcome.task_index);
            return;
        }

        // --- Miss streak tracking (str-b2my.9) ---
        // A batch is a "miss" when it ran but produced no new coverage.
        // Detection uses two tiers:
        // 1. If `summary` is present: precise check via coverage delta.
        // 2. If `summary` is absent (explore CLI path): rank == 0 on a
        //    non-exhausted, non-deferred batch signals no new discoveries.
        // Deferred re-enqueues already cleared the streak above.
        if deferred_budget.is_none() {
            let is_miss = match &outcome.summary {
                Some(s) => {
                    s.coverage_after.covered == s.coverage_before.covered
                        && s.new_classes == 0
                }
                None => outcome.rank == 0,
            };
            if is_miss {
                *self.miss_streak.entry(outcome.task_index).or_insert(0) += 1;
            } else {
                self.miss_streak.remove(&outcome.task_index);
            }
        }

        // Apply fresh cooldown to the re-enqueued function, unless this
        // is a deferred re-enqueue (logically a fresh arrival, no cooldown).
        if deferred_budget.is_none() {
            self.cooldown.insert(outcome.task_index, COOLDOWN_PENALTY);
        }

        // Replace the stored rank with the outcome's. Rank is not
        // accumulated across batches: each batch reports its own score
        // and the scheduler uses only the latest signal. When re-enqueuing
        // from deferred, rank resets to 0 (the deferred work hasn't run
        // yet, so there's no productivity signal).
        entry.rank = if deferred_budget.is_some() {
            0
        } else {
            outcome.rank
        };
        self.queue.push_back(entry);
    }

    /// Enqueue a function for scheduling, deferring if it is currently
    /// leased (in-flight) or merging if it is already queued.
    ///
    /// Use this when the caller's task list grows after construction —
    /// for example, when target discovery runs concurrently with batch
    /// execution and a new function appears that needs scheduling.
    ///
    /// **Lease-aware behaviour (str-b2my.12):**
    ///
    /// | Current state of `task_index` | Action | Return |
    /// |-------------------------------|--------|--------|
    /// | Not known (new function) | Added to queue tail, rank 0 | [`EnqueueResult::Queued`] |
    /// | In-flight (leased to a worker) | Budget stored in deferred map | [`EnqueueResult::Deferred`] |
    /// | Already in queue | Budget merged into queue entry | [`EnqueueResult::Merged`] |
    ///
    /// Deferred work is drained automatically by [`record_outcome`] when
    /// the lease is released. Multiple deferrals for the same in-flight
    /// function accumulate their budgets (with `None` meaning unbounded
    /// overriding any bounded value).
    ///
    /// `budget` matches the per-function budget semantics of the other
    /// constructors: `None` is unbounded, `Some(n)` caps total iterations
    /// across batches, `Some(0)` is silently inert (the entry is dropped
    /// on the next [`next_batch`] selection scan, mirroring how
    /// [`record_outcome`] handles a fully-spent budget).
    ///
    /// FIFO tie-break is preserved for new entries: among rank-0 entries,
    /// a newly queued entry is picked after every existing rank-0 entry,
    /// because it is pushed to the tail of the queue.
    ///
    /// [`next_batch`]: BatchScheduler::next_batch
    /// [`record_outcome`]: BatchScheduler::record_outcome
    pub fn enqueue(&mut self, task_index: usize, budget: Option<u32>) -> EnqueueResult {
        // In-flight: defer until lease is released.
        if self.in_flight.contains_key(&task_index) {
            let merged = match self.deferred.get(&task_index).copied() {
                Some(existing_budget) => merge_budgets(existing_budget, budget),
                None => budget,
            };
            self.deferred.insert(task_index, merged);
            return EnqueueResult::Deferred;
        }

        // Already queued: merge budget into the existing entry.
        if let Some(entry) = self.queue.iter_mut().find(|e| e.task_index == task_index) {
            entry.remaining = merge_budgets(entry.remaining, budget);
            return EnqueueResult::Merged;
        }

        // New function — append to queue.
        self.queue.push_back(Entry {
            task_index,
            remaining: budget,
            batches_completed: 0,
            rank: 0,
        });
        EnqueueResult::Queued
    }

    /// Returns `true` when all functions have been exhausted, there is
    /// no batch in flight, and no deferred work is pending.
    pub fn is_complete(&self) -> bool {
        self.in_flight.is_empty() && self.queue.is_empty() && self.deferred.is_empty()
    }

    /// Number of functions still in the queue (excludes in-flight batches).
    pub fn pending_count(&self) -> usize {
        self.queue.len()
    }

    /// Number of batches currently in flight.
    pub fn in_flight_count(&self) -> usize {
        self.in_flight.len()
    }

    /// Returns `true` if the given function is currently leased — i.e.,
    /// a worker is executing a batch for it and the outcome has not yet
    /// been recorded.
    pub fn is_leased(&self, task_index: usize) -> bool {
        self.in_flight.contains_key(&task_index)
    }

    /// Number of deferred enqueue requests waiting for a lease release.
    pub fn deferred_count(&self) -> usize {
        self.deferred.len()
    }

    /// Returns the current cooldown penalty for the given function.
    ///
    /// Returns 0 if the function has no active cooldown (either it hasn't
    /// run recently or the cooldown has fully decayed). Used by callers
    /// to display cooldown state in batch summary logs.
    pub fn cooldown_score(&self, task_index: usize) -> i64 {
        self.cooldown.get(&task_index).copied().unwrap_or(0)
    }

    /// Returns the current attempt penalty for the given function
    /// (str-b2my.9).
    ///
    /// The penalty equals `miss_streak * MISS_PENALTY_PER_STREAK`. Returns 0
    /// when the function has no consecutive no-progress batches. Used by
    /// callers to display attempt penalty in periodic batch summaries.
    pub fn attempt_penalty(&self, task_index: usize) -> i64 {
        self.miss_streak.get(&task_index).copied().unwrap_or(0) as i64
            * MISS_PENALTY_PER_STREAK
    }

    /// Configured batch size.
    pub fn batch_size(&self) -> u32 {
        self.batch_size
    }

    /// Returns `true` when all remaining work is revisiting known targets
    /// rather than exploring new frontier.
    ///
    /// Specifically, this is true when the queue and/or in-flight set is
    /// non-empty (there IS remaining work), every entry has completed at
    /// least one batch (`batches_completed > 0`), every entry's most recent
    /// rank is zero or below (no recent discoveries), and no deferred work
    /// is pending (no newly discovered targets waiting).
    ///
    /// This is a derived query over existing scheduler state — it does not
    /// introduce a separate scheduler mode or affect scheduling decisions.
    pub fn is_frontier_exhausted(&self) -> bool {
        if self.queue.is_empty() && self.in_flight.is_empty() {
            return false; // nothing left — complete, not fallback
        }
        if !self.deferred.is_empty() {
            return false; // new targets pending
        }
        self.queue
            .iter()
            .all(|e| e.batches_completed > 0 && e.rank <= 0)
            && self
                .in_flight
                .values()
                .all(|e| e.batches_completed > 0 && e.rank <= 0)
    }
}

/// Merge two optional budgets. `None` (unbounded) absorbs any bounded
/// value; two bounded values are summed.
fn merge_budgets(a: Option<u32>, b: Option<u32>) -> Option<u32> {
    match (a, b) {
        (None, _) | (_, None) => None,
        (Some(x), Some(y)) => Some(x.saturating_add(y)),
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
            summary: None,
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
            summary: None,
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
            summary: None,
        });

        let b3 = s.next_batch().expect("task 0 should be re-enqueued");
        assert_eq!(b3.task_index, 0, "re-enqueued task 0 must reappear");
        assert_eq!(b3.batch_number, 1, "batch_number advances on re-enqueue");
        s.record_outcome(BatchOutcome {
            task_index: 0,
            iterations_used: 50,
            exhausted: true,
            rank: 0,
            summary: None,
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
                summary: None,
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
                summary: None,
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
            summary: None,
        });

        let b2 = s.next_batch().unwrap();
        assert_eq!(b2.batch_size, 30);
        assert_eq!(b2.batch_number, 1);
        s.record_outcome(BatchOutcome {
            task_index: 0,
            iterations_used: 30,
            exhausted: false, // caller says not exhausted, but budget is now 0
            rank: 0,
            summary: None,
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
                summary: None,
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
                summary: None,
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
            summary: None,
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
            summary: None,
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
            summary: None,
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
            summary: None,
        });
        s.record_outcome(BatchOutcome {
            task_index: 0,
            iterations_used: 50,
            exhausted: true,
            rank: 0,
            summary: None,
        });
        s.record_outcome(BatchOutcome {
            task_index: 1,
            iterations_used: 50,
            exhausted: false,
            rank: 0,
            summary: None,
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
            summary: None,
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
            summary: None,
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
            summary: None,
        });
        let b = s.next_batch().expect("fourth batch");
        assert_eq!(b.task_index, 0);
        s.record_outcome(BatchOutcome {
            task_index: 0,
            iterations_used: 50,
            exhausted: true,
            rank: 0,
            summary: None,
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
            summary: None,
        });
        s.record_outcome(BatchOutcome {
            task_index: 1,
            iterations_used: 50,
            exhausted: false,
            rank: 10,
            summary: None,
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
            summary: None,
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
            summary: None,
        });
    }

    // ---------------- str-b2my.17: enqueue() ----------------

    #[test]
    fn enqueue_appends_to_empty_scheduler() {
        // Construct empty, enqueue one task, expect to receive it.
        let mut s = BatchScheduler::new(0, None, 50);
        assert!(s.is_complete(), "empty scheduler is complete");

        s.enqueue(0, Some(100));
        assert!(!s.is_complete(), "enqueue must un-complete the scheduler");
        assert_eq!(s.pending_count(), 1);

        let b = s.next_batch().expect("enqueued task must be schedulable");
        assert_eq!(b.task_index, 0);
        assert_eq!(b.batch_number, 0, "fresh enqueue starts at batch 0");
        assert_eq!(b.batch_size, 50);
    }

    #[test]
    fn enqueue_after_drain_reactivates_scheduler() {
        // Run a 1-task scheduler to completion, then enqueue a new task.
        let mut s = BatchScheduler::new(1, Some(50), 50);
        let b = s.next_batch().unwrap();
        s.record_outcome(BatchOutcome {
            task_index: b.task_index,
            iterations_used: 50,
            exhausted: true,
            rank: 0,
            summary: None,
        });
        assert!(s.is_complete(), "scheduler drained");
        assert!(s.next_batch().is_none());

        // Enqueue a new task with a fresh index — scheduler should resume.
        s.enqueue(99, Some(30));
        assert!(!s.is_complete());
        let b2 = s.next_batch().expect("re-activated by enqueue");
        assert_eq!(b2.task_index, 99);
        assert_eq!(b2.batch_size, 30, "budget < batch_size clamps");
        assert_eq!(b2.batch_number, 0);
    }

    #[test]
    fn enqueue_mid_flight_does_not_disturb_in_flight() {
        // Pop task 0, leaving it in flight. Enqueue task 99 while 0 is in
        // flight. Pop again — must return 99 (the only queued entry).
        // Then complete 0; the in_flight tracking must not have been corrupted.
        let mut s = BatchScheduler::new(1, None, 50);
        let b0 = s.next_batch().unwrap();
        assert_eq!(b0.task_index, 0);
        assert_eq!(s.in_flight_count(), 1);

        s.enqueue(99, None);
        assert_eq!(s.in_flight_count(), 1, "enqueue must not touch in_flight");
        assert_eq!(s.pending_count(), 1);

        let b99 = s.next_batch().unwrap();
        assert_eq!(b99.task_index, 99);
        assert_eq!(s.in_flight_count(), 2);

        // Both can record outcomes in any order.
        s.record_outcome(BatchOutcome {
            task_index: 0,
            iterations_used: 50,
            exhausted: true,
            rank: 0,
            summary: None,
        });
        s.record_outcome(BatchOutcome {
            task_index: 99,
            iterations_used: 50,
            exhausted: true,
            rank: 0,
            summary: None,
        });
        assert!(s.is_complete());
    }

    #[test]
    fn enqueue_respects_fifo_tie_break_with_existing_rank_zero() {
        // Existing tasks 0, 1 at rank 0. Enqueue task 99 at rank 0.
        // It must be picked AFTER 0 and 1 in the same round.
        let mut s = BatchScheduler::new(2, None, 50);
        s.enqueue(99, None);
        assert_eq!(s.pending_count(), 3);

        let order: Vec<usize> = (0..3)
            .map(|_| {
                let b = s.next_batch().unwrap();
                let ti = b.task_index;
                s.record_outcome(BatchOutcome {
                    task_index: ti,
                    iterations_used: 50,
                    exhausted: true,
                    rank: 0,
                    summary: None,
                });
                ti
            })
            .collect();
        assert_eq!(
            order,
            vec![0, 1, 99],
            "enqueue appends to tail; FIFO order preserved"
        );
    }

    #[test]
    fn enqueue_with_individual_budgets_then_enqueue_more() {
        // Build via with_individual_budgets, enqueue extras, ensure all run.
        let mut s = BatchScheduler::with_individual_budgets(&[Some(50), Some(50)], 50);
        s.enqueue(2, Some(50));
        s.enqueue(3, Some(50));
        assert_eq!(s.pending_count(), 4);

        let mut seen = std::collections::HashSet::new();
        while let Some(b) = s.next_batch() {
            seen.insert(b.task_index);
            s.record_outcome(BatchOutcome {
                task_index: b.task_index,
                iterations_used: 50,
                exhausted: true,
                rank: 0,
                summary: None,
            });
        }
        assert_eq!(seen, [0, 1, 2, 3].into_iter().collect());
    }

    #[test]
    fn enqueue_unbounded_keeps_cycling_with_existing_unbounded() {
        // 1 unbounded existing task. Enqueue another unbounded task.
        // Confirm round-robin between them for several rounds.
        let mut s = BatchScheduler::new(1, None, 10);
        s.enqueue(7, None);

        let mut order = Vec::new();
        for i in 0..6 {
            let b = s.next_batch().unwrap();
            order.push(b.task_index);
            s.record_outcome(BatchOutcome {
                task_index: b.task_index,
                iterations_used: 10,
                exhausted: i >= 4,
                rank: 0,
                summary: None,
            });
        }
        assert_eq!(order, vec![0, 7, 0, 7, 0, 7]);
        assert!(s.is_complete());
    }

    // --- Worker lease tests (str-b2my.12) ---

    #[test]
    fn enqueue_defers_when_in_flight() {
        let mut s = BatchScheduler::new(1, None, 50);
        let b = s.next_batch().unwrap();
        assert_eq!(b.task_index, 0);
        assert!(s.is_leased(0));

        // Enqueue for an in-flight function → deferred.
        let result = s.enqueue(0, Some(100));
        assert_eq!(result, EnqueueResult::Deferred);
        assert_eq!(s.deferred_count(), 1);

        // Record exhaustion — deferred work overrides it, function re-enqueues.
        s.record_outcome(BatchOutcome {
            task_index: 0,
            iterations_used: 50,
            exhausted: true,
            rank: 5,
            summary: None,
        });
        assert!(!s.is_leased(0));
        assert_eq!(s.deferred_count(), 0);
        assert_eq!(s.pending_count(), 1);

        // The re-enqueued entry gets the deferred budget.
        let b2 = s.next_batch().unwrap();
        assert_eq!(b2.task_index, 0);
        // Deferred resets batch counter.
        assert_eq!(b2.batch_number, 0);
        assert_eq!(b2.batch_size, 50); // min(100, 50)
    }

    #[test]
    fn enqueue_merges_when_queued() {
        let mut s = BatchScheduler::new(1, Some(50), 50);
        // Task 0 is in the queue with budget 50.
        let result = s.enqueue(0, Some(100));
        assert_eq!(result, EnqueueResult::Merged);

        // Budget should now be 150 (50 + 100). First batch uses 50,
        // leaving 100. Second batch uses 50, leaving 50. Third uses 50.
        let b = s.next_batch().unwrap();
        assert_eq!(b.batch_size, 50);
        s.record_outcome(BatchOutcome {
            task_index: 0,
            iterations_used: 50,
            exhausted: false,
            rank: 0,
            summary: None,
        });

        let b2 = s.next_batch().unwrap();
        assert_eq!(b2.batch_size, 50);
        s.record_outcome(BatchOutcome {
            task_index: 0,
            iterations_used: 50,
            exhausted: false,
            rank: 0,
            summary: None,
        });

        let b3 = s.next_batch().unwrap();
        assert_eq!(b3.batch_size, 50);
        s.record_outcome(BatchOutcome {
            task_index: 0,
            iterations_used: 50,
            exhausted: false,
            rank: 0,
            summary: None,
        });

        // Budget should be exhausted (150 - 50*3 = 0).
        assert!(s.next_batch().is_none());
        assert!(s.is_complete());
    }

    #[test]
    fn deferred_overrides_exhaustion() {
        let mut s = BatchScheduler::new(1, Some(50), 50);
        let b = s.next_batch().unwrap();
        assert_eq!(b.task_index, 0);

        // Defer new work while in-flight.
        s.enqueue(0, Some(200));

        // Mark exhausted — but deferred work exists, so it re-enqueues.
        s.record_outcome(BatchOutcome {
            task_index: 0,
            iterations_used: 50,
            exhausted: true,
            rank: 10,
            summary: None,
        });

        assert!(!s.is_complete());
        let b2 = s.next_batch().unwrap();
        assert_eq!(b2.task_index, 0);
        // Deferred budget (200) merged with remaining (0): 200.
        // batch_size is 50, so clamped.
        assert_eq!(b2.batch_size, 50);
        // Deferred resets batch_number.
        assert_eq!(b2.batch_number, 0);
    }

    #[test]
    fn multiple_deferred_accumulate() {
        // Start with bounded budget so we can verify the deferred merge.
        let mut s = BatchScheduler::new(1, Some(50), 50);
        let _ = s.next_batch().unwrap();

        // Defer twice while in-flight — budgets accumulate.
        assert_eq!(s.enqueue(0, Some(100)), EnqueueResult::Deferred);
        assert_eq!(s.enqueue(0, Some(200)), EnqueueResult::Deferred);
        assert_eq!(s.deferred_count(), 1);

        s.record_outcome(BatchOutcome {
            task_index: 0,
            iterations_used: 50,
            exhausted: true,
            rank: 0,
            summary: None,
        });

        // First batch consumed 50 of original budget (50), leaving 0.
        // Deferred merge: merge_budgets(Some(0), Some(300)) = Some(300).
        // With batch_size=50: 6 batches to exhaust.
        let mut batch_count = 0;
        while let Some(b) = s.next_batch() {
            assert_eq!(b.task_index, 0);
            batch_count += 1;
            s.record_outcome(BatchOutcome {
                task_index: 0,
                iterations_used: b.batch_size,
                exhausted: false,
                rank: 0,
                summary: None,
            });
        }
        assert_eq!(batch_count, 6); // 300 / 50 = 6
        assert!(s.is_complete());
    }

    #[test]
    fn deferred_unbounded_wins() {
        let mut s = BatchScheduler::new(1, Some(50), 50);
        let _ = s.next_batch().unwrap();

        // First defer bounded, then unbounded.
        s.enqueue(0, Some(100));
        s.enqueue(0, None); // unbounded overrides

        s.record_outcome(BatchOutcome {
            task_index: 0,
            iterations_used: 50,
            exhausted: true,
            rank: 0,
            summary: None,
        });

        // Re-enqueued with unbounded budget — keeps cycling.
        for i in 0..5 {
            let b = s.next_batch().unwrap();
            assert_eq!(b.task_index, 0);
            assert_eq!(b.batch_size, 50);
            s.record_outcome(BatchOutcome {
                task_index: 0,
                iterations_used: 50,
                exhausted: i == 4,
                rank: 0,
                summary: None,
            });
        }
        assert!(s.is_complete());
    }

    #[test]
    fn is_leased_tracks_in_flight() {
        let mut s = BatchScheduler::new(2, None, 50);
        assert!(!s.is_leased(0));
        assert!(!s.is_leased(1));

        let b = s.next_batch().unwrap();
        assert_eq!(b.task_index, 0);
        assert!(s.is_leased(0));
        assert!(!s.is_leased(1));

        s.record_outcome(BatchOutcome {
            task_index: 0,
            iterations_used: 50,
            exhausted: true,
            rank: 0,
            summary: None,
        });
        assert!(!s.is_leased(0));
    }

    #[test]
    fn is_complete_considers_deferred() {
        let mut s = BatchScheduler::new(1, None, 50);
        let _ = s.next_batch().unwrap();
        s.enqueue(0, Some(50));

        // In-flight + deferred → not complete.
        assert!(!s.is_complete());

        s.record_outcome(BatchOutcome {
            task_index: 0,
            iterations_used: 50,
            exhausted: true,
            rank: 0,
            summary: None,
        });
        // Deferred drained → re-enqueued → not complete yet.
        assert!(!s.is_complete());

        let b = s.next_batch().unwrap();
        s.record_outcome(BatchOutcome {
            task_index: b.task_index,
            iterations_used: b.batch_size,
            exhausted: true,
            rank: 0,
            summary: None,
        });
        // No deferred, no in-flight, no queued → complete.
        assert!(s.is_complete());
    }

    #[test]
    fn deferred_resets_batch_counter() {
        let mut s = BatchScheduler::new(1, None, 50);

        // Run 3 batches to advance batch_number.
        for _ in 0..3 {
            let b = s.next_batch().unwrap();
            s.record_outcome(BatchOutcome {
                task_index: 0,
                iterations_used: 50,
                exhausted: false,
                rank: 0,
                summary: None,
            });
            let _ = b;
        }

        // Start batch 3 and defer work.
        let b = s.next_batch().unwrap();
        assert_eq!(b.batch_number, 3);
        s.enqueue(0, Some(50));

        s.record_outcome(BatchOutcome {
            task_index: 0,
            iterations_used: 50,
            exhausted: true,
            rank: 0,
            summary: None,
        });

        // Deferred re-enqueue resets batch_number to 0.
        let b2 = s.next_batch().unwrap();
        assert_eq!(b2.batch_number, 0);
    }

    #[test]
    fn deferred_rank_resets_to_zero() {
        // When a function is re-enqueued from deferred, its rank should
        // reset to 0 (no productivity signal yet for the deferred work).
        let mut s = BatchScheduler::new(2, None, 50);

        // Pop task 0, set task 1's rank high via a batch.
        let _ = s.next_batch().unwrap(); // task 0
        let _b1 = s.next_batch().unwrap(); // task 1
        s.record_outcome(BatchOutcome {
            task_index: 1,
            iterations_used: 50,
            exhausted: false,
            rank: 10,
            summary: None,
        });

        // Defer work for task 0 while it's in-flight.
        s.enqueue(0, Some(100));

        // Release task 0 as exhausted. Deferred overrides, re-enqueues
        // with rank 0.
        s.record_outcome(BatchOutcome {
            task_index: 0,
            iterations_used: 50,
            exhausted: true,
            rank: 99, // high rank, but should be overridden by deferred
            summary: None,
        });

        // Task 1 has rank 10, task 0 has rank 0 (from deferred).
        // Task 1 should win.
        let next = s.next_batch().unwrap();
        assert_eq!(
            next.task_index, 1,
            "task 1 (rank 10) should beat deferred task 0 (rank 0)"
        );
    }

    // --- Worker batch summary tests (str-b2my.14) ---

    #[test]
    fn coverage_counts_from_metrics() {
        use crate::coverage_metrics::CoverageMetrics;
        let m = CoverageMetrics {
            total_branches: 10,
            z3_solved: 3,
            random_found: 2,
            user_provided: 1,
            uncovered: 4,
            symexpr_count: 8,
            unknown_count: 2,
            mcdc_metrics: None,
        };
        let c = CoverageCounts::from(&m);
        assert_eq!(c.total_branches, 10);
        assert_eq!(c.covered, 6); // 3 + 2 + 1
        assert_eq!(c.z3_solved, 3);
        assert_eq!(c.random_found, 2);
        assert_eq!(c.user_provided, 1);
        assert_eq!(c.uncovered, 4);
    }

    #[test]
    fn default_coverage_counts_is_all_zeros() {
        let c = CoverageCounts::default();
        assert_eq!(c.total_branches, 0);
        assert_eq!(c.covered, 0);
        assert_eq!(c.uncovered, 0);
    }

    #[test]
    fn summary_passes_through_record_outcome() {
        let mut s = BatchScheduler::new(1, None, 50);
        let b = s.next_batch().unwrap();
        s.record_outcome(BatchOutcome {
            task_index: b.task_index,
            iterations_used: 50,
            exhausted: false,
            rank: 5,
            summary: Some(WorkerBatchSummary {
                executions_run: 50,
                coverage_before: CoverageCounts::default(),
                coverage_after: CoverageCounts {
                    total_branches: 8,
                    covered: 3,
                    z3_solved: 2,
                    random_found: 1,
                    user_provided: 0,
                    uncovered: 5,
                },
                uncovered_remaining: 5,
                new_classes: 3,
                new_retained_inputs: 3,
                failures: 0,
            }),
        });
        // Function re-enqueued with rank 5.
        let b2 = s.next_batch().unwrap();
        assert_eq!(b2.task_index, 0);
        assert_eq!(b2.batch_number, 1);
    }

    #[test]
    fn summary_none_for_exhausted_outcome() {
        let mut s = BatchScheduler::new(1, Some(50), 50);
        let b = s.next_batch().unwrap();
        s.record_outcome(BatchOutcome {
            task_index: b.task_index,
            iterations_used: 50,
            exhausted: true,
            rank: 0,
            summary: None,
        });
        assert!(s.is_complete());
    }

    // --- Miss streak / attempt penalty tests (str-b2my.9) ---

    /// Helper: a WorkerBatchSummary that represents a no-progress batch.
    fn no_progress_summary() -> Option<WorkerBatchSummary> {
        Some(WorkerBatchSummary {
            executions_run: 50,
            coverage_before: CoverageCounts {
                total_branches: 8,
                covered: 3,
                z3_solved: 2,
                random_found: 1,
                user_provided: 0,
                uncovered: 5,
            },
            coverage_after: CoverageCounts {
                total_branches: 8,
                covered: 3,
                z3_solved: 2,
                random_found: 1,
                user_provided: 0,
                uncovered: 5,
            },
            uncovered_remaining: 5,
            new_classes: 0,
            new_retained_inputs: 0,
            failures: 0,
        })
    }

    /// Helper: a WorkerBatchSummary that represents a productive batch.
    fn progress_summary() -> Option<WorkerBatchSummary> {
        Some(WorkerBatchSummary {
            executions_run: 50,
            coverage_before: CoverageCounts {
                total_branches: 8,
                covered: 3,
                z3_solved: 2,
                random_found: 1,
                user_provided: 0,
                uncovered: 5,
            },
            coverage_after: CoverageCounts {
                total_branches: 8,
                covered: 5,
                z3_solved: 4,
                random_found: 1,
                user_provided: 0,
                uncovered: 3,
            },
            uncovered_remaining: 3,
            new_classes: 2,
            new_retained_inputs: 2,
            failures: 0,
        })
    }

    #[test]
    fn miss_streak_accumulates_penalty() {
        // Two functions; task 0 misses repeatedly while task 1 succeeds.
        let mut s = BatchScheduler::new(2, None, 50);

        // Batch 1: both miss (first batch, streak = 1 each).
        let b0 = s.next_batch().unwrap();
        assert_eq!(b0.task_index, 0);
        s.record_outcome(BatchOutcome {
            task_index: 0,
            iterations_used: 50,
            exhausted: false,
            rank: 0,
            summary: no_progress_summary(),
        });
        assert_eq!(s.attempt_penalty(0), MISS_PENALTY_PER_STREAK);

        let b1 = s.next_batch().unwrap();
        assert_eq!(b1.task_index, 1);
        s.record_outcome(BatchOutcome {
            task_index: 1,
            iterations_used: 50,
            exhausted: false,
            rank: 0,
            summary: no_progress_summary(),
        });
        assert_eq!(s.attempt_penalty(1), MISS_PENALTY_PER_STREAK);

        // Batch 2: task 0 misses again (streak = 2), task 1 makes progress
        // (streak reset).
        let b0 = s.next_batch().unwrap();
        s.record_outcome(BatchOutcome {
            task_index: b0.task_index,
            iterations_used: 50,
            exhausted: false,
            rank: 0,
            summary: no_progress_summary(),
        });

        let b1 = s.next_batch().unwrap();
        s.record_outcome(BatchOutcome {
            task_index: b1.task_index,
            iterations_used: 50,
            exhausted: false,
            rank: 3,
            summary: progress_summary(),
        });

        // Task 0 should have streak=2, task 1 should have streak=0.
        assert_eq!(s.attempt_penalty(0), 2 * MISS_PENALTY_PER_STREAK);
        assert_eq!(s.attempt_penalty(1), 0);
    }

    #[test]
    fn miss_streak_resets_on_progress() {
        let mut s = BatchScheduler::new(1, None, 50);

        // Miss twice.
        for _ in 0..2 {
            let _ = s.next_batch().unwrap();
            s.record_outcome(BatchOutcome {
                task_index: 0,
                iterations_used: 50,
                exhausted: false,
                rank: 0,
                summary: no_progress_summary(),
            });
        }
        assert_eq!(s.attempt_penalty(0), 2 * MISS_PENALTY_PER_STREAK);

        // Make progress — streak resets.
        let _ = s.next_batch().unwrap();
        s.record_outcome(BatchOutcome {
            task_index: 0,
            iterations_used: 50,
            exhausted: false,
            rank: 5,
            summary: progress_summary(),
        });
        assert_eq!(s.attempt_penalty(0), 0);
    }

    #[test]
    fn miss_streak_cleared_on_deferred_reenqueue() {
        let mut s = BatchScheduler::new(1, None, 50);

        // Build up a miss streak.
        let _ = s.next_batch().unwrap();
        // While in-flight, enqueue deferred work.
        s.enqueue(0, Some(100));
        s.record_outcome(BatchOutcome {
            task_index: 0,
            iterations_used: 50,
            exhausted: false,
            rank: 0,
            summary: no_progress_summary(),
        });
        // Deferred re-enqueue should clear the miss streak.
        assert_eq!(s.attempt_penalty(0), 0);
    }

    #[test]
    fn miss_streak_removed_on_exhaustion() {
        let mut s = BatchScheduler::new(1, None, 50);

        // Miss once to build streak.
        let _ = s.next_batch().unwrap();
        s.record_outcome(BatchOutcome {
            task_index: 0,
            iterations_used: 50,
            exhausted: false,
            rank: 0,
            summary: no_progress_summary(),
        });
        assert_eq!(s.attempt_penalty(0), MISS_PENALTY_PER_STREAK);

        // Exhaust — streak should be removed.
        let _ = s.next_batch().unwrap();
        s.record_outcome(BatchOutcome {
            task_index: 0,
            iterations_used: 10,
            exhausted: true,
            rank: 0,
            summary: no_progress_summary(),
        });
        assert_eq!(s.attempt_penalty(0), 0);
        assert!(s.is_complete());
    }

    #[test]
    fn miss_penalty_demotes_stuck_function_below_productive_one() {
        // Task 0 misses twice, task 1 has been productive. With equal
        // base ranks, the attempt penalty should cause task 1 to be
        // scheduled first.
        let mut s = BatchScheduler::new(2, None, 50);

        // Round 1: both complete with rank 0 (miss).
        for _ in 0..2 {
            let b = s.next_batch().unwrap();
            s.record_outcome(BatchOutcome {
                task_index: b.task_index,
                iterations_used: 50,
                exhausted: false,
                rank: 0,
                summary: no_progress_summary(),
            });
        }

        // Round 2: task 0 misses again (streak=2), task 1 makes progress
        // (streak resets to 0).
        let pick = s.next_batch().unwrap();
        s.record_outcome(BatchOutcome {
            task_index: pick.task_index,
            iterations_used: 50,
            exhausted: false,
            rank: 0,
            summary: no_progress_summary(),
        });
        let pick = s.next_batch().unwrap();
        s.record_outcome(BatchOutcome {
            task_index: pick.task_index,
            iterations_used: 50,
            exhausted: false,
            rank: 5,
            summary: progress_summary(),
        });

        // Round 3: task 1 should be picked first because task 0 has a
        // higher attempt penalty.
        let next = s.next_batch().unwrap();
        assert_eq!(next.task_index, 1, "productive task should be scheduled before stuck one");
    }

    #[test]
    fn miss_streak_via_rank_zero_without_summary() {
        // Explore CLI path: no summary, rank 0 signals miss.
        let mut s = BatchScheduler::new(1, None, 50);

        let _ = s.next_batch().unwrap();
        s.record_outcome(BatchOutcome {
            task_index: 0,
            iterations_used: 50,
            exhausted: false,
            rank: 0,
            summary: None,
        });
        assert_eq!(s.attempt_penalty(0), MISS_PENALTY_PER_STREAK);

        // Non-zero rank without summary clears streak.
        let _ = s.next_batch().unwrap();
        s.record_outcome(BatchOutcome {
            task_index: 0,
            iterations_used: 50,
            exhausted: false,
            rank: 3,
            summary: None,
        });
        assert_eq!(s.attempt_penalty(0), 0);
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
                summary: None,
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
                summary: None,
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
                summary: None,
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
                summary: None,
                });
                rounds += 1;
                if rounds > max_rounds * 3 {
                    break; // safety net for unbounded
                }
            }
        }

        /// After every task has recorded at least one outcome with an
        /// assigned rank, the next pick is always the task with the
        /// highest *effective* rank (stored rank minus cooldown and
        /// attempt penalties). We use strictly positive ranks so no
        /// batch triggers a miss-streak penalty (rank > 0 ⇒ not a miss).
        #[test]
        fn highest_rank_is_always_picked_next(
            ranks in proptest::collection::vec(1_i64..=50, 2..=6),
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
                    summary: None,
                });
            }

            // Compute expected effective ranks before the pick. Effective
            // rank = stored_rank - cooldown - attempt_penalty. All ranks
            // are > 0 so attempt_penalty is 0 for all tasks.
            let effective_ranks: Vec<i64> = (0..task_count)
                .map(|i| {
                    ranks[i]
                        - scheduler.cooldown_score(i)
                        - scheduler.attempt_penalty(i)
                })
                .collect();
            let max_effective = *effective_ranks.iter().max().unwrap();

            let picked = scheduler.next_batch().unwrap();
            prop_assert_eq!(
                effective_ranks[picked.task_index],
                max_effective,
                "next pick must be the task with the highest effective rank"
            );
        }

        /// str-b2my.17: dynamic enqueue. Start with `initial` tasks, then
        /// enqueue `extras` more (with disjoint task_indices). Every
        /// enqueued task must eventually be scheduled at least once, and
        /// total assigned iterations must respect each task's budget.
        #[test]
        fn enqueue_grows_schedulable_set(
            initial in 0_usize..6,
            extras in 1_usize..6,
            budget in 1_u32..200,
            batch_size in 1_u32..100,
        ) {
            let mut scheduler = BatchScheduler::new(initial, Some(budget), batch_size);

            // Enqueue extras with disjoint indices (initial..initial+extras).
            for i in 0..extras {
                scheduler.enqueue(initial + i, Some(budget));
            }

            let total_tasks = initial + extras;
            let mut per_task_seen: Vec<bool> = vec![false; total_tasks];
            let mut per_task_assigned: Vec<u64> = vec![0; total_tasks];

            while let Some(config) = scheduler.next_batch() {
                prop_assert!(
                    config.task_index < total_tasks,
                    "task_index {} out of range {}", config.task_index, total_tasks,
                );
                per_task_seen[config.task_index] = true;
                per_task_assigned[config.task_index] += config.batch_size as u64;
                scheduler.record_outcome(BatchOutcome {
                    task_index: config.task_index,
                    iterations_used: config.batch_size,
                    exhausted: false,
                    rank: 0,
                    summary: None,
                });
            }

            for (i, seen) in per_task_seen.iter().enumerate() {
                prop_assert!(*seen, "task {} (of {}) was never scheduled", i, total_tasks);
            }
            for (i, assigned) in per_task_assigned.iter().enumerate() {
                prop_assert!(
                    *assigned <= budget as u64,
                    "task {} assigned {} > budget {}", i, assigned, budget,
                );
            }
            prop_assert!(scheduler.is_complete());
        }

        /// Enqueue mid-flight: while one task is in flight, enqueue a new
        /// one. The in-flight task's outcome must still be recordable, and
        /// the new task must be schedulable.
        #[test]
        fn enqueue_mid_flight_preserves_in_flight(
            initial in 1_usize..5,
            new_index_offset in 1_usize..20,
            batch_size in 1_u32..50,
        ) {
            let mut scheduler = BatchScheduler::new(initial, None, batch_size);

            // Pop one task — it's now in flight.
            let in_flight = scheduler.next_batch().unwrap();
            let new_index = initial + new_index_offset;
            scheduler.enqueue(new_index, None);

            // The new task must be schedulable while the other is in flight.
            let popped_new = scheduler.next_batch().unwrap();
            prop_assert!(
                popped_new.task_index != in_flight.task_index,
                "fresh pick must not collide with in-flight task",
            );

            // Record both outcomes — neither should panic.
            scheduler.record_outcome(BatchOutcome {
                task_index: in_flight.task_index,
                iterations_used: batch_size,
                exhausted: true,
                rank: 0,
                summary: None,
            });
            scheduler.record_outcome(BatchOutcome {
                task_index: popped_new.task_index,
                iterations_used: batch_size,
                exhausted: true,
                rank: 0,
                summary: None,
            });
        }

        // --- Worker lease property tests (str-b2my.12) ---

        /// No task_index ever appears in two concurrent next_batch results.
        /// This is the core lease invariant: a function can only be leased
        /// to one worker at a time.
        #[test]
        fn no_double_lease(
            task_count in 2_usize..8,
            batch_size in 1_u32..50,
        ) {
            let mut scheduler = BatchScheduler::new(task_count, None, batch_size);
            let mut in_flight_set = std::collections::HashSet::new();
            let max_ops = task_count * 6;

            for _ in 0..max_ops {
                // Try to pop as many as available.
                while let Some(config) = scheduler.next_batch() {
                    prop_assert!(
                        in_flight_set.insert(config.task_index),
                        "task {} was dispatched while already in flight — lease violated",
                        config.task_index,
                    );
                }
                // Release all in-flight tasks.
                let to_release: Vec<_> = in_flight_set.drain().collect();
                if to_release.is_empty() {
                    break;
                }
                for ti in to_release {
                    scheduler.record_outcome(BatchOutcome {
                        task_index: ti,
                        iterations_used: batch_size,
                        exhausted: false,
                        rank: 0,
                        summary: None,
                    });
                }
            }
        }

        /// After interleaved enqueue-while-in-flight sequences, all
        /// deferred work eventually enters the queue and is scheduled.
        #[test]
        fn deferred_always_drains(
            task_count in 1_usize..5,
            defer_count in 1_usize..4,
            defer_budget in 1_u32..100,
            batch_size in 1_u32..50,
        ) {
            let mut scheduler = BatchScheduler::new(task_count, Some(defer_budget), batch_size);

            // Pop all tasks.
            let mut popped = Vec::new();
            while let Some(config) = scheduler.next_batch() {
                popped.push(config.task_index);
            }

            // Defer additional work for task 0 while it's in-flight.
            for _ in 0..defer_count {
                let result = scheduler.enqueue(popped[0], Some(defer_budget));
                prop_assert_eq!(result, EnqueueResult::Deferred);
            }
            prop_assert!(scheduler.deferred_count() > 0);

            // Release all tasks.
            for &ti in &popped {
                scheduler.record_outcome(BatchOutcome {
                    task_index: ti,
                    iterations_used: batch_size.min(defer_budget),
                    exhausted: true,
                    rank: 0,
                    summary: None,
                });
            }

            // Deferred should have been drained.
            prop_assert_eq!(scheduler.deferred_count(), 0);

            // Task 0 must be schedulable again (from deferred work).
            let mut saw_task_0 = false;
            while let Some(config) = scheduler.next_batch() {
                if config.task_index == popped[0] {
                    saw_task_0 = true;
                }
                scheduler.record_outcome(BatchOutcome {
                    task_index: config.task_index,
                    iterations_used: config.batch_size,
                    exhausted: true,
                    rank: 0,
                    summary: None,
                });
            }
            prop_assert!(saw_task_0, "deferred task 0 was never re-scheduled");
            prop_assert!(scheduler.is_complete());
        }

        /// Total iterations assigned across all batches for a task never
        /// exceeds original budget + all deferred budgets.
        #[test]
        fn deferred_budget_bounded(
            budget in 1_u32..200,
            deferred_budget in 1_u32..200,
            batch_size in 1_u32..100,
        ) {
            let mut scheduler = BatchScheduler::new(1, Some(budget), batch_size);

            // Pop the task.
            let config = scheduler.next_batch().unwrap();
            let mut total_assigned = config.batch_size as u64;

            // Defer additional budget while in-flight.
            scheduler.enqueue(0, Some(deferred_budget));

            // Release.
            scheduler.record_outcome(BatchOutcome {
                task_index: 0,
                iterations_used: config.batch_size,
                exhausted: true,
                rank: 0,
                summary: None,
            });

            // Drain remaining batches.
            while let Some(config) = scheduler.next_batch() {
                total_assigned += config.batch_size as u64;
                scheduler.record_outcome(BatchOutcome {
                    task_index: config.task_index,
                    iterations_used: config.batch_size,
                    exhausted: false,
                    rank: 0,
                    summary: None,
                });
            }

            // The remaining budget after the first batch is
            // (budget - batch_size) merged with deferred_budget.
            // Total should not exceed budget + deferred_budget.
            let max_total = budget as u64 + deferred_budget as u64;
            prop_assert!(
                total_assigned <= max_total,
                "assigned {} > max {} (budget={}, deferred={}, batch={})",
                total_assigned, max_total, budget, deferred_budget, batch_size,
            );
        }

        /// Random interleaving of enqueue calls for arbitrary task_indices
        /// (in-flight, queued, new, deferred) never panics and always
        /// returns a valid EnqueueResult.
        #[test]
        fn enqueue_never_panics(
            task_count in 1_usize..5,
            batch_size in 1_u32..50,
            ops in proptest::collection::vec(0_usize..10, 5..=20),
        ) {
            let mut scheduler = BatchScheduler::new(task_count, None, batch_size);
            let mut in_flight = Vec::new();

            for op in ops {
                match op % 3 {
                    0 => {
                        // Try to pop a batch.
                        if let Some(config) = scheduler.next_batch() {
                            in_flight.push(config.task_index);
                        }
                    }
                    1 => {
                        // Enqueue for a random task_index — may be new,
                        // in-flight, queued, or already deferred.
                        let target = op % (task_count + 3);
                        let result = scheduler.enqueue(target, Some(50));
                        prop_assert!(
                            matches!(result, EnqueueResult::Queued | EnqueueResult::Deferred | EnqueueResult::Merged),
                        );
                    }
                    _ => {
                        // Release one in-flight task if any.
                        if let Some(ti) = in_flight.pop() {
                            scheduler.record_outcome(BatchOutcome {
                                task_index: ti,
                                iterations_used: batch_size,
                                exhausted: false,
                                rank: 0,
                                summary: None,
                            });
                        }
                    }
                }
            }

            // Clean up: release all remaining in-flight.
            for ti in in_flight {
                scheduler.record_outcome(BatchOutcome {
                    task_index: ti,
                    iterations_used: batch_size,
                    exhausted: true,
                    rank: 0,
                    summary: None,
                });
            }
        }
    }

    // --- is_frontier_exhausted tests (str-b2my.5) ---

    #[test]
    fn frontier_exhausted_false_on_first_batch() {
        let mut s = BatchScheduler::new(2, None, 50);
        assert!(!s.is_frontier_exhausted(), "before any batch");

        let _b = s.next_batch().unwrap();
        assert!(
            !s.is_frontier_exhausted(),
            "in-flight entry on its first batch"
        );
    }

    #[test]
    fn frontier_exhausted_true_when_all_rank_zero() {
        let mut s = BatchScheduler::new(2, None, 50);

        let b0 = s.next_batch().unwrap();
        s.record_outcome(BatchOutcome {
            task_index: b0.task_index,
            iterations_used: 50,
            exhausted: false,
            rank: 0,
            summary: None,
        });
        let b1 = s.next_batch().unwrap();
        assert!(
            !s.is_frontier_exhausted(),
            "second function still on first batch"
        );
        s.record_outcome(BatchOutcome {
            task_index: b1.task_index,
            iterations_used: 50,
            exhausted: false,
            rank: 0,
            summary: None,
        });

        assert!(
            s.is_frontier_exhausted(),
            "all functions explored with no discoveries"
        );
    }

    #[test]
    fn frontier_exhausted_false_when_any_rank_positive() {
        let mut s = BatchScheduler::new(2, None, 50);

        let b0 = s.next_batch().unwrap();
        s.record_outcome(BatchOutcome {
            task_index: b0.task_index,
            iterations_used: 50,
            exhausted: false,
            rank: 3,
            summary: None,
        });
        let b1 = s.next_batch().unwrap();
        s.record_outcome(BatchOutcome {
            task_index: b1.task_index,
            iterations_used: 50,
            exhausted: false,
            rank: 0,
            summary: None,
        });

        assert!(
            !s.is_frontier_exhausted(),
            "function 0 still has positive rank"
        );
    }

    #[test]
    fn frontier_exhausted_false_with_deferred_work() {
        let mut s = BatchScheduler::new(1, None, 50);
        let b = s.next_batch().unwrap();

        assert_eq!(s.enqueue(0, Some(100)), EnqueueResult::Deferred);
        assert!(
            !s.is_frontier_exhausted(),
            "deferred work pending"
        );

        s.record_outcome(BatchOutcome {
            task_index: b.task_index,
            iterations_used: 50,
            exhausted: false,
            rank: 0,
            summary: None,
        });

        assert!(
            !s.is_frontier_exhausted(),
            "deferred work just drained, batches_completed reset"
        );
    }

    #[test]
    fn frontier_exhausted_false_when_complete() {
        let mut s = BatchScheduler::new(1, Some(50), 50);
        let b = s.next_batch().unwrap();
        s.record_outcome(BatchOutcome {
            task_index: b.task_index,
            iterations_used: 50,
            exhausted: true,
            rank: 0,
            summary: None,
        });

        assert!(s.is_complete());
        assert!(
            !s.is_frontier_exhausted(),
            "complete means nothing left, not fallback"
        );
    }

    #[test]
    fn frontier_exhausted_clears_on_new_discovery() {
        let mut s = BatchScheduler::new(2, None, 50);

        for _ in 0..2 {
            let b = s.next_batch().unwrap();
            s.record_outcome(BatchOutcome {
                task_index: b.task_index,
                iterations_used: 50,
                exhausted: false,
                rank: 0,
                summary: None,
            });
        }
        assert!(s.is_frontier_exhausted());

        let b = s.next_batch().unwrap();
        s.record_outcome(BatchOutcome {
            task_index: b.task_index,
            iterations_used: 50,
            exhausted: false,
            rank: 2,
            summary: None,
        });

        assert!(
            !s.is_frontier_exhausted(),
            "function 0 has positive rank again"
        );
    }

}
