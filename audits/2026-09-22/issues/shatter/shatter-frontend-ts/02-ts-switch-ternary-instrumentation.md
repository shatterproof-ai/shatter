---
slug: ts-switch-ternary-instrumentation
kind: new
title: "TS switch/ternary/value-position &&,|| are analyzed but never instrumented; analyze and instrument branch IDs desync and coverage is misattributed"
priority: P1
type: bug
labels: [typescript, instrumentation, coverage, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# TS switch/ternary/value-position &&,|| are analyzed but never instrumented; analyze and instrument branch IDs desync and coverage is misattributed

## Problem

The TS analyzer reports `switch`, `ternary`, `logical_and` and `logical_or` branches. The instrumentor wraps only `if` and loop conditions, and it numbers branches on its own. The core joins analysis branches to runtime discoveries **by id**, so once a function contains one of these constructs, every later id refers to a different branch. Two things follow.

- **Coverage and uncovered-target hints land on the wrong lines.** A run that takes an `if` marks the preceding ternary as covered, and it reports the `if` as "Uncovered".
- **Switch-only and ternary-only functions give the concolic engine no constraints.** Reports can still show "100% coverage" (line coverage) with "0/1 branches".

Closed str-wsg added the analyzer side only. Closed str-w0d.1 ("TS frontend: symbolic constraint extraction from branches") says it emits constraints "for each branch condition (if, switch, ternary, &&, ||)". That emission does not exist. `PARITY.md:50` also claims TS "Branch detection Y ... if/else, switch/match, loops".

## Evidence

Re-verified against the audit worktree at 56c86168 (2026-09-23).

- `shatter-ts/src/analyzer.ts:1364-1471`: emits `branch_type: "switch"` (:1374), `"ternary"` (:1393), `"logical_and"`/`"logical_or"` (:1410-1411).
- `shatter-ts/src/instrumentor.ts:1393-1411`: for `switch` it only inserts `__shatter_record(line)` per clause statement. There are no branch probes and no constraints.
- The instrumentor has no `isConditionalExpression` handling. Its only `ConditionalExpression` is `factory.createConditionalExpression` at `:2260`, which builds mock wrappers.
- `shatter-core/src/coverage_metrics.rs:477` `extract_targets_inner(&analysis.branches, &result.discoveries, ...)` marks `analysis.branches[i]` covered when `discoveries` contains `branch.id`.
- Probe `mixed(a, b)` (ternary on one line, `if` on the next):
  ```
  analyze    mixed: [[0,16,"ternary","a > 0"],[1,17,"if","b > 10"]]
  instrument mixed: __shatter_branch(0, 17, !!(b > 10), ...)   <- runtime id 0 is the `if`
  analyze    sw:    [[0,3,"switch","a === 1"],[1,4,"switch","a === 42"]]   instrument sw: 0 branch calls
  analyze    tern:  [[0,9,"ternary","a > 5"]]                               instrument tern: 0 branch calls
  analyze    logic (const ok = a > 0 && b > 0): [[0,2,"logical_and",...]]   instrument: 0 branch calls
  ```
- End to end at HEAD (audit verifier, fresh dir), `h(x) { return x > 1 ? 1 : 0; }`:
  - default explorer: `100 iters, 1 paths, 0/1 branches` plus `100% coverage (1/1 lines)`
  - `--concolic`: `21 iters, 1 paths, 0/1 branches`
- The limitation is written down as prose, not tracked. `shatter-ts/CLAUDE.md:285-287` and `shatter-core/tests/e2e_concolic.rs:2947-2953` both say the enum E2E reads `raw_results` "because the TS instrumentor records switch-case *lines* but emits no `branch_path` decisions".
- Audit sources: findings frontend-ts-02 and prior-17; `audits/2026-09-22/areas/frontend-ts.md` F2.

## Acceptance criteria

- [ ] The instrumentor emits a branch probe with a constraint for:
  - each `switch` case: `eq(discriminant, case)` for the case taken; `default` is the conjunction of the negations;
  - each ternary condition;
  - value-position `&&`, `||` and `??` (short-circuit decisions).

  The constraints are built through **both** `buildSymExpr` and `buildSymExprWithFlow`, with parity tests.
- [ ] Analyze and instrument agree on branch identity. Either they share one branch enumerator (one traversal that assigns ids), or instrument returns its id -> (line, type) map and the core joins on (line, type) instead of id.
- [ ] An alignment test over a fixture corpus asserts that analyze `(id, line)` == instrument `(id, line)` for every branch. It fails on current `main` (with `mixed`) and passes after the fix.
- [ ] The per-BranchType known-answer E2E fixtures from ts-branchtype-known-answer-fixtures pass for `switch`, `ternary`, `logical_and` and `logical_or`, with their expected-fail markers removed. If that issue has not landed, add those fixtures here. Each fixture discovers both outcomes through `branch_path`.
- [ ] The `raw_results` workaround is removed from the enum E2E: the test reads `result.executions`. The prose at `shatter-ts/CLAUDE.md:285-287` and the comment at `e2e_concolic.rs:2947-2953` are deleted.
- [ ] `h(x) = x > 1 ? 1 : 0` reports `2/2` branch outcomes (or the equivalent in the report's branch metric) under both the default explorer and `--concolic`. Paste the CLI output in the close note.
- [ ] If output changes, `PARITY.md:50` and the TS parity-matrix rows state what TS actually instruments. Run `task parity` and `task conformance`.
- [ ] Close-time proof: run `cargo test -p shatter-core --test e2e_concolic -- --ignored` directly (not a possibly cached Task run) and paste the summary. Record `task affected` `Gates selected`.

## Suggested approach

Build the shared branch enumerator first, so ids cannot drift again, then add the probes. Keep the probe expressions side-effect-free and evaluated once: wrap the condition value, not a re-evaluation. The ternary and logical probes must not change short-circuit semantics.

## Out of scope

- The engine-level random-vs-concolic parity suite (engine-parity-e2e, another bucket).
- Go/Rust instrumentor coverage of these branch types.
- New SymExpr operators beyond what the constraints need (ts-operators-collapse-to-unknown).

## Related

- str-wsg (closed; analyzer-only extraction). A reopen-note in this bucket points here.
- str-w0d.1 (closed; claimed switch/ternary/&&/|| constraint emission), str-ts3n (closed), str-jeen.81 (closed; expression-bodied arrows), str-6wmm.3 (closed; condition decomposition).
- concolic-early-termination (other bucket): some of the early stops on TS functions are downstream of this.

## Priority / type / size

P1 · bug · size L
