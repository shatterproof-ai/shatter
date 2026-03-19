# Isolation Mode Performance Corpus

Added in str-19pm.8 (corpus definition) and str-19pm.9 (runner support + reporting).
Part of the str-19pm epic (Exploration isolation modes).

## Purpose

These benchmark scenarios measure how isolation mode (`--isolation none|function|serial`,
added in str-19pm.1) affects wall-clock time and per-phase timing for explore and scan
commands across functions of varying complexity.

The corpus establishes a **timing matrix** for each isolation mode so that the impact of
each mode can be measured accurately and regressions can be detected over time.

## Corpus Matrix

### TypeScript — Explore

| Scenario id prefix | Function | Branch count | Complexity tier | Hot phase stressed |
|---|---|---|---|---|
| `isolation-explore-ts-simple-*` | `01-arithmetic.ts:classifyNumber` | 4 | simple | handshake (dominates short runs) |
| `isolation-explore-ts-medium-*` | `06-nested-control-flow.ts:classifyHttpResponse` | 15 | medium | execute × mixed numeric+string Z3 constraints |
| `isolation-explore-ts-branchier-*` | `16-cron-parser.ts:parseCron` | 22 | branchier | shrink/refine (many solver iterations needed) |

Each prefix has three variants: `-none`, `-function`, `-serial`.

### TypeScript — Scan

| Scenario id prefix | Target | Hot phase stressed |
|---|---|---|
| `isolation-scan-ts-*` | `examples/standalone/ts` (all functions) | execute transpile/module load (many functions, isolation overhead amortized) |

### Go — Explore

| Scenario id prefix | Function | Branch count | Complexity tier | Hot phase stressed |
|---|---|---|---|---|
| `isolation-explore-go-branchier-*` | `18-accept-language.go:NegotiateLanguage` | 13 | branchier | execute transpile + Go module load (Go frontier for isolation comparison) |

## Hot Phases

The timing output from `--timing summary --timing-format json` surfaces these phases:

| Phase path | What it measures | Which scenarios show it prominently |
|---|---|---|
| `frontend.remote.setup` | Total frontend startup (handshake + module load) | all scenarios |
| `frontend.remote.setup.module_load` | TS/Go module compilation on first use | simple (dominates), scan (per-process cost) |
| `frontend.remote.execute.*` | Actual function execution per concolic iteration | all explore scenarios |
| `cli.command` | Total wall-clock for the shatter command | all scenarios (primary regression metric) |
| `solver.shrink.*` | Shrink/refine iterations (not yet emitted) | branchier scenarios once solver timing lands |

## Running the Corpus

### Prerequisites

- Build the binary: `cargo build`
- (For `function`/`serial` variants) str-19pm.1 (`--isolation` flag) must be merged

### Quick run (one mode, one scenario)

```bash
python3 scripts/perf_runner.py run \
  --scenario isolation-explore-ts-simple-none \
  --results-dir .shatter/perf-runs/isolation
```

### Full isolation corpus (all modes, all functions)

```bash
# Run everything with the isolation- prefix
npx task perf-isolation

# Or with a custom output directory:
RESULTS_DIR=.shatter/perf-runs/isolation-$(date +%Y%m%d) npx task perf-isolation
```

### Generate the cross-mode comparison report

```bash
npx task perf-isolation-report

# The report is written to .shatter/perf-runs/isolation/isolation-report.md
# and .shatter/perf-runs/isolation/isolation-report.json
```

### Compare against a baseline

```bash
# After collecting a baseline run, compare a candidate against it:
python3 scripts/perf_compare.py \
  --baseline-dir benchmarks/baselines/isolation-baseline \
  --candidate-dir .shatter/perf-runs/isolation \
  --phase-abs-ms 20 --phase-pct 5
```

## Understanding the Cross-Mode Report

`perf_isolation_report.py` groups scenarios by base name (stripping `-none`/`-function`/`-serial`
suffix) and renders a wall-time comparison table plus a per-phase breakdown.

### Wall time table

```
| Scenario               | none    | function | serial  | fn overhead | serial overhead |
| isolation-explore-ts-… | 1200ms  | 1350ms   | 1500ms  |      +12.5% |         +25.0% |
```

- **fn overhead** = `(function_ms − none_ms) / none_ms × 100` — the per-function process
  boundary cost. Acceptable if < 20% for branchier functions with many iterations.
- **serial overhead** = `(serial_ms − none_ms) / none_ms × 100` — the fully-sequential,
  worst-case isolation cost.

### Key cost centers table

Shows per-mode timing for:

| Phase label | What it means |
|---|---|
| `setup (handshake)` | Frontend process startup including handshake negotiation |
| `module_load` | TS/Go module compilation (first-use cost per process restart) |
| `execute (total)` | Sum of all concolic execution iterations |
| `shrink/refine` | Solver shrink and refinement iterations (future) |

## When to Use Each Mode

| Mode | When to use |
|---|---|
| `none` (default) | All executions share a single frontend process. Fastest and lowest startup cost. Use unless you suspect cross-function state contamination or need process-level isolation for correctness. |
| `function` | Each exploration gets a fresh frontend process. Eliminates module-level caches and global state between concolic iterations. Use when `none` produces flaky results due to module-level side effects, or when testing functions that rely on clean process state. |
| `serial` | Sequential execution with no concurrency. Maximises reproducibility for profiling and debugging. Use to attribute CPU time cleanly (no sibling-process noise) or to debug race conditions between exploration threads. |

## Interpreting Regressions

**Same overhead across all modes**: regression is in the core engine (not isolation-specific).
Check `cli.command` total, then drill into `shatter-core` solver or orchestrator timing.

**Overhead only in `function`/`serial` vs `none`**: regression is in the frontend lifecycle —
process spawn cost, module reload, or handshake overhead. Check `frontend.remote.setup`
and `frontend.remote.setup.module_load`.

**Overhead only in `serial` vs `none`/`function`**: regression is in concurrency management —
the serial path is hitting a contention point that parallel execution hides. Check the
scheduling layer (str-19pm.2).

**`module_load` dominates (> 70% of total) for simple functions**: this is expected — startup
cost is amortised over few iterations. It's only a problem if it regresses significantly
compared to a prior baseline.

## Reporting by Isolation Mode

Scenario ids encode the mode as a suffix, making shell glob and `perf_compare.py`
selection straightforward:

```
isolation-explore-ts-medium-none     ← baseline (maximum overlap)
isolation-explore-ts-medium-function ← per-function process boundary overhead
isolation-explore-ts-medium-serial   ← fully sequential, worst-case isolation cost
```

The difference `serial − none` is the maximum isolation overhead for that function.
The difference `function − none` is the per-function process boundary cost.

## Adding More Scenarios

When adding new language frontends or new complexity tiers:

1. Choose a representative function (document expected branches in the source file header)
2. Add three scenario entries to `perf/scenarios.json` — one per isolation mode
3. Set `"isolation_mode"` to `"none"`, `"function"`, or `"serial"` accordingly
4. Use `cache_mode: "warm"` to isolate isolation-mode overhead from cold-start disk costs
5. Update the corpus matrix table in this document with the new tier row

## Related Issues

- str-19pm.1 — `--isolation` flag implementation
- str-19pm.2 — Scheduler policy layer
- str-19pm.8 — Corpus scenario definitions
- str-19pm.9 — Runner injection + cross-mode reporting (this issue)
