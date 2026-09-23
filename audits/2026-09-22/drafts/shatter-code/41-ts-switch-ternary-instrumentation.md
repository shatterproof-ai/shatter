# TS switch/ternary/value-position &&,|| are analyzed but never instrumented; analyze and instrument branch IDs desync and coverage is misattributed

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | bug |
| priority | P1 |
| labels | typescript,instrumentation,coverage,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | str-wsg, str-w0d.1, str-ts3n, str-jeen.81 |
| source findings | frontend-ts-02, prior-17 |

<!-- body -->
## Problem

The analyzer reports switch, ternary and value-position logical branches; the instrumentor records none of them and numbers branches independently. The core joins analysis branch ids with runtime ids, so coverage and uncovered-target hints land on the wrong branches, and ternary-only functions show '0/1 branches' with '100% coverage'. str-w0d.1 claimed switch/ternary/&&/|| emission.

## Current code facts / evidence

- `shatter-ts/src/instrumentor.ts:1393-1411` adds only `__shatter_record` for switch cases; no `isConditionalExpression` handling (only `factory.createConditionalExpression` at :2260 for mock wrappers).
- `shatter-ts/src/analyzer.ts:1364-1471` (ternary at :1384-1398) emits these branch types.
- `shatter-core/src/coverage_metrics.rs:477-521` `extract_targets_inner` joins by id.
- Repro `mixed(a)` (ternary then if): analyze → [0 ternary line 2, 1 if]; instrument → `__shatter_branch(0, 2, a>5)`.
- Repro `h(x){ return x > 1 ? 1 : 0; }`: random '100 iters, 1 paths, 0/1 branches' + '100% coverage (1/1 lines)'; --concolic '21 iters, 1 paths, 0/1 branches'.
- `shatter-ts/CLAUDE.md:285-287` documents an E2E reading raw_results because switch emits no branch_path.

## Acceptance criteria

- Instrumentor emits branch probes + constraints for switch cases (eq(discriminant, case)), ternaries and value-position &&/||/?? via both buildSymExpr and buildSymExprWithFlow.
- Analyze and instrument share one branch enumerator, or core joins on (line, type) using an instrument id→line map.
- Alignment test over a fixture corpus: analyze (id,line) == instrument (id,line).
- Known-answer E2E in e2e_concolic.rs per BranchType (if, else_if, switch, ternary, logical_and, logical_or, loops) finds both outcomes via branch_path; the raw_results workaround is removed.

## Suggested approach

Shared enumerator first, then probes.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: L

## References

- Audit findings: frontend-ts-02, prior-17 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-wsg, str-w0d.1, str-ts3n, str-jeen.81
