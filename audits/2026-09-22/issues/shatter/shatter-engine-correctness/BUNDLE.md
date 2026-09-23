# Bundle: shatter-engine-correctness (repo: shatter)

Audit 2026-09-22, final issue drafts for this bucket, revised after the Codex cross-check (see REVISION.md). Nothing in this bundle has been filed.

- **Bucket:** shatter-engine-correctness. Theme: wrong answers from the core engine (solver sort split, float constants, path counting, setup and mocks under concolic, shrinkers, the refine phase, invariants, solver timeouts, scan worker pool).
- **Repo / tracker:** shatter, `bd` in /home/ketan/project/shatter (prefix `str`).
- **Parent epic:** "Epic: Audit 2026-09-22 findings".
- **Line numbers** were re-verified on the audit branch, whose shatter-core/shatter-cli code is identical to `56c86168`. Runtime repro output is quoted from the audit's verified findings (findings.json) and was not re-run for this revision.
- **Contents:** 14 entries. There are 9 new issues (P1 x2, P2 x6, P3 x1), 2 reopen-notes (comments on closed str-0s76.6 and str-3ky9.4) and 3 notes on open issues (str-t854z, str-aureo, str-qwua7.49).
  - The earlier drafts `z3-mixed-int-real-sort-split` and `float-constant-rational-conversion` duplicated open str-t854z and str-aureo. They are now the notes `t854z-sort-split-note` and `aureo-float-constant-note`. The str-aureo note proposes raising it from P2 to P1.
  - `concolic-refine-execute-builder` was split into itself (request-field fix), `concolic-refine-path-accounting` and `execute-request-builder`.
- **Dependencies inside the bucket:** each reopen-note is posted after the new issue it points to is filed. concolic-refine-path-accounting is blocked by concolic-refine-execute-builder. execute-request-builder is blocked by both of those, and also by open str-qwua7.5 (add that edge when filing).

## Maintainer decisions (2026-09-23), which override the report and older drafts

- **D1 Releases:** keep x86_64-pc-windows-msvc and aarch64-unknown-linux-gnu in the release matrix and fix them: the Z3 header/static link on Windows, and openssl-sys under cross for aarch64. Release work closes only with a green release-run URL.
- **D2 shatter diff:** retire the snapshot `shatter diff` command and the unused Snapshot writer. spec-diff is the regression tool. Update SPEC, README and QUICKSTART. The `diff` name becomes free, and str-81xiw decides whether to use it. Correct the shatter-agents `shatter diff --staged` docs.
- **D3 Concolic positioning:** measure first. P1: a controlled default-vs-concolic benchmark. P1: fix concolic early termination. A follow-up decision issue, blocked by both, re-decides the "concolic-first" wording. No doc softening now.
- **D4 Beads hook stall:** retire the JSONL import in shatter and move to a Dolt remote. The first step checks whether the stale-JSONL import has been clobbering newer DB state. AGENTS.md drops `bd sync`. str-qwua7.28 is superseded. Add matching bento guidance. No BEADS_HOOK_TIMEOUT fix and no hook-bypass guidance.
- **D5 Git identity:** the leaked `[user]` section is already removed. Remaining work: a `.mailmap`, a git-state drift check (folded into str-qwua7.1 if it fits), and a snapshot of `.git/config` before and after test fixtures.
- **D6 Filing:** after reconciliation and the Codex cross-check, the maintainer runs one filer script. No agent files anything.

None of D1-D6 changes this bucket directly. D3 is adjacent: str-t854z (the Int/Real sort split, formerly draft z3-mixed-int-real-sort-split) is a likely contributor to concolic-early-termination (bucket shatter-concolic-and-engine-design).

<!-- file: 01-t854z-sort-split-note.md -->

---
slug: t854z-sort-split-note
kind: note-to-existing
title: "Note on str-t854z: Go Classify E2E repro, u8 range-bound twin, and executable E2E verification requirements"
priority: P1
type: bug
labels: [solver, z3, concolic, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-t854z
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-t854z: Go Classify E2E repro, u8 range-bound twin, and executable E2E verification requirements

**Target:** str-t854z (open, P1, "Numeric variables split across solver sorts"). Add a comment. No new issue.

This replaces the earlier audit draft `z3-mixed-int-real-sort-split`, which duplicated str-t854z (checked with `bd show str-t854z` on 2026-09-23). str-t854z already covers the root cause (separate `ints`/`reals` maps in `VarTable`, the hint override in `to_z3_expr`, the Real value overwriting the Int one in `extract_concrete_values`), two direct repros, a mixed-literal property, and a TS concolic fixture. The comment below adds only what str-t854z lacks.

## Comment text

> **Audit 2026-09-22 (finding core-01): additional evidence and verification requirements.**
>
> Code is unchanged since this issue was filed. At audit branch `audit-2026-09-22` (code identical to `56c86168`): `VarTable` is at `shatter-core/src/solver.rs:74-121`, the hint override is at `:500-503`, and `extract_concrete_values` is at `:1023-1066`.
>
> **1. A third symptom: integer range bounds are placed on the Int twin only.** `assert_int_param_ranges` (`solver.rs:172-180`) calls `vars.get_or_create_int(&p.name)`. When the same param also appears in a Real comparison, the Real twin has no range bound. Direct call: `n: u8` with `[n < 1000.5, n > 300.5]`, negating index 1, returned `Sat({"n": Float(0.0)})`. Acceptance addition: `assert_int_param_ranges` constrains the one variable chosen for the param, whatever its sort, and a solver unit test covers this repro.
>
> **2. A `solve_for_new_path` repro in addition to the `solve_constraints` ones.** x: Float, constraints `[x > 0.5 (Float const), NOT(x < 1 (Int const))]`, negating index 1, returned `Ok(Sat({"x": Float(1.5)}))` in 3 of 3 runs. The requested range was 0.5 < x < 1. The issue's parity clause already requires the fix to cover `solve_for_new_path`; this gives it a concrete test.
>
> **3. End-to-end impact on the Go frontend.** The Go frontend emits `{op: gt, const float 0.5}` next to `{op: lt, const int 1}` for one float param. For `func Classify(x float64) string` with nested `x > 0.5` / `x < 1`, `shatter explore mix.go --concolic --max-iterations 40 --clean` reported 23 iterations, 2 paths, stop reason `worklist_exhausted`, and 4/5 lines. `return "low"` was never reached, and no tried input was in (0.5, 1). The default random engine also runs the Z3Solver strategy, and it missed the branch at 100 iterations too.
>
> **4. Acceptance addition: a Go known-answer E2E, verified so that it cannot pass by being skipped.**
> - Add a `Classify`-shaped fixture (nested `x > 0.5` / `x < 1` on a `float64` param, three distinct returns) to `shatter-core/tests/e2e_concolic_go.rs`, next to the TS fixture the issue already requires in `shatter-core/tests/e2e_concolic.rs`. Assert that all three returns are reached under the concolic orchestrator.
> - Fixture location: these suites read fixtures from the external examples repo (`github.com/shatterproof-ai/examples`, found through `SHATTER_EXAMPLES_DIR` or `<tmp>/shatter-examples-main/standalone/{go,ts}` via `scripts/examples_checkout.py`). Use a self-contained fixture instead: an inline source string written to a tempdir, as `e2e_concolic.rs` already does with `TS_CLOSURE_FIXTURE` (`:277-279`). No coordinated examples-repo change is needed then. If the fixture goes in the examples repo instead, land and pin that change first and name the examples commit in the close note.
> - These subprocess tests are `#[ignore]`d (`e2e_concolic.rs` has 26, `e2e_concolic_go.rs` has 23). Plain `cargo test --test e2e_concolic_go` does not run them. Verify with `task e2e-go` and `task e2e-ts` (these run `cargo test --test ... -- --include-ignored`). The close note must quote the lines of test output that show the new test names reported as `ok`, both from a run on current `main` (expected FAIL) and from a run after the fix. A run that does not list the new test names does not count.
>
> **5. Test coverage gap.** The solver proptests (str-r6fr) never generate one param under mixed sorts, and the E2E known-answer fixtures use only int or string params. The mixed-literal property this issue already asks for closes this gap. Its assertion must check that every SAT model satisfies all asserted constraints, not only that the output has the right type.
>
> Related: str-aureo (float-constant encoding; same function `to_z3_expr`, coordinate if both are in flight), str-6ayh, str-r6fr. The audit's concolic early-termination diagnosis (bucket shatter-concolic-and-engine-design) may attribute lost branches to this issue.


---

<!-- file: 02-float-probe-paths-uncounted.md -->

---
slug: float-probe-paths-uncounted
kind: new
title: "Random explorer float probe marks paths seen without counting them; explore report says '0 path(s)' while progress line and spec show 2-4"
priority: P1
type: bug
labels: [explorer, report, coverage, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Random explorer float probe marks paths seen without counting them; explore report says '0 path(s)' while progress line and spec show 2-4

## Problem

In default (random/hybrid) explore mode, the path count and path rows in the explore report are wrong. The float-probe pre-pass writes its path hashes straight into `seen_paths` without counting them as new paths. The main loop then treats those paths as already seen. As a result the markdown report shows fewer paths than the explorer found, sometimes "0 path(s)" at 100% coverage. The stderr progress line and `--spec` from the same run show the right number.

The behavior-map cache written from the same result can also have `behaviors: []`, so `revalidate` inherits the error. Users are told a function has 0 or 1 behaviors when it has 3.

With `--concolic`, the report agrees with the progress line.

## Evidence

Code (line numbers re-checked on the audit branch, whose code is identical to `56c86168`):

- `shatter-core/src/explorer.rs:1212-1319` is the float probe. At `:1279-1283` it computes `path_hash` for the float and floor executions and calls `obs_state.seen_paths.insert(...)` directly. At `:1297-1301` it calls only `aggregator.push_raw_result(...)`, so probe paths never enter `unique_paths` / `new_path_executions`.
- The probe runs whenever `n_float * PROBE_COUNT * 2 < max_iterations` (`:1214-1216`).
- `shatter-cli/src/render.rs:100-137` renders `**{} path(s)**` from `result.unique_paths` (:106, :109) and rows from `new_path_executions` (:113), not from the accumulated path set.
- `shatter-core/tests/e2e_float_probe.rs` asserts only the probe classification, never user-visible path counts.

Observed during the audit (`--clean`, and `--no-cache` where noted, fresh directory):

- Go `Classify(x float64)`, `shatter explore mix.go --clean`: the progress line said `[batch 1/2] Classify: 100 iters, 2 paths, 2/2 branches`, but the report said `**0 path(s)** · **80%**` with no rows. Artifact `00003_Classify.json` had `unique_paths=0`, `new_path_executions=[]`, `raw_results=45`, and float_probe classification `integer_treating`. The concolic engine reports 2 paths for the same function.
- TS `explore 01-arithmetic.ts:compareMagnitudes --clean --no-cache`: stderr `100 iters, 4 paths, 3/3 branches`, markdown `**2 path(s)**`. The sum-large and both-small rows were missing, and `--spec-out` from the same run had 4 classes (goals-03).
- TS `safeDivide`: stderr `3 paths, 3/3 branches`, report `**0 path(s)** · 88%`, or `**1 path(s)**` in another run. The artifact's `raw_results` held 3 distinct branch paths (10/51/49 executions) with `unique_paths` 0 or 1 (goals-03, cli-ux-15). `classifyHttpResponse`: report 9, stderr 11.
- A trivial TS `g(x){ if (x>1) return 1; return 0 }`: batch line `2 paths, 1/1 branches`, report `**0 path(s)** · **100%**` and `Summary: 0 path(s)`, while `--spec` shows `Behavioral classes: 2` (prior-19).
- TS `fmt2` (n>10 'big', n<0 'neg', else throw), three runs of `explore --clean --max-iterations 30`: each printed `fmt2: 30 iters, 3 paths, 2/2 branches`. The reports showed 1 path (throw row only), 1 path ('big' only) and 0 paths at 100% coverage, and the spec in the same stdout said `Behavioral classes: 3` (artifacts-03). The output varies from run to run.
- Two same-named TS functions in `a.ts`/`b.ts`: stderr said 2 paths each, the markdown said 0 for both, and `.shatter-cache/behavior-maps/classify.json` had `behaviors: []` (goals-03).
- Rust `safe_divide`: stderr 2, report 1 (cli-ux-15).

## Acceptance criteria

- [ ] Float-probe executions go through the aggregator's normal observe/new-path accounting. No code path inserts into `seen_paths` without also recording the path as discovered.
- [ ] A regression test asserts rendered report path count == progress-line path count == `--spec` class count, in both random and concolic modes, on these known-answer fixtures:
  - From the external examples repo (`github.com/shatterproof-ai/examples`, resolved through `SHATTER_EXAMPLES_DIR` or `<tmp>/shatter-examples-main/standalone/` via `scripts/examples_checkout.py`): `standalone/ts/01-arithmetic.ts` (`classifyNumber`, `compareMagnitudes`), `standalone/ts/04-errors.ts` (`safeDivide`), and `standalone/rust/04_errors.rs` (`safe_divide`).
  - Self-contained, written as inline source strings to a tempdir by the test (no examples-repo change): a trivial 2-branch TS function (`g(x){ if (x>1) return 1; return 0 }`), the fall-through-throw TS `fmt2` shape (`n>10` returns 'big', `n<0` returns 'neg', otherwise throws), and a Go `Classify(x float64)` with nested `x > 0.5` / `x < 1`.
  - At close, quote the test output showing each fixture failing on current `main` and passing after the fix. If the test is `#[ignore]`d because it spawns frontends, run it with `-- --include-ignored` (or through the `task e2e-*` target) and quote the lines listing the test names as run; a run that skips it does not count.
- [ ] The behavior-map cache for these fixtures has one behavior per discovered path. A test asserts it, including the two-same-named-functions case (two self-contained TS files `a.ts` and `b.ts` that each define `classify`).
- [ ] `e2e_float_probe.rs` also asserts `unique_paths` / `new_path_executions` for a float-param fixture in random mode.
- [ ] The concolic path counts on the same fixtures do not change.
- [ ] `task affected` passes with its `Gates selected` output recorded, and `task e2e` passes (explorer change).

## Suggested approach

In the probe, replace the direct `seen_paths` inserts plus `push_raw_result` with the aggregator's normal observe call, so probe results count as new paths when they are new. Separately, consider rendering report rows from the merged accumulator, or from spec classes, so the report cannot drift from the engine's own counts. Add the three-way invariant test to the CLI integration tests.

## Out of scope

- Unifying path identity between the two engines (engine-path-identity-budget-config, bucket shatter-concolic-and-engine-design).
- The HTML "Paths Found" label that shows branches_covered (artifacts-13, tracked elsewhere).
- Resume keying (explore-resume-options-key).

## Priority

P1: the main user-facing report contradicts the engine's own output and hides discovered behaviors.

## Type

bug

## Dependencies

- Blocked by: none.
- Related: str-1hnm (closed; introduced the float probe), str-9q1z (closed; a different branch_count/unique_paths conflation), str-4o07, str-qwua7.57.

## References

Audit 2026-09-22 findings core-02, cli-ux-15, artifacts-03, goals-03 and prior-19, all one root cause (report §14 item 5: "file once"). Source drafts: `drafts/shatter-code/12-random-explorer-path-undercount.md` (kept) and `drafts/shatter-docs-ui/02-explore-report-underreports-paths.md` (merged into this issue and not filed separately).


---

<!-- file: 03-concolic-setup-teardown.md -->

---
slug: concolic-setup-teardown
kind: new
title: "Configured setup files are silently ignored under --concolic (every production caller passes setup_context=None), and the random explorer shrinks after teardown"
priority: P1
type: bug
labels: [concolic, setup, orchestrator, parity, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Configured setup files are silently ignored under --concolic (every production caller passes setup_context=None), and the random explorer shrinks after teardown

## Problem

Ownership of setup and teardown is inconsistent between the two engines.

Setup files are configured in `.shatter/config.yaml` (`defaults.setup`, `defaults.setup_level`, `defaults.setup_timeout`, and per-function `functions.<target>.setup`; see `docs/resource-parameters.md:60-75` and `shatter-core/src/config.rs:889-903`). There is no `--setup` CLI flag. The CLI only has `--setup-timeout` and `--fail-on-setup-error` (`shatter-cli/src/args.rs:699`, `:703`).

1. **Concolic ignores configured setup.** `orchestrator::explore` and `orchestrator::explore_with_oracle` accept a `setup_context`, but every production caller passes `None`. So `shatter explore <file> --concolic`, with a setup file configured, runs without setup and prints no warning. str-0s76.6 ("setup in concolic") was closed with "All callers updated". Its E2E injects a context directly into `orchestrator::explore`, so it passes even though the real pipeline never builds one. This issue replaces that closed issue; see the reopen-note setup-parity-reopen-note.
2. **The random explorer shrinks after per-function teardown.** Per-function teardown runs before the witness-shrinking phase, and the shrink Execute calls pass `setup_context: None`. Witnesses found under setup state are replayed without that state. The shrinker then either spends its budget on rejections or accepts a witness that depends on state that no longer exists. The orchestrator's copy of the shrink uses `setup_context.clone()`.
3. **Execution-level setup is not applied to shrink attempts in either engine.** With `setup_level: execution`, the random explorer's main loop runs setup and teardown around each execution (`shatter-core/src/explorer.rs:1629-1638` for the teardown). The shrink Execute calls are not bracketed that way, so each shrink attempt runs without the per-execution state.

## Reproduction

In a fresh directory with a TS target `f.ts` whose branch depends on state created by a setup file (for example, the setup writes a file and the target branches on `existsSync` of it):

```yaml
# .shatter/config.yaml
defaults:
  setup: "./setup/make-fixture.ts"
  setup_level: function
```

`shatter explore f.ts --clean` reaches the setup-dependent branch. `shatter explore f.ts --concolic --clean` does not, and prints no warning that setup was skipped.

## Evidence

Line numbers were re-checked on the audit branch, whose code is identical to `56c86168`:

- `shatter-core/src/orchestrator.rs:2453` `explore(...)` and `:2490` `explore_with_oracle(...)` take `setup_context: Option<SetupContextStack>` as the 7th argument.
- The production callers all pass `None` in that position:
  - `shatter-core/src/pipeline_orchestrator.rs:536-548` (`explore_with_oracle`, 7th arg `None` at :543)
  - `shatter-core/src/scan_orchestrator.rs:3103-3115` (`explore`, `None` at :3110)
  - `shatter-cli/src/commands/observe.rs:179-190` (`explore`, `None` at :186)
- `grep -c teardown shatter-core/src/orchestrator.rs` returns 0. `send_setup` is called only from `explorer.rs` and `observe.rs`.
- `shatter-cli/src/commands/explore.rs:5051` copies the resolved config value `resolved.setup` into `setup_file` (and `:5052` `setup_level`) of the random explorer's `ExploreConfig` only. No warning for `--concolic` was found.
- `shatter-core/tests/e2e_concolic.rs:1555` `orchestrator_explore_with_setup_context` calls `orchestrator::explore` directly with a hand-built context.
- `shatter-core/src/explorer.rs:1076-1077` derive `per_function_setup` and `per_execution_setup` from `config.setup_level` (`SetupLevel` has `Session`, `File`, `Function`, `Execution`; `shatter-core/src/protocol.rs:31-36`).
- `explorer.rs:1676-1683` runs per-function `send_teardown` before the `-- Witness shrinking phase --` at `:1692+`. The shrink Execute calls pass `setup_context: None` at `:1778`, `:1823` and `:1875`. The orchestrator's shrink passes `setup_context: setup_context.clone()` (`orchestrator.rs:3642`, `:3686`, `:3740`). Both copies came from 4d8001bc9 (str-28ea.6).
- Scan and observe set `setup_file: None` everywhere, so neither engine supports setup there. Whether that is intended is undocumented.

## Required lifecycle

The fix must implement this contract in both engines. Session- and file-level setup are owned outside the per-function explore call and are unchanged here.

| `setup_level` | Main exploration loop | Shrink phase | Teardown |
|---|---|---|---|
| `function` | one Setup before the first execution; every Execute carries its context | runs before function teardown; every shrink Execute carries the same live context | once, after the shrink phase |
| `execution` | Setup before and Teardown after each Execute | each shrink attempt is bracketed by its own Setup and Teardown, as in the main loop | after each attempt |

## Acceptance criteria

- [ ] One pipeline-level helper implements the lifecycle table above, and both engines receive their setup context from it. `run_pipeline`, `explore_with_scan_mode` and `observe` all use it (or reject setup explicitly, see below).
- [ ] A CLI-level E2E, not one that calls `orchestrator::explore` directly, uses a `.shatter/config.yaml` like the Reproduction above and shows that the setup side effect is visible to the target under `shatter explore <file> --concolic`, for `setup_level: function` and for `setup_level: execution`. TS is the minimum. Add Go and Rust if their frontends declare setup support in `protocol/parity-matrix.yaml`. At close, quote the test output from current `main` (fails) and after the fix (passes). If the test is `#[ignore]`d, run it with `-- --include-ignored` or its `task e2e-*` target, and quote the lines showing it ran.
- [ ] Shrink tests in both engines, with setup and shrinking enabled, one per level:
  - `function`: every shrink Execute request carries the live context, no Teardown request is sent before the last shrink Execute, and the shrunk witness still reproduces its path.
  - `execution`: every shrink Execute is preceded by a Setup and followed by a Teardown, and the shrunk witness still reproduces its path.
  - Assert on the sequence of requests sent (for example through a recording frontend double), so the test cannot pass under a level that never runs setup.
- [ ] If scan and observe intentionally do not support setup files, a configured setup under those commands produces an explicit error or warning, and a CLI test covers that. Otherwise they are wired through the same helper.
- [ ] `task affected` (with `Gates selected` recorded) and `task e2e` pass.

## Suggested approach

Move setup and teardown out of `explorer.rs` into the pipeline layer, so both engines receive a live context and teardown happens after the shrink phase in both. For execution-level setup, pass the engines a callback (or a small trait) that brackets one Execute with Setup/Teardown, and use it in both the main loop and the shrink loop. This also prepares str-qwua7.6.1's shared `select_witnesses`/shrink extraction, which currently leaves shrink behavior unchanged. The Execute-request builder (execute-request-builder) should take `setup_context` from this helper.

## Out of scope

- Adding setup support to frontends that lack it.
- Session- and file-level setup semantics.
- The shared Execute builder itself (execute-request-builder).

## Priority

P1: configured setup is silently dropped in the default-recommended engine, and a closed issue claims otherwise.

## Type

bug

## Dependencies

- Blocked by: none.
- Related: str-0s76.6 (closed but not fixed; gets setup-parity-reopen-note), str-0s76.12, str-qwua7.6.1, execute-request-builder.

## References

Audit 2026-09-22 findings core-03 (P1) and core-09 (P2, folded in). Report §11.3 lists str-0s76.6 as closed-but-unfixed, and §15.1 files this as new rather than reopening. Source draft: `drafts/shatter-code/13-concolic-setup-teardown-ownership.md`. Revised after the Codex cross-check: the draft previously cited a nonexistent `--setup` flag.


---

<!-- file: 04-setup-parity-reopen-note.md -->

---
slug: setup-parity-reopen-note
kind: reopen-note
title: "Note on closed str-0s76.6: configured setup files are still ignored under --concolic"
priority: P1
type: bug
labels: [concolic, setup, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [concolic-setup-teardown]
existing_id: str-0s76.6
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on closed str-0s76.6: configured setup files are still ignored under --concolic

**Target:** str-0s76.6 (closed). Add a comment. Do not reopen. The work is tracked in the new issue concolic-setup-teardown, so substitute its filed id for the slug when posting.

**Blocked-by note:** this comment points at the new issue, so post it after that issue is filed.

## Comment text

> **Audit 2026-09-22 (finding core-03): closed but not fixed.**
>
> This issue was closed with "All callers updated", but every production caller of `orchestrator::explore` / `explore_with_oracle` still passes `setup_context = None`:
>
> - `shatter-core/src/pipeline_orchestrator.rs:536-548` (7th arg `None`)
> - `shatter-core/src/scan_orchestrator.rs:3103-3115`
> - `shatter-cli/src/commands/observe.rs:179-190`
>
> Setup files come from `.shatter/config.yaml` (`defaults.setup` / `setup_level`, or per-function `setup`); there is no `--setup` flag. `shatter-cli/src/commands/explore.rs:5051-5052` copies the resolved `setup`/`setup_level` into the random explorer's config only, and `orchestrator.rs` contains no teardown call. So `shatter explore <file> --concolic`, with a setup file configured, runs without setup and does not warn. The closing E2E (`shatter-core/tests/e2e_concolic.rs:1555`, `orchestrator_explore_with_setup_context`) injects a context directly into `orchestrator::explore`, so it cannot detect that the pipeline never builds one.
>
> The fix (a pipeline-level setup/teardown helper for both engines, plus a CLI-level `--concolic` E2E driven by a configured setup file, at both `function` and `execution` setup levels) is tracked in **<id of concolic-setup-teardown>**. Line numbers verified on the audit branch (code identical to `56c86168`).


---

<!-- file: 05-concolic-mock-variation-regression.md -->

---
slug: concolic-mock-variation-regression
kind: new
title: "Concolic dynamic mock variation regressed (str-3ky9.4 undone by str-lebv/str-r59s); hidden behind _-prefixed params"
priority: P2
type: bug
labels: [concolic, mocking, regression, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Concolic dynamic mock variation regressed (str-3ky9.4 undone by str-lebv/str-r59s); hidden behind _-prefixed params

## Problem

str-3ky9.4 ("Orchestrator dynamic mock variation", closed 2026-03-10 in e0313728) added per-worklist-entry mock variation to the concolic orchestrator. Later MetaStrategy wiring made the mock parameters unused, and `_` prefixes silenced the compiler warning that would have flagged it. The concolic engine now explores with fixed mocks (`config.mocks`), while the random explorer still regenerates mocks on every iteration. Any branch that depends on a mocked dependency's return value can be unreachable under `--concolic`.

The generate-and-discard block also still consumes RNG draws, which shifts the rest of the seeded run for no benefit.

## Evidence

Line numbers were re-checked against `56c86168`:

- `shatter-core/src/orchestrator.rs:2640-2647`: `let _initial_mocks = if !config.mock_params.is_empty() { input_gen::generate_mock_values(...) } else { vec![] };` with the comment "Retained for future use; currently unused". `git log -S` attributes it to 9f2d2a3d (str-r59s, 2026-03-23).
- `shatter-core/src/orchestrator.rs:2147`: `solve_and_generate(.., _mock_params: &[MockParam], ..)` came from 0293c35c (str-lebv, 2026-03-14).
- Worklist entries built from strategies carry `mock_values: vec![]` (`orchestrator.rs:2223`, `:2259`, and also `:2894`, `:2915`), so they fall back to `config.mocks`.
- `input_gen::mutate_mock_values` (`input_gen.rs:4241`) has only test callers (`input_gen.rs:8081`, `:8533`).
- The random explorer regenerates mocks per iteration (`explorer.rs:1537`, `:2468`).
- There is no parity test for mock variation between the engines. The existing tests that look like one do not exercise the concolic engine: `concolic_mock_status_branches_discovered` (`shatter-core/tests/e2e_concolic.rs:1713`) and `concolic_mock_result_branches_discovered` (`:1809` area) use the fixture `standalone/ts/17-mock-branches.ts` from the external examples repo but call `shatter_core::explorer::explore_function`, the random explorer. Their doc comment says so: "Uses the random explorer (not orchestrator) because it regenerates mock values per iteration". The concolic name hides the regression.

## Acceptance criteria

Restoring per-entry mock variation in the concolic engine is the required outcome. Documenting fixed mocks and warning instead is not an acceptable resolution for this issue; if the maintainer decides against restoring it, close this issue as won't-fix and file that change separately.

- [ ] Worklist entries produced by the concolic MetaStrategy loop carry varied mock values (through `input_gen::mutate_mock_values`, or the random explorer's `generate_mock_values` path) whenever `config.mock_params` is non-empty. `mock_values: vec![]` is no longer used for strategy-produced entries at `orchestrator.rs:2223`, `:2259`, `:2894` and `:2915` when mock params exist.
- [ ] The dead `_initial_mocks` block (`orchestrator.rs:2640-2647`) is removed, and `_mock_params` in `solve_and_generate` (`:2147`) is used (the leading underscore is dropped) or removed from the signature.
- [ ] A new concolic E2E test in `shatter-core/tests/e2e_concolic.rs` runs `classifyStatus` from `standalone/ts/17-mock-branches.ts` through `orchestrator::explore` (not `explorer::explore_function`) and asserts that all four returns ("empty", "short", "medium", "long") are reached. At close, quote the test output from current `main` (fails) and after the fix (passes). The test is `#[ignore]`d like its neighbours, so run it through `task e2e-ts` (or `cargo test --test e2e_concolic -- --include-ignored <name>`) and quote the line showing it ran.
- [ ] The two existing random-explorer tests are renamed so their names do not say "concolic" (for example `random_mock_status_branches_discovered`).
- [ ] A seeded determinism test: two concolic runs with the same seed and non-empty `mock_params` produce the same sequence of mock values.
- [ ] `task affected` (with `Gates selected` recorded) and `task e2e` pass.

## Suggested approach

Restore the variation in the MetaStrategy loop, drawing from the orchestrator's seeded RNG. When a strategy produces a worklist entry, attach `mutate_mock_values(...)` output, as the random explorer does with `generate_mock_values`. Reuse the random explorer's generation helper rather than adding a third one.

## Out of scope

- Redesigning mock configuration or the mock-substitution frontends.
- The shared Execute builder (concolic-refine-execute-builder), though it should carry the per-entry mocks once they exist.

## Priority

P2: the verifier lowered core-04 from P1 to P2. This is a coverage-quality regression of a closed feature, not incorrect output.

## Type

bug

## Dependencies

- Blocked by: none.
- Related: str-3ky9.4 (closed but not fixed; gets mock-variation-reopen-note), str-lebv, str-r59s, str-3ky9.6.

## References

Audit 2026-09-22 finding core-04 (verified, corrected P1 to P2). Report §11.3 lists str-3ky9.4 as closed-but-unfixed. Source draft: `drafts/shatter-code/14-concolic-mock-variation-regression.md`.


---

<!-- file: 06-mock-variation-reopen-note.md -->

---
slug: mock-variation-reopen-note
kind: reopen-note
title: "Note on closed str-3ky9.4: concolic mock variation was undone by str-lebv/str-r59s"
priority: P2
type: bug
labels: [concolic, mocking, regression, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [concolic-mock-variation-regression]
existing_id: str-3ky9.4
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on closed str-3ky9.4: concolic mock variation was undone by str-lebv/str-r59s

**Target:** str-3ky9.4 (closed). Add a comment. Do not reopen. Substitute the filed id of concolic-mock-variation-regression for the slug when posting, after that issue is filed.

## Comment text

> **Audit 2026-09-22 (finding core-04): regressed after close.**
>
> The per-worklist-entry mock variation added here (e0313728) is no longer active in the concolic engine:
>
> - 0293c35c (str-lebv, MetaStrategy wiring, 2026-03-14) changed `solve_and_generate` to take `_mock_params: &[MockParam]`, which is unused (`shatter-core/src/orchestrator.rs:2147`).
> - 9f2d2a3d (str-r59s, 2026-03-23) added `_initial_mocks` ("Retained for future use; currently unused"), which is generated and discarded (`orchestrator.rs:2640-2647`) and still consumes RNG draws.
> - Strategy worklist entries carry `mock_values: vec![]` (`orchestrator.rs:2223`, `:2259`) and fall back to the fixed `config.mocks`.
> - `input_gen::mutate_mock_values` now has only test callers (`input_gen.rs:8081`, `:8533`).
>
> The random explorer still varies mocks on every iteration, so the two engines diverge. The E2E tests named `concolic_mock_*_branches_discovered` (`shatter-core/tests/e2e_concolic.rs:1713` onward) run the random explorer, so they did not catch this. Restoring the variation, with a concolic E2E on `17-mock-branches.ts` that runs through `orchestrator::explore`, is tracked in **<id of concolic-mock-variation-regression>**. Line numbers verified on the audit branch (code identical to `56c86168`).


---

<!-- file: 07-duplicate-value-shrinkers.md -->

---
slug: duplicate-value-shrinkers
kind: new
title: "Two value shrinkers: str-55ep/str-ddxe/tuple fixes landed in the dead input_gen copy; live shrink.rs removes required fields and proposes -1 for unsigned ints"
priority: P2
type: bug
labels: [shrinking, shatter-core, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Two value shrinkers: str-55ep/str-ddxe/tuple fixes landed in the dead input_gen copy; live shrink.rs removes required fields and proposes -1 for unsigned ints

## Problem

Both engines shrink through `crate::shrink::shrink_candidates`. Three shrinker fixes were applied to a different, unused function with the same name, `input_gen::shrink_candidates`. The live shrinker in `shrink.rs` therefore still:

- removes every object field, including required ones (str-55ep claimed to fix this);
- proposes `-1` for every integer, including unsigned and range-limited params (str-ddxe claimed range-aware ints);
- does not shrink positional objects (tuples); `input_gen` has `shrink_positional_object`.

The open issue str-v0yjq (enum_values shrink work) also targets the dead copy.

## Evidence

Line numbers were re-checked against `56c86168`:

- Production callers use the live shrinker: `shatter-core/src/orchestrator.rs:3716` and `shatter-core/src/explorer.rs:1850` call `crate::shrink::shrink_candidates`, and `explorer.rs:1802` calls `crate::shrink::grouped_shrink_candidates`.
- `shatter-core/src/input_gen.rs:3574` `pub fn shrink_candidates` (the body runs to about :3790) has no non-test caller. `input_gen.rs:3752` `shrink_positional_object` exists only there.
- `shatter-core/src/shrink.rs:512-540` `shrink_object` loops over all `fields` and removes each one ("Remove each field one at a time"), with no optional-only check.
- `shatter-core/src/shrink.rs:395-412` `shrink_int(value)` takes no type or range and always pushes `SHRINK_INT_NEG_ONE` (`-1`, defined at :199). `shrink_candidates` at `:363-380` dispatches `TypeInfo::Int { .. } => shrink_int(value)` and discards `int_width`/`int_signed`.
- f40facf1 (str-55ep, "shrink_object removes only optional fields") touched only `input_gen.rs`. 3d458b2a (str-ddxe, range-aware ints) edited `shrink.rs` but did not add range awareness there.
- Required-field removal is partly masked by the str-kn3f repair funnel in `shatter-core/src/planner_consumer.rs:312-330` (`repair_required_fields`). The audit found, but the verifier did not confirm, that the accepted witness (`current = bulk_trial`, `explorer.rs:1790`) may be stored unrepaired.

## Acceptance criteria

- [ ] `shrink.rs` keeps required object fields (removes only optional ones), respects `int_range()`/signedness (no negative candidates for unsigned params, and all candidates within range), and shrinks positional objects (tuples) while keeping their arity.
- [ ] `input_gen::shrink_candidates` and its private helpers are deleted, along with any tests that only exercise the dead copy. Tests worth keeping are ported to `shrink.rs`.
- [ ] Proptests in `shrink.rs`: for any value and `TypeInfo::Int` with a range, every candidate stays within `int_range`. For any object type, every candidate keeps all required fields. For tuples, arity is preserved. At close, show the new required-field and unsigned tests failing on current `main` and passing after the fix.
- [ ] Check whether the accepted shrink witness is stored with required-field repair applied, in both `explorer.rs` and `orchestrator.rs`. Fix it if not, and state the finding in the close note.
- [ ] A comment is added on open str-v0yjq, retargeting its enum_values shrink work at the live `shrink.rs` instead of `input_gen` (and the dead `recursive.rs`).
- [ ] `task affected` passes, with `Gates selected` recorded.

## Suggested approach

Port the three fixes (optional-only field removal, range-aware ints, positional objects) from `input_gen.rs` into `shrink.rs`, then delete the copy and add the proptests. Give `shrink_int` the `TypeInfo` (or the range) instead of the bare value.

## Out of scope

- The enum_values shrink work itself (str-v0yjq). This issue only retargets it.
- Extracting a shared shrink phase between the engines (str-qwua7.6.1).
- A crate-wide dead-code check (core-dead-code-removal, bucket shatter-concolic-and-engine-design).

## Priority

P2

## Type

bug

## Dependencies

- Blocked by: none.
- Related: str-v0yjq (open; retarget), str-55ep (closed but not fixed), str-ddxe (closed but not fixed), str-f4sow, str-kn3f, str-duens.

## References

Audit 2026-09-22 finding core-05 (verified, P2). Report §14 says to retarget str-v0yjq. Source draft: `drafts/shatter-code/15-duplicate-value-shrinkers.md`.


---

<!-- file: 08-concolic-refine-execute-builder.md -->

---
slug: concolic-refine-execute-builder
kind: new
title: "Concolic refine phase sends Execute with prepare_id: None and execution_profile: None, dropping TS execution adapters"
priority: P2
type: bug
labels: [concolic, orchestrator, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Concolic refine phase sends Execute with prepare_id: None and execution_profile: None, dropping TS execution adapters

## Problem

`refine_boundaries_async` in the concolic orchestrator builds its own `Command::Execute` requests with `prepare_id: None` and `execution_profile: None`. Every other Execute site in `orchestrator.rs` passes `prepare_id.clone()` and `config.execution_profile.clone()`. The refine phase therefore runs without the target's execution profile, which drops TS execution adapters, and it cannot reuse the prepared build.

This issue is only the request-field fix. The related work was split out after the cross-check:

- counting refine-phase executions as discovered paths: concolic-refine-path-accounting;
- one shared Execute-request builder for all sites: execute-request-builder;
- the `capture` flag at every site: open str-qwua7.5, which owns it.

## Evidence

Line numbers were re-checked on the audit branch, whose code is identical to `56c86168`:

- `shatter-core/src/orchestrator.rs:2321` `async fn refine_boundaries_async(...)` takes `setup_context` and `execute_plan` but no `prepare_id` or execution profile. Its Execute at `:2375-2384` sends `setup_context: setup_context.clone(), capture: false, prepare_id: None, execution_profile: None`. It is called from `:3527`.
- The other seven orchestrator Execute sites pass both fields: `:1695-1696`, `:2702-2703`, `:2715-2716`, `:3104-3105`, `:3644-3645`, `:3688-3689`, `:3742-3743`.
- Not verified: the audit guessed that a missing `prepare_id` forces a rebuild per Execute on Go and Rust. The close note should state what actually happens.

## Acceptance criteria

- [ ] `refine_boundaries_async` receives the run's `prepare_id` and `config.execution_profile`, and its Execute requests carry them. `capture` stays as it is; str-qwua7.5 owns that field.
- [ ] A unit test with a recording frontend double (a `Frontend` stand-in that records every `Command` it receives) runs a concolic exploration that enters the refine phase with a non-`None` `prepare_id` and `execution_profile`. It asserts that every Execute request sent during the refine phase carries both values. At close, quote the test failing on current `main` and passing after the fix.
- [ ] A TS execution-adapter target explored with `--concolic` reaches the refine phase without adapter errors. Name the target and quote the relevant log lines in the close note.
- [ ] `task affected` (with `Gates selected` recorded) and `task e2e` pass.

## Suggested approach

Thread `prepare_id: Option<&str>` and `execution_profile` through the `refine_boundaries_async` signature from its one caller at `:3527`, which already has both in scope.

## Out of scope

- Refine-phase path accounting (concolic-refine-path-accounting).
- The shared Execute builder (execute-request-builder).
- The `capture` flag (str-qwua7.5).

## Priority

P2

## Type

bug

## Dependencies

- Blocked by: none.
- Related: str-qwua7.5 (open; owns `capture` at every site), concolic-refine-path-accounting, execute-request-builder.

## References

Audit 2026-09-22 finding core-06 (verified, P2). Source draft: `drafts/shatter-code/16-concolic-refine-phase-execute-builder.md`. Split after the Codex cross-check, which found that the earlier draft bundled three deliverables and conflicted with str-qwua7.5's ownership.


---

<!-- file: 09-invariant-min-support.md -->

---
slug: invariant-min-support
kind: new
title: "Invariant inference over-generalizes: function-level invariants are published from a single execution (no minimum support)"
priority: P2
type: bug
labels: [spec, invariants, properties, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Invariant inference over-generalizes: function-level invariants are published from a single execution (no minimum support)

## Problem

Function-level invariant detection runs on any number of execution records, including one. `detect_invariants` keeps every template that holds on all specimens, so a single execution yields invariants like `x == <constant>` and `x > 0`, which are then published in the spec. Per-class inference already requires at least 2 records, but the function level has no floor at all.

Support is only partly visible to readers. The markdown spec prints `(satisfied/total)` next to each function invariant, so `(1/1)` shows up there. The YAML spec deliberately omits `satisfied_count`/`total_count`, and `confidence` is hard-coded to 1.0, so YAML consumers cannot tell a one-specimen invariant from a well-supported one.

Numeric comparison templates also compare only against 0, so real bounds (such as `0 <= x <= 100`) are never inferred. That is a separate enhancement and is out of scope here (see below).

## Evidence

Line numbers were re-checked on the audit branch, whose code is identical to `56c86168`:

- `shatter-core/src/spec.rs:394-400`: `detect_classified_invariants(&all_records, ...)` runs on all records with no minimum count. Per-class inference requires `class_records.len() >= 2` (`:411`).
- `shatter-core/src/invariants.rs:504` `detect_invariants`: keeps any template true on all specimens and returns early only on empty input.
- `shatter-core/src/invariants.rs:661`: `confidence: 1.0` is hard-coded, with `satisfied_count` and `total_count` both set to the specimen count. Removing or computing `confidence` is tracked in open str-qwua7.61.
- `shatter-core/src/spec.rs:568-575`: the markdown renderer prints `[confidence] (satisfied/total)`. The test at `spec.rs:2767-2775` asserts that YAML does **not** contain `satisfied_count` or `total_count`.
- `shatter-core/src/invariants.rs:262-335`: the three `NumericComparison` templates use `value: 0.0` (`:270`, `:291`, `:312`). The fourth entry (`:330-332`) is a `NumericConstant` whose `0.0` is a placeholder set during detection, not another comparison against 0.
- `invariants.rs` has 0 `proptest!` blocks. Open str-qwua7.47 (checked with `bd show` on 2026-09-23) owns the property "every returned invariant holds on the specimens it was inferred from".

## Acceptance criteria

- [ ] One minimum-support threshold applies before an invariant is reported, at both function and class level. The default is 5 specimens and it is configurable. The default and the config key are documented where spec options are documented. The existing per-class `>= 2` check uses the same constant or config value.
- [ ] A proptest: for any record set smaller than the threshold, `detect_classified_invariants` (or the spec-building function that calls it) emits no invariants; for record sets at or above it, the output equals what `detect_invariants` returns today for the same records. The "invariant holds on its source traces" property stays with str-qwua7.47 and is not duplicated here.
- [ ] A regression test: a function explored with a single execution produces no function-level invariants in either the markdown or the YAML spec. At close, quote it failing on current `main` and passing after the fix.
- [ ] Spec snapshots that change are regenerated and reviewed in the same change, and the close note summarizes the diff (which invariants disappeared and why).
- [ ] `task affected` passes, with `Gates selected` recorded.

## Suggested approach

Add the support threshold in `spec.rs`, next to the existing per-class `>= 2` check, so both call sites use one constant or config value. Coordinate with str-qwua7.61 on the confidence field: this issue covers the support threshold only.

## Out of scope

- Range or min/max templates. Observed extrema from sampled traces do not establish general bounds, so this needs its own design. File separately if wanted.
- Removing or computing `confidence` (str-qwua7.61).
- Proptest coverage of `invariants.rs`, including the source-trace property (str-qwua7.47).
- Showing support counts in YAML output.

## Priority

P2

## Type

bug

## Dependencies

- Blocked by: none.
- Related: str-qwua7.61 (confidence), str-qwua7.47 (owns the source-trace property).

## References

Audit 2026-09-22 finding core-11 (verified, P2). Source draft: `drafts/shatter-code/20-invariant-min-support-templates.md`.


---

<!-- file: 10-z3-default-query-timeout.md -->

---
slug: z3-default-query-timeout
kind: new
title: "Z3 has no default per-query timeout (solver can outlive timeout_explore), and scan --solver-timeout is silently discarded"
priority: P2
type: bug
labels: [solver, z3, timeout, cli, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Z3 has no default per-query timeout (solver can outlive timeout_explore), and scan --solver-timeout is silently discarded

## Problem

Without `--solver-timeout`, Z3 queries have no time limit. With solver offload, the Z3 call runs on a blocking thread, which the explore deadline checks cannot cancel. One hard query can therefore run past `timeout_explore` and the per-function budget. The only exception is `--mcdc`, which defaults to 10 s.

Separately, `shatter scan` accepts `--solver-timeout` and throws it away, so scan always runs with no solver limit.

## Evidence

Line numbers were re-checked on the audit branch, whose code is identical to `56c86168`:

- `shatter-cli/src/args.rs:658`, `:1073` and `:1420`: the `--solver-timeout` help reads "Z3 solver timeout in seconds per query. Default: no limit."
- `shatter-core/src/solver.rs:1214-1217`, `:1254-1257` and `:1343-1346`: all three solver entry points call `cfg.set_timeout_msec(ms)` only `if let Some(ms) = solver_timeout_ms`.
- `shatter-cli/src/helpers.rs:893-897`: the only default, `Some(10)` when `mcdc && solver_timeout.is_none()`.
- `shatter-cli/src/main.rs:593`: the `Scan` arm destructures `solver_timeout: _`, so the flag never reaches the scan solver config.
- How a timeout surfaces today: `SatResult::Unknown` becomes `Err(SolverError::Unknown(reason))` (`shatter-core/src/solver.rs:1413`). `SolveResult` has only `Sat` and `Unsat` (`solver.rs:49-54`). The concolic orchestrator matches `Ok(SolveResult::Unsat) | Err(_)` together (`shatter-core/src/orchestrator.rs:2110`) and counts both as an unsolvable constraint. The Z3Solver strategy (`shatter-core/src/strategy.rs:1243-1272`) drops every non-`Sat` result in a `_ =>` arm, whose comment says "Stall tracking is the orchestrator's responsibility". So a timeout is indistinguishable from UNSAT in both places.
- The 2026-09-04 audit's claim "Timeouts: cfg.set_timeout_msec on each query" holds only when a value is set.

## Acceptance criteria

- [ ] A default per-query Z3 timeout (e.g. 2 s, configurable through the flag and `.shatter/config.yaml`) applies in all modes and commands. It is set in one place, at solver-config construction. The `--mcdc` 10 s default is either kept as a documented override or folded into the same mechanism.
- [ ] The `--solver-timeout` help text and any docs state the new default.
- [ ] `Err(SolverError::Unknown(..))` is handled separately from `Ok(SolveResult::Unsat)` at `orchestrator.rs:2110` and in the Z3Solver strategy's result match (`strategy.rs:1268`). Each Unknown/timeout increments a `solver_unknown` (or similarly named) counter that appears in the explore artifact's stats, for both engines, and it does not increment the unsat/`param_fail_counts` accounting.
- [ ] `shatter scan --solver-timeout N` is honored. A CLI test asserts that the value reaches the solver config used by scan. At close, show the test failing on current `main` and passing after the fix.
- [ ] A test with a deliberately hard query (for example nonlinear integer arithmetic) completes within the default timeout plus a small margin, returns `SolverError::Unknown`, and increments the new counter rather than the unsat accounting. At close, quote it failing on current `main` (no counter; or no timeout) and passing after the fix.
- [ ] `task affected` (with `Gates selected` recorded) and `task e2e` pass. Run `task gauntlet` if help output snapshots change.

## Suggested approach

Resolve the effective timeout once, where the budgets are resolved (`resolve_mcdc_budgets` in `helpers.rs` or its successor), and pass `Some(default)` down instead of `None`. Wire `solver_timeout` through the `Scan` arm in `main.rs` the same way as for explore. Add the counter by splitting the `Ok(SolveResult::Unsat) | Err(_)` arm at `orchestrator.rs:2110` into an Unsat arm and an `Err(SolverError::Unknown(_))` arm, and do the same in the strategy's `_ =>` arm at `strategy.rs:1268` (return or record the distinction so the orchestrator can count it; the strategy comment already assigns stall tracking to the orchestrator).

## Out of scope

- Complexity-aware timeout allocation (str-w0d.3, deferred).
- Making blocking Z3 calls cancellable.

## Priority

P2

## Type

bug

## Dependencies

- Blocked by: none.
- Related: str-36cd (closed; added the optional flag with no default), str-w0d.3.

## References

Audit 2026-09-22 finding core-12 (verified, P2; the verifier found the discarded `scan --solver-timeout`). Source draft: `drafts/shatter-code/21-z3-default-timeout.md`.


---

<!-- file: 11-qwua7-49-rescope.md -->

---
slug: qwua7-49-rescope
kind: note-to-existing
title: "Note on str-qwua7.49: try_send/None premise refuted; real failure is an exhausted worker pool misreported as task timeouts"
priority: P2
type: bug
labels: [scan, worker-pool, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-qwua7.49
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-qwua7.49: try_send/None premise refuted; real failure is an exhausted worker pool misreported as task timeouts

**Target:** str-qwua7.49 (open). Add a comment and propose a retitle. No new issue.

## Comment text

> **Audit 2026-09-22 (finding core-10): the premise is refuted, so rescope.**
>
> **The original premise does not hold.** `scan_orchestrator.rs` contains no `try_send`. The pool-construction sites use `sender.send(fe).await.expect(...)` on a channel with capacity `max_workers` (`shatter-core/src/scan_orchestrator.rs:2237-2254`), so they cannot fail. `checkout()` (`:2268-2271`, `rx.recv().await.expect("pool should not be empty")`) cannot receive `None`, because `WorkerPool` owns its own sender, and shutdown drops it explicitly.
>
> **The real hazard.** `replace_dead_worker_if_needed` (`:2317-2327`) calls `reap_dead_slot()` when `Frontend::spawn` fails. After each task, the task loop calls `pool.maybe_grow(remaining)` (`:4743`). `maybe_grow` (`:2336-2363`) increments `live_count` before it starts a detached spawn, and decrements it again if that spawn fails. If spawns keep failing (for example a broken frontend install), `live_count` can reach 0 while queued tasks block in `pool.checkout().await` (`:4608`). Once no task is running, nothing calls `maybe_grow` again, and the blocked checkouts wait until `join_with_dynamic_watchdog` (`:5536`) fires. A spawn failure is then reported as a timeout rather than a spawn error. The verifier confirmed the mechanism. It did not confirm that each task is then reported as `phase_timeout('task')`; the first step of this work is to reproduce that and record the actual reported outcome.
>
> **Proposed rescope:**
> - Retitle: "Fail fast when the scan worker pool is exhausted".
> - Define exhaustion so a successful retry is never rejected: the pool is exhausted only when there are no live workers, **no spawn attempts in flight** (from `maybe_grow` or `replace_dead_worker_if_needed`), and tasks are still waiting. A plain `live_count == 0 && pending > 0` check is not enough: `maybe_grow` counts an in-flight spawn in `live_count` before it succeeds or fails, and a failed replacement can make `live_count` 0 for a moment while another spawn is still about to succeed. Track in-flight spawns explicitly, and keep the last spawn error.
> - When exhaustion is detected (at the point where the last in-flight spawn fails), wake every blocked `checkout()` (for example by closing the channel or signalling a `Notify`), and have `checkout()` return `Err(ScanError::WorkerPoolExhausted { last_spawn_error })` instead of panicking or waiting.
> - Regression tests, each failing on current `main` and passing after the fix, with output quoted in the close note:
>   1. Every spawn after the initial pool fails: the scan returns `WorkerPoolExhausted` containing the spawn error text, within a bound far below the watchdog timeout, and no task timeout is reported.
>   2. Transient failure: the first replacement spawn fails and the next succeeds. All tasks complete, and no `WorkerPoolExhausted` is returned.
>   3. Checkouts blocked at the moment of exhaustion are woken and receive the error (no task is left waiting).
> - Drop the `expect` → `?` conversion of the construction and checkout sites from scope, or keep it only as hygiene. Those sites are not reachable failure points.
> - The existing acceptance item "checkpoint and partial report still written, exit 2 at the CLI" still applies to the new error path.
>
> Line numbers verified on the audit branch (code identical to `56c86168`).


---

<!-- file: 12-aureo-float-constant-note.md -->

---
slug: aureo-float-constant-note
kind: note-to-existing
title: "Note on str-aureo: float constants above 2147.483647 wrap through a C int cast; propose raising to P1 and requiring exact-rational tests"
priority: P1
type: bug
labels: [solver, z3, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-aureo
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-aureo: float constants above 2147.483647 wrap through a C int cast; propose raising to P1 and requiring exact-rational tests

**Target:** str-aureo (open, P2, "Float constants lose precision before solving"). Add a comment and propose raising the priority to P1. No new issue.

This replaces the earlier audit draft `float-constant-rational-conversion`, which duplicated str-aureo (checked with `bd show str-aureo` on 2026-09-23). str-aureo already covers the `(*v * 1_000_000.0).round() as i64` encoding at `shatter-core/src/solver.rs:514-516`, the tiny-value collapse to 0 (its 1e-7 repro), exact binary-rational encoding, and extraction in scope. The comment adds a much larger affected range, found by the Codex cross-check, and makes the test requirements exact.

## Comment text

> **Audit 2026-09-22 (finding core-19, corrected by cross-check): the affected range is much larger than "precision beyond six decimals".**
>
> **1. Constants with magnitude above 2147.483647 are corrupted, not only rounded.** The pinned `z3` crate is 0.19.10 (`Cargo.lock`). Its `Real::from_rational(num: i64, den: i64)` passes `num as c_int` and `den as c_int` to `Z3_mk_real` (`z3-0.19.10/src/ast/real.rs:63-75`). The caller scales by 1e6 first, so the numerator overflows a 32-bit int once `|v| > 2147.483647`, and the `as` cast wraps silently. Example: `3000.0` becomes `3_000_000_000 as i32 = -1_294_967_296`, which Z3 reads as **-1294.967296**. A branch `if x > 3000.0` is therefore solved as `x > -1294.97`. Above about 9.2e12 the earlier `f64 as i64` cast also saturates, and `i64::MAX as i32` is `-1`, so every such constant becomes -0.000001. This was verified from source; the runtime was not re-run for this note.
>
> Thresholds such as 5000.0, 86400.0 or 1e6 are common in real code, so this is a wrong-answer bug in the core solver on ordinary inputs, of the same class as str-t854z (P1). **Proposed priority: P1.**
>
> **2. Test for exact rational equality, not f64 round-trip.** A shortest-round-trip decimal string does not meet this issue's contract. Binary f64 `0.1` is exactly `3602879701896397 / 36028797018963968`, while the decimal `"0.1"` is `1/10`. Both round-trip to the same f64, so a round-trip assertion would pass an inexact encoding. Required tests:
> - A unit test on the constant translation: for each value in a fixed corpus (`0.1`, `2147.483648`, `3000.0`, `-3000.0`, `1e13`, `1e-7`, `5e-324`, `f64::MAX`), the Z3 numeral equals the exact binary rational from `f64::integer_decode`, compared as big integers (numerator and denominator strings from Z3 against the expected ones). Build the numeral with `Real::from_rational_str` (present in z3 0.19.10, `real.rs:25`) or `Real::from_big_rational`, not `from_rational`.
> - A bounded proptest over finite f64 values with the same exact-equality assertion.
> - A regression test on current `main`: `x: Float`, constraint `x > 3000.0` (and `x < -3000.0`) must produce a model that satisfies the original f64 comparison. It must fail before the fix and pass after it, with both outputs quoted in the close note.
>
> **3. Model extraction must be fixed with the translation, or the new tests cannot pass.** Exact rationals for tiny or very large values have numerators and denominators beyond `i64`. `extract_concrete_values` (`solver.rs:1036-1045`) first calls `as_rational()`, which uses `Z3_get_numeral_small` and returns `None` outside `i64`. It then tries `val.to_string().parse::<f64>()`. Z3 prints such a numeral as an s-expression like `(/ 1.0 10000000.0)`, so the parse fails and the variable is left out of the model without any error. The fix must convert big rationals to the nearest f64 (for example through the numeral's decimal or rational string), or return `SolverError::Unsupported`. It must never drop the assignment. str-aureo already puts extraction in scope; this adds the concrete failure mode. Separate the tests: translation tests (point 2) check the Z3 numeral, and model round-trip tests check the extracted `ConcreteValue`.
>
> **4. NaN and infinities** go to `SolverError::Unsupported` explicitly, as the issue already requires. Add a test for each.
>
> Verify with `task affected` (record `Gates selected`) and `task e2e` (this is a solver change, and `task e2e` runs the `#[ignore]`d subprocess suites).
>
> Related: str-t854z (same function `to_z3_expr`; coordinate if both are in flight).


---

<!-- file: 13-concolic-refine-path-accounting.md -->

---
slug: concolic-refine-path-accounting
kind: new
title: "Concolic refine-phase executions are discarded after boundary-witness updates; paths reached only there never reach unique_paths, raw_results or the report"
priority: P2
type: bug
labels: [concolic, orchestrator, coverage, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [concolic-refine-execute-builder]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Concolic refine-phase executions are discarded after boundary-witness updates; paths reached only there never reach unique_paths, raw_results or the report

## Problem

`refine_boundaries_async` executes candidate inputs near each branch boundary, then uses each result only to update that branch's true/false witness. The execution results are never passed to observation or coverage accounting. A path or line reached only during refinement is missing from `covered_paths`, `unique_paths`, `raw_results`, the artifact and the report.

This changes a policy that open issue str-qwua7.5 currently assumes. str-qwua7.5's per-phase capture policy lists boundary refinement (its `:2314`, now `:2380`) as a **probe** site whose "outputs are compared for path/outcome equality and then discarded", so it keeps `capture: false`. If refine results become reported results, the refine site moves to the **reported** class, and under str-qwua7.5's rule it must honour the capture flag. This issue owns that policy change and must record it on str-qwua7.5.

## Evidence

Line numbers were re-checked on the audit branch, whose code is identical to `56c86168`:

- `shatter-core/src/orchestrator.rs:2386-2408`: after each refine Execute, the result is read only to find `took_side` for the target `branch_id`, which sets `tw` or `fw`. `exec_result` is then dropped. `BoundaryResult` (`:2411-2416`) carries only the witnesses and `executions_used`.
- The concolic `Classify` artifact from the audit (Go, nested x>0.5 / x<1) had `boundary_results` true_witness `[0.5000037571385455]` (20 executions) while `unique_paths=2`, and the report said 4/5 lines. The verifier noted that this witness is on branch 0 (x>0.5), and the artifact does not show whether it took the x<1 'low' side. So this run does not prove that a path was lost; the regression test below must construct a case that does.
- str-qwua7.5 (`bd show str-qwua7.5`, checked 2026-09-23) lists the refine site as a probe with `capture: false`.

## Acceptance criteria

- [ ] Refine-phase executions go through the same observation call the main loop uses, so a path first reached during refinement is counted once in `unique_paths`, appears in `raw_results` and `new_path_executions`, and is rendered in the report. Refine executions that reach an already-known path do not change the counts.
- [ ] A regression test constructs a target where one path is reachable only by an input the refine phase produces (for example a branch on `x == boundary + epsilon` that the solver and fuzz phases are configured not to reach, or a recording frontend double that returns a new branch path only for refine-phase inputs). It asserts that the path appears in the artifact and the rendered report. At close, quote the test failing on current `main` and passing after the fix.
- [ ] Capture policy for the refine site is settled with str-qwua7.5:
  - A comment is posted on str-qwua7.5 stating that the boundary-refinement site is now a reported site and follows the capture flag, with the id of this issue.
  - If str-qwua7.5 has landed first, the refine site honours `capture_side_effects` in this change, with a test. If it has not, str-qwua7.5's implementer applies the reported-site rule to the refine site, and this issue's close note says so.
- [ ] `task affected` (with `Gates selected` recorded) and `task e2e` pass.

## Suggested approach

Return the refine executions from `refine_boundaries_async` (or pass it the aggregator), and feed each through the observation path used by the main loop, marking the discovery method as boundary refinement.

## Out of scope

- The request fields `prepare_id`/`execution_profile` (concolic-refine-execute-builder, which this is blocked by because both edit the same function).
- The shared Execute builder (execute-request-builder).
- The random explorer's float-probe accounting (float-probe-paths-uncounted), the same class of bug in the other engine.

## Priority

P2

## Type

bug

## Dependencies

- Blocked by: concolic-refine-execute-builder (same function; land the small field fix first).
- Related: str-qwua7.5 (open; capture policy for the refine site changes here), float-probe-paths-uncounted, execute-request-builder.

## References

Audit 2026-09-22 finding core-06 (verified, P2), second half. Split from concolic-refine-execute-builder after the Codex cross-check.


---

<!-- file: 14-execute-request-builder.md -->

---
slug: execute-request-builder
kind: new
title: "Build every concolic and random-explorer Execute request through one helper so sites stop re-deciding prepare_id/execution_profile/setup_context/capture"
priority: P3
type: task
labels: [concolic, explorer, orchestrator, refactor, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [concolic-refine-execute-builder, concolic-refine-path-accounting]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Build every concolic and random-explorer Execute request through one helper so sites stop re-deciding prepare_id/execution_profile/setup_context/capture

## Problem

The two engines build `Command::Execute` inline at 14 sites (8 in `orchestrator.rs`, 6 in `explorer.rs`). Each site re-decides `prepare_id`, `execution_profile`, `setup_context` and `capture`. The audit found three bugs that come from sites drifting apart: the refine phase dropping `prepare_id`/`execution_profile` (concolic-refine-execute-builder), shrink sites passing `setup_context: None` (concolic-setup-teardown), and hard-coded `capture` values (str-qwua7.5). A single builder stops the next one.

This is a behaviour-preserving refactor. It lands after the field fixes, so it changes no request contents.

## Evidence

Line numbers were re-checked on the audit branch, whose code is identical to `56c86168`:

- `grep -c 'Command::Execute {'`: `shatter-core/src/orchestrator.rs` 8, `shatter-core/src/explorer.rs` 6. Other files also build Execute requests inline: `genetic_explorer.rs` 1, `observe.rs` 1, `revalidation.rs` 1, `recursive.rs` 4. `frontend.rs` (4) and `protocol.rs` (14) are the definitions and serde code, and `test_arbitraries.rs` (1) is a test generator.
- Orchestrator sites (by `capture:` literal): `:1694`, `:2380`, `:2701`, `:2714`, `:3103`, `:3643`, `:3687`, `:3741`. Explorer sites include the float-probe pair (`:1240-1270`) and the shrink sites (`:1778`, `:1823`, `:1875`).

## Scope

In scope: the Execute sites in `shatter-core/src/orchestrator.rs` and `shatter-core/src/explorer.rs`.

Out of scope, and not required to use the helper: `genetic_explorer.rs`, `observe.rs`, `revalidation.rs`, `recursive.rs`. The close note lists them as remaining inline sites. A follow-up may migrate them.

## Acceptance criteria

- [ ] One helper (for example `ExecuteRequestBuilder`, or a method on a small per-run context struct) builds the Execute request from the per-run values (`prepare_id`, `execution_profile`, `setup_context`, `plan`) plus per-call values (`function`, `inputs`, `mocks`, `capture`). `capture` is a parameter the caller passes, so str-qwua7.5's per-phase policy stays at the call sites.
- [ ] `grep -n 'Command::Execute {' shatter-core/src/orchestrator.rs shatter-core/src/explorer.rs` matches only inside the helper and inside `#[cfg(test)]` modules. The close note quotes the command and its output.
- [ ] No request contents change. Before the refactor, add a test with a recording frontend double that runs one random and one concolic exploration (with setup, a `prepare_id` and an `execution_profile` set) and snapshots the sequence of Execute requests. The same snapshot passes unchanged after the refactor. Quote both runs in the close note. If concolic-setup-teardown has not landed yet, the explorer shrink sites keep sending `setup_context: None` through an explicit, commented override on the helper, so this refactor does not quietly fix or change that behaviour.
- [ ] `task affected` (with `Gates selected` recorded) and `task e2e` pass.

## Suggested approach

Introduce the helper, then migrate sites one file at a time, running the snapshot test after each file.

## Out of scope

- Changing any field's value at any site. That belongs to concolic-refine-execute-builder, concolic-setup-teardown and str-qwua7.5.
- Session-lifecycle extraction (str-inct (b)) and shared-shrink extraction (str-qwua7.6 / str-qwua7.6.1).

## Priority

P3: a maintainability refactor. The user-visible bugs are fixed by the issues it depends on.

## Type

task

## Dependencies

- Blocked by: concolic-refine-execute-builder and concolic-refine-path-accounting (both edit the refine site). Also wait for open str-qwua7.5 to land, since it edits every orchestrator site's `capture` field; `blocked_by` only lists slugs in this bundle, so add str-qwua7.5 as a blocker when filing.
- Related: concolic-setup-teardown (its helper supplies `setup_context`), str-qwua7.6, str-qwua7.6.1, str-inct.

## References

Audit 2026-09-22 finding core-06, builder part, and core-16 (a duplicate of open str-qwua7.5, context only). Split from concolic-refine-execute-builder after the Codex cross-check and the same-runtime review, which found the earlier grep criterion could not pass because of Execute sites outside the two engine files.
