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
