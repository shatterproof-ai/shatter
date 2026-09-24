# Budget-allocation benchmark

Compares `shatter scan --concolic` under `defaults.exploration.budget_allocation`
`flat` (today's equal per-function budget) and `static` (each layer's total
split by a static score of each function's analysis, with surplus pooling),
at identical total budget. Epic str-03mfx; design in
`docs/superpowers/specs/2026-09-23-static-budget-allocation-design.md`.

## Running

```bash
cargo build --release
task bench-budget-alloc CORPUS=/tmp/shatter-examples-main/standalone/ts INCLUDE='*.ts'
task bench-budget-alloc CORPUS=/tmp/shatter-examples-main/standalone/go INCLUDE='*.go' RESULTS_DIR=target/bench-budget-go
task bench-budget-alloc CORPUS=~/project/zolem/internal INCLUDE='*.go' RESULTS_DIR=target/bench-budget-zolem
```

Each arm and seed runs in a fresh copy of the corpus with `--no-cache
--no-seeds --parallelism 1 --seed S`, so caches cannot leak between arms and
runs are repeatable (str-03mfx.5). The script also runs one knob-absent scan
and fails (exit 1) unless it matches the `flat` arm exactly for that seed.

For a real project, scan a scratch copy without its `shatter.config.json`
so the script's flags (not the project's sandbox wrapper) drive the scan, and
pass scan flags after `--`:

```bash
rsync -a --exclude=.git --exclude=shatter-report --exclude='.shatter*' ~/project/zolem/ /var/tmp/zolem-bench/
rm /var/tmp/zolem-bench/shatter.config.json
python3 scripts/bench_budget_alloc.py --corpus /var/tmp/zolem-bench --include 'internal/specs/*.go' \
  --max-iterations 10 --results-dir target/bench-budget-zolem-10 \
  -- --exclude '*_test.go' --timeout-total 1200 --timeout-per-fn 120 --build-timeout 300
```

If a direct scan cannot run, pass two report JSONs from separate runs with
`--from-json flat.json static.json`.

Committed results live under `benchmarks/budget-allocation/<date>-<corpus>/`
(`report.md` and `report.json`; the per-seed scan JSONs are ~0.5 MB each and
stay in `target/`).

## Reading the report

- **Totals** are medians over seeds. Under `static`, Σ allocated equals the
  flat total by construction; Σ executions used differs because functions
  stop at plateau or claim surplus.
- **Paired static − flat** is the headline: median per-seed delta of branches
  covered and lines covered at equal allocation, and the wall-clock delta.
- **Outcomes** shows how many functions ended by each `completion_outcome`.
- **Per function** (first seed) shows each function's static allocation and
  claims next to its coverage under both arms, sorted by allocation, so a
  loss can be traced to the functions that were starved.

## Results, 2026-09-23

Engine at main `d6727186` (all of str-03mfx.1/.2/.3/.5 landed); seeds 1, 2, 3;
`--parallelism 1`; idle machine. "Tight" is `--max-iterations 10` (50
executions per function under `flat`); "default" is `--max-iterations 100`.
The knob-absent sanity gate passed in every run.

| corpus | budget | fns | branches flat → static (median) | median Δ branches (per seed) | median Δ lines | Δ wall s |
|---|---|---|---|---|---|---|
| standalone/ts | default | 35 | 121 → 121 / 235 | +0 (0, 0, 0) | +0 | +0.5 |
| standalone/ts | tight | 35 | 109 → 112 / 235 | +1 (0, +1, +7) | +0 | +0.4 |
| standalone/go | default | 30 | 132 → 132 / 250 | +0 (0, 0, 0) | +0 (−1, +23, 0) | +0.9 |
| standalone/go | tight | 30 | 103 → 117 / 250 | **+16 (+18, +14, +16)** | +25 (+25, 0, +25) | +0.8 |
| zolem `internal/specs` | default | 24 | 33 → 33 / 43 | +0 (0, 0, 0) | +0 | −0.4 |
| zolem `internal/specs` | tight | 24 | 31 → 31 / 43 | +0 (0, 0, 0) | +0 | +0.1 |

Reading (every claim below is checkable in the committed `report.md` per
run: the "Per seed" table lists claimed executions and the functions that
lost or gained coverage for each seed, and "Per function" covers seed 1):

- **Static allocation only matters when the budget binds.** At the default
  budget almost every fixture function stops at plateau well below its cap
  (3 of 35 TS, 3 of 30 Go, and 0 of 24 zolem functions reach 500
  executions), so the two arms explore nearly the same paths. The extra
  `Σ executions used` under `static` at the default budget is functions that
  received a larger allocation and kept iterating without finding anything
  new. The one loss on record at the default budget is
  `01-arithmetic.go::CompareMagnitudes` (all three Go seeds): it used 253
  executions under `flat` but was allocated 157 under `static` and covered
  one line fewer (8 vs 9, same branches). It did not claim surplus because
  it was not productive by the recent-hits test when it hit its cap. This is
  the starvation risk the floor is meant to bound; at the default budget it
  cost one line on one function across the three corpora. (The Go seed-2
  +23 lines is `ValidateEmail`, which reached its flat cap in that seed.)
- **Under a tight budget the gain is real but concentrated.** In every Go
  tight seed the gain is one function, `15-email-validator.go::ValidateEmail`
  (seed 1: 2 → 20 branches and 6 → 31 lines, +18; seeds 2 and 3: +14 and
  +16) because static gives it 93 executions instead of 50. The score ranked
  it highest (loops, string parameter, many branches), which is the case the
  design targets. TS tight is the same function in seeds 2 and 3 (+1, +7)
  plus `computeStats` in seed 2. Two tight-budget losses are recorded, both
  `RomanToInt`/`romanToInt` (Go seed 3, TS seed 3), each outweighed by the
  gain in that seed; net Δ branches is ≥ 0 in every seed of every run.
- **Surplus claiming fires in two of six tight seeds** (Go seed 3 claimed 23
  executions, TS seed 2 claimed 40; the other seeds claimed 0 or 1) and never
  at the default budget, where nothing reaches its cap. Claims require a
  function to hit its execution cap while still productive, so this is
  expected; the claim path itself is exercised deterministically by the CLI
  test in `shatter-cli/tests/scan_budget_allocation.rs`.
- **zolem is a null result in both regimes, and the mechanism is plateau,
  not exhaustion.** At the default budget all 24 specs functions stop below
  their allocation under both arms (e.g. `NormalizeGeminiDiscovery` uses 233
  of 1106 allocated); at the tight budget 10 of 24 do. The coverage ceiling
  is set by opaque callees and error-only outcomes (9 of 24 functions), so
  more executions find nothing new and the per-seed tables show no function
  gained or lost.
- **Wall-clock cost is small but not uniformly sub-second:** median Δ is
  under one second in every run, with two seeds above it (Go tight seed 2:
  +2.7 s; Go default seed 3: +1.4 s) from the extra executions of functions
  that received larger allocations.

Recommendation for the epic decision: `static` is within one line of `flat`
where the budget does not bind and is materially better only when the per-function budget
is tight relative to the corpus's most complex function. Flipping the
default is safe on the evidence here but buys nothing at the default
`max_iterations`; the stronger case is for users who lower `max_iterations`
to bound scan time. Raw reports: `benchmarks/budget-allocation/2026-09-23-*/`.

## Caveats

- Coverage is counted as branch ids and lines from the scan JSON, not branch
  sides; a side-level count in the scan report is a follow-up.
- `static` applies to the concolic path only; the random explorer keeps
  flat budgets.
- Batched scans (`batch_size`) report summed allocation and claims per
  function.
- Keep the machine otherwise idle; wall-clock is part of the comparison.
- The default stays `flat` until a report justifies flipping it; that
  decision is taken on epic str-03mfx.
