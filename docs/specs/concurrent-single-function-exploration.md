# Concurrent Single-Function Exploration — Semantics Spec

Status: Draft (str-frc.1)
Owner: str-frc epic
Scope: defines the semantic contract that concrete implementation issues
str-frc.2 … str-frc.7 must satisfy. No implementation details beyond what is
needed to nail down observable behavior.

## 1. Goal and Boundary

Allow a single function's exploration loop in Shatter — currently a
sequential candidate → execute → feedback loop in
`shatter-core/src/explorer.rs::explore_function` (random) and
`shatter-core/src/orchestrator.rs::explore` (concolic) — to overlap
candidate generation with frontend execution by running multiple frontend
subprocesses ("observers") for one function.

In scope:

- Determinism contract under parallel observers.
- Setup/teardown lifecycle across observers.
- Candidate queue capacity, duplicate suppression, drain, and timeout.
- `ObservationOutput` ordering and progress counter semantics.
- Initial exploration mode coverage (random vs. concolic).
- Required regression fixtures.

Out of scope (per parent epic str-frc):

- Concurrent `Execute` requests on a single frontend subprocess.
- Scan-level parallelism (already handled).
- Frontend protocol changes.
- Public CLI/config knob design (str-frc.6).

## 2. Current Sequential Model (Reference)

Today, both code paths are strictly serial within one function:

- `explore_function` (random) seeds an `StdRng` from
  `config.seed.unwrap_or_else(from_os_rng)` and drives one `Frontend`
  through `Instrument`, optional `Setup` (per `SetupLevel::Function`),
  optional `Prepare`, then a loop of `Execute` per candidate.
- `orchestrator::explore` (concolic) builds a `MetaStrategy` and consumes
  one candidate at a time. Z3 solving runs synchronously inside
  `Z3SolverStrategy.feedback()` between executions. Note that the outer
  RNG in the concolic path is currently `StdRng::from_os_rng()` — it does
  not honor `config.seed`. This is an existing gap that the concurrency
  work must not make worse and should ideally close (see §3.2).
- `ObservationOutput` (`shatter-core/src/explorer.rs:241`) accumulates
  `new_path_executions`, `raw_results`, and `discoveries` in execution
  order.
- `SetupLevel` (`shatter-core/src/protocol.rs:31`) has variants `Session`,
  `File`, `Function`, `Execution`. `observe.rs` interleaves
  setup/teardown around each `Execute` for `SetupLevel::Execution`.

## 3. Determinism

### 3.1 Determinism Modes

Two modes are defined. `--seed` is a request for **bounded reproducibility**
by default; **strict reproducibility** is opt-in.

| Mode | Trigger | Guarantee |
|---|---|---|
| Bounded | `--seed S` (default under `observer_pool > 1`) | Same set of executed candidates and the same final `unique_paths`, `lines_covered`, `discoveries` set, `new_path_executions` set (modulo order). RNG state for candidate generation is seeded from `S`. Execution order across observers is unspecified. |
| Strict | `--seed S --deterministic` (or `observer_pool=1`) | Byte-identical `ObservationOutput` across runs, including ordering of `new_path_executions`, `raw_results`, and `discoveries`. Implies `observer_pool=1` for now. |
| Unseeded | no `--seed` | No reproducibility guarantee. |

Rationale: making strict reproducibility the default under parallelism
forces serial-in-effect execution and defeats the point of the epic.
Bounded reproducibility is enough for: regression diffs of coverage and
witness inputs, debugging path-discovery logic, and CI snapshot stability
once results are canonicalized (§5).

### 3.2 RNG Seeding Under Parallelism

- The orchestrator owns one master `StdRng` seeded from `config.seed` when
  set; otherwise from OS entropy. **The concolic path must adopt the same
  contract** — fix the existing `from_os_rng()` site in
  `orchestrator::explore` as part of str-frc.4 or earlier.
- Per-observer worker RNGs are derived deterministically from the master
  RNG (e.g. `StdRng::seed_from_u64(master.next_u64())`) at worker-spawn
  time. Each worker's RNG is independent — no shared mutable RNG.
- Candidate-generation strategies that are seed-sensitive
  (`MetaStrategy.next`, `generate_mock_values`, fuzzer mutation) must
  draw from a producer-side RNG that is also derived from the master
  RNG, **not** from the worker RNGs. This separates "what gets generated"
  from "which worker happened to run it."

### 3.3 What Bounded Reproducibility Promises

For two runs A and B with the same inputs, `--seed S`, and any
`observer_pool` size, after sorting by a stable canonical key:

- `unique_paths`, `lines_covered`, `total_lines`, `iterations` (see §6.2),
  `timed_out`, `mcdc_summary`: equal.
- Sets of `(branch_id, DiscoveryMethod)` tuples in `discoveries`: equal.
- Set of canonicalized `ExecutionSummary` entries in
  `new_path_executions`: equal.
- Set of `(inputs, mocks)` keys in `raw_results`: equal.

For two runs A and B with the same inputs, `--seed S`, and
`observer_pool=1`, additionally:

- The vectors above are byte-identical in order.

What bounded reproducibility does **not** promise:

- Wall-clock execution order.
- Which specific candidate triggered timeout cutoff under
  `timeout_explore`.
- Tie-breaking among candidates that discover the same path concurrently
  (see §6.3 attribution rule).

## 4. Setup, Execution, and Teardown Across Observers

The setup lifecycle is multiplied across N observer subprocesses. Each
`SetupLevel` is handled as follows.

### 4.1 `SetupLevel::Session`

- Setup runs once per observer subprocess at worker-spawn time, before
  any `Execute`. Teardown runs once per observer at pool shutdown.
- The session context is per-observer; observers do not share session
  state. Tests that rely on cross-execution state in a session must
  document that they tolerate per-observer state (str-frc.3 fixture).

### 4.2 `SetupLevel::File`

- Same as Session for the concurrency model: one setup + teardown per
  observer per file. Not multiplexed across observers.

### 4.3 `SetupLevel::Function`

- Setup runs once per observer per function entry, before that observer
  takes its first candidate for the function. Teardown runs once per
  observer when the candidate queue drains for that function.
- The shared `SetupManager` cache (`shatter-core/src/setup_manager.rs`) is
  per-observer, not pool-shared. `SetupManager::should_skip` is consulted
  against the per-observer cache only.

### 4.4 `SetupLevel::Execution`

- Setup and teardown bracket every `Execute` on the observer that runs
  it, identical to today's
  `observe_batch_with_per_execution_setup`. No change.

### 4.5 Failure Semantics

- A setup failure on one observer does not cancel other observers'
  in-flight executions. The failing observer is removed from the pool
  for the remainder of this function and its claimed candidate is
  returned to the queue (see §5.4 for re-claim policy).
- A teardown failure is logged and surfaces as a side-effect record but
  does not corrupt `ObservationOutput`.

## 5. Candidate Queue

### 5.1 Capacity

- Bounded MPMC queue. Default capacity:
  `min(4 * observer_pool, max_iterations.unwrap_or(256))`.
  Rationale: 4× pool size keeps observers fed without prematurely burning
  budget on candidates that may be invalidated by a fresh `feedback()`
  result; the `max_iterations` cap prevents over-allocating when the
  iteration budget is tiny.
- Capacity is internal in str-frc.5; externalized as a knob in str-frc.6.

### 5.2 Duplicate Suppression

The queue tracks a bounded LRU set of `path_hash`-equivalent candidate
fingerprints (computed from the candidate's `(inputs, mocks)` canonical
JSON, **not** from path hash — path is unknown pre-execution).

- On enqueue, candidates whose fingerprint is already in the LRU are
  dropped silently and counted under a `duplicates_suppressed`
  diagnostic counter.
- LRU size: 4× queue capacity. This is a best-effort filter — the
  authoritative dedup remains the post-execution `seen_paths` set in
  `ObserveState` / `covered_paths` in `ExploreState`.

### 5.3 Shutdown / Drain

- "Shutdown" is initiated when any of the following fire:
  termination_reason from the orchestrator (worklist exhausted, plateau,
  budget exceeded), `timeout_explore` trip, `max_iterations` reached, or
  a fatal error.
- Shutdown is **drain-then-stop**:
  1. Producers stop enqueueing new candidates.
  2. Observers finish their currently-claimed candidate. Their result is
     aggregated normally.
  3. Remaining queued (unclaimed) candidates are discarded.
  4. Per-observer per-function teardown runs.
- "Hard cancel" (only on process signal or unrecoverable error): claimed
  candidates are abandoned, observers are killed, teardown is best-effort.
  `ObservationOutput.timed_out=true` is set when the cause was the
  per-function timeout (preserving the existing `timed_out` semantics
  from str-gz8j).

### 5.4 Re-claim on Observer Failure

If an observer process dies or its `Execute` returns a frontend error
that is classified as "worker fault" (not "candidate fault"):

- The candidate is returned to the front of the queue at most once.
  Subsequent failure on the same candidate marks it as a frontend error
  and increments `iterations` without contributing a new path.
- Persistent observer fault (>= 3 consecutive worker faults) drops the
  observer from the pool. If pool size falls to 0, the loop terminates
  with the existing `ExploreError::Frontend` path.

### 5.5 Timeout

- `config.timeout_explore` is checked at the producer/aggregator side
  at queue-poll boundaries, not inside observer workers. This avoids
  killing in-flight `Execute` calls mid-flight unless the timeout
  exceeds an observer-side soft deadline (default: `timeout_explore`).
- On trip, drain-then-stop (§5.3) runs and `timed_out=true` is set.

## 6. Result Ordering and Progress Counters

### 6.1 Aggregation Seam

A single aggregator owns the authoritative state previously held inside
the `explore_function` / `orchestrator::explore` body:

- `ObserveState.seen_paths` (random) / `covered_paths` (concolic).
- `discoveries`, `seen_branch_ids`, `seen_branch_sides`,
  `frontier_set`, `target_branches`, `fitness_context`, `LoopBuckets`.
- `new_path_executions`, `raw_results`.

Observers send completed `(inputs, mocks, ExecuteResult)` tuples to the
aggregator over a channel. The aggregator processes them serially, in
arrival order. This is what str-frc.2 must extract.

### 6.2 `ObservationOutput` Ordering

- `iterations`: total of all `Execute` calls that produced a non-error
  result, across all observers. Failed `Execute` calls count toward the
  iteration budget under the same rules used today (the aggregator owns
  the increment).
- `unique_paths`, `lines_covered`, `total_lines`, `timed_out`,
  `mcdc_summary`, `nondeterministic_fields`, `float_probe_results`,
  `boundary_results`, `shrunk_witnesses`, `shrink_stats`,
  `abandoned_frontiers`, `opaque_suggestions`, `stubbed_modules`:
  values are pool-size-invariant under bounded reproducibility (§3.3).
- `new_path_executions`: ordered by aggregator arrival time under
  bounded reproducibility. Under strict reproducibility (`observer_pool=1`)
  the existing serial order applies. Snapshot/regression tests must sort
  by canonical key (e.g. `(branch_path_hash, canonical_inputs)`) before
  comparison.
- `raw_results`: same ordering rule as `new_path_executions`.
- `discoveries`: same. Under bounded reproducibility two runs may
  attribute the same `branch_id` to different `DiscoveryMethod`s when
  the same branch is found near-simultaneously by, say, the random
  generator on one observer and a Z3 solution on another; see §6.3.

### 6.3 Discovery Attribution Tie-Break

When two observers complete an execution that newly covers the same
`(branch_id, side)` and the aggregator processes them in either order:

- The first one to be aggregated wins attribution.
- Under `observer_pool=1`, attribution is deterministic and matches
  today's behavior.
- Under bounded reproducibility, attribution is allowed to vary across
  runs. Regression fixtures must not assert specific attribution unless
  the fixture forces `observer_pool=1`.

### 6.4 Progress Counters

`ExploreProgressSnapshot` is emitted at the existing
`PROGRESS_SUMMARY_INTERVAL_SECS` cadence by the aggregator only. Fields:

- `iterations`: aggregator's running total. Monotonic, but observers'
  in-flight executions are not counted until aggregation.
- `paths_found`, `branches_covered`, `mcdc_summary`: same as today,
  computed against aggregator state.
- `iters_since_new_discovery`: distance in `iterations` between the
  current snapshot and the last aggregator-observed new branch. Because
  observers may finish out of order, this can momentarily fail to advance
  even when in-flight observers have already discovered new branches.
  Acceptable — the invariant is monotonic non-decrease over time, not
  freshness.
- `iters_since_new_discovery` resets to 0 only at aggregation time.

## 7. Initial Implementation Mode Scope

### 7.1 Random First, Concolic Second

The first implementation (str-frc.3) covers **random exploration only**:

- Observer pool is wired into `explore_function`.
- Concolic `orchestrator::explore` continues to run sequentially.

Reasons:

- The concolic path's solver/feedback loop is tightly coupled to a
  single-threaded `MetaStrategy.feedback()` step. Decoupling that requires
  a separate scheduling seam (str-frc.4) that should not block the
  random observer pool from landing.
- The random path already has a clean candidate-generation step that
  does not need feedback synchronization between executions.

### 7.2 Concolic Path

str-frc.4 (concolic solver offload) lands after str-frc.2/3. It introduces
an async generator seam so Z3 solving runs concurrently with frontend
observation. Until then the concolic path is single-observer regardless
of `observer_pool` setting. The CLI knob (str-frc.6) must not silently
reject `observer_pool>1` for concolic — it is accepted and clamped to 1
with a warning, per existing parallelism conventions.

### 7.3 Custom Generators

str-frc.7 (decision-only) keeps the existing constraint that custom
generators run on a dedicated frontend, serialized. Observer pool members
do **not** receive `Generate` requests — only `Execute`. If the function
uses custom generators, the producer side calls `Generate` on a single
generator-frontend (today's behavior, preserved). Revisit only if
benchmarks justify generator-side parallelism.

## 8. Required Regression Fixtures

These must exist before str-frc.3 closes; str-frc.5 adds the queue-policy
fixtures.

| Fixture | Owner | Asserts |
|---|---|---|
| `random_pool_one_matches_serial` | str-frc.3 | `observer_pool=1` produces byte-identical `ObservationOutput` to today's serial path on a model function (e.g. `examples/go/04-nested-control-flow.go` analog in TS). |
| `random_pool_n_bounded_repro` | str-frc.3 | `observer_pool=4 --seed=42` run twice produces equal canonical sets per §3.3. Sort-and-compare. |
| `setup_function_per_observer` | str-frc.3 | With `SetupLevel::Function` and `observer_pool=4`, exactly N setup calls and N teardown calls are made (where N = observers actually claimed for this function). |
| `setup_execution_unchanged` | str-frc.3 | With `SetupLevel::Execution`, setup/teardown count equals `iterations`. |
| `worker_fault_recovery` | str-frc.3 | Killing one observer mid-run leaves `iterations` and `unique_paths` consistent with the surviving pool; one candidate may be re-tried. |
| `queue_drain_on_timeout` | str-frc.5 | `timeout_explore` trip yields `timed_out=true`, no queued-but-unclaimed candidate appears in `raw_results`, all observers performed teardown. |
| `queue_duplicate_suppression` | str-frc.5 | Repeating identical `(inputs, mocks)` tuples upstream produces a single `Execute` (best-effort; no test asserts strict 1:1, just `<= input_count`). |
| `queue_budget_accounting` | str-frc.5 | `iterations` counts executed candidates, not enqueued candidates. |
| `concolic_pool_clamps_to_one` | str-frc.4 | Setting `observer_pool=4` with concolic mode runs serially and emits a warning until str-frc.4 lands the offload. |
| `concolic_offload_keeps_attribution` | str-frc.4 | Async-solver mode preserves `DiscoveryMethod::Z3` attribution on solved candidates. |

Snapshot-style regression tests must canonicalize before comparison. Add
a helper `canonicalize_observation_output(&mut ObservationOutput)` in
test support that sorts the order-sensitive vectors by their canonical
key.

## 9. Child Issue Impact

These notes flag scope adjustments to existing str-frc child issues. The
team-lead applies any beads edits.

- **str-frc.2 (Observation aggregation seam):** Confirmed in scope. The
  seam owns the aggregator described in §6.1 and must support
  out-of-order arrivals. Acceptance criterion already covers this; no
  scope change.
- **str-frc.3 (Explore observer pool):** Acceptance criterion mentions
  "no concurrent Execute on one frontend" — keep. Add fixtures from §8.
  Confirmed scoped to random only per §7.1; this is consistent with the
  issue's existing wording but worth pinning down ("random only" should
  appear in the description).
- **str-frc.4 (Concolic solver offload):** Add a note that the concolic
  path must also adopt seeded RNG behavior matching §3.2. Currently the
  orchestrator uses `from_os_rng()` unconditionally. This is a
  pre-existing bug that this issue should fix as part of the offload to
  avoid muddying the parallel determinism model. **Recommend updating
  str-frc.4's description to call this out.**
- **str-frc.5 (Candidate queue backpressure):** In scope as described.
  Add the §5.4 re-claim and §5.5 timeout placement requirements to the
  acceptance criteria. The current criterion covers draining and dedup
  but not the re-claim-once policy.
- **str-frc.6 (Concurrency CLI knobs):** Confirm the warn-and-clamp
  behavior for `observer_pool>1` under concolic mode (§7.2) is part of
  the CLI parsing tests.
- **str-frc.7 (Custom generator concurrency):** §7.3 records the
  defer-with-rationale decision. If the team-lead wants to close
  str-frc.7 as "deferred, documented here," that is supported by this
  spec. Otherwise str-frc.7's acceptance criterion ("documented decision")
  is met by linking to §7.3.

## 10. Open Questions

These are flagged as questions, not blockers — sensible defaults are
adopted in the spec but the team-lead should confirm.

1. Strict-determinism flag name: `--deterministic`, `--strict-seed`, or
   inferring from `observer_pool=1`? Spec assumes the inferred form for
   now (no new flag), with `--deterministic` reserved for future use.
2. Re-claim retry count: spec says "once" (§5.4). Could be configurable
   alongside other retry policy in str-frc.6 if user demand surfaces.
3. Whether `iterations` should count enqueued-but-discarded candidates
   on shutdown. Spec says no (executed only); this matches today's
   counting and is the simpler default.
