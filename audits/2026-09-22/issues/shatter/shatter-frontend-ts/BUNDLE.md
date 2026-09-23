# Audit 2026-09-22 final issue drafts: bucket `shatter-frontend-ts`

- **Bucket:** shatter-frontend-ts. TypeScript frontend correctness and tests: flow map, switch/ternary instrumentation, shadowing, timeouts, request validation, parity tests, lifecycle and packaging.
- **Repo:** shatter. **Tracker:** bd in /home/ketan/project/shatter (prefix str).
- **Parent epic for new issues:** "Epic: Audit 2026-09-22 findings".
- **Status:** drafts only. Nothing has been filed (D6).
- **Revision:** revised 2026-09-23 against the Codex cross-check (`issues/crosscheck/shatter-frontend-ts.codex.md`); see `REVISION.md` in this directory for the finding-by-finding log.
- **Code baseline:** evidence line numbers were verified against the audit worktree `/home/ketan/.local/share/worktrees/shatter/audit-2026-09-22` at 56c86168 and re-checked unchanged at 793f2b0b (2026-09-23). No TS production code changed since the 2026-09-04 audit.
- **Audit paths:** paths under `audits/2026-09-22/` exist on branch `audit-2026-09-22`. Land them first (publish-audit-reports), or the filer inlines the evidence.
- **E2E proof convention:** close-time E2E proof uses the governed task with `--force` (for example `task --force e2e-ts`), never the bare `cargo test --test e2e_*` command (AGENTS.md "Shared-Machine Resource Etiquette").

## Maintainer decisions (2026-09-23)

None of D1-D6 changes the content of this bucket. They are listed so the reviewer can check that nothing here contradicts them. In particular, no draft here adds a timeout env var or hook-bypass guidance (D4), and nothing is filed by agents (D6). In particular, no draft here adds a timeout env var or hook-bypass guidance (D4), and nothing is filed by agents (D6).

- **D1 Releases:** keep Windows (x86_64-pc-windows-msvc) and aarch64-unknown-linux-gnu in the release matrix, and fix them (Z3 header/static link on Windows; openssl-sys under cross for aarch64), not drop them. Release work closes only with a green release-run URL.
- **D2 shatter diff:** retire the snapshot-diff command (`shatter diff`) and the unused Snapshot writer path. spec-diff is THE regression tool; SPEC/README/QUICKSTART are updated to match. The `diff` name becomes free, and str-81xiw decides whether diff-scoped exploration takes it. The shatter-agents plugin's `shatter diff --staged` docs are corrected to what exists.
- **D3 Concolic positioning:** measure first. P1 controlled default-vs-concolic benchmark (fixed seeds, fresh artifacts, examples corpus plus one downstream project), reported per release. P1 fix for concolic early termination (~21-35 iterations). A follow-up decision issue, blocked by both, re-decides the "concolic-first" positioning. No doc softening now. (ts-switch-ternary-instrumentation and ts-flow-map-program-point are plausible contributors to early termination on TS functions; they are linked, not merged.)
- **D4 Beads hook stall:** retire the JSONL import in shatter; move tracker sync to a Dolt remote; first verify whether the stale JSONL import has clobbered newer DB state; AGENTS.md drops `bd sync`; str-qwua7.28 is superseded; bento's beads-issue-flow gets matching guidance. No BEADS_HOOK_TIMEOUT env var and no hook-bypass guidance.
- **D5 Git identity:** the leaked `[user]` section was already removed. Add a `.mailmap` (test@example.com "Test"/"Test User" -> Ketan Gangatirkar <33678+ketang@users.noreply.github.com>, no history rewrite), a git-state check (identity override, example.com email, core.bare=true, hooksPath override; possibly folded into str-qwua7.1), and a `.git/config` before/after snapshot in test_git_fixture_isolation.py.
- **D6 Filing:** after reconciliation and the Codex cross-check, the maintainer runs one filer script. Agents file nothing.

## Filing order within this bucket

1. File the new issues (01, 02, 04, 05, 06, 08, 09, 12, 13, 14, 15, 16, 17, 18) as children of the shatter epic.
2. Post the comments (03 on closed str-wsg, a reopen-note that does not reopen; 07 on str-rf2v, after 15 and 01; 10 on str-mhinv.3, after 06; 11 on str-qwua7.31), substituting the new ids for the `<slug id>` placeholders.

There are no hard blocked-by edges inside this bucket. Soft couplings:
- 02 <-> 09: the fixtures land first with expected-fail markers, which 02 flips. 09 reads instrument-side ids from the instrumented output, so it does not need 02.
- 01 <-> 16: 16 is the core-side backstop split out of 01; either can land first.
- 07 -> 15: note 07 proposes that str-rf2v closes via option (a), pointing at implementation issue 15.
- 05 <-> 14: both touch the same async race in executor.ts.
- 06 resolves option (a) of note 10; 06 owns invalid request fixtures, 08 owns valid ones.
- 01, 04 and 13 prefer the shared builder/walker from 15 if it exists, but are not blocked by it.
- 14, 17 and 18 were one draft; they are independent.

## Index

| NN | Slug | Kind | Existing | P | Title |
|---|---|---|---|---|---|
| 01 | ts-flow-map-program-point | new | - | P1 | TS instrumentor data-flow map ignores program point: recorded path constraints contradict the concrete execution |
| 02 | ts-switch-ternary-instrumentation | new | - | P1 | TS switch/ternary/value-position &&,\|\| are analyzed but never instrumented; analyze and instrument branch IDs desync and coverage is misattributed |
| 03 | ts-branches-reopen-note | reopen-note | str-wsg | P1 | Reopen-note on closed str-wsg: switch/ternary/value-position &&,\|\| are analyzed but never instrumented |
| 04 | ts-shadowed-callback-params | new | - | P2 | TS analyzer resolves shadowed callback parameters to the outer function parameter |
| 05 | ts-timeout-classification | new | - | P2 | TS frontend classifies any target error whose message mentions 'timeout' as timed_out/infrastructure |
| 06 | ts-request-validation | new | - | P2 | TS parseRequest validates only the envelope: missing fields crash as internal_error; validCommands hand-maintained |
| 07 | rf2v-fourth-walker-and-analyze-dataflow | note-to-existing | str-rf2v | P2 | Note on str-rf2v: a fourth, already-diverged SSA flow walker in executor.ts; the TS analyzer ignores data flow; the parity matrix wrongly says TS analyze produces ite |
| 08 | ts-protocol-and-parity-tests-meaningful | new | - | P2 | TS protocol round-trip tests bypass the real wire path and schemas, and the builder-parity property test cannot detect drift |
| 09 | ts-branchtype-known-answer-fixtures | new | - | P2 | TS known-answer E2E fixture per BranchType: analyze (id, line) == instrument (id, line) and both outcomes discovered via branch_path |
| 10 | mhinv-3-planner-probe-not-supported | note-to-existing | str-mhinv.3 | P2 | Note on str-mhinv.3: TS answers planner-command probes with invalid_request 'Unknown command', not the not_supported its CLAUDE.md promises |
| 11 | qwua7-31-eslint-evidence | note-to-existing | str-qwua7.31 | P2 | Note on str-qwua7.31: typescript-eslint finds 72 production issues in shatter-ts, including dead code and a default export |
| 12 | ts-preflight-node-modules | new | - | P3 | TS preflight fails dependency-free projects (requires node_modules whenever project_root is set) and one failure blocks every later request |
| 13 | ts-operators-collapse-to-unknown | new | - | P3 | TS SymExpr builders collapse common operators to unknown (??, **, shifts, element access, as/!/satisfies, template literals) |
| 14 | ts-lifecycle-and-packaging-hygiene | new | - | P3 | shatter-ts lifecycle: async timeout timer never cleared (hidden by jest forceExit); shutdown and stdin EOF drop in-flight responses |
| 15 | ts-flow-analysis-consolidation | new | - | P2 | TS: consolidate the four SymExpr builders / flow walkers (analyzer, instrumentor x2, executor loop snapshots) into one flow-analysis module |
| 16 | core-constraint-consistency-guard | new | - | P2 | Core: check each recorded branch constraint against the concrete execution before solving, with language-aware evaluation that returns indeterminate when unsure |
| 17 | ts-packaging-hygiene | new | - | P3 | shatter-ts packaging: dual npm/pnpm lockfiles, tests emitted to dist/, standalone bundle cannot find its worker |
| 18 | ts-js-yaml-v4 | new | - | P3 | shatter-ts: upgrade js-yaml 3.x to 4.x and drop the hand-written js-yaml.d.ts |

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

`instrumentFunction` builds **one** data-flow map for the whole function body and uses that end-of-function map for every branch. Three more defects make it worse:

- A reassignment whose right-hand side resolves to `unknown` does not kill the old binding.
- Loop bodies are walked once, with no widening or havoc. This includes `for` incrementors and mutations inside loop conditions.
- Parameter lookup runs **before** the flow map (`resolveName` at `instrumentor.ts:346-351`, `buildSymExpr` at `:1871-1879`). A reassigned parameter therefore always resolves to the original parameter, and updating the flow map alone cannot fix it.

As a result the `constraint` recorded in `branch_path` can be **false for the inputs that actually ran**. Z3 then negates a constraint that has nothing to do with the branch and never flips it. Core triage (`triage::evaluate_constraint`) also uses these constraints to predict paths, so a wrong constraint can wrongly skip executions. That second effect was not traced end to end.

This is a soundness bug in the main concolic path for TypeScript, not a precision gap.

## Evidence

Line numbers were verified against the audit worktree at 56c86168 and re-checked unchanged at 793f2b0b (2026-09-23). No TS production code changed between the 2026-09-04 and 2026-09-22 audits.

- `shatter-ts/src/instrumentor.ts:187`: `const dataFlowMap = buildDataFlowMap(targetFunction, sourceFile, paramNames);` is built once, before instrumenting.
- `shatter-ts/src/instrumentor.ts:1710`: `wrapBranchCondition` calls `buildSymExpr(condition, ctx.paramNames, ctx.dataFlowMap)` with that one map for every branch.
- `shatter-ts/src/instrumentor.ts:498` and `:510`: `if (nextExpr.kind !== "unknown") { flowMap.set(...) }`. An unresolvable reassignment leaves the stale binding in place.
- `shatter-ts/src/instrumentor.ts:422-431`: the `for` body and incrementor, and the `while`/`do`/`for-in`/`for-of` bodies, are each visited once through `visitStatementsForDataFlow` / `visitExpressionForDataFlow`, with no havoc of loop-assigned variables. Loop **conditions** are not visited for mutations at all.
- `shatter-ts/src/instrumentor.ts:346-351` (`resolveName`) and `:1871-1873` (`buildSymExpr`) check `paramNames.has(name)` before consulting the flow map.
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
- No gate catches this. The builder property tests (`shatter-ts/src/property.test.ts:1187`) compare builders in isolation against a fixed `resolveName`. No test checks "constraint evaluated on concrete inputs == taken".
- Audit sources: finding frontend-ts-01 (`audits/2026-09-22/findings.json`), `audits/2026-09-22/areas/frontend-ts.md` F1. These paths exist on branch `audit-2026-09-22`.

## Acceptance criteria

- [ ] The constraint for each branch uses the flow map **as of that branch's program point**, not the end-of-function map.
- [ ] Reassigning a variable from an unresolvable expression sets it to `unknown`, so the old binding is gone.
- [ ] Havoc covers **every** mutation site inside a loop: the body, the `for` incrementor, and assignments or updates inside the loop condition (for example `while (x-- > 0)`). Every such variable is `unknown` at the loop head, inside the body, and after the loop, unless a sound widening is implemented.
- [ ] A reassigned parameter resolves through the program-point map, not straight to `param <name>`. Parameter lookup no longer short-circuits the flow map for names that are assigned anywhere in the function.
- [ ] Regression tests, each red on current `main` and green after the fix (the close note pastes both runs, for example `npx jest -t flow-map-program-point` before and after):

  | Function | Branch | Expected constraint left side |
  |---|---|---|
  | `stale` (above) | `x > 0` | `param a` |
  | `staleUnknown` (above) | `x > 10` | `unknown` |
  | `loopCarried` (above) | `while (i < 3)` and `acc > 10` | `unknown` (non-constant) |
  | `forIncr(a, n) { for (let i = a; i < n; i++) { if (i > 5) ... } }` | `i > 5` | `unknown` |
  | `condMut(a) { let x = a; while (x-- > 0) {} if (x > 3) ... }` | `x > 3` | `unknown` |
  | `paramReassign(a, b) { a = b; if (a > 0) ... }` | `a > 0` | `param b` |
  | `paramOpaque(a) { a = opaque(); if (a > 0) ... }` | `a > 0` | `unknown` |

- [ ] **Oracle test in the TS suite** (fast-check over generated inputs, or a fixture corpus with random inputs): execute each function, evaluate every recorded `branch_path[i].constraint` with JS semantics on the concrete inputs, and assert the result equals `taken`. Unknown constraints are skipped. The corpus contains every function in the table above plus `if`/`else` merges (ite). The test demonstrably fails on current `main` (paste the failing counterexample).
- [ ] The executor loop-snapshot walker (`shatter-ts/src/executor.ts:1497-1810`) gets the same kill/havoc semantics in this change, or a follow-up issue is filed and its id is in the close note.
- [ ] Close-time proof: `task --force e2e-ts` (the governed task; do not run the underlying `cargo test` bare, per AGENTS.md "Shared-Machine Resource Etiquette"). Paste the `test result:` summary line and confirm the passed count is non-zero. Record `task affected` `Gates selected`.

## Suggested approach

1. Snapshot the flow map at each branch during the transformer walk instead of calling `buildDataFlowMap` once at `:187`. Kill bindings on unknown reassignment. Before walking a loop, collect every name assigned in its condition, body and incrementor, and havoc those names at the loop head.
2. Collect the set of assigned parameter names up front, and route those names through the flow map (seeded with `param <name>`) instead of the `paramNames` short-circuit.
3. If ts-flow-analysis-consolidation (the shared flow-analysis module) exists when this is picked up, implement the fix there once. Do not block on it.

## Out of scope

- The core-side consistency guard: core-constraint-consistency-guard (this bucket).
- Consolidating the analyzer, instrumentor and executor builders/walkers (ts-flow-analysis-consolidation, str-rf2v).
- New operator support (ts-operators-collapse-to-unknown).
- Loop widening more precise than havoc.

## Related

- str-4kop (closed; SSA phi/ite for conditional reassignment) and str-mu3h (closed; closures over mutable state become unknown) are neighbouring precision work, not this bug.
- str-rf2v (open; builder consolidation investigation) and ts-flow-analysis-consolidation (this bucket).
- core-constraint-consistency-guard (this bucket): the core-side backstop, split out of this draft.
- concolic-early-termination (shatter-concolic-and-engine-design bucket): wrong constraints are one plausible contributor.

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

Verified at 56c86168 and re-checked unchanged at 793f2b0b (2026-09-23).

- `shatter-ts/src/analyzer.ts:1364-1471`: emits `branch_type: "switch"` once per `case` clause and none for `default` (:1364-1381), `"ternary"` (:1384-1398), and one `"logical_and"`/`"logical_or"` per top-level `&&`/`||` chain whose condition is the **whole** expression (:1401-1424, no recursion into nested chains).
- `shatter-ts/src/instrumentor.ts:1393-1411`: for `switch` it only inserts `__shatter_record(line)` per clause statement. There are no branch probes and no constraints.
- The instrumentor has no `isConditionalExpression` handling. Its only `ConditionalExpression` is `factory.createConditionalExpression` at `:2260`, which builds mock wrappers.
- There is no nullish branch type: generated `ALL_BRANCH_TYPES` (`shatter-ts/src/generated/protocol-enums.ts:78-88`) has no entry for `??`, and the analyzer emits nothing for it.
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

## Decision semantics (normative for this issue)

These definitions are part of the acceptance. If the implementer finds one unworkable, change it in this issue (comment) before implementing, and keep the analyzer and instrumentor in agreement.

| Construct | Branch ids | Decision recorded at runtime | Constraint for `taken` / not taken |
|---|---|---|---|
| `switch (d)` `case E:` | one per `case` clause (unchanged from the analyzer today) | one decision **per case label that JS evaluates**, in source order, until one matches. Matching label: `taken = true`. Each earlier evaluated label: `taken = false`. Labels after the match are not evaluated and record nothing. | `eq(d, E)` / `not(eq(d, E))` |
| `switch` `default:` | no id of its own | reaching `default` is the path where every evaluated case decision is `false`; no extra decision | n/a |
| fallthrough from a matched case into later clauses | none | **no** decision: executing a later clause's body through fallthrough does not imply `d === E` for that clause | n/a |
| ternary `c ? x : y` | one per ternary | `taken = truthy(c)` | truthiness of `c` / its negation |
| value-position `a && b` chain (top level only, as the analyzer emits today) | one per chain | `taken = truthy(a)` for the **leftmost** operand, that is "right side evaluated" | truthiness of the leftmost operand / its negation |
| value-position `a \|\| b` chain | one per chain | `taken = !truthy(a)`, "right side evaluated" | negated truthiness of `a` / truthiness of `a` |

The analyzer's `condition` / `condition_text` for `logical_and`/`logical_or` changes from the whole expression to the leftmost operand to match. `d` and each `E` are evaluated **exactly once**, as JS does; the probe must not re-evaluate either.

## Acceptance criteria

- [ ] The instrumentor emits branch probes with constraints for `switch` case labels, ternaries and value-position `&&`/`||` chains, exactly as in the table above. The constraints are built through **both** `buildSymExpr` and `buildSymExprWithFlow`, with parity tests.
- [ ] **One branch identity.** A single shared enumerator (one traversal, used by both analyze and instrument) assigns every branch an id. For each id, analyze's `(line, branch_type)` equals what the instrumentor emits for that id. The "join on (line, type) in the core" alternative is not acceptable: two branches of the same type on one line must still get distinct ids.
- [ ] **Alignment test** over a fixture corpus asserts analyze `(id, line)` == instrument `(id, line)` for every branch, where the instrument side is read from the `__shatter_branch(<id>, <line>, ...)` calls in the instrumented output. The corpus includes `mixed`, **two ternaries on one line**, **two `if` statements on one line**, and a ternary inside an `if` body. It fails on current `main` and passes after the fix.
- [ ] **Semantics tests** (unit or E2E). The fallthrough and default cases are red on current `main` (no decisions are recorded today); the side-effect cases are regression guards and may already pass:
  - fallthrough: `switch (a) { case 1: x++; case 2: return x; }` with `a = 1` records `taken = true` for case 1 and **no** decision for case 2;
  - effectful labels: a `case f():` whose `f` increments a counter shows the counter incremented once per evaluation, same as the uninstrumented function;
  - default: `a = 99` records `taken = false` for every case label and reaches `default`;
  - a ternary and an `&&` chain whose operands have side effects run each side effect exactly as often as uninstrumented code.
- [ ] The per-BranchType known-answer E2E fixtures from ts-branchtype-known-answer-fixtures pass for `switch`, `ternary`, `logical_and` and `logical_or`, with their expected-fail markers removed. If that issue has not landed, add those fixtures here.
- [ ] The `raw_results` workaround is removed from the enum E2E: the test reads `result.executions`. The prose at `shatter-ts/CLAUDE.md:285-287` and the comment at `e2e_concolic.rs:2947-2953` are deleted.
- [ ] `h(x) = x > 1 ? 1 : 0` reports `2/2` branch outcomes (or the equivalent in the report's branch metric) under both the default explorer and `--concolic`. Paste both CLI outputs in the close note.
- [ ] If output changes, `PARITY.md:50`, the TS rows of `protocol/parity-matrix.yaml` and `shatter-ts/CLAUDE.md` state what TS actually instruments and the decision semantics above. Run `task parity` and `task conformance`.
- [ ] Close-time proof: `task --force e2e-ts` (the governed task, not bare `cargo test`); paste the `test result:` line with a non-zero passed count and the lines for the enum and new branch-type tests. Record `task affected` `Gates selected`.

## Suggested approach

Build the shared branch enumerator first, so ids cannot drift again, then add the probes. For `switch`, hoist the discriminant into a temporary evaluated once, then wrap each case label so that the label is evaluated once and compared by `===` inside the probe (for example `case __shatter_case(id, line, tmp, <label>, ...)` returning the label value, or a rewrite into an equivalent `if` chain that preserves fallthrough). Ternary and logical probes wrap the condition value, not a re-evaluation, and must not change short-circuit behaviour.

## Out of scope

- `??` as a branch. There is no nullish BranchType in `protocol/registry.yaml`, and the core has no sound null model for it (see ts-operators-collapse-to-unknown). Adding one is a separate registry change.
- `&&`/`||` inside `if`/loop conditions (already covered by condition decomposition, str-6wmm.3).
- The engine-level random-vs-concolic parity suite (engine-parity-e2e, another bucket).
- Go/Rust instrumentor coverage of these branch types.
- New SymExpr operators beyond what the constraints need (ts-operators-collapse-to-unknown).

## Related

- str-wsg (closed; analyzer-only extraction). A reopen-note in this bucket points here.
- str-w0d.1 (closed; claimed switch/ternary/&&/|| constraint emission), str-ts3n (closed), str-jeen.81 (closed; expression-bodied arrows), str-6wmm.3 (closed; condition decomposition).
- ts-branchtype-known-answer-fixtures (this bucket): the E2E gate whose expected-fail markers this issue flips.
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
> The fix is tracked in <ts-switch-ternary-instrumentation id>. It covers instrumenting switch case labels, ternaries and value-position `&&`/`||` chains with defined decision semantics (fallthrough and `default` record no extra decision), one shared branch enumerator so analyze and instrument ids cannot drift (including same-line branches), an analyze/instrument id-alignment test, and per-BranchType known-answer E2E fixtures. Evidence: audit 2026-09-22 findings frontend-ts-02 and prior-17 (`audits/2026-09-22/areas/frontend-ts.md` F2).

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
- [ ] Record `task affected` `Gates selected` at close. If instrument output changes, run `task --force e2e-ts` (the governed task, not bare `cargo test`) and paste its `test result:` line with a non-zero passed count.

## Suggested approach

For the analyzer, the TypeChecker is already available (`analyzer.ts:362` `const checker = program.getTypeChecker()` in `analyzeFile`). Carry the parameter symbols into the builder instead of a name set. The instrumentor runs as a transformer, possibly without a checker, so a lexical scope stack (push the bound names at each function, arrow or block) is the cheaper route there.

## Out of scope

- Consolidating the builders (ts-flow-analysis-consolidation, str-rf2v). If that shared builder lands first, do the fix there.
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

`parseRequest` checks `id`, `protocol_version` and `command`, then casts the rest (`parsed as Request`). A request with a valid envelope but missing or ill-typed fields reaches the handlers and fails deep inside them, as `internal_error: Cannot read properties of undefined` or `file_not_found: undefined`. It should be rejected as `invalid_request` naming the bad field.

The command allow-list is also a hand-written literal that duplicates the generated `ALL_COMMANDS`. Because of that, a known-but-unsupported command (for example `get_invocation_plan`) is answered as `invalid_request "Unknown command"` instead of `not_supported`.

## Evidence

Verified at 56c86168 and re-checked unchanged at 793f2b0b (2026-09-23).

- `shatter-ts/src/handlers.ts:1113` `export function parseRequest(...)`, which checks the envelope, then returns `{ request: parsed as Request }` (through `:1159`).
- `shatter-ts/src/handlers.ts:1151`:
  ```ts
  const validCommands = ["handshake", "analyze", "instrument", "prepare", "execute", "setup", "teardown", "generate", "shutdown"];
  ```
  followed at `:1154` by `errorResponse(id, "invalid_request", `Unknown command: ...`)`.
- Generated `ALL_COMMANDS` is at `shatter-ts/src/generated/protocol-enums.ts:14-25` (`analyze, execute, generate, get_invocation_plan, handshake, instrument, prepare, setup, shutdown, teardown`), re-exported via `protocol.ts:18`.
- `SUPPORTED_CAPABILITIES` (`shatter-ts/src/handlers.ts:56-68`) is a **capability** list, not a command list: it contains `analyze, execute, instrument, prepare, setup, teardown, generate` plus `complex_type:*` entries, and it does **not** contain the control commands `handshake` or `shutdown`. It cannot be used directly as the dispatch set.
- Audit probes over stdio against `node dist/main.js` (full envelopes; the audit notes abbreviated them):
  - `{"protocol_version":"0.1.0","id":1,"command":"execute"}` -> `internal_error "Unhandled error: Cannot read properties of undefined (reading 'includes')"`;
  - `{"protocol_version":"0.1.0","id":2,"command":"analyze"}` -> `file_not_found "File not found: undefined"`.

  The verifier confirmed the code path, not the live probe.
- `protocol/fixtures/requests/invalid/` holds nine invalid requests (`missing-required-field.json` is the analyze probe above; also `prepare-missing-function.json`, `wrong-field-type.json`, `negative-id.json`, `unknown-command.json`, ...). No TS test drives them through `parseRequest` + `handleRequest`.
- `shatter-ts/CLAUDE.md:155-157` says conformance expects TS to "return a clean 'capability not supported' response" for planner commands. The only `get_invocation_plan` case (`protocol/conformance/conformance_cases.yaml:538`, `planner_runtime_value_go`) is `frontends: [go]`, so nothing checks that claim.
- Audit sources: finding frontend-ts-07 (and frontend-ts-08 for the `not_supported` half); `audits/2026-09-22/areas/frontend-ts.md` F7/F8.

## Acceptance criteria

- [ ] There is a per-command field validator for every command TS dispatches. A missing or ill-typed required field yields `invalid_request`, with a message naming the command and the field (for example `execute: missing required field 'inputs'`). Ideally the validators are generated from `protocol/registry.yaml` `field_model` through the existing codegen, so they cannot drift.
- [ ] Dispatch classification, with the literal at `handlers.ts:1151` removed:
  - `handshake` and `shutdown` are **control commands** and are always dispatched, whatever the capability list says;
  - the supported command set is an explicit constant (for example `SUPPORTED_COMMANDS`) of command names, separate from `SUPPORTED_CAPABILITIES`; a test asserts it is a subset of `ALL_COMMANDS` and that every non-`complex_type:` capability is in it;
  - a command in `ALL_COMMANDS` but not supported returns `not_supported` (matching `shatter-ts/CLAUDE.md:155-157`);
  - a command not in `ALL_COMMANDS` returns `invalid_request`.
- [ ] A unit test pins the classification for every value of `ALL_COMMANDS` plus one unknown command; `handshake` and `shutdown` must reach their handlers and return `handshake` / `shutdown_ack`.
- [ ] Every file in `protocol/fixtures/requests/invalid/` driven through `parseRequest` + `handleRequest` yields `invalid_request` (the test iterates the directory, so a new invalid fixture is covered automatically). This test fails on current `main` for at least `missing-required-field.json` and `prepare-missing-function.json`.
- [ ] Conformance cases for TS (and for the other frontends where cheap), with full envelopes:
  - malformed `execute` (no fields);
  - malformed `analyze` (no `file`);
  - a planner-command probe (`get_invocation_plan`) expecting `not_supported`;
  - `handshake` then `shutdown` still succeed.

  Each new negative case fails on current `main` and passes after the fix; paste the `task conformance` output (case names and pass counts) in the close note.
- [ ] If the wire behaviour changes, update `shatter-ts/CLAUDE.md` and `protocol/parity-matrix.yaml`, then run `task parity`.
- [ ] Record `task affected` `Gates selected` at close.

## Suggested approach

Generate the validators if codegen already walks `field_model`. Otherwise hand-write a small `validateRequest(command, obj)` table keyed by `ALL_COMMANDS`, plus a test asserting that every command has an entry.

## Out of scope

- Implementing planner commands in TS (str-mhinv.2 / str-mhinv.3).
- Go/Rust request validation beyond the shared conformance cases.
- Valid-fixture round trips (ts-protocol-and-parity-tests-meaningful).

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

**Target:** `str-rf2v` ("Investigate consolidating TS buildSymExpr implementations (analyzer/instrumentor triplication)", P2, OPEN; live state checked with `bd show str-rf2v` on 2026-09-23).
**Action:** post ONE comment (`bd comments add str-rf2v`). Keep the priority at P2, the title and the description unchanged. The comment adds evidence to the **investigation** and points at the implementation issue drafted alongside it. It does **not** widen str-rf2v into the refactor: str-rf2v's description says "Out of scope: the actual consolidation refactor", and that stays true.
**Ordering:** post after `ts-flow-analysis-consolidation` and `ts-flow-map-program-point` are filed, and substitute their ids for the `<...>` placeholders.

**Existing notes on str-rf2v (from the 2026-09-04 audit)** already reject option (b) and recommend collapsing to one builder with a resolver callback. This comment is consistent with them and adds the walker and analyzer-data-flow facts they did not cover.

## Comment text

> **Audit 2026-09-22 note** (findings frontend-ts-03 and frontend-ts-09; evidence in `audits/2026-09-22/areas/frontend-ts.md` F3/F9, re-checked at 793f2b0b).
>
> Three facts for this investigation. They strengthen the existing note's rejection of option (b) ("document the triplication as load-bearing and add a node-type parity test").
>
> **1. There is a fourth copy: an SSA flow walker in `executor.ts`, and it has already diverged.**
> `shatter-ts/src/executor.ts:1497-1810` contains `visitStatementsForLoopSnapshots` (:1497), `buildLoopSnapshotMutatedExpr` (:1728) and `mergeLoopSnapshotFlowMaps` (:1769). It is a near-copy of the instrumentor walker at `instrumentor.ts:361-632` (`visitStatementsForDataFlow` :361, `mergeFlowMaps` :569). `mergeFlowMaps` and `mergeLoopSnapshotFlowMaps` are line-for-line equivalent. The executor copy has no destructuring (`registerDestructuredBindings`), no `while`/`do`/`for-in`/`for-of` traversal, and no closure poisoning: a grep of 1497-1810 for those constructs finds 0 hits, against 3 in the instrumentor range. Neither this issue ("triplication") nor the parity table in `shatter-ts/CLAUDE.md` mentions it. The root CLAUDE.md "grep for the parallel path" rule names only `buildSymExpr`/`buildSymExprWithFlow`, so copies under other names escape it.
>
> **2. The analyzer's builder ignores data flow.**
> `shatter-ts/src/analyzer.ts:2280` (the non-exported `buildSymExpr`, "local copy for analyzer independence") knows only `paramNames`. For `const y = a * 2; if (y > 10)`, analyze emits `{gt, unknown, 10}` while instrument emits `{gt, a*2, 10}`. For `let x = a; if (x > 0)`, analyze gives `{gt, unknown, 0}`. Go's analyzer threads a flow map, so TS and Go `analyze` differ on the same source, and that divergence is not recorded anywhere.
>
> **3. The parity matrix is wrong about TS.**
> `protocol/parity-matrix.yaml:1096-1110` (`ite-symexpr-production-partial`) says "TypeScript produces ite expressions via data flow analysis in visitStatementsForDataFlow" and lists `affected_frontends: [rust]` for `affected_commands: [analyze]`. TS produces ite only on the instrument/execute path.
>
> **Resolution proposal:** close this investigation through option (a), "file the implementation issue with the design sketch". That implementation issue is drafted as <ts-flow-analysis-consolidation id>: one flow-analysis module (walker plus builder) used by the analyzer, instrumentor and executor, analyze/instrument SymExpr equality on a shared corpus, and the parity-matrix correction above. To close this issue, append the design sketch (resolver-callback builder per the existing note, plus the walker hooks) to that issue and cite it here. The program-point soundness fix is tracked separately in <ts-flow-map-program-point id> and does not wait for the consolidation.

---

---
slug: ts-protocol-and-parity-tests-meaningful
kind: new
title: "TS protocol round-trip tests bypass the real wire path and schemas, and the builder-parity property test cannot detect drift"
priority: P2
type: task
labels: [typescript, testing, protocol, parity, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# TS protocol round-trip tests bypass the real wire path and schemas, and the builder-parity property test cannot detect drift

## Problem

Two groups of TS tests look like protocol and parity coverage but do not exercise what they are named for.

1. **Protocol "round-trip" tests.** They assert `JSON.parse(JSON.stringify(x))` equals `x` for objects built by TS arbitraries. That is not quite a tautology: it does catch values JSON cannot carry (closed str-0z1im was exactly such a failure, on `-0`). But it never touches the real wire path (`serializeReplacer` in `sendResponse`, or `parseRequest`), the shared `protocol/schemas`, or `protocol/fixtures`. Yet `shatter-ts/Taskfile.yml` lists those directories and `shatter-core/src/protocol.rs` as test sources. Those are dead cache keys that suggest schema coverage that does not exist.
2. **Builder-parity property test.** A `buildSymExpr` / `buildSymExprWithFlow` parity describe block exists (`property.test.ts:1187`, 4 cases). *Verifier correction for finding tests-ci-12: the parity test is present, so the gap is narrower than "no parity test".* It has three limits:
   - it generates only node kinds that both builders already handle;
   - it compares only unknown vs non-unknown, not the output;
   - it does not cover the analyzer's builder or the executor's walker.

   A new node kind added to one builder, or a semantic difference between builders, passes it.

## Evidence

Verified at 56c86168 and re-checked unchanged at 793f2b0b (2026-09-23).

- `shatter-ts/src/property.test.ts:717` `describe("property: protocol message round-trips")`, whose cases through `:870` are plain JSON round-trips. The pattern repeats for SideEffect (`:871`), SymExpr (`:1002`), TypeInfo (`:1036`), BranchDecision (`:1047`) and TraceEvent (`:1059`). `serializeReplacer` (`shatter-ts/src/serialize.ts`, BigInt only) appears only in the separate BigInt tests (around `:1798-1850`).
- A grep of `shatter-ts/src/*.test.ts` finds no reference to `protocol/schemas` or `protocol/fixtures`.
- `shatter-ts/Taskfile.yml` `test` sources `:45-48` and `test-fast` sources `:62-65` list `../protocol/schemas/**/*.json`, `../protocol/fixtures/**/*.json` and `../shatter-core/src/protocol.rs`.
- The valid request fixtures are schema examples, not runnable scenarios: `protocol/fixtures/requests/valid/analyze.json` and `execute.json` target `src/example.ts::processOrder` (no such file ships with the fixtures), `execute.json` carries a `setup_context` for a non-existent setup, and `execute-with-prepare-id.json` uses a fabricated `prepare_id` (`a1b2c3d4e5f60001`). Run as-is, most would only produce `file_not_found` / unknown-prepare errors.
- `property.test.ts:1117` `arbBinOp` omits `in`/`instanceof`, which both token maps handle (`instrumentor.ts:2020-2023`). `:1097` `hasNonUnknownLeaf`, and the assertions at `:1187-1296`, check only unknown-ness.
- The analyzer builder (`analyzer.ts:2280`) is not exported and is untested. The executor loop-snapshot walker (`executor.ts:1497-1810`) is untested for parity.
- 7 of the 21 `shatter-ts/src/*.test.ts` files use fast-check. There are also semantic properties for `flattenConditions` and MC/DC masking (`property.test.ts` ~1587-1721), and SymExpr structural-validity checks. Keep those.
- str-0z1im (closed 2026-09-19, landed 4f673612): InvocationOutcome round-trip failed on `-0`. It is the precedent for the serialization guarantees below; there is no open overlap.
- Audit sources: findings frontend-ts-10, frontend-ts-11, tests-ci-12 (verifier: partially confirmed, parity block exists); `audits/2026-09-22/areas/frontend-ts.md` F10/F11.

## Acceptance criteria

- [ ] **Serialization guarantees are written down and tested on the real path.** Next to the tests, list what the wire encoding (`JSON.stringify(x, serializeReplacer)` as in `main.ts:17`) must preserve and what it deliberately normalizes. At minimum: BigInt -> `__complex_type: big_int`; `-0` (preserved or normalized to `0`, matching whatever str-0z1im decided); `NaN` / `Infinity` / `undefined` fields. A property test over the existing arbitraries serializes through `serializeReplacer`, parses, and asserts equality **modulo exactly those listed normalizations**.
- [ ] **Schema validation on the real wire path:** responses produced by the arbitraries, serialized as `sendResponse` does, are validated against `protocol/schemas` with ajv. Instrument/analyze outputs from a small fixture corpus are also validated. The test demonstrably catches a violation: a deliberately invalid response case is asserted to fail validation.
- [ ] **Valid request fixtures as executable scenarios.** A test harness:
  - materializes a temp project containing the source the fixtures reference (`src/example.ts` exporting `processOrder`, and a setup module if a fixture needs one);
  - substitutes the temp project root into `file` / `project_root` / `function` paths, and replaces fabricated ids (for example `prepare_id`) with the ids returned by earlier responses;
  - sends the fixtures in protocol order (`handshake`, `analyze`, `instrument`, `prepare`, `execute` variants, `setup`, `teardown`, `generate`, `shutdown`) through `parseRequest` + `handleRequest`;
  - asserts an **expected status per fixture** from a table in the test file. A fixture TS does not support (for example `get-invocation-plan.json`, `execute-with-runtime-value-plan.json`) has an explicit expected error code with a reason. `file_not_found`, `function_not_found` and unknown-prepare errors are never accepted as the expected result of a valid fixture.
  - A new file added to `protocol/fixtures/requests/valid/` without a table entry fails the test.
- [ ] **Builder parity by output:** replace the unknown-vs-non-unknown check with a fixed corpus that includes currently unsupported nodes (`??`, `**`, shifts, element access, `as`/`!`, template literals, `in`, `instanceof`). The test asserts **output equality modulo documented collapse rules** across all builders, the analyzer builder included (exported for tests, or deleted per ts-flow-analysis-consolidation). Each collapse rule is written down next to the test.
- [ ] `shatter-ts/Taskfile.yml` `sources:` match what the tests actually read: entries for inputs no test reads are removed, and inputs that tests read are added.
- [ ] The existing plain-JSON round-trip cases are removed, or rewritten to go through `serializeReplacer`/`parseRequest`. They are not kept alongside the new tests.
- [ ] Close-time proof: `npx jest` in `shatter-ts` runs the new suites (paste the per-suite counts), and `task affected` `Gates selected` is recorded.

## Suggested approach

Add `ajv` as a devDependency (it is not in `shatter-ts/package.json` today), load `protocol/schemas` in a jest `beforeAll`, and reuse the existing arbitraries. If TS output hits a schema defect that the protocol-schemas-reject-real-output issue (shatter-protocol-parity bucket) owns, mark that case expected-fail citing its id; do not weaken the schema here. For parity, a table-driven test over a source corpus is enough; fast-check on top is optional.

## Out of scope

- Invalid request fixtures: ts-request-validation (this bucket) drives `protocol/fixtures/requests/invalid/` and asserts `invalid_request`.
- Consolidating the builders (ts-flow-analysis-consolidation / str-rf2v).
- Adding operator support (ts-operators-collapse-to-unknown). This issue only makes the parity test able to see the gaps.
- Go/Rust schema tests.

## Related

- protocol-schemas-reject-real-output (other bucket): hand-maintained schemas already reject some real frontend output, so this suite may surface more of that.
- str-jalv (closed; builder parity properties), str-hicn (closed; WithFlow must handle every node buildSymExpr handles), str-rf2v (open), str-4btb (closed; removed trivial round-trips from parseRequest tests), str-qwua7.47 (open; Rust-only PBT), str-0z1im (closed; `-0` round-trip).

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

Verified at 56c86168 and re-checked unchanged at 793f2b0b (2026-09-23).

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
- The E2E tests are `#[ignore = "subprocess E2E; run via task e2e-ts or core:test-ignored"]`. `task e2e-ts` is checksum-cached, so a Task "pass" can execute nothing; `task --force e2e-ts` re-runs it through the governed wrapper (`Taskfile.yml:591-609`).
- The instrument response carries no branch metadata: `InstrumentResponse` (`shatter-ts/src/protocol.ts:211-216`) has `instrumented`, `output_file` and `instrumentable_line_count`, and the internal `InstrumentResult` (`shatter-ts/src/instrumentor.ts:19-31`) exposes only `branchCount`. The branch ids and lines exist only as the first two arguments of the `__shatter_branch(<id>, <line>, ...)` calls in the instrumented source.
- Audit sources: finding frontend-ts-18 (with frontend-ts-02 and prior-17); `audits/2026-09-22/areas/frontend-ts.md` F18; `drafts/shatter-agent/23-mechanical-parallel-parity-gates.md`.

## Acceptance criteria

- [ ] A table-driven TS known-answer fixture set in `shatter-core/tests/e2e_concolic.rs` (or a new `e2e_concolic_ts_branch_types.rs` wired into `task e2e-ts`) has one small function per TS-emitted BranchType: `if`, `else_if`, `switch`, `ternary`, `logical_and`, `logical_or`, `while` (including a `do` variant) and `for`. Each function has a known triggering input for each outcome. `select` is excluded as Go-only, and the fixture file states this.
- [ ] Each fixture asserts **(a)** that the analyze branch `(id, line)` set equals the instrument branch `(id, line)` set for that function, where the instrument side is extracted by a test helper that parses the `__shatter_branch(<id>, <line>, ...)` calls in the file named by the instrument response's `output_file` (no protocol change is needed; if ts-switch-ternary-instrumentation adds branch metadata to the response, the helper may switch to it), and **(b)** that both outcomes of each branch are discovered through `branch_path` in `result.executions`, not `raw_results`, under both the default explorer and `--concolic`.
- [ ] Today, `switch`, `ternary`, `logical_and` and `logical_or` fail. Those cases are marked expected-fail with the ts-switch-ternary-instrumentation id, so the suite runs green, and each marker **flips to a failure when the case starts passing**, so it must be removed. No silent `#[ignore]`.
- [ ] A test asserts that every value in `ALL_BRANCH_TYPES` has either a fixture here or an explicit exclusion with a reason (for example `select`: Go-only). Adding a BranchType to the registry without a TS fixture then fails the test.
- [ ] **The known prose workaround is tied to its issue.** The `raw_results` workaround (`shatter-ts/CLAUDE.md:285-287`, `e2e_concolic.rs:2947-2953`) is edited to cite the ts-switch-ternary-instrumentation id, so it is removed when that issue lands. No wider sweep is part of this issue (see Out of scope).
- [ ] Close-time proof: `task --force e2e-ts` (the governed task; do not run the underlying `cargo test` bare, per AGENTS.md "Shared-Machine Resource Etiquette"). If the fixtures live in a new test file, wire it into `e2e-ts-governed` first. Paste the per-test lines showing every fixture executed, the expected-fail cases reported as expected-fail, and the `test result:` line. Record `task affected` `Gates selected`.

## Suggested approach

Model the fixtures on `examples/go/05-conditional-merge.go` and `shatter-core/tests/e2e_concolic_go.rs`. Keep each function tiny, so the triggering inputs are obvious. Get the analyze side from the frontend `analyze` response, and the instrument side by parsing the `__shatter_branch` probes in the instrumented `output_file`. This keeps the issue independent of ts-switch-ternary-instrumentation: the fixtures land first with expected-fail markers, and that issue flips them.

## Out of scope

- Fixing the instrumentor (ts-switch-ternary-instrumentation).
- Go/Rust per-BranchType fixtures. File follow-ups if wanted; per-frontend parity of emitted branch types is a candidate drift-patrol check.
- The engine_parity random-vs-concolic suite and the `_`-param lint (engine-parity-e2e), and the completion-checklist rule "test workarounds must be filed as issues" (pipeline-close-reason-rule).
- A repo-wide sweep of other frontends' docs and tests for further prose workarounds. It has no fixed end point; if wanted, it belongs with the pipeline-close-reason-rule checklist work or its own bounded issue.

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
> - (a) behaviour changes: a command that is in generated `ALL_COMMANDS` but not in an explicit supported-**command** set returns `not_supported`, and truly unknown commands keep `invalid_request`. The supported-command set must be separate from `SUPPORTED_CAPABILITIES` (`handlers.ts:56-68`), which holds `complex_type:*` entries and omits the control commands `handshake` and `shutdown`; those two are always dispatched. Using `SUPPORTED_CAPABILITIES` as the dispatch set would reject handshake and shutdown; or
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

Verified at 56c86168 and re-checked unchanged at 793f2b0b (2026-09-23).

- `shatter-ts/src/instrumentor.ts:1862` `buildSymExpr` (handled node kinds through about `:1953`) and `:1984-2027` operator mapping (`binaryTokenToOp`), which has no cases for `??`, `**` or shifts. `buildSymExprWithFlow` (`:874`) and the analyzer copy (`analyzer.ts:2280`) have the same gaps.
- The instrumentor has no `isAsExpression`, `isNonNullExpression`, `isSatisfiesExpression` or `isElementAccessExpression` handling (grep finds 0 hits).
- Probes:
  - `const v = a ?? 7; if (v > 3)` -> `{unknown} > 3`;
  - `if (2 ** a! > 8)` -> `unknown > 8`;
  - `if (xs[i] === 7)` -> `unknown === 7`.

  The verifier reproduced the `??` and element-access probes; it did not probe `**` or `!`.
- Core `BinOpKind` (`shatter-core/src/sym_expr.rs:95-114`) has `Shl`, `Shr` and `BitClear` (commented "Go-specific"), `In` and `InstanceOf`. It has **no** power operator and no unsigned right shift. The TS `BinOpKind` union (`shatter-ts/src/protocol.ts:514`) lacks `shl`/`shr` (see str-qwua7.37).
- Core solver limits (`shatter-core/src/solver.rs`): `ConstValue::Null | ConstValue::Undefined => Int(0)` (`:522`), so null, undefined and numeric `0` are indistinguishable to Z3; `BinOpKind::Shl | Shr` (with the other bitwise ops) return `SolverError::Unsupported("bitwise operator ... not yet supported in Z3 solver")` (`:601-608`). The core triage evaluator likewise maps both null and undefined to JSON `null` (`triage.rs:369`).
- Audit sources: finding frontend-ts-14; `audits/2026-09-22/areas/frontend-ts.md` F14.

## Acceptance criteria

- [ ] `as`, `!`, `satisfies` and `<T>x` type assertions (and parentheses) are unwrapped before building, in every builder.
- [ ] `a ?? b` stays `unknown` as a **documented collapse rule**, because the core has no sound null model (null and undefined solve as integer `0`, so `ite(eq(a, null) or eq(a, undefined), b, a)` would pick `b` for `a = 0`). One sound exception is allowed: when the TypeChecker (or a syntactic literal) proves the left operand cannot be null or undefined, `a ?? b` reduces to `a`. A test pins `x ?? 7` with `x: number | undefined` to `unknown` and, if the exception is implemented, `n ?? 7` with `n: number` to `param n`. Real nullish support needs a core null model and is a separate issue.
- [ ] `<<` and `>>` map to the core `Shl`/`Shr` **on the wire**, with `shl`/`shr` added to the TS `BinOpKind`. This is emission only: the Z3 solver rejects `Shl`/`Shr` today (`solver.rs:601-608`), so these constraints will not be solvable. The close note must not claim solver support, and a test asserts that a shift constraint reaches the core as `shl`/`shr` and is reported as unsupported (not a crash or a wrong model). Solver support is a separate core issue.
- [ ] `>>>` and `**` either get a core op wired through Z3 or stay `unknown`, with the choice recorded as a documented collapse rule. Do not map `**` to `Mul`.
- [ ] Element access with a literal key (`o["k"]`, `xs[0]`) becomes a `param.path` segment. A non-literal index stays `unknown`, as a documented rule.
- [ ] Each construct has a unit test in **both** `buildSymExpr` and `buildSymExprWithFlow`, and in the analyzer builder unless ts-flow-analysis-consolidation has deleted it. Each test fails on current `main`.
- [ ] One known-answer E2E case using `as` or `!` unwrapping (not `??` or shifts, which the solver cannot use) shows the concolic engine flipping a branch it could not flip before. Close-time proof: `task --force e2e-ts` (the governed task, not bare `cargo test`); paste that test's line and the `test result:` line. Record `task affected` `Gates selected`.
- [ ] Any new op that reaches the wire is added to `protocol/schemas` and PROTOCOL.md (coordinate with protocol-schemas-reject-real-output), and `task parity` passes.

## Suggested approach

Do this inside the shared builder if ts-flow-analysis-consolidation has landed, so all builders gain the operators at once. Otherwise add each construct to all three builders in one change, and extend the output-equality parity corpus from ts-protocol-and-parity-tests-meaningful. Start with the unwrapping, which is cheap and high-yield.

## Out of scope

- Template literal and string-concatenation solving (Z3 string theory). Keep these `unknown` unless it is trivial.
- BigInt semantics.
- Go/Rust operator coverage.

## Related

- str-qwua7.37 (open; SymExpr construction spec, TS lacks shl/shr), str-rf2v (open), ts-flow-analysis-consolidation (this bucket), str-a4c (closed; added Shl/Shr to BinOpKind for Go, no solver support), protocol-schemas-reject-real-output (shatter-protocol-parity bucket; schemas lack shl/shr).

## Priority / type / size

P3 · feature · size S-M

---

---
slug: ts-lifecycle-and-packaging-hygiene
kind: new
title: "shatter-ts lifecycle: async timeout timer never cleared (hidden by jest forceExit); shutdown and stdin EOF drop in-flight responses"
priority: P3
type: bug
labels: [typescript, lifecycle, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# shatter-ts lifecycle: async timeout timer never cleared (hidden by jest forceExit); shutdown and stdin EOF drop in-flight responses

Scope note: the slug is kept from the earlier combined draft. The packaging items (dual lockfiles, tests emitted to `dist/`, standalone bundle worker name) moved to ts-packaging-hygiene, and the js-yaml major upgrade moved to ts-js-yaml-v4. This issue is only process-lifecycle correctness.

## Problem

1. **Timer leak.** Every async `execute` starts a `setTimeout` for the harness timeout and never clears it, so each call leaves a pending timer of up to `timeoutMs`. Jest's `forceExit: true` hides leaked handles like this one from the test suite.
2. **Shutdown and EOF drop in-flight work.** Requests are handled concurrently (each stdin line starts its own `handleRequest` promise). The `shutdown` handler terminates the instrumentation worker immediately, so an `instrument` still running in that worker fails with `Worker terminated`. On stdin `close`, the process calls `process.exit(0)` without waiting for pending promises, so responses in flight are lost, including the `shutdown_ack`.

Impact today is low, because the core waits for each response before sending the next request. It becomes real for any pipelined client, and the leaked handles weaken the test suite.

## Evidence

Verified at 56c86168 and re-checked unchanged at 793f2b0b (2026-09-23).

- `shatter-ts/src/executor.ts:1272-1282`: `Promise.race([syncResult, new Promise((_, reject) => setTimeout(() => reject(new Error("async execution timed out")), timeoutMs))])`. The timer id is not kept, so it can never be cleared.
- `shatter-ts/jest.config.js:7`: `forceExit: true` ("avoid hanging on worker threads that outlive tests").
- `shatter-ts/src/handlers.ts:1046-1069`: the `shutdown` case clears caches and runs `await _worker.terminate()` (`:1058-1061`) before returning `shutdown_ack`, without waiting for requests still using the worker.
- `shatter-ts/src/main.ts:76-79`: `rl.on("close", () => { ...; process.exit(0); })` exits without awaiting in-flight `handleRequest` promises.
- Audit probes (not re-run by the verifier): piping `handshake, shutdown` then EOF produced no `shutdown_ack`; `shutdown` during an in-flight `instrument` produced `internal_error "Unhandled error: Worker terminated"`.
- `shatter-ts/src/main.ts:23` hard-codes `"Starting TypeScript frontend (protocol 0.1.0)"`, although `PROTOCOL_VERSION` is imported at `main.ts:13`.
- Audit sources: findings frontend-ts-06 and frontend-ts-17; `audits/2026-09-22/areas/frontend-ts.md` F6/F17.

## Acceptance criteria

- [ ] The async race keeps its timer id and calls `clearTimeout` when the race settles either way (for example in `finally`). `unref()` alone does **not** satisfy this: the timer would still stay allocated until it fires. A unit test with jest fake timers asserts that no timer is pending after a fast async execute resolves, and after one that rejects.
- [ ] `forceExit` is removed from `jest.config.js`. The close note pastes the summary of an `npx jest --detectOpenHandles` run in `shatter-ts` showing no open handles; any other leaks it finds are fixed in this issue or filed with ids in the close note.
- [ ] The frontend tracks in-flight request promises. On `shutdown` it stops accepting new work, awaits in-flight requests (bounded by a timeout), and only then terminates the worker and sends `shutdown_ack`. On stdin `close` it awaits in-flight requests (same bound) before exiting.
- [ ] Regression tests that spawn `node dist/main.js`, each red on current `main`:
  - pipe `handshake`, `shutdown`, then EOF; assert `shutdown_ack` is received;
  - send `instrument` for a real fixture and then `shutdown` immediately without waiting; assert the `instrument` response is a success (not `internal_error ... Worker terminated`) and is written **before** `shutdown_ack`.
- [ ] The startup banner uses `PROTOCOL_VERSION`.
- [ ] Record `task affected` `Gates selected`. If the shutdown response ordering is protocol-visible, update `shatter-ts/CLAUDE.md` and run `task conformance`.

## Suggested approach

Keep a `Set<Promise>` of in-flight handlers in `main.ts`. Move the worker termination out of the `shutdown` handler into a drain step that `main.ts` runs after the in-flight set is empty. The timer fix touches the same race as ts-timeout-classification, which adds a dedicated timeout error class; if both are picked up together, do them in one change.

## Out of scope

- Outcome classification (ts-timeout-classification).
- Packaging (ts-packaging-hygiene) and the js-yaml upgrade (ts-js-yaml-v4).
- ESLint adoption (str-qwua7.31).

## Priority / type / size

P3 · bug · size S-M

---

---
slug: ts-flow-analysis-consolidation
kind: new
title: "TS: consolidate the four SymExpr builders / flow walkers (analyzer, instrumentor x2, executor loop snapshots) into one flow-analysis module"
priority: P2
type: task
labels: [typescript, parity, refactor, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# TS: consolidate the four SymExpr builders / flow walkers (analyzer, instrumentor x2, executor loop snapshots) into one flow-analysis module

## Problem

The TS frontend has four hand-maintained implementations of symbolic-expression construction and data-flow walking, and they already produce different output for the same source:

- `shatter-ts/src/analyzer.ts:2280` `buildSymExpr` (parameters only, no data flow);
- `shatter-ts/src/instrumentor.ts:1862` `buildSymExpr` and `:874` `buildSymExprWithFlow`;
- the instrumentor flow walker `instrumentor.ts:361-632` (`visitStatementsForDataFlow`, `mergeFlowMaps`);
- the executor loop-snapshot walker `shatter-ts/src/executor.ts:1497-1810` (`visitStatementsForLoopSnapshots`, `buildLoopSnapshotMutatedExpr`, `mergeLoopSnapshotFlowMaps`), which lacks destructuring, `while`/`do`/`for-in`/`for-of` traversal and closure poisoning.

str-rf2v is the **investigation** into consolidating them; its description puts the refactor out of scope and asks for option (a), "file the implementation issue with the design sketch". This is that implementation issue, split out of the audit's rf2v note after the Codex cross-check pointed out that widening str-rf2v by comment would contradict its own scope.

## Evidence

Verified at 793f2b0b (2026-09-23).

- The four locations above, and `bd show str-rf2v` (description: "Out of scope: the actual consolidation refactor"; existing 2026-09-04 notes recommend "Collapse to one builder `buildSymExpr(node, resolver)`" and reject option (b)).
- Divergence examples: for `const y = a * 2; if (y > 10)`, analyze emits `{gt, unknown, 10}` and instrument emits `{gt, a*2, 10}`. A grep of `executor.ts:1497-1810` for `registerDestructuredBindings`, `isWhileStatement` or closure poisoning finds 0 hits, against 3 in the instrumentor range.
- `protocol/parity-matrix.yaml:1096-1110` (`ite-symexpr-production-partial`) says TS produces ite through `visitStatementsForDataFlow` and lists `affected_frontends: [rust]` for `analyze`. TS analyze does not produce ite.
- Audit sources: findings frontend-ts-03 and frontend-ts-09; `audits/2026-09-22/areas/frontend-ts.md` F3/F9.

## Acceptance criteria

- [ ] **Parity-matrix correction (may land first, as its own commit):** `ite-symexpr-production-partial` lists `typescript` in `affected_frontends` for `analyze` and says TS produces ite only on instrument/execute, until the analyzer uses the shared module. `task parity` passes. Once the analyzer does produce ite, the entry is updated again in the same change.
- [ ] One flow-analysis module (for example `shatter-ts/src/flow-analysis.ts`) containing one walker parameterized by hooks (for example `onBranch`, `onLoopIteration`) and one SymExpr builder that takes a name resolver. The analyzer, the instrumentor (both paths) and the executor loop snapshots all use it. The old copies are deleted, including the duplicate operator tables (`binaryTokenToOp`/`unaryTokenToOp` vs `mapBinaryOp`/`mapUnaryOp`).
- [ ] The kill/havoc/program-point semantics from ts-flow-map-program-point live in the shared module (if that issue landed first, its regression tests still pass unchanged).
- [ ] **Output-equality test:** on a shared fixture corpus of flow-tracked locals (derived locals, if/else reassignment producing ite, loops, destructuring, closures), analyze and instrument produce **identical** SymExpr for the same condition. The test is red on current `main` for at least the `const y = a * 2` case.
- [ ] The builder output-equality test from ts-protocol-and-parity-tests-meaningful runs against the single builder (or is reduced to what still makes sense) and passes.
- [ ] `shatter-ts/CLAUDE.md` builder-parity section and the root CLAUDE.md "parallel parity" bullet name the new module instead of the deleted builders.
- [ ] Close-time proof: `task --force e2e-ts` (the governed task) with its `test result:` line and a non-zero passed count; `task conformance` output; `task affected` `Gates selected`. If any analyze output changes, regenerate TS conformance goldens and list the changed cases.

## Suggested approach

Follow the existing str-rf2v notes: `buildSymExpr(node, resolver)`, where the resolver is param-only for a plain lookup and flow-map-backed otherwise, and `resolvePropertyChain` consults the resolver for the base. Sequence after str-qwua7.37 (SymExpr construction spec) if it has landed, so the unknown-collapse rule is decided once.

## Out of scope

- New operator support (ts-operators-collapse-to-unknown); land it in the shared builder afterwards.
- Go/Rust builders.

## Related

- str-rf2v (open; the investigation this implements; the rf2v-fourth-walker-and-analyze-dataflow note links the two).
- ts-flow-map-program-point (this bucket; independent soundness fix, not blocked on this).
- ts-protocol-and-parity-tests-meaningful, ts-operators-collapse-to-unknown (this bucket).
- str-qwua7.37 (open; SymExpr construction spec).

## Priority / type / size

P2 · task · size L

---

---
slug: core-constraint-consistency-guard
kind: new
title: "Core: check each recorded branch constraint against the concrete execution before solving, with language-aware evaluation that returns indeterminate when unsure"
priority: P2
type: feature
labels: [core, concolic, solver, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Core: check each recorded branch constraint against the concrete execution before solving, with language-aware evaluation that returns indeterminate when unsure

## Problem

A frontend can record a branch constraint that is false for the inputs that actually ran (ts-flow-map-program-point shows TS doing this today). The core passes such constraints straight to Z3, which then negates something unrelated to the branch, and to triage, which uses them to predict paths. Nothing in the core notices the contradiction, so a frontend soundness bug becomes silent lost coverage.

The core already has a concrete evaluator, `triage::evaluate_constraint` (`shatter-core/src/triage.rs:316`). It is **not** a safe oracle for every frontend:

- `BinOpKind::Div` / `Mod` evaluate in `f64` (`triage.rs:442-455`), so a Go/Rust integer condition such as `a / 2 == 1` at `a = 3` evaluates to false where the target computed true.
- `ConstValue::Null` and `ConstValue::Undefined` both become JSON `null` (`triage.rs:369`), and `eval_eq` treats `(null, null)` as equal (`triage.rs:473`), so JS `null === undefined` evaluates to true.

An unconditional "downgrade on mismatch" guard built on it would discard correct Go/Rust constraints. This issue was split out of ts-flow-map-program-point (audit draft frontend-ts-01) after the Codex cross-check flagged exactly this.

## Evidence

Verified at 793f2b0b (2026-09-23).

- `shatter-core/src/triage.rs:316` `pub fn evaluate_constraint(expr, params, param_names) -> Option<Value>`; returns `None` for `Unknown` and unsupported ops.
- `shatter-core/src/triage.rs:442-455`: `Div`/`Mod` through `as_f64` and `eval_arith` with `f64` closures.
- `shatter-core/src/triage.rs:369`: `ConstValue::Null | ConstValue::Undefined => Value::Null`.
- `evaluate_constraint` has no caller outside `triage.rs`; the orchestrator uses it only through `TriageState` (`orchestrator.rs:1653`).
- Audit sources: finding frontend-ts-01 (`audits/2026-09-22/areas/frontend-ts.md` F1); Codex cross-check finding 2 (`audits/2026-09-22/issues/crosscheck/shatter-frontend-ts.codex.md`).

## Acceptance criteria

- [ ] An evaluation mode that takes the frontend language (or the per-op semantics it implies) and returns **indeterminate** (`None`) for any sub-expression whose semantics it does not model exactly for that language. At minimum: integer `Div`/`Mod` for Go and Rust (truncating integer semantics, or indeterminate), JS `null` vs `undefined` (distinct, or indeterminate), and any op the evaluator does not implement.
- [ ] Before a recorded `branch_path` constraint reaches the solver, the core evaluates it on the concrete inputs. If the result is determinate and contradicts `taken`, the constraint is replaced by `unknown` for solving and triage, and a counter (for example `inconsistent_constraints`) is incremented in the run stats or artifact. Indeterminate results change nothing.
- [ ] Unit tests (proptest where it fits the invariant "determinate result implies agreement with a reference evaluation"):
  - a deliberately inconsistent TS constraint is downgraded and counted;
  - Go `a / 2 == 1` at `a = 3` with `taken: true` is **not** downgraded;
  - a JS `x === null` constraint at `x = undefined` is not downgraded as inconsistent;
  - an `Unknown` or unsupported-op constraint is left untouched.
- [ ] The counter is visible somewhere a user or gate can read it (run summary, stats JSON or artifact), and the E2E suites for all three frontends report `0` on their fixture corpora once ts-flow-map-program-point has landed. If a non-zero count shows up on Go or Rust fixtures, file each as a frontend bug and cite it in the close note.
- [ ] Close-time proof: `task --force e2e-ts`, `task --force e2e-go` and `task --force e2e-rust` (the governed tasks, not bare `cargo test`), pasting each `test result:` line with a non-zero passed count. Record `task affected` `Gates selected`.

## Suggested approach

Add a language parameter (or a small semantics struct) to the evaluator rather than a second evaluator. Hook the check where the orchestrator takes in execute responses, before constraints are stored for solving and before triage sees them. Keep the random explorer path in mind: if it stores constraints too, apply the same check there (root CLAUDE.md "parallel parity").

## Out of scope

- Fixing the TS flow map (ts-flow-map-program-point).
- Making the evaluator fully precise for every language; indeterminate is an acceptable answer.

## Related

- ts-flow-map-program-point (this bucket): the frontend bug this guards against.
- concolic-early-termination (shatter-concolic-and-engine-design bucket): the counter helps attribute early stops.

## Priority / type / size

P2 · feature · size M

---

---
slug: ts-packaging-hygiene
kind: new
title: "shatter-ts packaging: dual npm/pnpm lockfiles, tests emitted to dist/, standalone bundle cannot find its worker"
priority: P3
type: chore
labels: [typescript, packaging, cleanup, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# shatter-ts packaging: dual npm/pnpm lockfiles, tests emitted to dist/, standalone bundle cannot find its worker

Split from the earlier combined ts-lifecycle-and-packaging-hygiene draft (audit finding frontend-ts-16). Each item below is a separate commit; they share one issue because all three are build-output hygiene in `shatter-ts` with the same validation (build, bundle, walkthrough).

## Problem

- Both `shatter-ts/package-lock.json` and `shatter-ts/pnpm-lock.yaml` exist. The Taskfile installs with npm, so the pnpm lockfile drifts silently.
- `tsc` emits every `*.test.ts` into `dist/`, so test code ships in the build output.
- The `bundle` script emits `dist/bundle.js` and `dist/worker-bundle.js`, but the instrumentation worker is resolved as `worker.js` next to the bundle. A standalone bundle (without the `tsc` output beside it) fails on the first `instrument`.

## Evidence

Verified at 56c86168 and re-checked unchanged at 793f2b0b (2026-09-23).

- `shatter-ts/package-lock.json` and `shatter-ts/pnpm-lock.yaml` both present; `pnpm-lock.yaml` was last touched incidentally in aca09d8b.
- `shatter-ts/tsconfig.json:16-17`: `"include": ["src"]`, `"exclude": ["node_modules", "dist", "src/__fixtures__"]`. All 21 `src/*.test.ts` compile into `dist/`.
- `shatter-ts/package.json:11` `bundle`: `--outfile=dist/bundle.js` and `--outfile=dist/worker-bundle.js`.
- `shatter-ts/src/instrumentation-worker.ts:49`: `workerPath ?? path.join(__dirname, "worker.js")`.
- *Verifier correction:* `node dist/bundle.js` works in a normal `dist/`, because `tsc` also emits `dist/worker.js`, which the bundle falls back to. Only a standalone bundle fails, with `Cannot find module .../worker.js`. The CLI's embedded path already renames the file (`shatter-cli/build.rs:93` reads `worker-bundle.js`; `shatter-cli/src/embedded_frontend.rs:42` extracts it as `worker.js`), so installed CLIs are not affected.
- Audit sources: finding frontend-ts-16; `audits/2026-09-22/areas/frontend-ts.md` F16.

## Acceptance criteria

- [ ] `pnpm-lock.yaml` is deleted; npm is the one package manager. `shatter-ts/CLAUDE.md` (or README) says so, and `npm ci` in a clean checkout succeeds.
- [ ] A `tsconfig.build.json` used by the `build` script excludes `**/*.test.ts`. After `task ts:build`, `find shatter-ts/dist -name '*.test.js'` prints nothing (paste it), and ts-jest still type-checks the tests (`npx jest` passes).
- [ ] The bundle and worker names agree: the bundle emits `worker.js`, or the worker path is passed explicitly. A test or scripted check copies **only** the bundle output into an empty temp directory, starts it, and completes an `instrument` request; it fails on current `main`. If the file name changes, `shatter-cli/build.rs` and `embedded_frontend.rs` are updated in the same change.
- [ ] Because the embedded frontend is touched, run `task walkthrough` and paste its pass line. Record `task affected` `Gates selected`.

## Out of scope

- Lifecycle fixes (ts-lifecycle-and-packaging-hygiene).
- The js-yaml upgrade (ts-js-yaml-v4).
- Changing how the CLI embeds the frontend beyond the worker file name.
- Pointing `package.json` `main`/`bin` at the bundle (a publishing decision; file separately if wanted).

## Priority / type / size

P3 · chore · size S

---

---
slug: ts-js-yaml-v4
kind: new
title: "shatter-ts: upgrade js-yaml 3.x to 4.x and drop the hand-written js-yaml.d.ts"
priority: P3
type: chore
labels: [typescript, dependencies, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# shatter-ts: upgrade js-yaml 3.x to 4.x and drop the hand-written js-yaml.d.ts

Split from the earlier combined ts-lifecycle-and-packaging-hygiene draft (audit finding frontend-ts-16), because a dependency major upgrade has its own risk and validation.

## Problem

`shatter-ts` depends on `js-yaml` `^3.14.2` and carries a hand-written type declaration for it. js-yaml 4 changed the API: `safeLoad`/`safeDump` were removed and `load` became safe by default. Staying on 3.x keeps a hand-maintained `.d.ts` that can drift from the library and leaves the frontend on an old major line. It also means the one call site uses 3.x `load`, which by default accepts the `!!js/function` / `!!js/regexp` / `!!js/undefined` types (it builds JS functions from YAML text); the file it parses (`.shatter/config.yaml`) comes from whatever repository Shatter is pointed at.

## Evidence

Verified at 793f2b0b (2026-09-23).

- `shatter-ts/package.json` `dependencies`: `"js-yaml": "^3.14.2"`.
- `shatter-ts/src/js-yaml.d.ts`: hand-written declaration of `load` only ("no `@types/js-yaml` is installed").
- The only call site: `shatter-ts/src/opaque-stub-registry.ts:28` `import { load as loadYaml } from "js-yaml"`, used to read `<projectRoot>/.shatter/config.yaml` (`:264`).
- Audit sources: finding frontend-ts-16; `audits/2026-09-22/areas/frontend-ts.md` F16.

## Acceptance criteria

- [ ] `js-yaml` is `^4` with `@types/js-yaml` as a devDependency; `shatter-ts/src/js-yaml.d.ts` is deleted.
- [ ] The call site in `opaque-stub-registry.ts` (and any other found by `grep -rn js-yaml shatter-ts/src`, listed in the close note) is migrated. 4.x `load` is not given a schema that re-enables the `js` types.
- [ ] A test loads a representative `.shatter/config.yaml` with `ts_runtime_values` entries and asserts the parsed registry is unchanged from 3.x. A second test asserts that a config containing `!!js/function` is rejected with a clear error instead of being turned into a JS function object; it fails on current `main`.
- [ ] `npx jest` in `shatter-ts` passes; `task walkthrough` pass line pasted (the embedded bundle changes); `task affected` `Gates selected` recorded.

## Out of scope

- Other dependency upgrades.
- Packaging (ts-packaging-hygiene) and lifecycle (ts-lifecycle-and-packaging-hygiene).

## Priority / type / size

P3 · chore · size S
