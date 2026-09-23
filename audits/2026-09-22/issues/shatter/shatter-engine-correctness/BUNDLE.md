# Bundle: shatter-engine-correctness (repo: shatter)

Audit 2026-09-22, final issue drafts for this bucket. Nothing in this bundle has been filed.

- **Bucket:** shatter-engine-correctness. Theme: wrong answers from the core engine (solver sort split, path counting, setup and mocks under concolic, shrinkers, the refine phase, invariants, solver timeouts).
- **Repo / tracker:** shatter, `bd` in /home/ketan/project/shatter (prefix `str`).
- **Parent epic:** "Epic: Audit 2026-09-22 findings".
- **Line numbers** were re-verified against the audit branch at `56c86168`. Runtime repro output is quoted from the audit's verified findings (findings.json) and was not re-run for this draft.
- **Contents:** 12 entries. There are 9 new issues (P1 x3, P2 x5, P3 x1), 2 reopen-notes (comments on closed str-0s76.6 and str-3ky9.4) and 1 note on open str-qwua7.49.
- **Dependencies inside the bucket:** each reopen-note is posted after the new issue it points to is filed. There are no other blocked-by edges.

## Maintainer decisions (2026-09-23), which override the report and older drafts

- **D1 Releases:** keep x86_64-pc-windows-msvc and aarch64-unknown-linux-gnu in the release matrix and fix them: the Z3 header/static link on Windows, and openssl-sys under cross for aarch64. Release work closes only with a green release-run URL.
- **D2 shatter diff:** retire the snapshot `shatter diff` command and the unused Snapshot writer. spec-diff is the regression tool. Update SPEC, README and QUICKSTART. The `diff` name becomes free, and str-81xiw decides whether to use it. Correct the shatter-agents `shatter diff --staged` docs.
- **D3 Concolic positioning:** measure first. P1: a controlled default-vs-concolic benchmark. P1: fix concolic early termination. A follow-up decision issue, blocked by both, re-decides the "concolic-first" wording. No doc softening now.
- **D4 Beads hook stall:** retire the JSONL import in shatter and move to a Dolt remote. The first step checks whether the stale-JSONL import has been clobbering newer DB state. AGENTS.md drops `bd sync`. str-qwua7.28 is superseded. Add matching bento guidance. No BEADS_HOOK_TIMEOUT fix and no hook-bypass guidance.
- **D5 Git identity:** the leaked `[user]` section is already removed. Remaining work: a `.mailmap`, a git-state drift check (folded into str-qwua7.1 if it fits), and a snapshot of `.git/config` before and after test fixtures.
- **D6 Filing:** after reconciliation and the Codex cross-check, the maintainer runs one filer script. No agent files anything.

None of D1-D6 changes this bucket directly. D3 is adjacent: z3-mixed-int-real-sort-split is linked as a likely contributor to concolic-early-termination (bucket shatter-concolic-and-engine-design).

<!-- file: 01-z3-mixed-int-real-sort-split.md -->

---
slug: z3-mixed-int-real-sort-split
kind: new
title: "Z3 translation declares separate Int and Real constants for one numeric param; solver returns wrong SAT models (0.5<x<1 gives x=1.5)"
priority: P1
type: bug
labels: [solver, z3, concolic, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Z3 translation declares separate Int and Real constants for one numeric param; solver returns wrong SAT models (0.5<x<1 gives x=1.5)

## Problem

A float param can be compared against both float and int constants, as in `if x > 0.5 { if x < 1 {...} }`. In that case the Z3 translation creates `Real x` and `Int x` as two unrelated constants with the same name. Model extraction then lets one overwrite the other. The solver returns "SAT" models that violate the path condition, so neither engine reaches the branch. Integer range bounds (such as u8) are asserted only on the Int twin, so the Real twin can leave the declared range.

This is a wrong-answer bug in the core solver. It affects any frontend that emits mixed-sort comparisons on one numeric param. The Go frontend does this: `{op:gt, const float 0.5}` next to `{op:lt, const int 1}`.

## Evidence

Line numbers were re-checked against `56c86168` (audit branch `audit-2026-09-22`):

- `shatter-core/src/solver.rs:500-503` (in `to_z3_expr`, which starts at :491): the declared param sort overrides the hint only when `!sorts_compatible(declared, hint_sort)`. For any numeric declared sort, the comparison's Int/Real hint wins.
- `shatter-core/src/solver.rs:74-121`: `VarTable` keeps separate `ints` and `reals` maps. `get_or_create_int` (:95-100) calls `Int::new_const(name)` and `get_or_create_real` (:102-107) calls `Real::new_const(name)` under the same name.
- `shatter-core/src/solver.rs:1023-1066`: `extract_concrete_values` inserts ints and then reals into one `HashMap`, so the Real value silently overwrites the Int value.
- `shatter-core/src/solver.rs:172-180`: `assert_int_param_ranges` binds only `vars.get_or_create_int(&p.name)`. The Real twin is unconstrained.
- Direct call observed during the audit: `solve_for_new_path` with x:Float and constraints [x>0.5 (float const), NOT(x<1 (int const))], negating index 1, returned `Ok(Sat({"x": Float(1.5)}))` in 3/3 runs. The wanted range was 0.5<x<1.
- n:u8 with [n<1000.5, n>300.5], negating index 1, returned `Sat({"n": Float(0.0)})`.
- End to end with a Go `func Classify(x float64) string` that has nested `x > 0.5` / `x < 1`: `shatter explore mix.go --concolic --max-iterations 40 --clean` reported 23 iterations, 2 paths, `worklist_exhausted`, and 4/5 lines. `return "low"` was never reached, and no tried input was in (0.5, 1). The random/hybrid engine, which also runs the Z3Solver strategy, missed it at 100 iterations too.
- Root cause of the gap: the solver proptests (str-r6fr) never generate one param under mixed sorts, and the E2E known-answer fixtures use only int or string params.

## Acceptance criteria

- [ ] Each variable gets exactly one Z3 sort, chosen from `param_sorts` before translation. Mixed-sort comparisons coerce at the use site (`Real::from_int` / `to_int`, or by coercing the constant) and never declare a second constant.
- [ ] `extract_concrete_values` has a debug assertion that no name appears in more than one sort table.
- [ ] `assert_int_param_ranges` constrains the single chosen variable, whatever its sort.
- [ ] Solver unit tests for both repros above return models that satisfy the constraints. At close, show that they fail on current `main` and pass after the fix, with test names and the before/after output in the close note.
- [ ] A proptest generates one param compared under mixed Int/Real constants and checks that every SAT model satisfies all asserted constraints.
- [ ] A known-answer E2E fixture shaped like `Classify` (nested x>0.5 / x<1 on a float param) exists for Go (`examples/go/`, exercised by `shatter-core/tests/e2e_concolic_go.rs`) and for TS (`shatter-core/tests/e2e_concolic.rs`). Under `--concolic` it reaches all three returns. Record the `cargo test --test e2e_concolic_go` and `cargo test --test e2e_concolic` output in the close note.
- [ ] `task affected` passes, and its `Gates selected` output is recorded.

## Suggested approach

Resolve the sort once per variable in `VarTable`, from `param_sorts` with a fallback to the first-seen hint. `get_or_create_int`/`get_or_create_real` then return a coerced view of the single constant instead of declaring a twin. Coercing constants to the variable's sort is simpler than coercing variables. Check the Int-variable, Real-constant case (`n < 1000.5`) carefully: coerce the variable to Real, or use ceil/floor on the constant, rather than truncating.

## Out of scope

- The float-constant scaling bug (`(v*1e6).round() as i64`). It is tracked separately as float-constant-rational-conversion.
- Concolic early termination in general. It is filed separately as concolic-early-termination (bucket shatter-concolic-and-engine-design), which may list this issue as a contributing cause.

## Priority

P1: the solver returns wrong answers and the engine reports false exhaustion.

## Type

bug

## Dependencies

- Blocked by: none.
- Related: str-6ayh, str-r6fr (solver proptests), concolic-early-termination.

## References

Audit 2026-09-22 finding core-01 (verified, P1). Source draft: `drafts/shatter-code/11-z3-int-real-sort-split.md`.

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

Code (line numbers re-checked against `56c86168`):

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
- [ ] A regression test asserts rendered report path count == progress-line path count == `--spec` class count, in both random and concolic modes. It covers these known-answer fixtures: TS `01-arithmetic.ts` (classifyNumber, compareMagnitudes), TS safeDivide, a trivial 2-branch fixture, the fall-through-throw `fmt2` shape, a float-param fixture (Go `Classify`-shaped), and Rust `safe_divide`. At close, show the test failing on current `main` and passing after the fix, with the output quoted in the close note.
- [ ] The behavior-map cache for these fixtures has one behavior per discovered path. A test asserts it, including the two-same-named-functions case.
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
title: "--setup is silently ignored under --concolic (every production caller passes setup_context=None), and the random explorer shrinks after teardown"
priority: P1
type: bug
labels: [concolic, setup, orchestrator, parity, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# --setup is silently ignored under --concolic (every production caller passes setup_context=None), and the random explorer shrinks after teardown

## Problem

Ownership of setup and teardown is inconsistent between the two engines.

1. **Concolic ignores `--setup`.** `orchestrator::explore` and `orchestrator::explore_with_oracle` accept a `setup_context`, but every production caller passes `None`. So `shatter explore --concolic --setup f` runs without setup and prints no warning. str-0s76.6 ("setup in concolic") was closed with "All callers updated". Its E2E injects a context directly into `orchestrator::explore`, so it passes even though the real pipeline never builds one. This issue replaces that closed issue; see the reopen-note setup-parity-reopen-note.
2. **The random explorer shrinks after teardown.** Per-function teardown runs before the witness-shrinking phase, and the shrink Execute calls pass `setup_context: None`. Witnesses found under setup state are replayed without that state. The shrinker then either spends its budget on rejections or accepts a witness that depends on state that no longer exists. The orchestrator's copy of the shrink correctly uses `setup_context.clone()`.

## Evidence

Line numbers were re-checked against `56c86168`:

- `shatter-core/src/orchestrator.rs:2453` `explore(...)` and `:2490` `explore_with_oracle(...)` take `setup_context: Option<SetupContextStack>` as the 7th argument.
- The production callers all pass `None` in that position:
  - `shatter-core/src/pipeline_orchestrator.rs:536-548` (`explore_with_oracle`, 7th arg `None` at :543)
  - `shatter-core/src/scan_orchestrator.rs:3103-3115` (`explore`, `None` at :3110)
  - `shatter-cli/src/commands/observe.rs:179-190` (`explore`, `None` at :186)
- `grep -c teardown shatter-core/src/orchestrator.rs` returns 0. `send_setup` is called only from `explorer.rs` and `observe.rs`.
- `shatter-cli/src/commands/explore.rs:5051` resolves `--setup` into `setup_file` for the random explorer config only. No CLI warning for `--concolic` was found.
- `shatter-core/tests/e2e_concolic.rs:1555` `orchestrator_explore_with_setup_context` calls `orchestrator::explore` directly with a hand-built context.
- `shatter-core/src/explorer.rs:1676-1683` runs per-function `send_teardown` before the `-- Witness shrinking phase --` at `:1692+`. The shrink Execute calls pass `setup_context: None` at `:1778`, `:1823` and `:1875`. The orchestrator's shrink passes `setup_context: setup_context.clone()` (`orchestrator.rs:3642`, `:3686`, `:3740`). Both copies came from 4d8001bc9 (str-28ea.6).
- Scan and observe set `setup_file: None` everywhere, so neither engine supports setup there. Whether that is intended is undocumented.

## Acceptance criteria

- [ ] One pipeline-level helper sends Setup, passes the resulting context to whichever engine runs, and sends Teardown after shrinking. `run_pipeline`, `explore_with_scan_mode` and `observe` all use it.
- [ ] A CLI-level or pipeline-level E2E, not one that calls `orchestrator::explore` directly, shows that a setup side effect is visible to the target under `shatter explore --concolic --setup <file>`. TS is the minimum. Add Go and Rust if their frontends declare setup support in `protocol/parity-matrix.yaml`. At close, show the test failing on current `main` and passing after the fix.
- [ ] Random-explorer shrinking runs before teardown with the live setup context. A test with setup and shrinking both enabled asserts that the shrink Execute requests carry the context and that the shrunk witness still reproduces its path.
- [ ] If scan and observe intentionally do not support `--setup`, passing it errors or warns explicitly, and a CLI test covers that. Otherwise they are wired through the same helper.
- [ ] `task affected` (with `Gates selected` recorded) and `task e2e` pass.

## Suggested approach

Move setup and teardown out of `explorer.rs` into the pipeline layer, so both engines receive a live context and teardown happens after the shrink phase in both. This also prepares str-qwua7.6.1's shared `select_witnesses`/shrink extraction, which currently leaves shrink behavior unchanged. The Execute-request builder in concolic-refine-execute-builder should take `setup_context` from this helper.

## Out of scope

- Adding setup support to frontends that lack it.
- The shared Execute builder itself (concolic-refine-execute-builder).

## Priority

P1: a user flag is silently dropped in the default-recommended engine, and a closed issue claims otherwise.

## Type

bug

## Dependencies

- Blocked by: none.
- Related: str-0s76.6 (closed but not fixed; gets setup-parity-reopen-note), str-0s76.12, str-qwua7.6.1, concolic-refine-execute-builder.

## References

Audit 2026-09-22 findings core-03 (P1) and core-09 (P2, folded in). Report §11.3 lists str-0s76.6 as closed-but-unfixed, and §15.1 files this as new rather than reopening. Source draft: `drafts/shatter-code/13-concolic-setup-teardown-ownership.md`.

---

<!-- file: 04-setup-parity-reopen-note.md -->

---
slug: setup-parity-reopen-note
kind: reopen-note
title: "Note on closed str-0s76.6: --setup is still ignored under --concolic"
priority: P1
type: bug
labels: [concolic, setup, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [concolic-setup-teardown]
existing_id: str-0s76.6
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on closed str-0s76.6: --setup is still ignored under --concolic

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
> `shatter-cli/src/commands/explore.rs:5051` wires `--setup` into the random explorer config only, and `orchestrator.rs` contains no teardown call. So `shatter explore --concolic --setup <file>` runs without setup and does not warn. The closing E2E (`shatter-core/tests/e2e_concolic.rs:1555`, `orchestrator_explore_with_setup_context`) injects a context directly into `orchestrator::explore`, so it cannot detect that the pipeline never builds one.
>
> The fix (a pipeline-level setup/teardown helper for both engines, plus a CLI-level `--concolic --setup` E2E) is tracked in **<id of concolic-setup-teardown>**. Line numbers verified at `56c86168`.

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
- There is no parity test for mock variation between the engines.

## Acceptance criteria

- [ ] Concolic worklist entries carry varied mock values (through `mutate_mock_values` or an equivalent), or concolic documents that mocks are fixed and warns at runtime when `mock_params` is non-empty. Record which option was taken in the close note. If the second is chosen, update `protocol/parity-matrix.yaml` / the relevant CLAUDE.md engine notes.
- [ ] The dead `_initial_mocks` block is removed, and `_mock_params` is either used or removed from the signature.
- [ ] A concolic E2E fixture whose branch depends on a mocked dependency's return value reaches both sides under `--concolic` (TS at minimum, in `shatter-core/tests/e2e_concolic.rs`). At close, show the test failing on current `main` and passing after the fix.
- [ ] `task affected` (with `Gates selected` recorded) and `task e2e` pass.

## Suggested approach

Restore the variation in the MetaStrategy loop. When a strategy produces a worklist entry, attach `mutate_mock_values(...)` output, as the random explorer does with `generate_mock_values`. Reuse the random explorer's generation helper rather than adding a third one.

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
> The random explorer still varies mocks on every iteration, so the two engines diverge. Restoring the variation (or explicitly documenting and warning about fixed mocks) is tracked in **<id of concolic-mock-variation-regression>**. Line numbers verified at `56c86168`.

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
title: "Concolic refine phase drops prepare_id/execution_profile and discards the paths it reaches; introduce one shared Execute-request builder"
priority: P2
type: bug
labels: [concolic, orchestrator, refactor, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Concolic refine phase drops prepare_id/execution_profile and discards the paths it reaches; introduce one shared Execute-request builder

## Problem

`refine_boundaries_async` builds its own `Command::Execute` requests with `prepare_id: None` and `execution_profile: None`. This drops TS execution adapters and forces re-preparation. It uses the results only to update boundary witnesses (true/false witnesses), so paths and lines reached during refinement never reach `covered_paths`, `raw_results` or the report.

The root cause is that each engine builds `Command::Execute` inline at eight or more sites, and each site re-decides `prepare_id`, `execution_profile`, `setup_context` and `capture`. The same pattern causes the open capture-flag bug str-qwua7.5, which this issue links but does not re-describe.

## Evidence

Line numbers were re-checked against `56c86168`:

- `shatter-core/src/orchestrator.rs:2321` `async fn refine_boundaries_async`. The Execute at `:2374-2384` sends `setup_context: setup_context.clone(), capture: false, prepare_id: None, execution_profile: None`.
- Refine results update only the tw/fw witnesses. They are not fed into observation or coverage accounting.
- The concolic `Classify` artifact from the audit (Go, nested x>0.5 / x<1) had `boundary_results` true_witness `[0.5000037571385455]` (20 executions) while `unique_paths=2`, and the report said 4/5 lines. The verifier noted that the witness is on branch 0 (x>0.5), and the artifact does not show whether it took the x<1 'low' side. The "forces a rebuild per execute on Go/Rust" consequence is not verified.
- Inline Execute construction sites in `orchestrator.rs` (by `capture:` literal): :1694, :2380, :2701, :2714, :3103, :3643, :3687, :3741. The random explorer has more (`explorer.rs` shrink sites at :1778/:1823/:1875 and the float-probe sites). str-qwua7.5 tracks the capture-flag part of this.

## Acceptance criteria

- [ ] A single helper builds every `Command::Execute` request from the engine config: `prepare_id`, `execution_profile`, `setup_context`, `capture`, plus mocks and plan. All Execute sites in both `orchestrator.rs` and `explorer.rs` use it. A grep for `Command::Execute {` / `ProtoCommand::Execute {` outside the helper and tests returns nothing, and the close note records that grep.
- [ ] Refine-phase executions go through normal observation and coverage accounting, so paths and lines reached only during refinement appear in `unique_paths`, `raw_results` and the report.
- [ ] Test: a TS execution-adapter target keeps its `execution_profile` (and `prepare_id`) on refine-phase Execute requests. Assert on the requests sent, for example through a recording frontend double.
- [ ] Test: a path reached only in the refine phase appears in the explore report and artifact. At close, show it failing on current `main` and passing after the fix.
- [ ] str-qwua7.5 is closed through this helper, or updated with a comment saying the helper is the fix vehicle. Do not duplicate its capture-flag description here.
- [ ] `task affected` (with `Gates selected` recorded) and `task e2e` pass.

## Suggested approach

Add an `ExecuteRequestBuilder`, or a function on the engine config, that owns the field decisions. Migrate the sites one by one (refine first), then route the refine results through the same aggregator call the main loop uses. `setup_context` should come from the pipeline-level helper in concolic-setup-teardown when that lands, but this issue does not depend on it: the builder can take the context it is given.

## Out of scope

- The capture-flag semantics themselves (str-qwua7.5).
- Session-lifecycle extraction (str-inct (b)).
- Broader shared-shrink extraction (str-qwua7.6 / str-qwua7.6.1).

## Priority

P2

## Type

bug

## Dependencies

- Blocked by: none.
- Related: str-qwua7.5 (open; core-16 is a duplicate of it, so link only), str-qwua7.6, str-inct, concolic-setup-teardown, float-probe-paths-uncounted (same "executions not counted" class of bug in the other engine).

## References

Audit 2026-09-22 findings core-06 (verified, P2) and core-16 (a duplicate of open str-qwua7.5, context only). Source draft: `drafts/shatter-code/16-concolic-refine-phase-execute-builder.md`.

---

<!-- file: 09-invariant-min-support.md -->

---
slug: invariant-min-support
kind: new
title: "Invariant inference over-generalizes: no minimum support at function level, and only 0-anchored numeric templates"
priority: P2
type: bug
labels: [spec, invariants, properties, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Invariant inference over-generalizes: no minimum support at function level, and only 0-anchored numeric templates

## Problem

Function-level invariant detection runs on any number of execution records, including one. `detect_invariants` keeps every template that holds on all specimens, so a single execution yields invariants like `x == <constant>` and `x > 0`, which are then published in the spec. Numeric comparison templates compare only against 0, so real bounds (such as `0 <= x <= 100`) are never inferred, and the output is dominated by trivially true or overfitted facts. Confidence is hard-coded to 1.0, so the spec cannot signal weak support.

## Evidence

Line numbers were re-checked against `56c86168`:

- `shatter-core/src/spec.rs:395-400`: `detect_classified_invariants(&all_records, ...)` runs on all records with no minimum count. Per-class inference requires `class_records.len() >= 2` (`:411`).
- `shatter-core/src/invariants.rs:504` `detect_invariants`: keeps any template true on all specimens and returns early only on empty input.
- `shatter-core/src/invariants.rs:661`: `confidence: 1.0` is hard-coded. Removing or computing it is tracked in open str-qwua7.61.
- `shatter-core/src/invariants.rs:253-340`: `NumericComparison` candidates use only `value: 0.0` (`:270`, `:291`, `:312`, `:332`). There are no range or min/max templates.
- `invariants.rs` has 0 `proptest!` blocks. Proptest coverage is tracked in open str-qwua7.47.

## Acceptance criteria

- [ ] A minimum-support threshold (default at least 5 specimens, configurable) applies before an invariant is reported at both function and class level. The default and the config key are documented where spec options are documented.
- [ ] Range templates (observed min/max bounds) are added, or the close note gives explicit reasoning for not adding them.
- [ ] A proptest checks that every inferred invariant holds on all of its source traces, and that no invariant is emitted from fewer than N specimens.
- [ ] A regression test: a function explored with a single execution produces no function-level invariants. At close, show it failing on current `main` and passing after the fix.
- [ ] Spec snapshots that change are regenerated and reviewed in the same change, and the diff is summarized in the close note.
- [ ] `task affected` passes, with `Gates selected` recorded.

## Suggested approach

Add the support threshold in `spec.rs`, next to the existing per-class `>= 2` check, so both call sites use one constant or config value. Add range templates in `invariants.rs`. Coordinate with str-qwua7.61 on the confidence field: this issue covers the support threshold and templates, and str-qwua7.61 covers the confidence value.

## Out of scope

- Removing or computing `confidence` (str-qwua7.61).
- General proptest coverage of `invariants.rs` beyond the invariant above (str-qwua7.47).

## Priority

P2

## Type

bug

## Dependencies

- Blocked by: none.
- Related: str-qwua7.61, str-qwua7.47.

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

Line numbers were re-checked against `56c86168`:

- `shatter-cli/src/args.rs:658`, `:1073` and `:1420`: the `--solver-timeout` help reads "Z3 solver timeout in seconds per query. Default: no limit."
- `shatter-core/src/solver.rs:1214-1217`, `:1254-1257` and `:1343-1346`: all three solver entry points call `cfg.set_timeout_msec(ms)` only `if let Some(ms) = solver_timeout_ms`.
- `shatter-cli/src/helpers.rs:893-897`: the only default, `Some(10)` when `mcdc && solver_timeout.is_none()`.
- `shatter-cli/src/main.rs:593`: the `Scan` arm destructures `solver_timeout: _`, so the flag never reaches the scan solver config.
- The 2026-09-04 audit's claim "Timeouts: cfg.set_timeout_msec on each query" holds only when a value is set.

## Acceptance criteria

- [ ] A default per-query Z3 timeout (e.g. 2 s, configurable through the flag and `.shatter/config.yaml`) applies in all modes and commands. It is set in one place, at solver-config construction. The `--mcdc` 10 s default is either kept as a documented override or folded into the same mechanism.
- [ ] The `--solver-timeout` help text and any docs state the new default.
- [ ] A Z3 `Unknown`/timeout result is recorded as a frontier stall and is visible in artifact stats (a counter in the explore artifact), not silently treated as UNSAT.
- [ ] `shatter scan --solver-timeout N` is honored. A CLI test asserts that the value reaches the solver config used by scan. At close, show the test failing on current `main` and passing after the fix.
- [ ] A test with a deliberately hard query (for example nonlinear integer arithmetic) completes within the default timeout plus a small margin and records the stall.
- [ ] `task affected` (with `Gates selected` recorded) and `task e2e` pass. Run `task gauntlet` if help output snapshots change.

## Suggested approach

Resolve the effective timeout once, where the budgets are resolved (`resolve_mcdc_budgets` in `helpers.rs` or its successor), and pass `Some(default)` down instead of `None`. Wire `solver_timeout` through the `Scan` arm in `main.rs` the same way as for explore. Add the stall counter where `SolveResult::Unknown` is handled in the orchestrator.

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
> **The real hazard.** `replace_dead_worker_if_needed` (`:2317-2327`) calls `reap_dead_slot()` when `Frontend::spawn` fails. The task loop then calls `pool.maybe_grow(remaining)` (`:4743`), which retries a detached spawn. If spawns keep failing (for example a broken frontend install), `live_count` reaches 0 while queued tasks block in `pool.checkout().await` (`:4607`). They stay blocked until `join_with_dynamic_watchdog` (`:5536`) fires, so a spawn failure is reported as a task timeout rather than a spawn error. The verifier confirmed the mechanism. It did not confirm that each task is then reported as `phase_timeout('task')`, so check that first.
>
> **Proposed rescope:**
> - Retitle: "Fail fast when the scan worker pool is exhausted".
> - Detect `live_count == 0 && pending > 0` and return `ScanError::WorkerPoolExhausted { last_spawn_error }` instead of letting checkouts block.
> - Add a regression test where every respawn fails, and assert that the new error is returned (with the spawn error text) and no task timeout is reported. It must fail on current `main` and pass after the fix.
> - Drop the `expect` → `?` conversion of the construction and checkout sites from scope, or keep it only as hygiene. Those sites are not reachable failure points.
>
> Line numbers verified at `56c86168`.

---

<!-- file: 12-float-constant-rational-conversion.md -->

---
slug: float-constant-rational-conversion
kind: new
title: "Float constants converted to Z3 via (v*1e6).round() as i64: saturate above ~9.2e12 and become 0 below 5e-7"
priority: P3
type: bug
labels: [solver, z3, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Float constants converted to Z3 via (v*1e6).round() as i64: saturate above ~9.2e12 and become 0 below 5e-7

## Problem

Float literals in path constraints are converted to Z3 reals by scaling by 1e6, rounding and casting to `i64`. Rust's float-to-int `as` saturates, so any constant above about 9.2e12 becomes `i64::MAX / 1e6`. Any constant with magnitude below 5e-7 becomes 0, and everything in between loses precision beyond six decimal places. Branches on large thresholds (timestamps in ms or ns, byte counts) or tiny epsilons are then solved against the wrong constant, and the solver can report SAT or UNSAT incorrectly.

## Evidence

Line numbers were re-checked against `56c86168`:

- `shatter-core/src/solver.rs:514-517`, inside `to_z3_expr`'s `SymExpr::Const` arm:
  ```rust
  ConstValue::Float(v) => {
      let scaled = (*v * 1_000_000.0).round() as i64;
      Ok(Z3Ast::Real(Real::from_rational(scaled, 1_000_000)))
  }
  ```
- The 2026-09-04 audit noted the x1e6 scaling but not these edge cases. No existing issue covers them.

## Acceptance criteria

- [ ] Float constants become an exact Z3 rational: numerator and denominator built from the f64 mantissa and exponent (as big integers or a decimal string if needed), or `Real::from_real_str` with the shortest round-trip representation. NaN and infinities are rejected explicitly with `SolverError::Unsupported`, not silently mapped.
- [ ] Proptests with extreme magnitudes (1e15, 1e-9, subnormals, negative values) check that the Z3 value equals the f64 exactly. For example: assert `x == c` and check that the model value round-trips to the same f64.
- [ ] A regression test in which a constraint `x > 1e13` (and `x < 1e-9`, `x > 0`) produces a model satisfying the original f64 comparison. At close, show it failing on current `main` and passing after the fix.
- [ ] `task affected` (with `Gates selected` recorded) and `task e2e` pass (solver change).

## Suggested approach

Use `f64::integer_decode` (or an equivalent via `to_bits`) to get mantissa, exponent and sign. Build the rational as `mantissa * 2^exp`, or `mantissa / 2^-exp`, with Z3 integer terms, or format it as an exact decimal string for `from_real_str`. Add the tests.

## Out of scope

- The Int/Real sort split (z3-mixed-int-real-sort-split).
- Model-to-f64 extraction precision in `extract_concrete_values` (`num as f64 / den as f64`). Mention it in the close note if it proves to be a problem.

## Priority

P3

## Type

bug

## Dependencies

- Blocked by: none.
- Related: z3-mixed-int-real-sort-split (same file, and both edit `to_z3_expr`, so coordinate if both are in flight).

## References

Audit 2026-09-22 finding core-19 (verified, P3). Source draft: `drafts/shatter-code/24-float-constant-rational-conversion.md`.
