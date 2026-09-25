# Area review: shatter-ts frontend (L1, L2, L4) — audit 2026-09-22

Reviewer scope: `shatter-ts/` (src, tests, build config, CLAUDE.md), the TS rows of
`protocol/parity-matrix.yaml`, the `ts-conventions` skill, and how the Rust core
consumes TS output (`coverage_metrics.rs`, `project.rs`). Observation only: no source
edits, no tracker changes.

Method:
- Read all of `shatter-ts/CLAUDE.md`, `package.json`, `tsconfig.json`, `jest.config.js`,
  `Taskfile.yml`, `main.ts`, the dispatch/validation parts of `handlers.ts`, the three
  SymExpr builders, the instrumentor's data-flow pass, the executor's loop-snapshot walker,
  and the outcome/timeout classification code.
- Copied `shatter-ts/` into a scratch dir and ran `npm ci`, then `npx tsc --noEmit`
  (exit 0, 15.6 s), `npm run bundle`, `npx tsc`. I didn't install anything in the audit
  worktree.
- Wrote small **probe programs** (bundled with esbuild) that call `instrumentFunction` /
  `analyzeFile` directly. I also drove the real frontend (`node dist/main.js`) over
  JSON-over-stdio with handshake/analyze/instrument/execute requests. Probe sources are
  quoted inline below.
- Ran typescript-eslint 8 (`recommendedTypeChecked`, plus the rules the ts-conventions skill
  says it enforces) in the scratch copy to put numbers on str-qwua7.31.
- Deduped against the 2026-09-04 audit (`frontends-protocol.md` §A2/A5/B) and against bd
  (`str-qwua7.*`, `str-rf2v`, `str-mhinv.2`, and a full-text scan of `.beads/issues.jsonl`
  for ternary/switch/flow/timeout terms).

**Context:** `git diff --stat <2026-09-04>..16794cef -- shatter-ts/src` touches only
`executor.test.ts` and `property.test.ts`. **No TS production code has changed since the
last audit**, so every TS production finding from 2026-09-04 still stands. The new findings
below are ones that audit missed.

---

## Positives worth preserving

- `strict: true` + `noUncheckedIndexedAccess: true` (tsconfig.json:9-10), and `tsc --noEmit`
  is clean.
- typescript-eslint `recommendedTypeChecked` finds **0 `no-explicit-any`** and 0
  `ban-ts-comment` in production code. Only 72 production findings in ~16.7k lines, most of
  them cosmetic (details in F12). Adopting ESLint would be cheap.
- Generated enums (`src/generated/protocol-enums.ts`) come from the registry with a
  DO-NOT-EDIT header, and the `ALL_ERROR_CODES` count is pinned by a test
  (property.test.ts:696).
- The adapter architecture (`runtime-hooks.ts`: `ResolverAdapter` / `SandboxProvider` /
  `InvocationHook`, all resolved from `ExecutionProfile.adapters` by id) is clean and easy to
  extend. The str-26fhi work that shares `buildInstrumentedSandbox` /
  `assembleCoverageFields` between the direct and adapter paths is a good example of
  "one runtime, two entry points".
- `fs-write-redirect.ts` is thorough (it also shims `fs.promises` and the destinations of
  `cp`/`link`/`symlink`), and it reads the env var per call so it is safe under concurrent
  dispatch.
- Error responses mostly carry good remediation text (`unsupported_missing_global: … -
  enable the 'browser-globals' adapter`).
- fast-check is used for real: 76 `fc.assert` calls in property.test.ts plus 6 other files.

---

## Findings

### F1 [P1, L1/L5] Flow-insensitive data-flow map records path constraints that contradict the execution

`instrumentFunction` builds **one** `dataFlowMap` for the whole function body before
instrumenting (instrumentor.ts:187). It then passes that single end-of-function map to every
branch (`wrapBranchCondition` → `buildSymExpr(condition, ctx.paramNames, ctx.dataFlowMap)`,
instrumentor.ts:1710). On top of that, a reassignment whose RHS resolves to `unknown` does
**not** kill the previous binding (`if (nextExpr.kind !== "unknown") flowMap.set(...)`,
instrumentor.ts:498-500). Loop bodies are visited once, with no widening or havoc
(instrumentor.ts:422-431).

Probe (`instrumentFunction` output, `__shatter_branch` 4th argument):

```ts
export function stale(a: number, b: number) { let x = a; if (x > 0) {...} x = b; if (x > 100) {...} }
//  branch 0 (x > 0)  → { bin_op gt, left: param b, right: 0 }     ← should be param a
export function staleUnknown(a: number) { let x = a; x = opaque(); if (x > 10) {...} }
//  branch 0 (x > 10) → { bin_op gt, left: param a, right: 10 }    ← should be unknown
export function loopCarried(a: number) { let acc = a; let i = 0; while (i < 3) { acc = acc + 1; i++; } if (acc > 10) ... }
//  while (i < 3)     → { lt, left: (0 + 1), right: 3 }            ← constant, wrong
//  acc > 10          → { gt, left: a + 1, right: 10 }             ← actually a + 3
```

End to end through the real frontend (`execute` of `stale` with `inputs: [5, 0]`):

```json
"return_value":"pos",
"branch_path":[{"branch_id":0,"line":3,"taken":true,
  "constraint":{"kind":"expr","expr":{"kind":"bin_op","op":"gt",
    "left":{"kind":"param","name":"b","path":[]},"right":{"kind":"const","type":"int","value":0}}}}]
```

The branch was **taken** under a constraint (`b > 0`) that is **false** for the concrete
inputs (b = 0). Z3 negates `b > 0`, varies `b`, and never flips the branch. Core triage
(`shatter-core/src/triage.rs:316 evaluate_constraint`) evaluates these constraints to predict
paths, so a wrong constraint can also cause executions to be wrongly skipped. I did not trace
that effect end to end.

No existing issue: a bd full-text scan for flow-insensitive/stale/reassign/dataFlowMap found
nothing. str-rf2v covers only builder triplication.

**Recommendation:**
1. Compute the flow map at each program point during the transformer walk: snapshot it at
   each branch; on assignment of an unresolvable RHS, set the name to `unknown` (kill it);
   at loop heads, havoc every variable assigned in the loop body.
2. Add an oracle test: for a fixture corpus, execute with random inputs, evaluate every
   recorded `branch_path[i].constraint` on the concrete inputs, and assert it equals `taken`.
3. Add the same consistency check in the core (triage's evaluator already exists): an
   inconsistent constraint is downgraded to `unknown` and counted in telemetry, so every
   frontend is protected.

**Agent root cause:** the only documented contract (CLAUDE.md:10-19, root CLAUDE.md
"Parallel parity") is about which **AST node types** the builders handle, not whether the
emitted constraint is semantically correct. The property tests compare builders in isolation
against a fixed `resolveName`, and no test checks "constraint(concrete inputs) == taken", so
the bug stays invisible to every gate.

### F2 [P1, L1/L5] switch, ternary and standalone `&&`/`||` are analyzed but never instrumented, and branch IDs desync

`analyzer.ts:1364-1471` reports `switch`, `ternary` and `logical_and`/`logical_or` branches.
The instrumentor only wraps `if`/loop conditions: `switch` gets line records only
(instrumentor.ts:1393-1411), and there is no `ConditionalExpression` handler. The two passes
number branches independently, so the IDs diverge.

Probe:

```
analyze  mixed: [[0,16,"ternary","a > 0"],[1,17,"if","b > 10"]]
instrument mixed: __shatter_branch(0, 17, !!(b > 10), …)     ← runtime id 0 = the `if`
analyze  sw:   [[0,3,"switch","a === 1"],[1,4,"switch","a === 42"]]   instrument sw: branchCount = 0
analyze  tern: [[0,9,"ternary","a > 5"]]                               instrument tern: branchCount = 0
analyze  logic (const ok = a > 0 && b > 0): [[0,2,"logical_and",…]]    instrument: 0 branch calls
```

The core joins the two by id: `coverage_metrics.rs:477-521 extract_targets_inner` marks
`analysis.branches[i]` covered when `discoveries` contains `branch.id`. For `mixed`, a run
that takes the `if` marks the **ternary** (line 16) covered and reports the `if` (line 17,
hint `b > 10`) as "Uncovered". The coverage targets and hints users see are therefore
attributed to the wrong lines. Switch-only and ternary-only functions give the concolic
engine no constraints at all.

CLAUDE.md:286-287 mentions this only in passing ("TS `switch` records case *lines* but emits
no `branch_path` decisions"), and the e2e test works around it by reading `raw_results`.
PARITY.md:50 claims "Branch detection Y … switch/match". Closed issues str-w0d.1 ("emit
symbolic constraints … for each branch condition (if, switch, ternary, &&, ||)") and
str-sunh ("ternary … already standard AST nodes the instrumentor handles") claim coverage
that doesn't exist.

**Recommendation:** have the analyzer and instrumentor share one branch enumerator (one
traversal that assigns IDs), or have instrument return its id→line map and have the core join
on (line, type). Instrument switch cases (one decision per case, with an `eq(discriminant,
case)` constraint), ternaries, and value-position logical expressions. Add a test asserting
`analyze.branches[*].(id,line)` == instrumented `(id,line)` over a fixture corpus.

**Agent root cause:** issues were closed on unit-level evidence, with no known-answer e2e per
branch type. The limitation was written into CLAUDE.md prose as an aside instead of being
filed. There is also no analyze/instrument ID-alignment invariant anywhere.

### F3 [P2, L2/L4] TS analyze-side conditions ignore data flow; the parity matrix claims otherwise

The analyzer's builder (`analyzer.ts:2280`, "local copy for analyzer independence") knows only
`paramNames`. Probe `derived` (`const y = a * 2; if (y > 10)`):

- analyze → `{gt, left: unknown, right: 10}`
- instrument → `{gt, left: a*2, right: 10}`

`protocol/parity-matrix.yaml:1096-1107` (`ite-symexpr-production-partial`) says TS produces
ite "via data flow analysis in visitStatementsForDataFlow" and lists only Rust as affected for
`analyze`. In fact TS produces ite only on the execute path. Go's analyze threads a flow map
(same entry), so on the same source TS and Go `analyze` differ, and this divergence isn't
recorded.

**Recommendation:** either feed the (fixed, per-point) flow map into the analyzer builder, or
add TS to the divergence's `affected_frontends` for `analyze` and correct the text. Fold this
into str-rf2v.

**Agent root cause:** the parity matrix is prose that no gate verifies against the code.

### F4 [P2, L1] Shadowed parameter names resolve to the outer parameter

In `shadow(xs: number[], x: number)`, `xs.filter((x) => { if (x > 5) … })` produces the
analyze condition `{param x} > 5`: the callback's own `x` is mistaken for the outer parameter.
Every builder resolves identifiers by name (`paramNames.has(expr.text)`) with no scope check.
Only analyze was verified; the instrumentor does not branch-instrument callbacks, so there was
nothing to observe there.

**Recommendation:** in the analyzer, resolve through the TypeChecker (`getSymbolAtLocation`
compared with the parameter declaration's symbol). In the instrumentor, track a scope stack of
bound names.

### F5 [P2, L1/L6] Target errors that mention "timeout" are reported as `timed_out` / `infrastructure`

`classifyError` (executor.ts:281-286) returns `infrastructure` when `/timed?\s*out/i` matches
the message. `isTimeoutError` (executor.ts:3293-3298) then returns true because
`TIMEOUT_PATTERNS` includes `"timeout"`. Probe: a target that throws
`new RangeError("timeout must be positive")` for `ms <= 0` returns

```json
"outcome": {"status":"timed_out","short_reason":"timeout must be positive", ...}   error_category: "infrastructure"
```

This is ordinary input validation by the target, but it is reported as a harness timeout, so
reports and gates treat it as an infrastructure fault. No existing issue.

**Recommendation:** only harness-owned timeouts should produce `timed_out`: that means
`ERR_SCRIPT_EXECUTION_TIMEOUT`, plus a dedicated `ShatterAsyncTimeout` error class for the
async race at executor.ts:1276. Keep `classifyConnectionFailure` for mocked network
failures only.

### F6 [P2, L1] The async-timeout race leaks a timer on every async execution, and `forceExit` hides it

At executor.ts:1274-1281, `Promise.race([p, new Promise((_, reject) => setTimeout(reject,
timeoutMs))])` never clears the timer, so every async execute leaves a 15 s pending timer.
`jest.config.js:6` (`forceExit: true`, "avoid hanging on worker threads") hides leaked handles
like this one from the test suite.

**Recommendation:** use `clearTimeout` in `finally` (or `timer.unref()`). Remove `forceExit`
and run once with `--detectOpenHandles` to find the real leaks.

### F7 [P2, L1] Requests are validated only at the envelope level

`parseRequest` (handlers.ts:1113-1159) checks `id`, `protocol_version` and `command`, then
returns `parsed as Request`. Probes:

- `{"command":"execute"}` (no fields) → `internal_error "Unhandled error: Cannot read properties of undefined (reading 'includes')"`
- `{"command":"analyze"}` → `file_not_found "File not found: undefined"`

This contradicts ts-conventions ("Validate incoming data and parse into typed discriminated
unions"; "Define types explicitly; do not infer trusted shapes from external data").
`validCommands` (handlers.ts:1151) is also a hand-maintained list that duplicates the
generated `ALL_COMMANDS`.

**Recommendation:** add per-command field validators that return `invalid_request` naming the
bad field; ideally generate them from `registry.yaml field_model` (see str-qwua7 item 3 of
the prior audit). Derive the dispatch set from `ALL_COMMANDS`.

### F8 [P2, L2] CLAUDE.md says probes of planner commands return "capability not supported"; they actually return `invalid_request`

shatter-ts/CLAUDE.md:155-157: "conformance tests (`task conformance`) expect TS to return a
clean 'capability not supported' response … when these are probed." Actual:

```json
{"id":2,"status":"error","code":"invalid_request","message":"Unknown command: get_invocation_plan"}
```

The only `get_invocation_plan` conformance case (`conformance_cases.yaml:538-567`) is
`frontends: [go]`, so nothing checks the claim.

**Recommendation:** return `not_supported` for commands in `ALL_COMMANDS` that are not in
`SUPPORTED_CAPABILITIES`, keep `invalid_request` for truly unknown commands, and add a TS/Rust
conformance case for it.

**Agent root cause:** the CLAUDE.md claim was written as an expectation and never tied to a
test.

### F9 [P2, L1/L4] A fourth copy of the SSA flow walker lives in executor.ts and has already diverged

`executor.ts:1497-1810` (`visitStatementsForLoopSnapshots`,
`visitVariableDeclarationListForLoopSnapshots`, `buildLoopSnapshotMutatedExpr`,
`mergeLoopSnapshotFlowMaps`) is a near-copy of instrumentor.ts:361-632. `mergeFlowMaps` vs
`mergeLoopSnapshotFlowMaps` are line-for-line equivalent. The copy already lacks destructuring
(`registerDestructuredBindings`), `while`/`do`/`for-in`/`for-of` traversal, and closure
poisoning. Neither CLAUDE.md's parity table nor str-rf2v mentions it (str-rf2v counts three
builders).

**Recommendation:** extract a `flow-analysis.ts` module with one walker parameterized by
hooks (onBranch, onLoopIteration) and use it from both the instrumentor and the executor.
Doing F1 in that module fixes both.

### F10 [P2, L1] The protocol "round-trip" tests are tautological and never touch the real wire path or the shared schemas

`property.test.ts:717-760` asserts `JSON.parse(JSON.stringify(x))` equals `x` for objects
built by TS arbitraries. That holds for any plain object. The tests don't use
`serializeReplacer` (the real `sendResponse` path) or `parseRequest`. No TS test reads
`protocol/schemas/**` or `protocol/fixtures/**`: grep over `*.test.ts` finds only the
`SHATTER_EXAMPLES_DIR` fallback. Yet `Taskfile.yml` (`test`/`test-fast` sources) lists
`../protocol/schemas/**/*.json`, `../protocol/fixtures/**/*.json` and
`../shatter-core/src/protocol.rs` as test inputs. Those are dead cache keys that suggest
schema coverage that doesn't exist.

**Recommendation:** validate arbitrary responses (serialized with `serializeReplacer`) against
`protocol/schemas` with ajv, and feed `protocol/fixtures` requests through `parseRequest` +
`handleRequest`. Either make the Taskfile sources real or remove them.

**Agent root cause:** the root CLAUDE.md rule "Frontend protocol handlers have round-trip
tests (serialize → deserialize → verify)" is satisfied in letter only; no reviewer checks
what the round-trip actually exercises.

### F11 [P2, L1] The builder-parity property test cannot detect the drift it exists for

`property.test.ts:1187-1296`:

- The generator emits only node kinds both builders already handle.
- It compares only unknown vs non-unknown, never the output.
- `arbBinOp` omits `in`/`instanceof`, which both token maps handle.
- The analyzer's builder (analyzer.ts:2280, not exported) and the executor walker (F9) are
  not covered.

A new node kind added to one builder passes this test. (Prior audit A5 noted the
collapse-semantics gap; still true.)

**Recommendation:** use a fixed corpus that includes currently unsupported nodes (see F14),
assert **output equality** modulo documented collapse rules across all builders, and export
the analyzer builder for tests (or delete it per str-rf2v).

### F12 [P2, L1] Still no ESLint (confirms str-qwua7.31), now with numbers

typescript-eslint 8 `recommendedTypeChecked` plus the skill's rules, run on production files
(tests excluded): **72 findings**:

| Rule | Count |
|---|---|
| no-unnecessary-type-assertion | 23 |
| no-unused-vars | 14 |
| no-unsafe-call | 7 |
| no-unsafe-return | 6 |
| unbound-method | 5 |
| consistent-type-imports | 5 |
| no-unsafe-member-access | 4 |
| no-unsafe-assignment | 3 |
| no-base-to-string | 2 |
| no-restricted-exports (default export) | 1 |

Examples:
- Dead `SUBPROCESS_SYMBOLS` (executor.ts:132, unused since str-3ky9.12, 2026-03-11).
- Unused `checker` (analyzer.ts:1890) and `sourceFile` (instrumentor.ts:298).
- `logger.ts:22` default export, which violates the skill's "no default exports".
- `String(thrownError)` at executor.ts:2808 can yield `[object Object]` in connection-failure
  messages.

Tests: 77 more (19 no-unsafe-assignment, 6 no-require-imports, 3 no-implied-eval).

**Recommendation:** as in str-qwua7.31. Start with `recommendedTypeChecked` plus the claimed
rules, fix the 72, and wire `ts:lint` into `ts:test-fast` and `task check`.

### F13 [P2, L4/L5] The preflight check fails any project with no `node_modules`, and one failure poisons the whole session

`runPreflight` (handlers.ts:203-218) fails whenever `<project_root>/node_modules` is missing.
The CLI derives `project_root` from the nearest `package.json` / `tsconfig.json` / `go.mod` /
`Cargo.toml` (shatter-core/src/project.rs:10-15). So a dependency-free TS package
(tsconfig-only, or a `package.json` without deps) fails, and so does a `.ts` file inside a Go
or Rust repo. Probe: analyze with `project_root` = a directory with no node_modules →
`preflight_failed: missing_node_modules: …/node_modules`, even for a file with no imports.
`preflightFailure` is a single module-level value (handlers.ts:190), so one bad root fails
every later request in the process, including requests for other roots.

I didn't run this end to end through the `shatter` CLI (no CLI build in this pass), so
confidence is medium.

**Recommendation:** require `node_modules` only when `package.json` declares
dependencies/devDependencies, or fail lazily on the first `MODULE_NOT_FOUND`. Key
`preflightFailure` by root.

### F14 [P3, L1] Common operators and nodes collapse to `unknown`

`??`, `**`, `<<`/`>>`/`>>>`, `ElementAccessExpression` (`xs[i]`), `ConditionalExpression`,
template literals, `as`/`!`/`satisfies`, BigInt literals and postfix unary all produce
`unknown`. Probe `nullish` (`const v = a ?? 7; if (v > 3)`) gives `unknown > 3`; `2 ** a! > 8`
gives `unknown > 8`; `xs[i] === 7` gives `unknown === 7`. The first two were listed in the
2026-09-04 audit (A5, P3) but not filed as an issue (bd full-text: no ElementAccess/nullish
issue).

**Recommendation:** unwrap `as`/`!`/`satisfies`/`<T>x` first (cheap), then add `??` as an ite
over `eq(x, null)`, and `ElementAccess` with a literal key as a `param.path` segment.

### F15 [P2, L2] CLAUDE.md is still stale (prior claims confirmed) plus new errors

Still true from the prior audit (tracked as str-qwua7.24/.25/.34):
- builder line numbers `~278-352` / `~860-951` (actual 874 / 1862), and the table omits the
  analyzer builder and the executor walker
- "TS is the only frontend that produces `ite`" (:43-44; Go produces it, matrix :1100)
- "TS declares support for `outcome` only" (:149)
- dangling divergence IDs `loop-body-states-typescript-only` (:52) and
  `error-code-preflight-failed-typescript-only` (:208); 0 hits in the matrix

New:
- `src/browser-globals-recognizer.js` / `src/handlers.js` (:380-381): the sources are `.ts`
- "str-jeen.40 will refine bucketing once a dedicated status / code is introduced" (:375-376):
  str-jeen.40 closed 2026-07-06, and the same file (:196-203) says it landed
- "Key Files" (:5-8) lists 2 of ~30 modules, and none of the modules the contracts depend on
- the claims in F2 (switch), F3 (ite on analyze) and F8 (not_supported)

**Recommendation:** str-qwua7.24/.25. In addition, add a docs check that every
`src/<name>.(js|ts)` path and `str-*` id cited in crate CLAUDE.md resolves.

### F16 [P3, L4] Build and package hygiene

- Two lockfiles. The Taskfile installs with npm (`package-lock.json`), while `pnpm-lock.yaml`
  was last touched incidentally in aca09d8b (2026-08-25).
- `npm run bundle` output doesn't run on its own. `bundle.js` loads
  `path.join(__dirname, "worker.js")` (instrumentation-worker.ts:50), but the script emits
  `worker-bundle.js`. Probe: `node dist/bundle.js` → `Cannot find module …/dist/worker.js` on
  the first instrument. Only `shatter-cli/build.rs:93` + `embedded_frontend.rs:42` rename it.
- `tsc` emits all 21 test files into `dist/` (tsconfig excludes only `__fixtures__`).
- js-yaml ^3 with a hand-written `js-yaml.d.ts`.
- `package.json main/bin` point at the unbundled `dist/main.js`, which is not what ships.

**Recommendation:** delete `pnpm-lock.yaml`, have the bundle emit `worker.js` (or pass the
worker path through an env var), add a `tsconfig.build.json` that excludes `*.test.ts`, and
move to js-yaml 4 + @types.

### F17 [P3, L1] Stdin EOF and shutdown drop in-flight responses

`main.ts:74-77` calls `process.exit(0)` on stdin `close`, even while requests are still in
flight.

- Probe: piping `handshake, shutdown` and then EOF produces **no `shutdown_ack`**.
- `shutdown` terminates the worker under an in-flight `instrument` → `internal_error
  "Unhandled error: Worker terminated"`.
- `main.ts:23` hard-codes "protocol 0.1.0" instead of `PROTOCOL_VERSION`.

Impact is low today because the core waits for each response.

**Recommendation:** track pending promises, and on close/shutdown await them (with a bound)
before exiting.

### F18 [P2, AGENT] Branch-type claims closed without per-type e2e known-answer tests

Covered in F2: str-w0d.1 and str-sunh were closed while claiming switch/ternary/logical
constraint emission. The root CLAUDE.md models integration tests on known-answer functions,
but the TS e2e suite (`e2e_concolic.rs`) has no per-`branch_type` known-answer fixture that
asserts both arms are reached **through `branch_path`**. The enum e2e reads `raw_results` to
get around exactly this gap (CLAUDE.md:285-287).

**Recommendation:**
1. Add a TS known-answer fixture per `BranchType` (if, else_if, switch, ternary, logical_and,
   logical_or, while, for, do) that asserts analyze/instrument ID alignment and that both
   outcomes are discovered through `branch_path`.
2. Add a drift-patrol/parity check that every `BranchType` the analyzer emits is also emitted
   by the instrumentor, per frontend.
3. Add "workarounds in tests must be filed as issues" to the completion checklist.

---

## Prior-audit TS items re-checked

| Prior item | Status 2026-09-22 |
|---|---|
| No ESLint (str-qwua7.31) | still true; quantified in F12 |
| Three builders; collapse semantics; property-chain base via flow (A5, str-rf2v) | still true (code unchanged); plus a 4th walker (F9) |
| CLAUDE.md stale lines / ite / outcome-only / dangling IDs (str-qwua7.24/.25/.34) | still true (F15) |
| TS lacks `plan`/`get_invocation_plan` types (str-mhinv.2) | still true; plus the wrong error code (F8) |
| `ErrorCategory` "unknown" not in registry (prior B P3) | still true (protocol.ts:631-635 vs registry.yaml:69-71) |
| React shim always-on default resolver, not profile-gated (goals: adapters B-) | still true (`getDefaultResolverAdapters`, executor.ts:830-860) |

## Grades

- L1 code quality: **C+**. The type discipline is excellent. But F1 and F2 are correctness
  bugs in the core product path (symbolic constraints and branch identity), and several
  smaller defects (F5, F6, F7) exist too.
- L2 docs vs code: **C**. CLAUDE.md is rich but carries at least 9 false or stale claims;
  the parity matrix overclaims ite on analyze.
- L4 design: **B-**. The adapter/hook design is good. SymExpr and flow analysis are copied
  in four places with no shared semantics or spec, and analyze and instrument number
  branches independently.
