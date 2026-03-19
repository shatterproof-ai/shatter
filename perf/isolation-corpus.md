# Isolation Mode Performance Corpus

Added in str-19pm.8. Part of the str-19pm epic (Exploration isolation modes).

## Purpose

These benchmark scenarios measure how isolation mode (`--isolation none|function|serial`,
added in str-19pm.1) affects wall-clock time and per-phase timing for explore and scan
commands across functions of varying complexity.

The corpus establishes a **baseline timing matrix** before the flag exists so that the
impact of each mode can be measured accurately against a known-good reference.

## Status

`isolation_mode` in `scenarios.json` is currently a **metadata placeholder**. The runner
does not inject `--isolation` into commands — runner support lands in str-19pm.9. Until
then, all three mode variants of each scenario produce identical timing (equivalent to the
future default of `none`), giving us the zero-difference baseline.

Once str-19pm.9 lands, `perf_runner.py` will expand each scenario with an `isolation_mode`
field by injecting `--isolation <mode>` into the command before the target argument.

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
| `frontend.remote.setup.module_load` | TS/Go module compilation on first use | simple (dominates), scan (per-process cost) |
| `frontend.remote.execute.*` | Actual function execution per concolic iteration | all explore scenarios |
| `cli.command` | Total wall-clock for the shatter command | all scenarios (primary regression metric) |
| `frontend.remote.*` (aggregate) | All time inside the frontend subprocess | compare across isolation modes |

Shrink/refine phases will surface under the solver/orchestrator timing once the Z3 solve
and path-refinement loops emit timing nodes. Branchier scenarios maximise iterations and
therefore expose any per-iteration overhead amplified across modes.

## Running the Corpus

```bash
# Run all isolation corpus scenarios (after building):
python3 scripts/perf_runner.py --scenarios isolation-explore-ts-simple-none,isolation-explore-ts-simple-function,isolation-explore-ts-simple-serial

# Or run the full isolation group:
python3 scripts/perf_runner.py --filter 'isolation-'

# Compare results across modes (example, after str-19pm.9 adds mode support):
python3 scripts/perf_compare.py \
  perf/results/isolation-explore-ts-branchier-none/latest \
  perf/results/isolation-explore-ts-branchier-serial/latest \
  --phase-abs-ms 20 --phase-pct 5
```

## Reporting by Isolation Mode

To slice timing output by mode, compare scenario results with the same function but
different `isolation_mode` values. The scenario ids encode the mode as a suffix, making
shell glob and `perf_compare.py` selection straightforward:

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
5. Update this document with the new tier row

## Related Issues

- str-19pm.1 — `--isolation` flag implementation
- str-19pm.2 — Scheduler policy layer
- str-19pm.9 — Runner support for `isolation_mode` field and isolation perf guard
