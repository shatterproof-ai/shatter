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

For zolem, run from the zolem checkout with `SHATTER_BIN` pointing at the
release binary. Zolem's sandboxed `make shatter-focused` cannot forward the
allocation knob; if the direct scan cannot run there, pass the two report
JSONs from separate focused runs with `--from-json flat.json static.json`.

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
