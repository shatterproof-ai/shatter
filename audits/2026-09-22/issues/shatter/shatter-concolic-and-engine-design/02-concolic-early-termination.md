---
slug: concolic-early-termination
kind: new
title: "Concolic explorer stops after ~21-35 executions with zero solver-guided inputs on most hard TS functions; find root cause and fix"
priority: P1
type: bug
labels: [audit-2026-09-22, concolic, orchestrator, solver]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Concolic explorer stops after ~21-35 executions with zero solver-guided inputs on most hard TS functions; find root cause and fix

## Problem

On the audit's fresh `--concolic` run over 21 hard TypeScript example functions, most functions stopped well short of the 100-iteration budget and left most branches uncovered. Every one of the 21 artifacts reports `solver_guided_inputs: 0`. The Z3 loop that `--concolic` exists for appears to contribute nothing: the worklist drains after the seed inputs (and an occasional fuzz phase). Until this is fixed, any concolic-vs-default comparison (concolic-vs-default-benchmark) measures a broken engine.

## Evidence

Run: `audits/2026-09-22/goals-runs/ts-sub-concolic.err` (fresh directory, `-w 4`). Artifacts: `goals-runs/ts-concolic-fresh/shatter-artifacts/explore-results/`.

| Function | File | Iters | Paths | Branches |
|---|---|---|---|---|
| computeArea | 05-unions.ts | 21 | 1 | 0/6 |
| matchRoute | 10-path-router.ts | 21 | 1 | 1/19 |
| classifyStatus | 17-mock-branches.ts | 21 | 1 | 0/3 |
| loadOrDefault | 17-mock-branches.ts | 21 | 1 | 1/2 |
| classifyConfigs | 17-mock-branches.ts | 22 | 2 | 1/4 |
| negotiateLanguage | 18-accept-language.ts | 23 | 2 | 1/12 |
| routeRequest | 05-unions.ts | 26 | 2 | 1/8 |
| authorizeRequest | 07-auth-validation.ts | 29 | 3 | 3/14 |
| classifyHttpResponse | 06-nested-control-flow.ts | 35 | 6 | 10/13 |

Tabulated from the artifact JSON with:

```
cd audits/2026-09-22/goals-runs/ts-concolic-fresh/shatter-artifacts/explore-results
python3 -c "import json,glob
for f in sorted(glob.glob('*/0*.json')):
  o=json.load(open(f))['observation']
  print(f, o['iterations'], o['stop_reason'], o['solver_guided_inputs'], len(o['raw_results']))"
```

- All 21 functions report `stop_reason: worklist_exhausted` and `solver_guided_inputs: 0`. That includes functions that ran exactly 100 iterations (processStateMachine, validateJwt, parseSemver, validateEmail, parsePreference, parseDotenv). This suggests that either the stop-reason mapping is wrong, or those runs also ended by draining the worklist at the budget boundary.
- The stderr shows `Coverage plateau — entering fuzz phase targeting N opaque branch(es)` eight times (lines 9, 14, 19, 33, 48, 62, 64, 76).
- The artifact's `solver_guided_inputs` is `r.z3_generated + r.boundary_generated + r.drill_generated` (`shatter-core/src/pipeline.rs:951`), so Z3, boundary and drilling each produced 0 follow-ups, or the counters are not incremented on this path.

### Relevant code at audit HEAD (56c86168)

- Termination checks in `observe_one`: `shatter-core/src/orchestrator.rs:1621-1646`. They cover max_iterations (a unique-path cap), max_executions, timeout, and `plateau_threshold` (default 20, `orchestrator.rs:203`). The CLI sets 20 (or 60 with `--mcdc`) at `shatter-cli/src/commands/explore.rs:5146`, `scan_orchestrator.rs:3083` and `observe.rs:110`.
- The loop's default termination is `WorklistExhausted` (`orchestrator.rs:2557`). The CoveragePlateau handler enters the fuzz phase or breaks (`orchestrator.rs:2972-3180`).
- The "21 = 1 + plateau_threshold 20" pattern and the "21 = seed-set size" pattern are both consistent with the data. Which one applies is the first thing to establish.

### Likely contributors (link, do not duplicate)

- z3-mixed-int-real-sort-split (bucket shatter-engine-correctness): one parameter becomes two unrelated Z3 variables, and model extraction overwrites values across sorts, so SAT models can produce no usable input.
- ts-switch-ternary-instrumentation (bucket shatter-frontend-ts): switch/ternary/value-position `&&`/`||` are analyzed but emit no `branch_path` decisions, so there is nothing to negate on those branches.
- known-answer-ratchet-and-ts-discriminants: computeArea's discriminant literal is widened to `str`, and the generated `{"kind":"true",...}` matches no case (`areas/goals.md`).

## Acceptance criteria

- [ ] Root cause documented in this issue. For at least computeArea, matchRoute and negotiateLanguage, the note says which of these ends the loop: worklist exhaustion, plateau, or unsat/unknown/solver-error handling. It also says why no solver-guided input is produced (no path constraints emitted, Z3 unsat/unknown, model extraction dropped values, or counters not incremented).
- [ ] `stop_reason` and `solver_guided_inputs` in the explore artifact are accurate for the concolic path. A unit test covers each `TerminationReason` -> `StopReason` mapping, including a run that hits `max_iterations`.
- [ ] Fix landed. On a fresh run of the same 21 functions, no function whose branches are not all covered stops before its budget with `worklist_exhausted` unless the issue documents why the worklist is empty. `solver_guided_inputs > 0` on functions with solvable numeric/string branches (e.g. matchRoute, negotiateLanguage).
- [ ] Known-answer E2E test (in `shatter-core/tests/e2e_concolic.rs`, driven through `pipeline_orchestrator`/CLI wiring, not `orchestrator::explore` directly). It uses a fixture with a branch reachable only via a solver-generated input, and asserts:
  - the run exceeds the old ~21-execution stop point;
  - the branch is reached with a solver provenance.

  Proof at close: the commit SHA where the test fails on the pre-fix code, the passing run output after the fix, and the command/output of a forced (uncached) `task e2e` run.
- [ ] Re-run of the 21-function subset attached to the close note (per-function iters / branches / stop_reason / solver_guided_inputs, before and after).
- [ ] Losses traced to z3-mixed-int-real-sort-split or ts-switch-ternary-instrumentation are listed with those issue IDs rather than fixed here.

## Suggested approach

1. Reproduce on one function (computeArea or matchRoute) with `RUST_LOG=debug` in a fresh artifact dir. Log each worklist push/pop with its source (seed, z3, boundary, drill, fuzz) and each solver call result (sat/unsat/unknown/error).
2. Check whether branch decisions for these functions carry `SymExpr` path conditions at all (the TS instrumentor may emit decisions without symbolic constraints for object-field or string ops). If the conditions are there, check the Z3 results.
3. Fix the accounting (`stop_reason`, `solver_guided_inputs`) first, so later runs are self-explaining.

## Out of scope

- The benchmark itself (concolic-vs-default-benchmark).
- The positioning decision (concolic-positioning-decision).
- Path-identity or budget-semantics unification (engine-path-identity-budget-config).
- The fixes in the linked sort-split and TS-instrumentation issues.

## Metadata

- Priority: P1 (D3)
- Type: bug
- Labels: audit-2026-09-22, concolic, orchestrator, solver
- Parent epic: Epic: Audit 2026-09-22 findings
- Blocked by: none
- Related: z3-mixed-int-real-sort-split, ts-switch-ternary-instrumentation, known-answer-ratchet-and-ts-discriminants, concolic-vs-default-benchmark; blocks concolic-positioning-decision
- Source findings: goals-08 (split from draft shatter-code/80)
- Decision refs: D3
