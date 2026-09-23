# Static execution-budget allocation for scans

Date: 2026-09-23. Status: approved in chat (brainstorm 2026-09-22/23), amended after Codex cross-check (2026-09-23, 12 findings, all addressed below). Tracker: epic str-03mfx, children str-03mfx.1 (A), .2 (B), .3 (C), .4 (D), .5 (E).

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
coverage where it can be had.

## Non-goals

- No learned or history-based allocation (tier 2) and no decision-model
  allocation (tier 3). The score is a documented integer-weighted formula.
- No change to single-function `shatter explore`.
- No change to wall-clock budgeting (`timeout_per_fn`, `timeout_total`).
- No new clap flags; the knob is config-only, reachable through `--set`.
- The random explorer (`explorer.rs`) is untouched: it interprets
  `max_iterations` as executions and already claims from the surplus.
  `static` applies to the concolic path only.

## Compatibility contract

One knob, `defaults.exploration.budget_allocation`, with two values:

- `flat` (default): **behavior identical to today**, including the dead
  surplus (donation in mismatched units, no concolic claiming). Nothing in
  this spec runs under `flat` except reading the knob.
- `static`: everything below.

New scan-JSON fields are emitted only when non-zero
(`skip_serializing_if`), so `flat` output stays byte-identical to the
pre-change binary. The e2e test in child C checks this against a JSON
captured from the pre-change `main` binary, not against a same-binary run.

## Design

### 1. Allocator module (`shatter-core/src/budget_alloc.rs`, new)

Pure functions, no I/O.

```rust
pub struct BudgetFeatures { branches, opaque_branches, loops, deps, module_imports, complex_params, param_nesting, lines }
pub fn features(analysis: &FunctionAnalysis) -> BudgetFeatures
pub fn score(f: &BudgetFeatures) -> f64                      // >= 1.0 always
pub struct Demand { pub score: f64, pub floor: u32, pub ceiling: u32 }
pub struct Allocation { pub per_function: Vec<u32>, pub total: u32, pub infeasible: Option<Infeasible> }
pub fn allocate(demands: &[Demand], total: u32) -> Allocation
```

`features` reads only `FunctionAnalysis`: `branches.len()`, branches whose
`condition` is `None` (opaque to Z3), `loops.len()`, `dependencies.len()`,
dependencies of kind `ModuleImport` (`module_imports`; the static analogue of an unmocked import, which is a runtime-detection kind), `complex_params` = parameters whose
`TypeInfo` is `Str`, `Array`, `Object`, `Union`, or `Nullable` of one of
those, `param_nesting` = maximum container depth over all parameters (scalar
0; `Array`/`Nullable` add 1 plus their element's depth; `Object` adds 1 plus
the max over fields; `Union` adds 1 plus the max over variants), and
`lines = end_line − start_line + 1`.

`score` is `1 + w_b·branches + w_o·opaque + w_l·loops + w_d·deps + w_m·module_imports + w_p·complex_params + w_n·nesting + w_s·lines/20`
with integer weights held in one `const` table (initial: b=1, o=2, l=3, d=1,
m=2, p=2, n=1, s=1). The weights are the only tunable and the benchmark is
what tunes them.

`allocate` is water-filling with per-function bounds, so the total is
conserved by construction:

1. If `Σ floor > total`, scale every floor down proportionally (integer
   floor) and record `Infeasible::FloorsScaled`. If `Σ ceiling < total`,
   every function gets its ceiling, the remainder is dropped and recorded as
   `Infeasible::CeilingsBind(remainder)`. Otherwise `infeasible = None`.
2. Give every function its floor. `remaining = total − Σ floor`.
3. Loop: among functions not yet at their ceiling ("open"), distribute
   `remaining` proportionally to score, rounding down. Any function whose
   share would exceed its ceiling is set to its ceiling and closed; recompute
   `remaining` and repeat while any function was closed in the pass.
4. Hand out the final rounding remainder one execution at a time to open
   functions in descending score, ties by ascending index, until zero.

Invariants (proptests): `Σ per_function == total` whenever
`Σ floor ≤ total ≤ Σ ceiling`; every share within its own `[floor, ceiling]`;
among functions that end **open** (strictly between their bounds), a higher
score never receives a smaller increment above its floor (`share − floor`); with equal scores and equal bounds the split is
uniform; deterministic; `allocate` never panics for any non-negative input
including `total = 0`, empty input, and `ceiling < floor` (treated as
`ceiling = floor`).

Per-function bounds at the call site: `floor = budget_floor` (default 20),
`ceiling = default_max_executions(f) × budget_ceiling_factor` (default 4.0)
where `default_max_executions(f)` is what `concolic_scan_max_executions`
yields today (500, or 100 with custom generators). Layer total =
`Σ default_max_executions(f)` over the **runnable** functions of the layer
(cached and skipped functions are excluded before allocation, see §3).
`budget_ceiling_factor < 1.0` and `budget_floor == 0` are rejected at config
load with a message naming the key.

### 2. Config knob

`ExplorationConfig` (the `defaults.exploration:` block) gains

```yaml
defaults:
  exploration:
    budget_allocation: flat | static     # default flat
    budget_floor: 20                     # executions, >= 1
    budget_ceiling_factor: 4.0           # × per-function default, >= 1.0
```

Reachable via `--set defaults.exploration.budget_allocation=static`. A unit
test resolves a config with the key set through the same path the CLI uses
and asserts the value reaches the scan config; a second test asserts the
`--set` form lands in the same place.

### 3. Scan wiring (`scan_orchestrator.rs`)

Per topological layer, **after** the runnable task set is known (cache hits
and unsupported functions removed) and before dispatch: when `static`,
compute `features`/`score` per runnable function, build `Demand`s, call
`allocate` with the layer total, and set each task's `max_executions` to its
share and `max_iterations` to `max(1, share / 5)` (integer floor; 1:1 for
custom-generator functions). Log per function at debug as
`budget: <qualified_id> score=<S:.1> executions=<N>` and per layer at info as
`budget: layer <k>: <n> functions, total <T>, share min/median/max <a>/<b>/<c>`;
if `infeasible` is set, log it at warn once per layer.

Replica expansion and batched execution today split only `max_iterations`
across replicas / batches; under `static` they split `max_executions` the
same way (each replica gets `share / replicas`, remainder to the first), so a
function's total across replicas equals its share. A unit test covers the
replica split.

The `BudgetSurplus` and `ClaimPolicy` types move from `scan_orchestrator.rs`
to `budget_alloc.rs` with `pub use budget_alloc::{BudgetSurplus, ClaimPolicy};`
left in `scan_orchestrator.rs`, so the orchestrator can depend on them
without importing the scan module.

### 4. Surplus fix and concolic claiming (under `static` only)

- Donation uses executions and accounts for claims:
  `donate = (allocated + claimed) − consumed`, never negative.
- `orchestrator::ExploreConfig` gains `budget_surplus: Option<Arc<BudgetSurplus>>`
  (default `None`) and `claim_policy: ClaimPolicy` (default). The scan passes
  the layer surplus into the concolic config only when `static`.
- The effective caps live on the loop-local `ExploreBudget` struct, which
  gains `max_executions: Option<usize>` and `max_iterations: Option<usize>`
  (both initialised from the config), `claimed: u32`, and
  `recent_new_paths: VecDeque<bool>` of capacity `ClaimPolicy.window`, pushed
  once per execution. Both termination checks compare against `budget`, not
  `config`. When either cap would terminate, a surplus is attached, and
  `should_claim(count of true flags)` holds, call
  `try_claim(policy.max_claimable(available), 1)` and on a non-zero claim `c`
  add `c` to `budget.max_executions`, `max(1, c / 5)` to
  `budget.max_iterations` (`c` for custom-generator functions), `c` to
  `budget.claimed`, and continue. `clamp_fuzz_budget` and progress hints read
  the caps from `budget`. `ExploreConfig` is never mutated. Claims are not
  bounded by the per-function ceiling; the ceiling shapes the static split,
  the surplus corrects it.
- `ExploreResult.budget_claimed: u32`; scan JSON `FunctionReport` gains
  `budget_allocated: u32` and `budget_claimed: u32`, serialized only when
  non-zero; one scan-total summary line `budget: donated D, claimed C
  executions` after the existing summary when either is non-zero.

### 5. Deterministic fuzz phase (child E, prerequisite for the benchmark)

The in-loop fuzz phase seeds its RNG with `StdRng::from_os_rng()`, so two
runs with the same `--seed` differ. It is changed to derive from the
exploration seed when one is set (`seed ^ 0xF0ZZ` style domain separation,
new `StdRng` per fuzz phase from a counter) and to keep OS entropy only when
no seed is configured. A unit test asserts two seeded runs of the fuzz phase
over a scripted frontend produce identical candidate sequences. This is
independent of the allocator and lands first so the benchmark can attribute
differences to allocation.

### 6. Measurement (`benchmarks/budget-allocation/`, new)

A scan-level benchmark script `scripts/bench_budget_alloc.py` that, per
corpus and per arm (`flat`, `static`), runs `shatter scan` in a **fresh copy
of the corpus** with `--no-cache`, `--seed <s>`, `--parallelism 1`, and the
arm's `--set`, for `k` seeds (default 3), and measures per function:
**branch sides covered** (from the run's raw execution results, the same
`branch_id × 2 + taken` measure the frontier benchmark uses; exposed on
`FunctionReport` as `branch_sides_covered` / `branch_sides_total`, emitted
only when non-zero like the other new fields), lines covered,
`completion_outcome`/`completion_reason`, executions **used** (`iterations`),
`budget_allocated`, `budget_claimed`, and wall-clock. Report: per corpus, a
paired table over seeds of Σ sides, Σ lines, Σ executions used vs Σ
allocated, donated/claimed, wall-clock, with medians and per-seed rows; plus a
score-ordered per-function table. Sanity gate: the `flat` arm's Σ sides and Σ
lines equal a knob-absent run with the same seed and fresh copy (exact, since
child E makes runs repeatable); mismatch fails the script.

Corpora: examples `standalone/ts` and `standalone/go` (via the Taskfile
target), and zolem `internal/`. For zolem the script is run from the zolem
checkout inside its sandbox conventions by invoking `shatter scan` directly
with `SHATTER_BIN`, the sandbox flags `scripts/shatter-scan-lib.sh` sets, and
a separate output directory per arm; if that cannot be made to work without
changing zolem, add pass-through of `--seed`, `--parallelism` and `--set` to
`scripts/shatter-focused-scan.sh` on a zolem branch and record it in the
report. The report states which route was used.

### 7. Testing

- Unit + proptests for `features`, `score`, `allocate` (invariants in §1),
  the replica split, and a unit test on real analyses that
  `score(01-arithmetic.ts::classifyNumber) < score(16-cron-parser.ts::parseCron)`.
- Config tests in §2.
- Unit tests for donation with claims (allocated 100, claimed 50, used 110 →
  donate 40; used 160 → 0) and for concolic claiming with a scripted frontend
  (surplus 50; a function still finding paths at its cap claims and both caps
  rise; a plateaued function does not claim; a custom-generator function
  hitting both caps at once claims once).
- Child E: seeded fuzz determinism test.
- E2e (child C, run via `task e2e-ts`): three-file TS corpus copied to a temp
  dir, `--seed 1 --parallelism 1 --no-cache`, both arms: Σ allocated equal;
  allocations ordered like the allocator's own scores; `flat` JSON equals a
  JSON captured from the pre-change binary after removing timestamp and
  duration fields (listed in the test); under `static`, donated + claimed > 0.
- Existing surplus and explorer claim tests unchanged.
- No protocol-visible frontend change, so no parity-matrix update.

## Delivery

Five children, one branch each: A (allocator, knob, scan wiring incl.
replica split, module move), E (seeded fuzz RNG; independent), B (surplus
fix and concolic claiming; blocked by A), C (e2e test; blocked by A and B),
D (benchmark, Taskfile target, doc, first report; blocked by A, B, E). The
decision to flip the default to `static` is taken on the epic after reading
D's report.

## Acceptance

1. `task affected` green on every child; `task e2e-ts` and `task e2e-go`
   green on A, B, C, E.
2. Under `flat` (default) the scan JSON for the e2e corpus equals the
   pre-change binary's output after removing timestamp and duration fields.
3. D's report shows, per corpus, paired over seeds at identical Σ allocated:
   Σ branch sides and Σ lines for `flat` and `static`, Σ executions used,
   and wall-clock. Whether `static` wins is the report's finding, not a
   precondition; if it loses anywhere the report names the functions.
4. Donated and claimed are non-zero in the `static` arm.

## Risks

- Score weights are guesses until the benchmark tunes them; shipping default
  `flat` bounds the risk.
- Claims raise loop-local caps; any code that reads `config.max_executions`
  or `config.max_iterations` as a constant must be found (grep) and switched
  to `budget`.
- Layer totals shrink with layer size; a one-function layer cannot
  redistribute. Expected.
- Equal allocated totals do not imply equal executions used (plateau stops
  early); the report shows both so the comparison is honest.
