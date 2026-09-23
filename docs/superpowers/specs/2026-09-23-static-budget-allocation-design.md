# Static execution-budget allocation for scans

Date: 2026-09-23. Status: approved in chat (brainstorm 2026-09-22/23). Tracker: epic str-03mfx, children str-03mfx.1 (A), .2 (B), .3 (C), .4 (D).

## Problem

`shatter scan` gives every function the same exploration budget: 100 unique
paths (`max_iterations`), 500 executions (`max_executions`, 5× unless custom
generators are configured), a 20-execution plateau stop, and one wall-clock
`timeout_per_fn`. Analyzed code varies by two orders of magnitude in how much
exploration it can absorb: a three-branch getter saturates in 10 executions, a
string parser with loops and opaque calls is still finding paths at 500. The
only adaptation today is reactive (plateau stop; the strategy bandit inside a
function) and the one redistribution mechanism, the per-layer
`BudgetSurplus`, is broken in two ways:

1. Donation mixes units: it donates `max_iterations − exploration.iterations`,
   but `max_iterations` is the unique-path cap (100) and `iterations` is
   total executions, so the donation is usually zero or meaningless.
2. Claiming is wired only in the random explorer (`explorer.rs`). The concolic
   orchestrator, which is the scan default, never calls `try_claim`, so
   nothing ever receives donated budget.

## Goal

Redistribute a **fixed** per-layer total of executions across the layer's
functions according to a static estimate of need computed from the analysis
Shatter already has, and make surplus pooling actually work so that the
static estimate's errors self-correct. Same total cost as today; more
coverage where it can be had. Cycle-time benefit follows when a user lowers
the default because trivial functions no longer waste their share.

## Non-goals

- No learned or history-based allocation (tier 2) and no decision-model
  allocation (tier 3). The score is a documented integer-weighted formula.
- No change to single-function `shatter explore`.
- No change to wall-clock budgeting (`timeout_per_fn`, `timeout_total`).
- No new clap flags (the CLI enum is near its clap stack budget); the knob
  is config-only, reachable through the existing `--set`.

## Design

### 1. Allocator module (`shatter-core/src/budget_alloc.rs`, new)

Pure functions, no I/O.

```rust
pub struct BudgetFeatures { branches, opaque_branches, loops, deps, unmocked_deps, complex_params, param_nesting, lines }
pub fn features(analysis: &FunctionAnalysis) -> BudgetFeatures
pub fn score(f: &BudgetFeatures) -> f64          // >= 1.0 always
pub struct Allocation { pub per_function: Vec<u32>, pub total: u32 }
pub fn allocate(scores: &[f64], total: u32, floor: u32, ceiling: u32) -> Allocation
```

`features` reads only `FunctionAnalysis`: `branches.len()`, branches whose
`condition` is `None` (opaque to Z3), `loops.len()`, `dependencies.len()`,
dependencies of kind `UnmockedImport`, `complex_params` = parameters whose
`TypeInfo` is `Str`, `Array`, `Object`, `Union`, or `Nullable` of one of
those, `param_nesting` = maximum container depth over all parameters (scalar
0; `Array`/`Nullable` add 1 plus their element's depth; `Object` adds 1 plus
the max over fields; `Union` adds 1 plus the max over variants), and
`lines = end_line − start_line + 1`.

`score` is `1 + w_b·branches + w_o·opaque + w_l·loops + w_d·deps + w_u·unmocked + w_p·complex_params + w_n·nesting + w_s·lines/20`
with integer weights held in one `const` table (initial: b=1, o=2, l=3, d=1,
u=2, p=2, n=1, s=1). The weights are the only tunable and the benchmark is
what tunes them; the constant table carries a comment naming the benchmark.

`allocate` splits `total` proportionally to score, rounds down, clamps each
share to `[floor, ceiling]`, and then redistributes any residue (from
rounding and from clamping) proportionally among functions not at their
ceiling, iterating until the residue is zero or every function is capped.
Invariants (proptests): `sum(per_function) == total` whenever
`n·floor <= total <= n·ceiling`; every share within `[floor, ceiling]`; a
higher score never receives less than a lower one; with equal scores the
split is uniform; `allocate` is deterministic.

Defaults: `floor = 20`, ceiling for a function = `4 × default_max_executions(f)`
where `default_max_executions(f)` is what `concolic_scan_max_executions`
yields today (500, or 100 with custom generators); layer total = the sum of
`default_max_executions(f)` over the layer. `allocate` is called with the
layer-wide maximum ceiling, and each share is additionally clamped to its own
function's ceiling before residue redistribution.

### 2. Config knob

`ExplorationConfig` (already under `exploration:`) gains

```yaml
exploration:
  budget_allocation: flat | static     # default flat
  budget_floor: 20                     # executions
  budget_ceiling_factor: 4.0           # × per-function default
```

Reachable via `--set exploration.budget_allocation=static`. `flat` reproduces
today's behavior exactly.

### 3. Scan wiring (`scan_orchestrator.rs`)

Per topological layer, before tasks are dispatched: when
`budget_allocation == static`, compute `features`/`score` for each function
in the layer from `analysis_map`, call `allocate` with the layer total, and
set each task's `max_executions` to its share and `max_iterations` to
`share / 5` (preserving today's ratio; custom-generator functions use the
1:1 ratio they have today). Log one line per function at debug
(`name: score S → N executions`) and one per layer at info
(`layer k: N functions, total T executions, min/median/max share`).

The `BudgetSurplus` and `ClaimPolicy` types move from `scan_orchestrator.rs`
to the new `budget_alloc.rs` (re-exported from their old path) so the
orchestrator can depend on them without importing the scan module.

### 4. Surplus fix and concolic claiming

- Donation uses executions: `allocated_executions − total_executions`.
- `orchestrator::ExploreConfig` gains `budget_surplus: Option<Arc<BudgetSurplus>>`
  and `claim_policy: ClaimPolicy` (default: none / default policy, so every
  existing construction is unchanged apart from `..Default::default()`
  literals, which already exist for the frontier ranker).
- The effective cap lives on the loop-local `ExploreBudget` struct
  (`unique_paths`, `total_executions`, `plateau_counter`, `explore_start`),
  which gains `max_executions: Option<usize>` (initialised from the config),
  `claimed: u32`, and `recent_new_paths: VecDeque<bool>` of capacity
  `ClaimPolicy.window`, pushed once per execution. The `MaxExecutions`
  termination check compares against `budget.max_executions`. When it would
  terminate, a surplus is attached, and `should_claim(count of true flags)`
  holds, call `try_claim(policy.max_claimable(available), 1)` (`1` is the
  minimum worthwhile claim) and, on a non-zero claim, add it to
  `budget.max_executions` and `budget.claimed` and continue. `clamp_fuzz_budget`
  and progress hints read the cap from `budget`. `ExploreConfig` is never
  mutated.
- Record claims on `ExploreResult` (`budget_claimed: u32`) and expose
  `budget_allocated` / `budget_claimed` on the scan JSON `FunctionReport`
  (serde default 0) so the summary line and the benchmark can report them.

### 5. Measurement (`benchmarks/budget-allocation/`, new)

A scan-level benchmark script `scripts/bench_budget_alloc.py` that runs
`shatter scan` twice per corpus with identical seed, `--parallelism 1`, and
identical layer totals, once with `flat` and once with `static`, and
compares from the scan JSON `FunctionReport` fields: Σ `branches_covered` /
Σ `branch_count`, Σ `lines_covered` / Σ `total_lines`, functions by
`completion_outcome`/`completion_reason`, Σ `iterations` (executions used),
Σ `budget_allocated`, Σ `budget_claimed`, and wall-clock. Corpora: the examples
checkout `standalone/ts` and `standalone/go` (reproducible, minutes) and
zolem `internal/` via `make shatter-focused` (real, sandboxed, ~10 min
warm). Output: `report.md` with one table per corpus, flat vs static, plus
the per-function allocation table for inspection. Sanity gate: the `flat`
run's Σ branches_covered and Σ lines_covered must match a third run with the
knob absent (one rerun allowed for subprocess timing noise; two mismatches
fail), proving the harness changes nothing else.

### 6. Testing

- Unit + proptests for `features`, `score`, `allocate` (invariants above).
- Unit tests for the surplus fix (donation in executions) and for claiming in
  the concolic orchestrator with a mock frontend, mirroring the random
  explorer's existing claim tests.
- An e2e scan test over the examples checkout asserting that with `static`
  the layer total equals `flat`'s, a known trivial fixture (`01-arithmetic`)
  gets fewer executions than a known complex one (`16-cron-parser`), and
  `flat` output is byte-identical to today's.
- Existing surplus tests extended for the moved module path.
- Parity: `explorer.rs` (random) already claims; after this change both paths
  claim. No protocol-visible change, so no parity-matrix update.

## Delivery

Four children, one branch each: A (allocator, knob, scan wiring, module
move), B (surplus fix and concolic claiming; blocked by A), C (e2e scan
test; blocked by A and B), D (benchmark, Taskfile target, doc, first
report; blocked by A and B). The decision to flip the default to `static`
is taken on the epic after reading D's report.

## Acceptance

1. `cargo test -p shatter-core` green including new proptests; `task affected`
   green; `cargo test --test e2e_concolic` and `--test e2e_concolic_go` green.
2. `--set exploration.budget_allocation=flat` (default) leaves scan output
   byte-identical on the examples corpus.
3. Benchmark report shows, at identical total executions, `static` covers at
   least as many branch sides as `flat` on both fixture corpora and on zolem,
   and states the wall-clock delta. If it does not, the weights or the
   feature set are wrong; the report says which fixtures lost coverage and
   the default stays `flat`.
4. Surplus donations and claims are non-zero in the `static` run (proof the
   pooling now works).

## Risks

- Score weights are guesses until the benchmark tunes them; shipping default
  `flat` bounds the risk.
- Claiming in the concolic loop extends `max_executions` at runtime; every
  place that reads `config.max_executions` as a constant (fuzz-phase clamp,
  progress reporting) must read the extended cap instead.
- Layer totals shrink with layer size; a one-function layer cannot
  redistribute. Expected and acceptable.
