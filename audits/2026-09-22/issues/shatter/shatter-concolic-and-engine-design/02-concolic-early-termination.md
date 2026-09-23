---
slug: concolic-early-termination
kind: new
title: "Diagnose why --concolic explore runs end after 21-35 executions on most hard TS functions (diagnosis only; fix is concolic-early-termination-fix)"
priority: P1
type: bug
labels: [audit-2026-09-22, concolic, orchestrator, solver, diagnosis]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [explore-stop-reason-accounting]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Diagnose why --concolic explore runs end after 21-35 executions on most hard TS functions (diagnosis only; fix is concolic-early-termination-fix)

## Problem

On the audit's fresh `shatter explore --concolic` run over 21 hard TypeScript example functions, 12 functions stopped after 21-35 executions against a 100-iteration budget, and most of them left most branches uncovered. What ended those runs is not known yet, and the artifacts cannot answer it:

- **The artifacts' `stop_reason` and `solver_guided_inputs` are not trustworthy.** `ExploreResultAccumulator::into_result` (`shatter-cli/src/commands/explore.rs:190-330`) never copies `stop_reason` or `solver_guided_inputs` from the batch `ObservationOutput`. It fills both from `..Default::default()`, so every explore artifact reports `stop_reason: worklist_exhausted` (the `#[default]` variant, `shatter-core/src/explorer.rs:446-447`) and `solver_guided_inputs: 0`, whatever the engine did. That reporting bug is filed separately as explore-stop-reason-accounting and blocks this issue.
- **The solver is not idle.** The same artifacts record `z3` discoveries for 6 of the 21 functions (table below). An earlier draft of this issue claimed "zero solver-guided inputs", read from the defaulted field. That claim was wrong.

What remains unexplained is the early stop itself. Both "21 = 1 seed + `plateau_threshold` 20" (a coverage plateau, `orchestrator.rs:203`, `:1643-1647`) and "21 = seed-set size, then the worklist drains" fit the data. So does a correct stop, where the solver has nothing left to negate because the instrumentor emits no constraints for the branch (see Likely contributors). This issue decides which one applies. It is a diagnosis. The fix belongs to concolic-early-termination-fix, which this issue blocks.

## Evidence

Run (audit 2026-09-22, audit HEAD 56c86168): `shatter explore --concolic -w 4` over 9 files copied from the examples corpus `ts/` directory into a fresh directory. The files were 05-unions, 06-nested-control-flow, 07-auth-validation, 10-path-router, 14-semver, 15-email-validator, 17-mock-branches, 18-accept-language and 20-dotenv-parser. The exact command line and the examples SHA were not recorded. The raw files are local to the audit worktree and untracked: `audits/2026-09-22/goals-runs/` is listed in `.git/info/exclude`. Everything needed is therefore reproduced inline here.

Per-function results. `iters` and `paths` and `branches` come from the stderr `[batch]` lines. `raw` is `len(observation.raw_results)` from the artifact. `disc` is the artifact's `discoveries` grouped by method.

| Function | File | iters | raw | paths | branches | disc |
|---|---|---|---|---|---|---|
| computeArea | 05-unions.ts | 21 | 21 | 1 | 0/6 | none |
| routeRequest | 05-unions.ts | 26 | 26 | 2 | 1/8 | user 1 |
| classifyHttpResponse | 06-nested-control-flow.ts | 35 | 35 | 6 | 10/13 | z3 9, user 1 |
| processStateMachine | 06-nested-control-flow.ts | 100 | 43 | 3 | 2/12 | user 2 |
| validateJwt | 07-auth-validation.ts | 100 | 22 | 3 | 2/8 | user 2 |
| authorizeRequest | 07-auth-validation.ts | 29 | 29 | 3 | 3/14 | z3 2, user 1 |
| matchRoute | 10-path-router.ts | 21 | 21 | 1 | 1/19 | user 1 |
| resolveMiddleware | 10-path-router.ts | 29 | 29 | 5 | 6/7 | user 4, z3 2 |
| parseSemver | 14-semver.ts | 100 | 50 | 3 | 3/6 | user 3 |
| compareSemver | 14-semver.ts | 30 | 30 | 7 | 6/8 | user 6 |
| satisfiesRange | 14-semver.ts | 35 | 35 | 8 | 7/16 | user 7 |
| validateEmail | 15-email-validator.ts | 100 | 27 | 8 | 13/19 | z3 4, user 2, fuzzed 7 |
| classifyStatus | 17-mock-branches.ts | 21 | 21 | 1 | 0/3 | none |
| loadOrDefault | 17-mock-branches.ts | 21 | 21 | 1 | 1/2 | user 1 |
| classifyConfigs | 17-mock-branches.ts | 22 | 22 | 2 | 1/4 | user 1 |
| parsePreference | 18-accept-language.ts | 100 | 45 | 3 | 4/7 | user 4 |
| sortPreferences | 18-accept-language.ts | 95 | 95 | 13 | 2/2 | user 2 |
| negotiateLanguage | 18-accept-language.ts | 23 | 23 | 2 | 1/12 | user 1 |
| findSeparator | 20-dotenv-parser.ts | 29 | 29 | 3 | 2/2 | user 1, z3 1 |
| stripInlineComment | 20-dotenv-parser.ts | 100 | 100 | 25 | 5/5 | user 5 |
| parseDotenv | 20-dotenv-parser.ts | 100 | 27 | 6 | 7/12 | user 4, fuzzed 3 |

Observations:

- For 5 of the 100-iteration functions, `raw` is far below `iters` (for example validateJwt 100 vs 22). `iterations` is `total_executions` (`shatter-core/src/pipeline.rs:944`), so some execution phase (the plateau fuzz phase is the likely one) spends budget without recording `raw_results`. That is a separate accounting question worth answering here.
- stderr shows `Coverage plateau — entering fuzz phase targeting N opaque branch(es)` eight times (`ts-sub-concolic.err` lines 9, 14, 19, 33, 48, 62, 64, 76).
- The orchestrator's own termination reasons are set correctly at the terminating branch (`orchestrator.rs:1621-1647` returns `MaxIterations`/`MaxExecutions`/`TimeoutExplore`/`CoveragePlateau`, and `:3179` assigns it). `WorklistExhausted` (`:2557`) is only the initial value. The artifact values are wrong because of the CLI accumulator, not the orchestrator.

Durable reproduction, once explore-stop-reason-accounting lands: copy those 9 files from the examples checkout (`scripts/examples_checkout.py`; record its SHA) into an empty directory and run the command below. Then tabulate `iterations`, `stop_reason`, `solver_guided_inputs`, `len(raw_results)` and discoveries per artifact.

```
shatter explore --concolic -w 4 --max-iterations 100 <the 9 files>
```

### Likely contributors (link, do not duplicate)

- str-t854z (existing issue; audit note t854z-sort-split-note): one parameter becomes two unrelated Z3 variables, so SAT models can produce unusable inputs.
- ts-switch-ternary-instrumentation (bucket shatter-frontend-ts): switch, ternary and value-position `&&`/`||` emit no `branch_path` decisions, so there is nothing to negate.
- ts-union-discriminant-literals: computeArea's discriminant literal is widened to `str`, and the generated `{"kind":"true",...}` matches no case.

## Acceptance criteria

- [ ] A re-run of the 9-file subset, made after explore-stop-reason-accounting lands, is attached to the close note. It records the examples SHA, the shatter commit, the exact command, and a per-function table of `iterations`, `stop_reason`, `solver_guided_inputs`, `len(raw_results)` and discoveries by method.
- [ ] For computeArea, matchRoute, negotiateLanguage and classifyStatus, the close note states:
  - the actual `TerminationReason`, taken from a debug log of the orchestrator loop and not only from the artifact;
  - for each uncovered branch, why no solver-guided input reached it. The reason is one of: no `SymExpr` path constraint emitted; Z3 unsat, unknown or error (quoted); model extraction dropped or mis-sorted values; or the input was generated and executed but took the same path.
- [ ] The close note explains the `iterations` > `len(raw_results)` gap: which phase consumes executions without recording them, and whether that is intended.
- [ ] The close note ends with a verdict of **defect** or **expected**, with the reasoning. For **defect**, it names the code sites and a minimal fixture (function body plus expected branch) for concolic-early-termination-fix to use as its known-answer test. For **expected**, it lists the linked issues that account for each loss, and concolic-early-termination-fix is closed with a link to this note.
- [ ] Every loss attributed to str-t854z, ts-switch-ternary-instrumentation or ts-union-discriminant-literals is added as a comment on that issue, with the function name and evidence.

## Suggested approach

1. Reproduce one function (computeArea or matchRoute) with `RUST_LOG=debug` in a fresh directory. Log each worklist push and pop with its source (seed, z3, boundary, drill, fuzz), and each solver call result (sat, unsat, unknown, error).
2. Check whether the branch decisions carry `SymExpr` path conditions. If they do, check the Z3 results and model extraction.

## Out of scope

- Any code fix (concolic-early-termination-fix).
- The artifact field accounting (explore-stop-reason-accounting).
- The benchmark (concolic-vs-default-benchmark) and the positioning decision (concolic-positioning-decision).

## Metadata

- Priority: P1 (D3)
- Type: bug (diagnosis)
- Labels: audit-2026-09-22, concolic, orchestrator, solver, diagnosis
- Parent epic: Epic: Audit 2026-09-22 findings
- Blocked by: explore-stop-reason-accounting
- Blocks: concolic-early-termination-fix
- Related: str-t854z, ts-switch-ternary-instrumentation, ts-union-discriminant-literals, concolic-vs-default-benchmark
- Source findings: goals-08 (split from draft shatter-code/80)
- Decision refs: D3
