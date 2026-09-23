# Audit 2026-09-22 final issue drafts: bucket `shatter-frontend-ts`

- **Bucket:** shatter-frontend-ts. TypeScript frontend correctness and tests: flow map, switch/ternary instrumentation, shadowing, timeouts, request validation, parity tests.
- **Repo:** shatter. **Tracker:** bd in /home/ketan/project/shatter (prefix str).
- **Parent epic for new issues:** "Epic: Audit 2026-09-22 findings".
- **Status:** drafts only. Nothing has been filed (D6).
- **Code baseline:** evidence line numbers were re-verified against the audit worktree `/home/ketan/.local/share/worktrees/shatter/audit-2026-09-22` at 56c86168 (2026-09-23). No TS production code changed since the 2026-09-04 audit.
- **Audit paths:** paths under `audits/2026-09-22/` exist on branch `audit-2026-09-22`. Land them first (publish-audit-reports), or the filer inlines the evidence.

## Maintainer decisions (2026-09-23)

None of D1-D6 changes the content of this bucket. They are listed so the reviewer can check that nothing here contradicts them.

- **D1 Releases:** keep Windows (x86_64-pc-windows-msvc) and aarch64-unknown-linux-gnu in the release matrix, and fix them (Z3 header/static link on Windows; openssl-sys under cross for aarch64), not drop them. Release work closes only with a green release-run URL.
- **D2 shatter diff:** retire the snapshot-diff command (`shatter diff`) and the unused Snapshot writer path. spec-diff is THE regression tool; SPEC/README/QUICKSTART are updated to match. The `diff` name becomes free, and str-81xiw decides whether diff-scoped exploration takes it. The shatter-agents plugin's `shatter diff --staged` docs are corrected to what exists.
- **D3 Concolic positioning:** measure first. P1 controlled default-vs-concolic benchmark (fixed seeds, fresh artifacts, examples corpus plus one downstream project), reported per release. P1 fix for concolic early termination (~21-35 iterations). A follow-up decision issue, blocked by both, re-decides the "concolic-first" positioning. No doc softening now. (ts-switch-ternary-instrumentation and ts-flow-map-program-point are plausible contributors to early termination on TS functions; they are linked, not merged.)
- **D4 Beads hook stall:** retire the JSONL import in shatter; move tracker sync to a Dolt remote; first verify whether the stale JSONL import has clobbered newer DB state; AGENTS.md drops `bd sync`; str-qwua7.28 is superseded; bento's beads-issue-flow gets matching guidance. No BEADS_HOOK_TIMEOUT env var and no hook-bypass guidance.
- **D5 Git identity:** the leaked `[user]` section was already removed. Add a `.mailmap` (test@example.com "Test"/"Test User" -> Ketan Gangatirkar <33678+ketang@users.noreply.github.com>, no history rewrite), a git-state check (identity override, example.com email, core.bare=true, hooksPath override; possibly folded into str-qwua7.1), and a `.git/config` before/after snapshot in test_git_fixture_isolation.py.
- **D6 Filing:** after reconciliation and the Codex cross-check, the maintainer runs one filer script. Agents file nothing.

## Filing order within this bucket

1. File the new issues (01, 02, 04, 05, 06, 08, 09, 12, 13, 14) as children of the shatter epic.
2. Post the comments (03 on closed str-wsg, a reopen-note that does not reopen; 07 on str-rf2v; 10 on str-mhinv.3; 11 on str-qwua7.31), substituting the new ids for the `<slug id>` placeholders.

There are no hard blocked-by edges inside this bucket. Soft couplings:
- 02 <-> 09: the fixtures land with expected-fail markers, which 02 flips.
- 05 <-> 14: both touch the same async race in executor.ts.
- 06 resolves option (a) of note 10.
- 01, 04 and 13 prefer str-rf2v's shared builder/walker if it exists, but are not blocked by it.

## Index

| NN | Slug | Kind | Existing | P | Title |
|---|---|---|---|---|---|
| 01 | ts-flow-map-program-point | new | - | P1 | TS instrumentor data-flow map ignores program point: recorded path constraints contradict the concrete execution |
| 02 | ts-switch-ternary-instrumentation | new | - | P1 | TS switch/ternary/value-position &&,|| are analyzed but never instrumented; analyze and instrument branch IDs desync and coverage is misattributed |
| 03 | ts-branches-reopen-note | reopen-note | str-wsg | P1 | Reopen-note on closed str-wsg: switch/ternary/value-position &&,|| are analyzed but never instrumented |
| 04 | ts-shadowed-callback-params | new | - | P2 | TS analyzer resolves shadowed callback parameters to the outer function parameter |
| 05 | ts-timeout-classification | new | - | P2 | TS frontend classifies any target error whose message mentions 'timeout' as timed_out/infrastructure |
| 06 | ts-request-validation | new | - | P2 | TS parseRequest validates only the envelope: missing fields crash as internal_error; validCommands hand-maintained |
| 07 | rf2v-fourth-walker-and-analyze-dataflow | note-to-existing | str-rf2v | P2 | Note on str-rf2v: a fourth, already-diverged SSA flow walker in executor.ts; the TS analyzer ignores data flow; the parity matrix wrongly says TS analyze produces ite |
| 08 | ts-protocol-and-parity-tests-meaningful | new | - | P2 | TS protocol round-trip tests are tautological and the builder-parity property test cannot detect drift |
| 09 | ts-branchtype-known-answer-fixtures | new | - | P2 | TS known-answer E2E fixture per BranchType: analyze (id, line) == instrument (id, line) and both outcomes discovered via branch_path |
| 10 | mhinv-3-planner-probe-not-supported | note-to-existing | str-mhinv.3 | P2 | Note on str-mhinv.3: TS answers planner-command probes with invalid_request 'Unknown command', not the not_supported its CLAUDE.md promises |
| 11 | qwua7-31-eslint-evidence | note-to-existing | str-qwua7.31 | P2 | Note on str-qwua7.31: typescript-eslint finds 72 production issues in shatter-ts, including dead code and a default export |
| 12 | ts-preflight-node-modules | new | - | P3 | TS preflight fails dependency-free projects (requires node_modules whenever project_root is set) and one failure blocks every later request |
| 13 | ts-operators-collapse-to-unknown | new | - | P3 | TS SymExpr builders collapse common operators to unknown (??, **, shifts, element access, as/!/satisfies, template literals) |
| 14 | ts-lifecycle-and-packaging-hygiene | new | - | P3 | shatter-ts hygiene: async timeout timer leak hidden by jest forceExit, stdin EOF drops in-flight responses, dual lockfiles, tests emitted to dist, standalone bundle worker name, js-yaml v3 |

---

---
slug: ts-flow-map-program-point
kind: new
title: "TS instrumentor data-flow map ignores program point: recorded path constraints contradict the concrete execution"
priority: P1
type: bug
labels: [typescript, instrumentation, concolic, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# TS instrumentor data-flow map ignores program point: recorded path constraints contradict the concrete execution

## Problem

`instrumentFunction` builds **one** data-flow map for the whole function body and uses that end-of-function map for every branch. Two more defects make it worse. A reassignment whose right-hand side resolves to `unknown` does not kill the old binding. Loop bodies are walked once, with no widening or havoc. As a result the `constraint` recorded in `branch_path` can be **false for the inputs that actually ran**. Z3 then negates a constraint that has nothing to do with the branch and never flips it. Core triage (`triage::evaluate_constraint`) also uses these constraints to predict paths, so a wrong constraint can wrongly skip executions. That second effect was not traced end to end.

This is a soundness bug in the main concolic path for TypeScript, not a precision gap.

## Evidence

Re-verified against the audit worktree at 56c86168 (2026-09-23). No TS production code changed between the 2026-09-04 and 2026-09-22 audits.

- `shatter-ts/src/instrumentor.ts:187`: `const dataFlowMap = buildDataFlowMap(targetFunction, sourceFile, paramNames);` is built once, before instrumenting.
- `shatter-ts/src/instrumentor.ts:1710`: `wrapBranchCondition` calls `buildSymExpr(condition, ctx.paramNames, ctx.dataFlowMap)` with that one map for every branch.
- `shatter-ts/src/instrumentor.ts:498` and `:510`: `if (nextExpr.kind !== "unknown") { flowMap.set(...) }`. An unresolvable reassignment leaves the stale binding in place.
- `shatter-ts/src/instrumentor.ts:422-431`: `for`/`while`/`do`/`for-in`/`for-of` bodies are visited once through `visitStatementsForDataFlow`, with no havoc of loop-assigned variables.
- Probes (calling `instrumentFunction` directly; 4th argument of `__shatter_branch`):

  ```ts
  export function stale(a: number, b: number) { let x = a; if (x > 0) {...} x = b; if (x > 100) {...} }
  //  branch 0 (x > 0)  -> { bin_op gt, left: param b, right: 0 }   // should be param a
  export function staleUnknown(a: number) { let x = a; x = opaque(); if (x > 10) {...} }
  //  branch 0 (x > 10) -> { bin_op gt, left: param a, right: 10 }  // should be unknown
  export function loopCarried(a: number) { let acc = a; let i = 0; while (i < 3) { acc = acc + 1; i++; } if (acc > 10) ... }
  //  while (i < 3)     -> { lt, left: (0 + 1), right: 3 }           // constant, wrong
  //  acc > 10          -> { gt, left: a + 1, right: 10 }            // actually a + 3
  ```

- Through the real frontend (`node dist/main.js`), `execute` of `stale` with `inputs: [5, 0]`:

  ```json
  "branch_path":[{"branch_id":0,"line":3,"taken":true,
    "constraint":{"kind":"expr","expr":{"kind":"bin_op","op":"gt",
      "left":{"kind":"param","name":"b","path":[]},"right":{"kind":"const","type":"int","value":0}}}}]
  ```

  The branch was taken while its recorded constraint `b > 0` is false for `b = 0`. The audit verifier reproduced `stale` independently; it did not re-run the `staleUnknown`/`loopCarried` probes.
- `shatter-core/src/triage.rs:316` already provides `pub fn evaluate_constraint(...)`, so a core-side consistency check needs no new evaluator.
- No gate catches this. The builder property tests (`shatter-ts/src/property.test.ts:1187`) compare builders in isolation against a fixed `resolveName`. No test checks "constraint evaluated on concrete inputs == taken".
- Audit sources: finding frontend-ts-01 (`audits/2026-09-22/findings.json`), `audits/2026-09-22/areas/frontend-ts.md` F1. These paths exist on branch `audit-2026-09-22`.

## Acceptance criteria

- [ ] The constraint for each branch uses the flow map **as of that branch's program point**, not the end-of-function map.
- [ ] Reassigning a variable from an unresolvable expression sets it to `unknown`, so the old binding is gone.
- [ ] Every variable assigned inside a loop body is `unknown` at the loop head and after the loop, unless a sound widening is implemented.
- [ ] The `stale`, `staleUnknown` and `loopCarried` probes above are regression tests: `param a`, `unknown`, and `unknown`/non-constant respectively. Each test fails on current `main` and passes after the fix; the close note shows both runs.
- [ ] **Oracle test** (fast-check over generated inputs, or a fixture corpus with random inputs): execute each function, evaluate every recorded `branch_path[i].constraint` on the concrete inputs, and assert the result equals `taken`. Unknown constraints are skipped. The corpus includes reassignment-after-branch, opaque-call reassignment, loop-carried, and `if`/`else` merges (ite).
- [ ] **Core-side guard:** before a constraint reaches the solver, the core evaluates it with `triage::evaluate_constraint` against the concrete inputs. If the result contradicts `taken`, the constraint is downgraded to `unknown` and counted in telemetry or artifact stats (for example `inconsistent_constraints`). This protects every frontend. A Rust unit test feeds a deliberately inconsistent constraint and asserts the downgrade and the count.
- [ ] Close-time proof: the TS E2E suite was actually executed, not served from the Task cache. Run `cargo test -p shatter-core --test e2e_concolic -- --ignored` directly and paste the summary line. Also run `task affected` and record its `Gates selected` output.

## Suggested approach

1. Snapshot the flow map at each branch during the transformer walk instead of calling `buildDataFlowMap` once at `:187`. Kill bindings on unknown reassignment, and havoc loop-assigned names at the loop head.
2. If str-rf2v's shared flow-analysis module (one walker with `onBranch` / `onLoopIteration` hooks) exists by the time this is picked up, implement the fix there once. That also fixes the executor's loop-snapshot copy (`shatter-ts/src/executor.ts:1497-1810`). Do not block on str-rf2v: fixing the instrumentor walker directly is acceptable, as long as the executor copy gets the same kill/havoc semantics or a follow-up is filed.
3. Add the core-side guard in the orchestrator's constraint intake, reusing `triage::evaluate_constraint`.

## Out of scope

- Consolidating the analyzer, instrumentor and executor builders/walkers (str-rf2v).
- Making the analyzer use data flow (a note on str-rf2v covers this).
- New operator support (ts-operators-collapse-to-unknown).
- Loop widening more precise than havoc.

## Related

- str-4kop (closed; SSA phi/ite for conditional reassignment) and str-mu3h (closed; closures over mutable state become unknown) are neighbouring precision work, not this bug.
- str-rf2v (open; builder consolidation). See the rf2v-fourth-walker-and-analyze-dataflow note in this bucket.
- concolic-early-termination (other bucket): wrong constraints are one plausible contributor.

## Priority / type / size

P1 · bug · size M

---

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

---

---
slug: ts-branches-reopen-note
kind: reopen-note
title: "Reopen-note on closed str-wsg: switch/ternary/value-position &&,|| are analyzed but never instrumented"
priority: P1
type: note
labels: [typescript, instrumentation, audit]
parent_epic: ""
blocked_by: []
existing_id: str-wsg
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Reopen-note on closed str-wsg

**Target:** `str-wsg` ("Extract branches from TypeScript AST in analyzer", P1, CLOSED).
**Action:** add a comment only (`bd comments add str-wsg`). Do **not** reopen.
**Ordering:** file after `ts-switch-ternary-instrumentation` so the comment can cite its real id. Replace `<ts-switch-ternary-instrumentation id>` below before posting.

## Comment text

> **Audit 2026-09-22 note.** This work added `switch`, `ternary` and `logical_and`/`logical_or` branch extraction to the TS **analyzer** only (`shatter-ts/src/analyzer.ts:1364-1471`). The instrumentor was never extended:
>
> - `switch` gets line records only (`instrumentor.ts:1393-1411`);
> - there is no `ConditionalExpression` handling;
> - value-position `&&`/`||` get no probe.
>
> Analyze and instrument also number branches independently. The core joins them by id (`coverage_metrics.rs:477 extract_targets_inner`), so coverage and "Uncovered" hints are attributed to the wrong branches. Example: for a ternary followed by an `if`, runtime id 0 is the `if` but analysis id 0 is the ternary. At HEAD, `h(x) = x > 1 ? 1 : 0` reports `0/1 branches` alongside `100% coverage (1/1 lines)`. Closed str-w0d.1 claimed constraint emission for these branch types, and that emission does not exist.
>
> The fix is tracked in <ts-switch-ternary-instrumentation id>. It covers instrumenting switch cases, ternaries and value-position `&&`/`||`/`??`, sharing one branch enumerator (or joining on line/type), an analyze/instrument id-alignment test, and per-BranchType known-answer E2E fixtures. Evidence: audit 2026-09-22 findings frontend-ts-02 and prior-17 (`audits/2026-09-22/areas/frontend-ts.md` F2).

---

---
slug: ts-shadowed-callback-params
kind: new
title: "TS analyzer resolves shadowed callback parameters to the outer function parameter"
priority: P2
type: bug
labels: [typescript, analyze, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# TS analyzer resolves shadowed callback parameters to the outer function parameter

## Problem

The SymExpr builders turn an identifier into a function parameter by **name only**. There is no scope check. Inside `shadow(xs, x)`, the callback in `xs.filter((x) => { if (x > 5) ... })` has its own `x`, but its condition is recorded as a condition on the **outer** parameter `x`. So the analyzer reports a branch the outer `x` does not control, and the solver may be handed a wrong constraint.

## Evidence

Re-verified against the audit worktree at 56c86168 (2026-09-23).

- `shatter-ts/src/analyzer.ts:2280` (non-exported `buildSymExpr`); `:2289` `if (paramNames.has(expr.text))`.
- `shatter-ts/src/instrumentor.ts:1862` (`buildSymExpr`); `:1872` `if (paramNames.has(expr.text))`. `buildSymExprWithFlow` (`:874`) resolves through a `resolveName` callback that is likewise name-based.
- Probe: `shadow(xs: number[], x: number)` containing `xs.filter((x) => { if (x > 5) ... })`. Analyze records the condition `{param x} > 5`. The audit verifier reproduced this.
- The instrument side was **not** verified. The instrumentor probably does not branch-instrument callback bodies, so there may be nothing to observe there today. It has the same name-only lookup, so it becomes live as soon as callbacks are instrumented.
- Audit sources: finding frontend-ts-04; `audits/2026-09-22/areas/frontend-ts.md` F4.

## Acceptance criteria

- [ ] The analyzer resolves identifiers through the TypeChecker: `checker.getSymbolAtLocation(id)` is compared with the symbol of the parameter declaration. A same-named inner binding is not a parameter; it becomes `unknown`, or a local, according to the builder's rules.
- [ ] The instrumentor gets the same guarantee, through a scope stack of bound names or the same symbol check. This includes `buildSymExprWithFlow`'s `resolveName`, so a flow-map entry for the outer `x` is not used for an inner `x`.
- [ ] Regression tests for the `shadow` repro in analyze, and in instrument if callbacks are instrumented. The analyze test fails on current `main` and passes after the fix. Add variants: arrow-parameter shadowing, a `const x` shadow in a nested block, and a destructured `({ x }) =>` shadow.
- [ ] Record `task affected` `Gates selected` at close. Run `cargo test -p shatter-core --test e2e_concolic -- --ignored` directly if instrument output changes.

## Suggested approach

For the analyzer, the TypeChecker is already available (`analyzer.ts:362` `const checker = program.getTypeChecker()` in `analyzeFile`). Carry the parameter symbols into the builder instead of a name set. The instrumentor runs as a transformer, possibly without a checker, so a lexical scope stack (push the bound names at each function, arrow or block) is the cheaper route there.

## Out of scope

- Consolidating the builders (str-rf2v). If str-rf2v's shared builder lands first, do the fix there.
- Instrumenting callback bodies as branch sites.

## Priority / type / size

P2 · bug · size S

---

---
slug: ts-timeout-classification
kind: new
title: "TS frontend classifies any target error whose message mentions 'timeout' as timed_out/infrastructure"
priority: P2
type: bug
labels: [typescript, error-handling, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# TS frontend classifies any target error whose message mentions 'timeout' as timed_out/infrastructure

## Problem

The TS executor picks the outcome from a substring match on the error **message**. User code that throws `new RangeError("timeout must be positive")` for `ms <= 0` is ordinary input validation. It is reported as `outcome.status: "timed_out"` with `error_category: "infrastructure"`, so reports and gates treat an expected target error as a harness fault. The same applies to any user error whose message contains "timeout" or "timed out".

## Evidence

Re-verified against the audit worktree at 56c86168 (2026-09-23).

- `shatter-ts/src/executor.ts:277-286` `classifyError`: returns `infrastructure` for `ERR_SCRIPT_EXECUTION_TIMEOUT` **or** for a message matching `/timed?\s*out/i`.
- `shatter-ts/src/executor.ts:331` `TIMEOUT_PATTERNS`, which includes the bare `"timeout"`; the loop over it is at `:356`.
- `shatter-ts/src/executor.ts:3293-3298` `isTimeoutError`: true for `ERR_SCRIPT_EXECUTION_TIMEOUT`, or for an infrastructure-classified error whose message matches `TIMEOUT_PATTERNS`. That maps to `timed_out`.
- There are only two real harness timeouts:
  - the vm sync timeout (`ERR_SCRIPT_EXECUTION_TIMEOUT`);
  - the async race at `executor.ts:1274-1281`, which rejects with a plain `new Error("async execution timed out")`. It carries no distinguishing class or code, which is why message matching was used.
- Probe from the audit: a target throwing `new RangeError("timeout must be positive")` returned `"outcome":{"status":"timed_out","short_reason":"timeout must be positive",...}` with `error_category: "infrastructure"`. The verifier confirmed the code path but did not re-run the probe.
- Analogous Rust pattern: str-qwua7.33 (classify by downcast, not substring).
- Audit sources: finding frontend-ts-05; `audits/2026-09-22/areas/frontend-ts.md` F5.

## Acceptance criteria

- [ ] Only harness-owned timeouts produce `timed_out` / `infrastructure`:
  - the vm error code `ERR_SCRIPT_EXECUTION_TIMEOUT`;
  - a dedicated error class (for example `ShatterAsyncTimeout`) thrown by the async race at `executor.ts:1274-1281`, and recognized by `instanceof` or a unique code, not by message.
- [ ] User-thrown errors are classified by type/origin and never by message substring. `TIMEOUT_PATTERNS` is removed, or restricted to errors that provably come from the harness.
- [ ] Negative test: a target throwing `new RangeError("timeout must be positive")` gets a runtime (thrown-error) outcome carrying that error, and no `timed_out`. A positive test for each real timeout path (sync vm timeout, async race) still yields `timed_out`. The negative test fails on current `main` and passes after the fix.
- [ ] If the wire-visible outcome changes for any existing fixture, update `shatter-ts/CLAUDE.md` and the parity contract, and run `task parity` + `task conformance`.
- [ ] Record `task affected` `Gates selected` at close.

## Suggested approach

Introduce the timeout error class in the race and check it first in `classifyError`/`isTimeoutError`. Leave `classifyConnectionFailure` for mocked network failures only. ts-lifecycle-and-packaging-hygiene touches the same race to add `clearTimeout`, so do both in one change if they are picked up together.

## Out of scope

- The timer leak in the same race, which is in ts-lifecycle-and-packaging-hygiene. Doing it together is fine.
- Go/Rust outcome classification.

## Priority / type / size

P2 · bug · size S

---

---
slug: ts-request-validation
kind: new
title: "TS parseRequest validates only the envelope: missing fields crash as internal_error; validCommands hand-maintained"
priority: P2
type: bug
labels: [typescript, protocol, validation, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# TS parseRequest validates only the envelope: missing fields crash as internal_error; validCommands hand-maintained

## Problem

`parseRequest` checks `id`, `protocol_version` and `command`, then casts the rest (`parsed as Request`). A request with a valid envelope but missing or ill-typed fields reaches the handlers and fails deep inside them, as `internal_error: Cannot read properties of undefined` or `file_not_found: undefined`. It should be rejected as `invalid_request` naming the bad field. The command allow-list is also a hand-written literal that duplicates the generated `ALL_COMMANDS`. Because of that, a known-but-unsupported command (for example `get_invocation_plan`) is answered as `invalid_request "Unknown command"` instead of `not_supported`.

## Evidence

Re-verified against the audit worktree at 56c86168 (2026-09-23).

- `shatter-ts/src/handlers.ts:1113` `export function parseRequest(...)`, which checks the envelope, then returns `{ request: parsed as Request }` (through `:1159`).
- `shatter-ts/src/handlers.ts:1151`:
  ```ts
  const validCommands = ["handshake", "analyze", "instrument", "prepare", "execute", "setup", "teardown", "generate", "shutdown"];
  ```
  followed at `:1154` by `errorResponse(id, "invalid_request", `Unknown command: ...`)`.
- Generated `ALL_COMMANDS` is at `shatter-ts/src/generated/protocol-enums.ts:14` (re-exported via `protocol.ts:18`); `SUPPORTED_CAPABILITIES` is a literal at `handlers.ts:56`.
- Audit probes over stdio against `node dist/main.js`:
  - `{"command":"execute"}` with no fields -> `internal_error "Unhandled error: Cannot read properties of undefined (reading 'includes')"`;
  - `{"command":"analyze"}` -> `file_not_found "File not found: undefined"`.

  The verifier confirmed the code, not the live probe.
- `shatter-ts/CLAUDE.md:155-157` says conformance expects TS to "return a clean 'capability not supported' response" for planner commands. The only `get_invocation_plan` case (`protocol/conformance/conformance_cases.yaml:538`, `planner_runtime_value_go`) is `frontends: [go]`, so nothing checks that claim.
- The ts-conventions skill says to *prefer* validating incoming data into typed discriminated unions. It is advice, not an enforced rule.
- Audit sources: finding frontend-ts-07 (and frontend-ts-08 for the `not_supported` half); `audits/2026-09-22/areas/frontend-ts.md` F7/F8.

## Acceptance criteria

- [ ] There is a per-command field validator for every supported command. A missing or ill-typed required field yields `invalid_request`, with a message naming the command and the field (for example `execute: missing required field 'inputs'`). Ideally the validators are generated from `protocol/registry.yaml` `field_model` through the existing codegen, so they cannot drift.
- [ ] The dispatch set is derived from generated `ALL_COMMANDS`, and the literal at `handlers.ts:1151` is removed. A command in `ALL_COMMANDS` but not in `SUPPORTED_CAPABILITIES` returns `not_supported`, which matches `shatter-ts/CLAUDE.md:155-157`. A command not in `ALL_COMMANDS` returns `invalid_request`.
- [ ] Conformance cases for TS (and for all frontends where cheap):
  - malformed `execute` (no fields);
  - malformed `analyze` (no `file`);
  - a planner-command probe (`get_invocation_plan`) expecting `not_supported`.

  Each case fails on current `main` and passes after the fix, with `task conformance` output in the close note.
- [ ] If the wire behaviour changes, update `shatter-ts/CLAUDE.md` and `protocol/parity-matrix.yaml`, then run `task parity`.
- [ ] Record `task affected` `Gates selected` at close.

## Suggested approach

Generate the validators if codegen already walks `field_model`. Otherwise hand-write a small `validateRequest(command, obj)` table keyed by `ALL_COMMANDS`, plus a test asserting that every command has an entry.

## Out of scope

- Implementing planner commands in TS (str-mhinv.2 / str-mhinv.3).
- Go/Rust request validation beyond the shared conformance cases.

## Related

- str-mhinv.3 (open): the mhinv-3-planner-probe-not-supported note in this bucket points here as the implementation home for the `not_supported` item.
- str-qwua7.7 (closed; registry extractor sources); str-4btb (closed; parseRequest test consolidation).

## Priority / type / size

P2 · bug · size S-M

---

---
slug: rf2v-fourth-walker-and-analyze-dataflow
kind: note-to-existing
title: "Note on str-rf2v: a fourth, already-diverged SSA flow walker in executor.ts; the TS analyzer ignores data flow; the parity matrix wrongly says TS analyze produces ite"
priority: P2
type: note
labels: [typescript, parity, audit]
parent_epic: ""
blocked_by: []
existing_id: str-rf2v
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-rf2v

**Target:** `str-rf2v` ("Investigate consolidating TS buildSymExpr implementations (analyzer/instrumentor triplication)", P2, OPEN).
**Action:** post ONE comment (`bd comments add str-rf2v`). Keep the priority at P2. Do not change the title; the comment asks the implementer to widen the scope.
**Ordering:** post after `ts-flow-map-program-point`, `ts-protocol-and-parity-tests-meaningful` and `ts-operators-collapse-to-unknown` are filed, and substitute their ids for the `<...>` placeholders.

## Comment text

> **Audit 2026-09-22 note** (findings frontend-ts-03 and frontend-ts-09; evidence in `audits/2026-09-22/areas/frontend-ts.md` F3/F9, re-checked at 56c86168).
>
> Three additions to the consolidation scope. With them, option (b) in this issue ("document the triplication as load-bearing and add a node-type parity test") is not sufficient.
>
> **1. There is a fourth copy: an SSA flow walker in `executor.ts`, and it has already diverged.**
> `shatter-ts/src/executor.ts:1497-1810` contains `visitStatementsForLoopSnapshots` (:1497), `buildLoopSnapshotMutatedExpr` (:1728) and `mergeLoopSnapshotFlowMaps` (:1769). It is a near-copy of the instrumentor walker at `instrumentor.ts:361-632` (`visitStatementsForDataFlow` :361, `mergeFlowMaps` :569). `mergeFlowMaps` and `mergeLoopSnapshotFlowMaps` are line-for-line equivalent. The executor copy has no destructuring (`registerDestructuredBindings`), no `while`/`do`/`for-in`/`for-of` traversal, and no closure poisoning: a grep of 1497-1810 for those constructs finds 0 hits, against 3 in the instrumentor range. Neither this issue ("triplication") nor the parity table in `shatter-ts/CLAUDE.md` mentions it. The root CLAUDE.md "grep for the parallel path" rule names only `buildSymExpr`/`buildSymExprWithFlow`, so copies under other names escape it.
>
> **2. The analyzer's builder ignores data flow.**
> `shatter-ts/src/analyzer.ts:2280` (the non-exported `buildSymExpr`, "local copy for analyzer independence") knows only `paramNames`. For `const y = a * 2; if (y > 10)`, analyze emits `{gt, unknown, 10}` while instrument emits `{gt, a*2, 10}`. For `let x = a; if (x > 0)`, analyze gives `{gt, unknown, 0}`. Go's analyzer threads a flow map, so TS and Go `analyze` differ on the same source, and that divergence is not recorded anywhere.
>
> **3. The parity matrix is wrong about TS.**
> `protocol/parity-matrix.yaml:1096-1110` (`ite-symexpr-production-partial`) says "TypeScript produces ite expressions via data flow analysis in visitStatementsForDataFlow" and lists `affected_frontends: [rust]` for `affected_commands: [analyze]`. TS produces ite only on the instrument/execute path. Until the analyzer builder uses the flow map, `typescript` belongs in `affected_frontends` for `analyze`, and the description should say so.
>
> **Proposed additions to acceptance:**
> - Widen the deliverable to **one flow-analysis module** (for example `shatter-ts/src/flow-analysis.ts`): a single walker parameterized by hooks (`onBranch`, `onLoopIteration`) and used by the instrumentor, the executor loop snapshots and the analyzer. It is paired with one SymExpr builder whose flow resolution is optional.
> - The program-point fix from <ts-flow-map-program-point id> (snapshot per branch, kill on unknown reassignment, havoc loop-assigned variables) lives in that module once. That fixes the executor copy as well.
> - On a shared fixture corpus of flow-tracked locals (derived locals, if/else reassignment producing ite, loops), analyze and instrument produce **identical** SymExpr for the same condition. A test asserts this.
> - `protocol/parity-matrix.yaml` `ite-symexpr-production-partial` is accurate in the same change: either TS analyze now produces ite, or `typescript` is added to `affected_frontends` for `analyze` with corrected text. Run `task parity`.
> - The builder output-equality test in <ts-protocol-and-parity-tests-meaningful id> covers the analyzer builder, which is exported for tests or deleted by this consolidation.
> - Related: <ts-operators-collapse-to-unknown id> should land in the shared builder, so that operator support cannot diverge again.

---

---
slug: ts-protocol-and-parity-tests-meaningful
kind: new
title: "TS protocol round-trip tests are tautological and the builder-parity property test cannot detect drift"
priority: P2
type: task
labels: [typescript, testing, protocol, parity, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# TS protocol round-trip tests are tautological and the builder-parity property test cannot detect drift

## Problem

Two groups of TS tests look like protocol and parity coverage but do not exercise what they are named for.

1. **Protocol "round-trip" tests.** They assert `JSON.parse(JSON.stringify(x))` equals `x` for objects built by TS arbitraries, which holds for any plain object. They never touch the real wire path (`serializeReplacer` in `sendResponse`, or `parseRequest`), the shared `protocol/schemas`, or `protocol/fixtures`. Yet `shatter-ts/Taskfile.yml` lists those directories and `shatter-core/src/protocol.rs` as test sources. Those are dead cache keys that suggest schema coverage that does not exist.
2. **Builder-parity property test.** A `buildSymExpr` / `buildSymExprWithFlow` parity describe block exists (`property.test.ts:1187`, 4 cases). *Verifier correction for finding tests-ci-12: the parity test is present, so the gap is narrower than "no parity test".* It has three limits:
   - it generates only node kinds that both builders already handle;
   - it compares only unknown vs non-unknown, not the output;
   - it does not cover the analyzer's builder or the executor's walker.

   A new node kind added to one builder, or a semantic difference between builders, passes it.

## Evidence

Re-verified against the audit worktree at 56c86168 (2026-09-23).

- `shatter-ts/src/property.test.ts:717` `describe("property: protocol message round-trips")`, whose cases through `:870` are plain JSON round-trips. The pattern repeats for SideEffect (`:871`), SymExpr (`:1002`), TypeInfo (`:1036`), BranchDecision (`:1047`) and TraceEvent (`:1059`). `serializeReplacer` appears only in the separate BigInt tests (around `:1798-1850`).
- A grep of `shatter-ts/src/*.test.ts` finds no reference to `protocol/schemas` or `protocol/fixtures`.
- `shatter-ts/Taskfile.yml` `test` sources `:45-48` and `test-fast` sources `:62-65` list `../protocol/schemas/**/*.json`, `../protocol/fixtures/**/*.json` and `../shatter-core/src/protocol.rs`.
- `property.test.ts:1117` `arbBinOp` omits `in`/`instanceof`, which both token maps handle (`instrumentor.ts:2020-2023`). `:1097` `hasNonUnknownLeaf`, and the assertions at `:1187-1296`, check only unknown-ness.
- The analyzer builder (`analyzer.ts:2280`) is not exported and is untested. The executor loop-snapshot walker (`executor.ts:1497-1810`) is untested for parity.
- 7 of the 21 `shatter-ts/src/*.test.ts` files use fast-check. There are also semantic properties for `flattenConditions` and MC/DC masking (`property.test.ts` ~1587-1721), and SymExpr structural-validity checks. Keep those; the remaining gaps are schema validation of emitted output and builder output equality.
- Audit sources: findings frontend-ts-10, frontend-ts-11, tests-ci-12 (verifier: partially confirmed, parity block exists); `audits/2026-09-22/areas/frontend-ts.md` F10/F11.

## Acceptance criteria

- [ ] **Schema validation on the real wire path:** responses produced by fast-check arbitraries, serialized with `serializeReplacer` exactly as `sendResponse` does, are validated against `protocol/schemas` with ajv. Instrument/analyze outputs from a small fixture corpus are also validated. The test demonstrably catches a violation: include a deliberately invalid response case, asserted to fail validation.
- [ ] **Fixtures through the real parser:** every request in `protocol/fixtures` that targets TS is driven through `parseRequest` + `handleRequest`. The test asserts the response shape and status, not only that no exception was thrown.
- [ ] **Builder parity by output:** replace the unknown-vs-non-unknown check with a fixed corpus that includes currently unsupported nodes (`??`, `**`, shifts, element access, `as`/`!`, template literals, `in`, `instanceof`). The test asserts **output equality modulo documented collapse rules** across all builders, the analyzer builder included (exported for tests, or deleted per str-rf2v). Each collapse rule is written down next to the test.
- [ ] `shatter-ts/Taskfile.yml` `sources:` match what the tests actually read: entries for inputs no test reads are removed, and inputs that tests read are added.
- [ ] The existing tautological round-trip cases are removed, or rewritten to go through `serializeReplacer`/`parseRequest`. They are not kept alongside the new tests.
- [ ] Close-time proof: `npx jest` in `shatter-ts` runs the new suites (paste the counts), and `task affected` `Gates selected` is recorded.

## Suggested approach

Add `ajv` as a devDependency (it is not in `shatter-ts/package.json` today), load `protocol/schemas` in a jest `beforeAll`, and reuse the existing arbitraries. If TS output hits a schema defect that the protocol-schemas-reject-real-output issue (shatter-protocol-parity bucket) owns, mark that case expected-fail citing its id; do not weaken the schema here. For parity, a table-driven test over a source corpus is enough; fast-check on top is optional.

## Out of scope

- Consolidating the builders (str-rf2v).
- Adding operator support (ts-operators-collapse-to-unknown). This issue only makes the parity test able to see the gaps.
- Go/Rust schema tests.

## Related

- protocol-schemas-reject-real-output (other bucket): hand-maintained schemas already reject some real frontend output, so this suite may surface more of that.
- str-jalv (closed; builder parity properties), str-hicn (closed; WithFlow must handle every node buildSymExpr handles), str-rf2v (open), str-4btb (closed; removed trivial round-trips from parseRequest tests), str-qwua7.47 (open; Rust-only PBT), str-0z1im.

## Priority / type / size

P2 · task · size M

---

---
slug: ts-branchtype-known-answer-fixtures
kind: new
title: "TS known-answer E2E fixture per BranchType: analyze (id, line) == instrument (id, line) and both outcomes discovered via branch_path"
priority: P2
type: task
labels: [typescript, e2e, testing, parity, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# TS known-answer E2E fixture per BranchType: analyze (id, line) == instrument (id, line) and both outcomes discovered via branch_path

## Problem

Branch-type capability claims for the TS frontend were closed without any per-type known-answer test. str-w0d.1 claimed constraint emission for `if, switch, ternary, &&, ||`, and str-wsg covered analyzer extraction only. The TS E2E suite (`shatter-core/tests/e2e_concolic.rs`) has no fixture per `BranchType` asserting that both arms are reached **through `branch_path`**. As a result, `switch`, `ternary` and value-position `&&`/`||` have been analyzed but never instrumented (ts-switch-ternary-instrumentation), and no gate noticed.

A known workaround was also written as prose instead of being filed. The enum E2E reads `raw_results` because switch emits no `branch_path`. This issue adds the missing gate, so that a branch type can only be declared supported with an end-to-end test behind it.

This was split from audit draft agent/23. The engine-level random-vs-concolic parity suite, the `_`-prefixed-param lint and the checklist/close-reason rule stay in engine-parity-e2e (shatter-concolic-and-engine-design bucket).

## Evidence

Re-verified against the audit worktree at 56c86168 (2026-09-23).

- Generated `ALL_BRANCH_TYPES` (`shatter-ts/src/generated/protocol-enums.ts:78-88`): `else_if, for, if, logical_and, logical_or, select, switch, ternary, while`. `select` is Go-only.
- The TS analyzer emits them at `shatter-ts/src/analyzer.ts`:
  - `if`/`else_if` at :1340;
  - `switch` at :1374;
  - `ternary` at :1393;
  - `logical_and`/`logical_or` at :1410-1411;
  - `while` at :1436, and `do` also emits `while` (:1453);
  - `for` at :1477.
  - `for-of`/`for-in` emit **no** branch; they only walk the body (:1460-1464).
- The instrumentor probes only `if` and loop conditions. For `switch` it adds line records only (`instrumentor.ts:1393-1411`), and it has no `ConditionalExpression` handling.
- The prose workarounds are the only ones found by a grep of `shatter-ts/CLAUDE.md`, `shatter-ts/src/*.test.ts` and `e2e_concolic.rs` for workaround/limitation wording:
  - `shatter-ts/CLAUDE.md:285-287`;
  - `shatter-core/tests/e2e_concolic.rs:2947-2953`: "the TS instrumentor records switch-case *lines* but emits no `branch_path` decisions, so the orchestrator's path-based dedup collapses all switch executions into one empty-path entry".
- The E2E tests are `#[ignore = "subprocess E2E; run via task e2e-ts or core:test-ignored"]`. `task e2e-ts` is checksum-cached, so a Task "pass" can execute nothing.
- Audit sources: finding frontend-ts-18 (with frontend-ts-02 and prior-17); `audits/2026-09-22/areas/frontend-ts.md` F18; `drafts/shatter-agent/23-mechanical-parallel-parity-gates.md`.

## Acceptance criteria

- [ ] A table-driven TS known-answer fixture set in `shatter-core/tests/e2e_concolic.rs` (or a new `e2e_concolic_ts_branch_types.rs` wired into `task e2e-ts`) has one small function per TS-emitted BranchType: `if`, `else_if`, `switch`, `ternary`, `logical_and`, `logical_or`, `while` (including a `do` variant) and `for`. Each function has a known triggering input for each outcome. `select` is excluded as Go-only, and the fixture file states this.
- [ ] Each fixture asserts **(a)** that the analyze branch `(id, line)` set equals the instrument branch `(id, line)` set for that function, and **(b)** that both outcomes of each branch are discovered through `branch_path` in `result.executions`, not `raw_results`, under both the default explorer and `--concolic`.
- [ ] Today, `switch`, `ternary`, `logical_and` and `logical_or` fail. Those cases are marked expected-fail with the ts-switch-ternary-instrumentation id, so the suite runs green, and each marker **flips to a failure when the case starts passing**, so it must be removed. No silent `#[ignore]`.
- [ ] A test asserts that every value in `ALL_BRANCH_TYPES` has either a fixture here or an explicit exclusion with a reason (for example `select`: Go-only). Adding a BranchType to the registry without a TS fixture then fails the test.
- [ ] **Test workarounds recorded in prose are filed as issues.** The `raw_results` workaround (`shatter-ts/CLAUDE.md:285-287`, `e2e_concolic.rs:2947-2953`) is tracked by ts-switch-ternary-instrumentation: the prose and the comment are edited to cite that issue id. Any other test workaround found by a sweep of `shatter-*/CLAUDE.md` and `shatter-core/tests/*.rs` for "workaround", "reads raw_results", "known limitation" or "does not yet" in test context is filed as its own issue and cited next to the prose. The close note lists the sweep command and each hit with its issue id, or states that there were none.
- [ ] Close-time proof: run `cargo test -p shatter-core --test <suite> -- --ignored` **directly**, not through a possibly cached Task run, and paste the per-test output showing every fixture executed and the expected-fail cases reported as expected-fail. Record `task affected` `Gates selected`.

## Suggested approach

Model the fixtures on `examples/go/05-conditional-merge.go` and `shatter-core/tests/e2e_concolic_go.rs`. Keep each function tiny, so the triggering inputs are obvious. Get the analyze side from the frontend `analyze` response, and the instrument side from the `instrument` response's branch ids and lines (or from the id -> (line, type) map if ts-switch-ternary-instrumentation adds one).

## Out of scope

- Fixing the instrumentor (ts-switch-ternary-instrumentation).
- Go/Rust per-BranchType fixtures. File follow-ups if wanted; per-frontend parity of emitted branch types is a candidate drift-patrol check.
- The engine_parity random-vs-concolic suite, the `_`-param lint, and the completion-checklist rule "test workarounds must be filed as issues" (engine-parity-e2e).

## Related

- ts-switch-ternary-instrumentation (this bucket) turns the expected-fail cases green.
- engine-parity-e2e (shatter-concolic-and-engine-design bucket): the sibling split of agent/23.
- str-wsg, str-w0d.1, str-ts3n (closed without per-type E2E); str-u394l.4 (agent rules drift lint).

## Priority / type / size

P2 · task · size M

---

---
slug: mhinv-3-planner-probe-not-supported
kind: note-to-existing
title: "Note on str-mhinv.3: TS answers planner-command probes with invalid_request 'Unknown command', not the not_supported its CLAUDE.md promises"
priority: P2
type: note
labels: [typescript, protocol, parity, audit]
parent_epic: ""
blocked_by: []
existing_id: str-mhinv.3
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-mhinv.3

**Target:** `str-mhinv.3` ("Plan field unsupported behavior", P1, OPEN).
**Action:** post one comment (`bd comments add str-mhinv.3`) that adds an acceptance item. Do not change the priority.
**Ordering:** post after `ts-request-validation` is filed and substitute its id.

## Comment text

> **Audit 2026-09-22 note** (finding frontend-ts-08, confirmed; re-checked at 56c86168).
>
> This issue pins clean unsupported behaviour for plan-bearing `execute` requests. A related gap is the **planner commands themselves**.
>
> - `shatter-ts/CLAUDE.md:155-157` says conformance tests "expect TS to return a clean 'capability not supported' response" when planner commands are probed.
> - In fact the TS dispatch allow-list is a hand-written literal (`shatter-ts/src/handlers.ts:1151`: `handshake, analyze, instrument, prepare, execute, setup, teardown, generate, shutdown`), so a probe returns:
>   ```json
>   {"id":2,"status":"error","code":"invalid_request","message":"Unknown command: get_invocation_plan"}
>   ```
> - The only `get_invocation_plan` conformance case (`protocol/conformance/conformance_cases.yaml:538`, `planner_runtime_value_go`) is `frontends: [go]`, so nothing checks the documented contract.
>
> **Acceptance item to add here:** TS (and Rust, if it has the same gap) and the docs agree on the planner-command probe response. One of two things happens:
> - (a) behaviour changes: a command that is in generated `ALL_COMMANDS` but not in `SUPPORTED_CAPABILITIES` returns `not_supported`, and truly unknown commands keep `invalid_request`; or
> - (b) the contract changes: `shatter-ts/CLAUDE.md:155-157` and `protocol/parity-matrix.yaml` document `invalid_request`.
>
> Either way, a TS (and Rust) conformance case for a `get_invocation_plan` probe pins it, and `task conformance` shows the case running.
>
> Option (a) is already an acceptance criterion of <ts-request-validation id>, which derives the dispatch set from `ALL_COMMANDS`. If that lands first, this item closes by pointing to it.

---

---
slug: qwua7-31-eslint-evidence
kind: note-to-existing
title: "Note on str-qwua7.31: typescript-eslint finds 72 production issues in shatter-ts, including dead code and a default export"
priority: P2
type: note
labels: [typescript, lint, audit]
parent_epic: ""
blocked_by: []
existing_id: str-qwua7.31
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-qwua7.31

**Target:** `str-qwua7.31` ("Add ESLint to shatter-ts with the rules ts-conventions already claims are enforced", P2, OPEN).
**Action:** post one comment (`bd comments add str-qwua7.31`). Keep P2. The comment corrects the title's premise but does not rename the issue; the maintainer may choose to.

## Comment text

> **Audit 2026-09-22 note** (finding frontend-ts-12, partially confirmed; evidence in `audits/2026-09-22/areas/frontend-ts.md` F12).
>
> **Still true:** there is no ESLint in `shatter-ts/package.json` or `shatter-ts/Taskfile.yml`.
>
> **Premise correction (verifier):** the ts-conventions skill says *prefer* ESLint. It does not claim that lint is enforced today. The title's "already claims are enforced" overstates this. The work is still warranted.
>
> **New, quantified evidence.** typescript-eslint 8 `recommendedTypeChecked`, plus the rules the skill names, was run on a scratch copy over production files only (tests excluded). It reported **72 findings** in about 16.7k lines:
>
> | Rule | Count |
> |---|---|
> | no-unnecessary-type-assertion | 23 |
> | no-unused-vars | 14 |
> | no-unsafe-call | 7 |
> | no-unsafe-return | 6 |
> | unbound-method | 5 |
> | consistent-type-imports | 5 |
> | no-unsafe-member-access | 4 |
> | no-unsafe-assignment | 3 |
> | no-base-to-string | 2 |
> | no-restricted-exports (default export) | 1 |
>
> Test files add 77 more (19 no-unsafe-assignment, 6 no-require-imports, 3 no-implied-eval). The verifier did not re-run ESLint, so treat the counts as approximate. The examples below were re-checked in code at 56c86168:
> - `shatter-ts/src/executor.ts:132`: `SUBPROCESS_SYMBOLS` is dead (declared, never read; unused since str-3ky9.12, 2026-03-11).
> - `shatter-ts/src/logger.ts:22`: a default export, against the skill's "no default exports".
> - `shatter-ts/src/analyzer.ts:1890` unused `checker`; `shatter-ts/src/instrumentor.ts:298` unused `sourceFile`.
> - `shatter-ts/src/executor.ts:2808`: `String(thrownError)` can yield `[object Object]` in connection-failure messages.
>
> Zero `no-explicit-any` and zero `ban-ts-comment` findings in production code, so adoption is cheap.
>
> **Proposed acceptance additions:**
> - Adopt `recommendedTypeChecked` plus the skill's named rules, and fix or explicitly disable (with a reason) each of the 72 production findings.
> - Wire `ts:lint` into `ts:test-fast` and `task check`. At close, show `task check` actually running the lint step, not a cached no-op.
> - Either align the ts-conventions skill wording with what is enforced, or retitle this issue.

---

---
slug: ts-preflight-node-modules
kind: new
title: "TS preflight fails dependency-free projects (requires node_modules whenever project_root is set) and one failure blocks every later request"
priority: P3
type: bug
labels: [typescript, install, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# TS preflight fails dependency-free projects (requires node_modules whenever project_root is set) and one failure blocks every later request

## Problem

The TS frontend's preflight fails whenever `<project_root>/node_modules` is missing, whether or not the project has any dependencies. The core derives `project_root` from the nearest `package.json`, `tsconfig.json`, `go.mod` or `Cargo.toml`. So these all get `preflight_failed: missing_node_modules`, even for a file with no imports:

- a tsconfig-only TS project;
- a `package.json` with no dependencies;
- possibly a `.ts` file inside a Go or Rust repo.

The failure is stored in a single module-level variable. Once set, it fails every later request in the same frontend process, including requests for other roots. The code comment says this stickiness is intentional ("one failure authoritative"). The verifier therefore treats it as a design choice to revisit rather than a bug, and downgraded the finding to P3.

## Evidence

Re-verified against the audit worktree at 56c86168 (2026-09-23).

- `shatter-ts/src/handlers.ts:190` `let preflightFailure: PreflightFailure | null = null;` (a module-level value).
- `shatter-ts/src/handlers.ts:203-218` `runPreflight(projectRoot)` returns early only when `projectRoot` is null or empty, then fails on a missing `node_modules` (`PREFLIGHT_REASON_MISSING_NODE_MODULES`, `:178`).
- `preflightFailure` is checked by the analyze, instrument, prepare, execute and setup handlers (`:438`, `:503`, `:559`, `:639`, `:880`).
- `shatter-core/src/project.rs:10-15` `MARKERS`: `package.json`, `tsconfig.json` (TypeScript), `go.mod`, `Cargo.toml`.
- Audit probe: analyze with `project_root` set to a directory without `node_modules` returned `preflight_failed: missing_node_modules: .../node_modules` for a file with no imports.
- Not verified end to end through the `shatter` CLI. It is also unverified whether the core ever passes a Go/Cargo root to the TS frontend. Confidence is medium.
- Introduced by str-jeen.26 (closed; node_modules preflight) and str-jeen.40 (closed; `preflight_failed` code). Neither handled dependency-free projects or per-root scoping.
- Audit sources: finding frontend-ts-13 (verifier: partially confirmed, P2 -> P3); `audits/2026-09-22/areas/frontend-ts.md` F13.

## Acceptance criteria

- [ ] `node_modules` is required only when `package.json` declares `dependencies` or `devDependencies`, or the frontend fails lazily on the first `MODULE_NOT_FOUND` with the same `preflight_failed` remediation text.
- [ ] Either the preflight failure is keyed per project root, so a failure for root A does not fail requests for root B, or the "one failure authoritative" design is re-confirmed and the reason recorded in `shatter-ts/CLAUDE.md` next to the preflight section, with a test pinning it.
- [ ] Tests:
  - a zero-dependency TS project (tsconfig-only, and a `package.json` with no deps) runs analyze + explore successfully through the CLI. Paste the `shatter explore` output in the close note;
  - a project with declared deps and no `node_modules` still gets `preflight_failed`.

  The first test fails on current `main`.
- [ ] If the error surface changes, update `shatter-ts/CLAUDE.md` and the parity contract, and run `task parity` + `task conformance`. Record `task affected` `Gates selected`.

## Suggested approach

Read `package.json` once in `runPreflight`. Replace the module-level value with a `Map<root, PreflightFailure>`, unless the maintainer keeps the sticky design.

## Out of scope

- Installing dependencies automatically.
- Go/Rust preflight.

## Priority / type / size

P3 · bug · size S

---

---
slug: ts-operators-collapse-to-unknown
kind: new
title: "TS SymExpr builders collapse common operators to unknown (??, **, shifts, element access, as/!/satisfies, template literals)"
priority: P3
type: feature
labels: [typescript, instrumentation, solver, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# TS SymExpr builders collapse common operators to unknown (??, **, shifts, element access, as/!/satisfies, template literals)

## Problem

Branches over many everyday TS expressions produce `unknown` constraints, so the solver cannot steer toward them:

- `??`, `**`, `<<`/`>>`/`>>>`;
- element access (`xs[i]`), `ConditionalExpression` operands;
- template literals, BigInt literals, postfix unary;
- type-only wrappers `as` / `!` / `satisfies` / `<T>x`.

The type-only wrappers are the worst case: they change nothing at runtime, yet they hide a fully resolvable expression. The 2026-09-04 audit listed `??` and `**` (area A5, P3), but they were never filed.

## Evidence

Re-verified against the audit worktree at 56c86168 (2026-09-23).

- `shatter-ts/src/instrumentor.ts:1862` `buildSymExpr` (handled node kinds through about `:1953`) and `:1984-2027` operator mapping (`binaryTokenToOp`), which has no cases for `??`, `**` or shifts. `buildSymExprWithFlow` (`:874`) and the analyzer copy (`analyzer.ts:2280`) have the same gaps.
- The instrumentor has no `isAsExpression`, `isNonNullExpression`, `isSatisfiesExpression` or `isElementAccessExpression` handling (grep finds 0 hits).
- Probes:
  - `const v = a ?? 7; if (v > 3)` -> `{unknown} > 3`;
  - `if (2 ** a! > 8)` -> `unknown > 8`;
  - `if (xs[i] === 7)` -> `unknown === 7`.

  The verifier reproduced the `??` and element-access probes; it did not probe `**` or `!`.
- Core `BinOpKind` (`shatter-core/src/sym_expr.rs:95-114`) has `Shl`, `Shr` and `BitClear` (commented "Go-specific"), `In` and `InstanceOf`. It has **no** power operator and no unsigned right shift. The TS `BinOpKind` union (`shatter-ts/src/protocol.ts:514`) lacks `shl`/`shr` (see str-qwua7.37).
- Audit sources: finding frontend-ts-14; `audits/2026-09-22/areas/frontend-ts.md` F14.

## Acceptance criteria

- [ ] `as`, `!`, `satisfies` and `<T>x` type assertions (and parentheses) are unwrapped before building, in every builder.
- [ ] `a ?? b` becomes `ite(eq(a, null) or eq(a, undefined), b, a)`, or the closest faithful encoding the core's null model supports. Document the choice next to the builder.
- [ ] `<<` and `>>` map to the core `Shl`/`Shr`, with `shl`/`shr` added to the TS `BinOpKind`.
- [ ] `>>>` and `**` either get a core op wired through Z3 or stay `unknown`, with the choice recorded as a documented collapse rule. Do not map `**` to `Mul`.
- [ ] Element access with a literal key (`o["k"]`, `xs[0]`) becomes a `param.path` segment. A non-literal index stays `unknown`, as a documented rule.
- [ ] Each construct has a unit test in **both** `buildSymExpr` and `buildSymExprWithFlow`, and in the analyzer builder unless str-rf2v has deleted it. Each test fails on current `main`.
- [ ] One known-answer E2E case (`??` or `as`) shows the concolic engine flipping a branch it could not flip before. Run `cargo test -p shatter-core --test e2e_concolic -- --ignored <case>` directly and paste the output. Record `task affected` `Gates selected`.
- [ ] Any new op that reaches the wire is added to `protocol/schemas` and PROTOCOL.md (coordinate with protocol-schemas-reject-real-output), and `task parity` passes.

## Suggested approach

Do this inside the shared builder if str-rf2v's consolidation has landed, so all builders gain the operators at once. Otherwise add each construct to all three builders in one change, and extend the output-equality parity corpus from ts-protocol-and-parity-tests-meaningful. Start with the unwrapping, which is cheap and high-yield.

## Out of scope

- Template literal and string-concatenation solving (Z3 string theory). Keep these `unknown` unless it is trivial.
- BigInt semantics.
- Go/Rust operator coverage.

## Related

- str-qwua7.37 (open; which node kinds are mandatory, TS lacks shl/shr), str-rf2v (open), str-a4c (core shifts), protocol-schemas-reject-real-output (shatter-protocol-parity bucket; schemas lack shl/shr).

## Priority / type / size

P3 · feature · size S-M

---

---
slug: ts-lifecycle-and-packaging-hygiene
kind: new
title: "shatter-ts hygiene: async timeout timer leak hidden by jest forceExit, stdin EOF drops in-flight responses, dual lockfiles, tests emitted to dist, standalone bundle worker name, js-yaml v3"
priority: P3
type: chore
labels: [typescript, cleanup, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# shatter-ts hygiene: async timeout timer leak hidden by jest forceExit, stdin EOF drops in-flight responses, dual lockfiles, tests emitted to dist, standalone bundle worker name, js-yaml v3

## Problem

These are small robustness and packaging defects in `shatter-ts`, each independently fixable. None is user-visible today, but some of them hide real problems from the test suite: `forceExit` masks leaked handles.

## Evidence

Re-verified against the audit worktree at 56c86168 (2026-09-23).

1. **Timer leak (frontend-ts-06; verifier P3).**
   - `shatter-ts/src/executor.ts:1274-1281`: `Promise.race([syncResult, new Promise((_, reject) => setTimeout(() => reject(new Error("async execution timed out")), timeoutMs))])`. The timer is never cleared or `unref`'d, so every async execute leaves a pending timer of up to `timeoutMs`.
   - `shatter-ts/jest.config.js:6`: `forceExit: true` ("avoid hanging on worker threads that outlive tests") hides leaked handles like this one.
   - Practical impact is minor, because the frontend is long-lived and each timer is bounded.
2. **Stdin EOF / shutdown drop in-flight responses (frontend-ts-17).**
   - `shatter-ts/src/main.ts:76-79`: `rl.on("close", () => { ...; process.exit(0); })` exits without awaiting in-flight `handleRequest` promises.
   - Audit probes (not re-run by the verifier): piping `handshake, shutdown` then EOF produced no `shutdown_ack`; `shutdown` during an in-flight `instrument` produced `internal_error "Unhandled error: Worker terminated"`.
   - `main.ts:23` hard-codes `"Starting TypeScript frontend (protocol 0.1.0)"`, although `PROTOCOL_VERSION` is imported at `main.ts:13`.
   - Low impact today, because the core waits for each response.
3. **Packaging (frontend-ts-16).**
   - Both `shatter-ts/package-lock.json` and `shatter-ts/pnpm-lock.yaml` exist. The Taskfile installs with npm; `pnpm-lock.yaml` was last touched incidentally in aca09d8b.
   - `shatter-ts/tsconfig.json:16-17` includes `src` and excludes only `src/__fixtures__`, so `tsc` emits all 21 `*.test.ts` into `dist/`.
   - `shatter-ts/package.json:11` `bundle` emits `dist/bundle.js` and `dist/worker-bundle.js`, but `shatter-ts/src/instrumentation-worker.ts:49` resolves `path.join(__dirname, "worker.js")`.
     - *Verifier correction:* `node dist/bundle.js` works in a normal `dist/`, because `tsc` also emits `dist/worker.js`, which the bundle falls back to. Only a standalone bundle (without the tsc output) fails with `Cannot find module .../worker.js` on the first instrument. The CLI's embedded path renames the file correctly (`shatter-cli/build.rs:93`, `embedded_frontend.rs:42`).
   - `package.json` `main`/`bin` point at the unbundled `dist/main.js`.
   - `js-yaml` is `^3.14.2`, with a hand-written `shatter-ts/src/js-yaml.d.ts`.
- Audit sources: findings frontend-ts-06, frontend-ts-16, frontend-ts-17; `audits/2026-09-22/areas/frontend-ts.md` F6/F16/F17.

## Acceptance criteria

- [ ] The async race clears its timer in `finally`, or `unref`s it. `forceExit` is removed from `jest.config.js` after one `npx jest --detectOpenHandles` run whose findings are fixed. The close note pastes that run's summary showing no open handles.
- [ ] On stdin `close` and on `shutdown`, the frontend awaits pending request promises, bounded by a timeout, before exiting. A test pipes `handshake, shutdown`, then EOF, and asserts that `shutdown_ack` is received; it fails on current `main`. The startup banner uses `PROTOCOL_VERSION`.
- [ ] `pnpm-lock.yaml` is deleted (npm is the one package manager).
- [ ] A `tsconfig.build.json` (used by `build`) excludes `**/*.test.ts`. After `task ts:build` there are no `*.test.js` files in `dist/`, while ts-jest typechecking of tests still works.
- [ ] The bundle and worker names agree: the bundle emits `worker.js`, or the worker path is passed explicitly. A standalone bundle (bundle output alone, in an empty directory) completes an `instrument` request. `shatter-cli/build.rs` / `embedded_frontend.rs` are updated if the file name changes.
- [ ] `js-yaml` is 4.x with `@types/js-yaml`, and the hand-written `js-yaml.d.ts` is removed.
- [ ] Record `task affected` `Gates selected`. Because the embedded frontend is touched, also run the walkthrough (`task walkthrough`) and paste its pass line.

## Suggested approach

Make independent small commits, one per item. The timer fix touches the same race as ts-timeout-classification, which adds a dedicated timeout error class. If both are picked up together, do them in one change.

## Out of scope

- Outcome classification (ts-timeout-classification).
- ESLint adoption (str-qwua7.31).
- Changing how the CLI embeds the frontend beyond the worker file name.

## Priority / type / size

P3 · chore · size M
