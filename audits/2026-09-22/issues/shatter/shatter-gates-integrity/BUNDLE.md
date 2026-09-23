# Bundle: shatter-gates-integrity (audit 2026-09-22)

- **Bucket:** shatter-gates-integrity. Theme: gates must prove they executed. Covers Task checksum poisoning, sources, affected routing, pre-commit, verifier evidence, the dead gauntlet checker and unwired test modules.
- **Repo / tracker:** shatter, bd in /home/ketan/project/shatter (prefix str).
- **Parent epic:** Epic: Audit 2026-09-22 findings.
- **Status:** final drafts. Nothing is filed. Evidence was re-verified on 2026-09-23 against the audit snapshot (`56c86168`). Main differs from it only in `shatter-core/Taskfile.yml` (str-6nul9, 20692b08).

## Maintainer decisions (2026-09-23). These override the report and the old drafts.

- **D1 Releases:** keep Windows (x86_64-pc-windows-msvc) and aarch64-unknown-linux-gnu in the release matrix and fix them (Z3 header or static link on Windows; openssl-sys under cross for aarch64). Do not drop them. Release work closes only with a green release-run URL.
- **D2 shatter diff:** retire the snapshot-diff command (`shatter diff`) and the unused Snapshot writer path. spec-diff is the regression tool. Update SPEC, README and QUICKSTART. The `diff` name becomes free, and whether str-81xiw takes it is left to that epic. Correct the shatter-agents plugin's `shatter diff --staged` docs to describe what exists today.
- **D3 Concolic positioning:** measure first. P1: a controlled default-vs-concolic benchmark, reported per release. P1: fix concolic early termination. A follow-up decision issue, blocked by both, re-decides the "concolic-first" positioning. No doc softening now.
- **D4 Beads hook stall:** retire the JSONL import and move tracker sync to a Dolt remote. The first step checks whether the stale import has been clobbering newer DB state. AGENTS.md drops `bd sync`. str-qwua7.28 is superseded. bento's beads-issue-flow gets matching guidance. No BEADS_HOOK_TIMEOUT env var and no hook-bypass guidance anywhere.
- **D5 Git identity:** the leaked `[user]` section was already removed. Add a `.mailmap`, a git-state check (folded into str-qwua7.1), and a before/after `.git/config` snapshot in test_git_fixture_isolation.py.
- **D6 Filing:** after reconciliation and the Codex cross-check, the maintainer runs one filer script. Agents file nothing.

Decision touching this bucket: **D4**. `fast-hermetic-precommit` makes the hook fast and contains no bypass guidance.

## Contents

| # | Slug | Kind | Target | P | Blocked by |
|---|---|---|---|---|---|
| 01 | task-list-json-poisons-checksums | note-to-existing | str-qwua7.3 | P1 | - |
| 02 | ci-executed-leaf-guard | new | - | P1 | task-list-json-poisons-checksums |
| 03 | task-sources-cover-real-inputs | new | - | P1 | - |
| 04 | affected-gates-routing | new | - | P2 | - |
| 05 | fast-hermetic-precommit | new | - | P1 | - |
| 06 | wire-every-test-module | new | - | P2 | - |
| 07 | gauntlet-scan-checker-consumes-json | new | - | P1 | - |
| 08 | gauntlet-checker-reopen-note | reopen-note | str-jeen.57 | P1 | gauntlet-scan-checker-consumes-json (file first, for its id) |
| 09 | verifier-per-language-evidence | new | - | P2 | - (plus a one-line companion note on str-qwua7.55) |
| 10 | gate-telemetry-executed-vs-cached | new | - | P2 | task-list-json-poisons-checksums |

`task-list-json-poisons-checksums` as a blocker means the fix tracked on str-qwua7.3.

---

<!-- file: 01-task-list-json-poisons-checksums.md -->

---
slug: task-list-json-poisons-checksums
kind: note-to-existing
title: "Root cause for str-qwua7.3: meta test's `task --list-all --json` writes checksums for 39 tasks, so `task check` and CI skip every stage-2/3 test"
priority: P1
type: bug
labels: [quality-gates, ci, taskfile, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-qwua7.3
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-qwua7.3: root cause found (meta test's `task --list-all --json` poisons Task checksums)

**Target:** open issue `str-qwua7.3` ("Explain why stage-2/3 tasks report 'up to date' right after .task/checksum is deleted", P1).
**Action:** post the comment below on str-qwua7.3, then widen its acceptance criteria as shown. Keep str-qwua7.3 at P1. Do not file a new issue. The follow-ups `ci-executed-leaf-guard` and `gate-telemetry-executed-vs-cached` in this bucket are blocked by str-qwua7.3.

## Comment text (post verbatim)

> **Root cause found (audit 2026-09-22, findings gates-01 and tests-ci-01).**
>
> **Problem.** `task check` exits 0 in a fresh worktree, but no stage-2 or stage-3 test leaf runs: each one prints `Task "X" is up to date`. The cause is the meta-stage unit test `test_every_emitted_gate_is_a_real_task`. It runs `task --list-all --json` with cwd set to the repo root. On go-task 3.50 and 3.53, computing the JSON `up_to_date` field writes `.task/checksum/*` for every task that has `sources:`. Stage 1 runs `meta` first, so stages 2 and 3 find fresh checksums and skip their work. That is why deleting `.task/checksum` did not force the leaves to execute, which is the question this issue asked.
>
> **Evidence (re-verified 2026-09-23 against the audit snapshot; main differs only in shatter-core/Taskfile.yml, from str-6nul9).**
> - `scripts/test_affected_gates.py:203-212` (`test_every_emitted_gate_is_a_real_task`) runs `subprocess.run(["task", "--list-all", "--json"], cwd=ROOT, ...)`.
> - `Taskfile.yml:459` runs that module in `meta` (`python3 -m unittest scripts.test_affected_gates`). `meta` is a dep of `check-static` (`Taskfile.yml:535-546`), which runs before `check-unit` (`:548`) and `check-integration` (`:564`).
> - Repro with go-task 3.50.0 in a `git archive` copy of HEAD: `task --list-all` writes 0 checksum files, while `task --list-all --json` writes 39 (core-test-ignored, go-test, ts-test, cli-test, rust-fe-test, conformance, parity, ...). After that, `task sub:test` prints `is up to date` even when a source file has been edited. Adding `--dry`, or dropping `--json`, avoids the writes. A scratch repro with `includes: sub: {taskfile: ./sub, dir: ./sub}` behaves the same way.
> - Local evidence: `audits/2026-09-22/gates/check.log:780-799,942` (the `is up to date` lines). The poisoned cache is snapshotted under `audits/2026-09-22/gates/poisoned-task-cache-snapshot/`.
> - CI evidence: in run 35756993223 (task v3.53.1), cli:test, conformance, core:test-ignored, docs-smoke, go:test, go:vet, parity, rust-fe:test, rust-rt:test, ts:install and ts:test all report up to date. All 4 `test result:` lines in the log come from the separate shatter-llm step.
> - Introduced 2026-08-29 in 8ae14f13 and b12e3054 (str-35vtk.8). CI wall time dropped from about 330-613 s to 154-225 s from 2026-08-30 onward.
>
> **Fix direction.** Remove the side effect from the meta stage. The simplest options are to run the listing with `TASK_TEMP_DIR` pointed at a throwaway `mktemp -d`, or to add `--dry`. Parsing the Taskfile YAML also works. Then add a regression test to `meta` that snapshots `.task/checksum` before and after it runs.
>
> **Widened acceptance for this issue** (replaces "explain why"):
> 1. No meta-stage command mutates the repo's `.task/`: no `task --list-all --json` against the live tree without `--dry` or an isolated `TASK_TEMP_DIR`. A grep test in `meta` fails if a new unguarded `task --list*` call appears in `scripts/test_*.py`.
> 2. A new regression test runs the `meta` task (or its listing helper) against a scratch copy, then asserts that `.task/checksum` has no new or changed entries and that a subsequent stage-2 leaf executes instead of reporting `is up to date`. It must fail on the current tree and pass after the fix. Record both results in the close reason.
> 3. A real re-run of main: in a fresh worktree, `rm -rf .task && task check` shows every stage-2 and stage-3 test leaf executing, with no `is up to date` line for any test leaf. Attach the log excerpt or path to the close reason. Test failures this surfaces are triaged under `ci-executed-leaf-guard`, not here.
> 4. Optional: file an upstream go-task issue describing the `--list-all --json` side effect and record its link here.
>
> **Out of scope here.** The CI guard against hollow passes and triage of newly surfacing failures (`ci-executed-leaf-guard`). Executed-vs-cached receipts (str-qwua7.2). Missing `sources:` globs (`task-sources-cover-real-inputs`).

## Filer notes

- Priority stays P1. Add the labels `quality-gates, ci, taskfile, audit` if they are missing.
- Add a `related` link to str-qwua7.2 and str-35vtk.8.
- Add a parent or related link to the epic "Epic: Audit 2026-09-22 findings" if the tracker allows a second parent. Otherwise mention the epic in the comment only.

---

<!-- file: 02-ci-executed-leaf-guard.md -->

---
slug: ci-executed-leaf-guard
kind: new
title: "CI 'Full landing gate' has run no product tests since 2026-08-29: add an executed-leaf guard and triage what fails once tests really run"
priority: P1
type: bug
labels: [ci, quality-gates, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [task-list-json-poisons-checksums]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# CI 'Full landing gate' has run no product tests since 2026-08-29: add an executed-leaf guard and triage what fails once tests really run

## Problem

The meta-stage checksum poisoning, whose root cause is recorded on str-qwua7.3 (see the `task-list-json-poisons-checksums` note), means the CI `task check` step has run no unit, integration, E2E, conformance or parity tests since about 2026-08-29. Every merge to `main` since then has passed on a hollow green. Once the str-qwua7.3 fix lands, real failures will appear and need triage. CI also needs a guard so a hollow pass cannot happen again silently. Until then, two CLAUDE.md claims are false: "Full = Landing, CI" in the Test Tiers table, and "Regression snapshots are ... verified in CI" in Code Quality Standards.

Related: str-qwua7.2 (open) covers the general executed-vs-skipped receipt system for the verifier, CI and pre-completion. This issue covers only the simple CI guard and the triage. str-35vtk.21 ("CI runs full landing gate") was closed on run 33277936601, and the guarantee it recorded no longer holds.

## Evidence

- `gh run view 35756993223 --log` shows `Task "go:test" is up to date`, and the same for `core:test-ignored`, `conformance`, `parity`, `cli:test`, `rust-fe:test`, `rust-rt:test`, `ts:test` and `go:vet`. All `test result:` lines come from the separate shatter-llm steps (`.github/workflows/ci.yml:99-103`).
- CI job durations on main were 330-613 s from 2026-08-10 to 08-27, and 154-225 s from 2026-08-30 onward.
- `.github/workflows/ci.yml:88-89` runs `task check`. Nothing inspects whether its leaves executed. `scripts/test_ci_workflow_structure.py` (run only in CI, `ci.yml:110`) checks workflow shape only.
- Failures already known when the leaves are forced to run locally (audit 2026-09-22):
  - str-k7czv: Go loader test. Root cause is filed as `go-config-discovery-unbounded` (shatter-frontend-go bucket).
  - 10 TS handler timeouts under load: filed as `ts-handlers-test-timeouts` (shatter-test-hygiene bucket).
  - bench_frontier_ranking timeout in `core:test-ignored`: str-6nul9 landed on main after the audit snapshot (20692b08, merged 70465921). It excludes the benchmark with a nextest `-E` filter and with `--skip bench_frontier_ranking` on the `cargo test` fallback (`shatter-core/Taskfile.yml:61-66` on main). CI has no cargo-nextest, so CI takes the fallback path. Confirm that the skip holds there.

## Acceptance criteria

- [ ] CI fails when the `task check` output reports `is up to date` for any test leaf. Either grep the step log against an explicit list of test-leaf task names, or run `task check` with a fresh `TASK_TEMP_DIR=$(mktemp -d)` and grep as a backstop. A unit test (in `scripts/test_ci_workflow_structure.py` or a new module wired into `meta`) feeds the guard a sample log containing `Task "cli:test" is up to date` and asserts it exits non-zero.
- [ ] Proof the guard works: link a CI run, on a branch or with the fix temporarily reverted, where the guard fails on a hollow check. Also link a green CI run on main after str-qwua7.3's fix in which every stage-2 and stage-3 leaf executed (non-zero `test result:` lines or jest/go test summaries for each leaf). Put both run URLs in the close reason.
- [ ] Every failure the first real CI run surfaces is either fixed or filed, and the issue ids are listed in the close reason.
- [ ] The CLAUDE.md "Test Tiers" and "Code Quality Standards" CI claims are re-checked against the green run and left true, or corrected.

## Suggested approach

1. Wait for the str-qwua7.3 fix.
2. Add a post-step to `.github/workflows/ci.yml` that runs `task check 2>&1 | tee check.log`, then fails if `grep -E 'Task "(cli:test|rust-fe:test|rust-rt:test|ts:test|go:test|go:vet|core:test-ignored|conformance|parity)" is up to date' check.log` matches. Keep the list in a script with a unit test, not inline YAML, so it can grow.
3. Re-run CI on main and triage what fails.

## Out of scope

- The executed-vs-skipped receipt system (str-qwua7.2) beyond this CI grep guard.
- Missing `sources:` globs (`task-sources-cover-real-inputs`). Gate telemetry (`gate-telemetry-executed-vs-cached`).
- Adding cargo-nextest or a CI nextest profile (`nextest-ci-profile-and-stale-parity-fallback`, shatter-ci-workflows bucket).

## Metadata

- Priority: P1. Type: bug. Size: M.
- Labels: ci, quality-gates, audit.
- Parent epic: Epic: Audit 2026-09-22 findings.
- Blocked by: task-list-json-poisons-checksums (the str-qwua7.3 fix).
- Related: str-qwua7.2, str-35vtk.21, str-6nul9, str-k7czv.
- Source findings: gates-02 (audit 2026-09-22; evidence under `audits/2026-09-22/`).

---

<!-- file: 03-task-sources-cover-real-inputs.md -->

---
slug: task-sources-cover-real-inputs
kind: new
title: "Task `sources:` omit real test inputs (CLI tests/templates/build.rs, embedded frontends, E2E frontend trees, parity matrix, runtime crate, rust-fe tests/): add the globs and a sources-coverage meta test"
priority: P1
type: bug
labels: [quality-gates, taskfile, parity, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Task `sources:` omit real test inputs: add the globs and a sources-coverage meta test

## Problem

go-task's checksum cache skips a leaf when none of its declared `sources:` has changed. Several test leaves omit files that affect their result, so an edit that matters can be answered with a cached pass. The checksum caching was added for speed (str-35vtk) without the invariant "a relevant change must never be skipped", and without any test for it. This issue merges audit drafts code/03 and agent/20 (report §15.1). The affected-selector half of agent/20 is filed separately as `affected-gates-routing`.

Related: str-qwua7.2 (open) questions checksum caching in general and covers executed-vs-cached receipts. It does not list the missing globs or add a coverage test.

## Evidence (re-verified 2026-09-23 against the audit snapshot; only shatter-core/Taskfile.yml has changed on main since)

- **cli:test**, at `shatter-cli/Taskfile.yml:16-24`, and **cli:test-fast** (`:35`ff) list only `src/**/*.rs`, `Cargo.toml`, `../Cargo.lock`, core `src/**/*.rs`, core `Cargo.toml` and `../.config/nextest.toml`. They miss:
  - `shatter-cli/tests/**` (30 entries)
  - `shatter-cli/templates/**`: askama templates `explore_fn.md` and `scan.md`, used at `render.rs:17,40`
  - `shatter-cli/build.rs`, which embeds shatter-ts (`build.rs:90`) and shatter-go (`build.rs:163`)
  - the embedded frontend trees `shatter-ts/src/**`, `shatter-go/**/*.go` and `shatter-core/build.rs`
- **core:test-ignored**, at `shatter-core/Taskfile.yml:37-45`, runs the Go, TS and Rust E2E suites (`--run-ignored all` / `--include-ignored`), but lists no frontend source trees (`shatter-ts/src`, `shatter-go`, `shatter-rust/src`, `shatter-rust-runtime/src`). `test-ignored-fast` has the same gap.
- **workspace-test** (test-standard), at `Taskfile.yml:115-128`, runs the Go and Rust E2E suites. It lists `shatter-cli/**/*.rs` and `shatter-llm/**/*.rs`, but no frontend sources and no `shatter-cli/templates/**`.
- **parity**, at `Taskfile.yml:245-263`: its sources (`:252-261`) omit `protocol/parity-matrix.yaml`, `protocol/PARITY.md`, `scripts/validate-parity.py` and `protocol/conformance/run_golden_tests.py`, although `parity-governed` (`:265-276`) runs `validate-parity.py` and `run_golden_tests.py` over them. `affected-gates.py` selects `parity` for any `protocol/` change, and Task then skips it as up to date when only those files changed.
- **rust-fe:test**, at `shatter-rust/Taskfile.yml:13-19`, lists `src/**/*.rs`, `Cargo.toml`, `Cargo.lock` and `../.config/nextest-standalone.toml`. It omits `tests/**/*.rs` (`tests/codegen_parity.rs`) and `../shatter-rust-runtime/src/**`. The executor tests build harnesses against the runtime crate through `find_runtime_crate_path` (`shatter-rust/src/executor.rs`, around line 1198).
- **ts:test**, at `shatter-ts/Taskfile.yml:38-49`, omits `jest.config.js`, which exists.
- **shatter-llm**: `check` has no shatter-llm test leaf (str-35vtk.36, open, P3). CI covers it with two extra steps (`.github/workflows/ci.yml:99-103`). Any llm test task added under str-35vtk.36 must declare `shatter-llm/**` sources. The meta test below must cover it once that task exists.

## Acceptance criteria

- [ ] Each task named above declares the missing globs, including the `-fast` twins, which must stay in sync per the existing wiring tests.
- [ ] A new meta test, wired into `meta` (for example extending `scripts/test_test_tier_wiring.py` or a new `scripts/test_task_sources_coverage.py`), parses the Taskfiles as YAML. It does **not** use `task --list-all --json`; see str-qwua7.3. It asserts:
  1. For a declared table of test task → required input globs (crate `tests/`, `templates/`, `build.rs`, each executed or embedded frontend tree, the parity matrix, validator scripts and the runtime crate), each glob appears in that task's `sources:`.
  2. Every git-tracked file under a test task's crate or declared input roots (`git ls-files`) matches some `sources:` glob of a task that tests it. Files that do not feed a test go in an explicit allowlist with a reason.
- [ ] Proof: the meta test fails on the current tree (paste the failing assertion into the close reason) and passes after the globs are added.
- [ ] Forced-execution proof: after the fix, `touch shatter-cli/templates/scan.md && task cli:test` executes tests, and `touch protocol/parity-matrix.yaml && task parity` executes `validate-parity.py`. Neither prints `is up to date`. Record both in the close reason.

## Suggested approach

Add the globs first. Then write the table-driven meta test, with the `git ls-files` sweep as its second half. Alternatively, drop Task checksum caching for test leaves entirely and rely on cargo, go and jest incrementality. If you choose that, record the decision on str-qwua7.2, and the meta test shrinks to asserting that test leaves have no `sources:`.

## Out of scope

- Diff-scoped gate selection in `scripts/affected-gates.py` (`affected-gates-routing`).
- The root cause of the meta-stage checksum poisoning (str-qwua7.3).
- Folding shatter-llm into `task check` (str-35vtk.36). This issue only requires its sources once the task exists.

## Metadata

- Priority: P1. Type: bug. Size: S-M.
- Labels: quality-gates, taskfile, parity, audit.
- Parent epic: Epic: Audit 2026-09-22 findings.
- Blocked by: none. It is independent of str-qwua7.3, but the forced-execution proof is only meaningful once str-qwua7.3 is fixed.
- Related: str-qwua7.2, str-35vtk, str-35vtk.36.
- Source findings: tests-ci-04, frontend-rust-07 (sources part), protocol-parity-04. Drafts shatter-code/03 and shatter-agent/20.

---

<!-- file: 04-affected-gates-routing.md -->

---
slug: affected-gates-routing
kind: new
title: "`task affected` misroutes: .md (askama templates, frontend CLAUDE.md) goes only to the hollow `docs` gate, docs-smoke is never selected, core/frontend changes skip cli:test, runtime skips rust-fe:test, shatter-llm falls through to a `check` that doesn't test it"
priority: P2
type: bug
labels: [quality-gates, taskfile, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# `task affected` misroutes several path classes, so the pre-completion gate can pass without running the checks that cover a change

## Problem

The diff-scoped selector `scripts/affected-gates.py` (built under str-35vtk.8) leaves out validators for several classes of path. `task affected` is the pre-completion gate and runs on pre-push, so it can pass without running the checks that cover a change. The selector's tests pin historical merge selections rather than the dependency graph.

## Evidence (re-verified 2026-09-23 by calling `select_gates` directly on the audit snapshot, which is unchanged on main)

| Path | `select_gates([path])` today | Missing |
|---|---|---|
| `shatter-cli/templates/scan.md` | `['docs']` | cli:test, cli:clippy, gauntlet |
| `README.md` | `['docs']` | docs-smoke |
| `shatter-ts/CLAUDE.md` (parity contract) | `['docs']` | parity, conformance |
| `shatter-core/src/report/html.rs` | `['smoke','core:clippy','core:test']` | cli:test |
| `shatter-ts/src/index.ts` | `[..., 'ts:test', 'e2e-ts', 'parity', 'conformance']` | cli:test (build.rs embeds the TS frontend) |
| `shatter-go/main.go` | `[..., 'go:test', 'e2e-go', 'parity', 'conformance']` | cli:test (build.rs embeds the Go frontend) |
| `shatter-rust-runtime/src/lib.rs` | `['smoke','rust-rt:clippy','rust-rt:test','e2e-rust']` | rust-fe:test (executor tests build against the runtime) |
| `shatter-llm/src/jev.rs` | `['smoke','check']` | shatter-llm clippy and tests (`check` runs neither) |

- `scripts/affected-gates.py:126-127` maps any `*.md` or `docs/` path to `{'docs'}` before any crate rule. That makes the templates → gauntlet rule at `:162` unreachable for the `.md` templates.
- `docs-smoke`, which validates README, QUICKSTART, SPEC and docs/INDEX against the built CLI, is in neither `GATE_ORDER` (`:13-38`) nor `_classify` (`:123-184`).
- The `docs` task (`Taskfile.yml:353-378`) consists of `test -f` checks plus markdownlint, vale and lychee. Each of the three prints `[skip] ... not installed` locally and in CI (str-qwua7.46).
- `shatter-llm/` matches no rule, so it falls through to `check` (`:195-200`). `check-static` and `check-unit` run no shatter-llm clippy or tests. CI covers them with separate steps (`.github/workflows/ci.yml:91-103`), and str-35vtk.36 is open to fold them in.

## Acceptance criteria

- [ ] Crate-prefix rules are evaluated before the generic `.md` or `docs/` rule. `shatter-cli/templates/**` selects cli:test, cli:clippy and gauntlet. `shatter-{ts,go,rust}/CLAUDE.md` selects parity and conformance.
- [ ] `README.md`, `QUICKSTART.md`, `SPEC.md`, `docs/INDEX.md` and `scripts/docs-smoke*` select `docs-smoke`, and `docs-smoke` is added to `GATE_ORDER`.
- [ ] `shatter-core/`, `shatter-ts/` and `shatter-go/` paths also select `cli:test`. `shatter-rust-runtime/` also selects `rust-fe:test`.
- [ ] `shatter-llm/` has an explicit rule selecting shatter-llm clippy and test tasks. Create `llm:clippy` and `llm:test` if str-35vtk.36 has not yet. It no longer falls through to `check`.
- [ ] Table-driven cases in `scripts/test_affected_gates.py` cover every row of the table above. They fail on today's tree (record this in the close reason) and pass after the fix. Any new task-name lookup in those tests must not run `task --list-all --json` against the live tree (str-qwua7.3).

## Suggested approach

Reorder `_classify` so crate prefixes win. Add the mappings and the `docs-smoke` gate. Extend the table in `test_affected_gates.py`. Coordinate with str-35vtk.36 (shatter-llm tasks) and str-qwua7.46 (docs linters fail under `CI=1`).

## Out of scope

- Installing or enforcing the doc linters (str-qwua7.46).
- Folding shatter-llm into `task check` (str-35vtk.36).
- Missing Task `sources:` globs (`task-sources-cover-real-inputs`).
- `e2e` running twice in `pre-completion-e2e`, which is filed in `collapse-test-tiers` (shatter-test-hygiene bucket).

## Metadata

- Priority: P2. Type: bug. Size: S-M.
- Labels: quality-gates, taskfile, audit.
- Parent epic: Epic: Audit 2026-09-22 findings.
- Blocked by: none.
- Related: str-qwua7.46, str-35vtk.36, str-35vtk.8.
- Source findings: gates-06, tests-ci-05, frontend-rust-07 (selector part). Drafts shatter-code/04 and shatter-agent/20 (selector half).

---

<!-- file: 05-fast-hermetic-precommit.md -->

---
slug: fast-hermetic-precommit
kind: new
title: "Pre-commit hook runs the full shatter-core/shatter-cli test suite (incl. E2E) on every commit: make it fast (<=30 s) and hermetic, and stop re-gating the same tree 3-4 times"
priority: P1
type: task
labels: [git-hooks, quality-gates, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Pre-commit hook runs the full shatter-core/shatter-cli test suite on every commit: make it fast and hermetic

## Problem

On every commit the pre-commit hook runs `cargo test` for all of shatter-core and/or shatter-cli, which is 3,345 core lib tests plus the integration and E2E suites, and then clippy. The pre-push hook then runs `task affected`, and the land verifier and the pre-push `task check` on main run the gates again. The same tree is gated three or four times. Hook cost has no budget and no measurement. str-npdt (closed) added the pre-commit test run without a latency target, and the gate-dedup epic str-35vtk never covered pre-commit. Hook failures unrelated to the diff (an unbuilt TS dist in fresh worktrees, ambient `/tmp/.shatter` config, Go build timeouts under load) came right before most of the commits that skipped hooks in recent sessions.

The goal is a hook that is fast and deterministic enough that nobody needs to skip it. This issue must **not** add or document any way to bypass hooks (maintainer decision D4, 2026-09-23). The separate beads post-checkout hook stall is handled by the D4 beads issues (`beads-jsonl-import-clobber-check`, `beads-retire-jsonl-import-dolt-remote`) and is out of scope here.

## Evidence (re-verified 2026-09-23 against the audit snapshot, unchanged on main)

- `scripts/precommit-rust.sh:28-37` runs `cargo test -p shatter-core` and/or `-p shatter-cli` (the full suite, integration tests included) at `:33`, then `cargo clippy ... -D warnings` at `:34`. It also runs `cargo test` plus clippy in `shatter-rust` (`:36`) and `shatter-rust-runtime` (`:37`) when those crates change.
- `scripts/setup-hooks.sh:92-94` installs `precommit-rust.sh` as the pre-commit body. The pre-push body (`:96-190`) runs `task affected`, or `task check` for main, directly.
- Neither hook nor `precommit-rust.sh` goes through bento's `run-heavy` load regulator (see bento `docs/specs/2026-06-18-run-heavy-load-regulation-design.md`) or `scripts/gate-wrapper.sh`'s machine-wide semaphore. A grep for `run-heavy` in `scripts/` finds nothing.
- Session measurements since 2026-09-04 (audit finding sessions-04): a commit with hooks takes a median of 60 s (p90 124 s) against 2 s without hooks. A push with hooks takes a median of 80 s in the foreground and 308 s in the background, with a maximum of 728 s. The land verifier then re-runs the gates on the merge preview (land.py median about 780 s).
- Hook failures unrelated to the diff:
  - 9f13ca23, 2026-09-19T14:09: `TypeScript frontend not built: .../shatter-ts/dist/main.js does not exist` in a fresh worktree.
  - 87606e10, 2026-09-19T15:58: `discover_configs` tests failed on an ambient `/tmp/.shatter` (str-dl2pj).
  - b6375e4e: Go build timeout under a sustained load average of about 100-170.

## Acceptance criteria

- [ ] pre-commit runs only fast, hermetic checks on the staged crates (`cargo check` and/or `cargo clippy -D warnings`, and optionally `cargo fmt --check` on staged files). It runs no tests that need built frontends, examples checkouts or ambient config. Target: 30 s or less on a warm cache.
- [ ] Measured proof: the close reason records wall time for 5 representative commits (a core-only change, a cli-only change, a frontend-only change, a docs-only change, and a cold new worktree), before and after. The median is at most 30 s and none of them needs a built frontend.
- [ ] pre-push accepts a fresh verifier or affected receipt for the exact same tree (the mechanism tracked by str-35vtk.24/.25) instead of re-running the gate. If .24/.25 have not landed, this item is linked as blocked on them and the rest of this issue can close without it.
- [ ] Hook-invoked gates run under `run-heavy` or `gate-wrapper.sh`, so they respect the machine-wide slot.
- [ ] The per-hook budget (pre-commit ≤30 s, and the pre-push expectation) is documented in CONTRIBUTING.md and AGENTS.md. It is a budget, not bypass instructions.
- [ ] A test in `meta` asserts that `precommit-rust.sh` invokes no `cargo test` and no `cargo nextest`. It fails on today's script.

## Suggested approach

Replace `cargo test` in `precommit-rust.sh` with `cargo check`/`clippy` on the staged packages, and move tests to pre-push, where `task affected` already covers them. Wrap the pre-push `task` call in `run-heavy`. Wire receipt reuse once str-35vtk.24/.25 provide it. Coordinate with str-jttrf, which covers hook-run tests that leak `GIT_DIR`. Most of that fragility disappears once pre-commit stops running tests.

## Out of scope

- Any documented or scripted way to skip hooks (D4).
- The beads post-checkout JSONL import stall (the D4 beads issues in the shatter-tracker-and-beads bucket).
- Redesigning the landing verifier (`verifier-per-language-evidence`, str-qwua7.55).
- Unrelated refactors in the touched files.

## Metadata

- Priority: P1. Type: task. Size: M.
- Labels: git-hooks, quality-gates, audit.
- Parent epic: Epic: Audit 2026-09-22 findings.
- Blocked by: none. The receipt-reuse criterion depends on str-35vtk.24/.25.
- Related: str-35vtk.24, str-35vtk.25, str-jttrf, str-npdt, str-dl2pj.
- Source findings: sessions-04. Draft shatter-code/83.

---

<!-- file: 06-wire-every-test-module.md -->

---
slug: wire-every-test-module
kind: new
title: "Meta test: every scripts/test_*.py and demo/test_*.py must be run by a gate (8 modules, ~209 tests unwired today)"
priority: P2
type: task
labels: [testing, quality-gates, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Meta test: every scripts/test_*.py and demo/test_*.py must be run by a gate

## Problem

Adding a test file and wiring it into a gate are separate manual steps, and nothing checks the second step. Eight script test modules, about 209 test methods, are never run by `task check`, `task affected` or CI. They include the regression test that str-qwua7.7 added. Agents report "regression test added" and treat the work as done. The `meta` task lists its test modules one by one (`Taskfile.yml:449-466`), so every new module must be added there by hand.

## Evidence (re-verified 2026-09-23)

The following modules are referenced by no Taskfile, workflow or demo/scripts shell script. The check looked for the module basename in `Taskfile.yml`, `*/Taskfile.yml`, `.github/` and `demo/` or `scripts/` `*.sh`/`*.yml`. Test method counts come from `grep -c "def test_"`.

| Module | Tests | Note |
|---|---|---|
| `scripts/test_build_cache_doctor.py` | 57 | |
| `scripts/test_docs_smoke.py` | 54 | |
| `scripts/test_gate_pressure.py` | 30 | |
| `scripts/test_gate_event_log.py` | 25 | |
| `scripts/test_validate_parity.py` | 24 | includes the str-qwua7.7 regression test (4cf2165f) |
| `scripts/test_validate_protocol_registry.py` | 11 | only mentioned as a manual step at `protocol/GOVERNANCE.md:120-121` |
| `demo/test_gauntlet_check_output.py` | 6 | appears only as a path trigger at `scripts/affected-gates.py:177`; it also pins a dead output format (see `gauntlet-scan-checker-consumes-json`) |
| `scripts/test_perf_compare.py` | 2 | |

- `scripts/test_ci_workflow_structure.py` runs only in CI (`.github/workflows/ci.yml:110`), never locally. str-35vtk.35 (open) covers this one file.
- `scripts/test_broad_run_validation_gate` and `scripts/test_kapow_refute_agent` run only in non-gate tasks (`Taskfile.yml:922`, `:932`).
- `scripts/gate-event-log.py` and `scripts/gate-pressure.py` are invoked by nothing. By contrast, `scripts/test_gate_receipt.py` does run in `meta` (`Taskfile.yml:460`).
- `scripts/test_test_tier_wiring.py` already exists and runs in `meta` (`Taskfile.yml:466`). It is the natural home for the new check.

## Acceptance criteria

- [ ] A meta test, for example in `scripts/test_test_tier_wiring.py`, discovers `scripts/test_*.py`, `scripts/test_*.sh` and `demo/test_*.py`. It fails for any module that no Taskfile task reachable from `check` runs, whether through `python3 -m unittest scripts.<module>`, a direct path, or a discover pattern. An allowlist with a reason per entry is permitted, for example a module intentionally run only by a named non-gate task.
- [ ] Proof: the meta test fails on today's tree, listing the 8 modules above (record the output in the close reason), and passes after wiring.
- [ ] Each listed module is wired into `meta`, or into a better-fitting gate, and passes. A module can instead be deleted, with the reason in the commit. Failures uncovered by wiring are fixed or filed, with ids in the close reason. `demo/test_gauntlet_check_output.py` is wired as part of `gauntlet-scan-checker-consumes-json`. This issue only requires that it be reachable.
- [ ] `scripts/gate-event-log.py` and `scripts/gate-pressure.py` are either invoked by `gate-wrapper.sh` or the verifier, or deleted along with their tests.
- [ ] `protocol/GOVERNANCE.md:120-121` no longer describes the validator tests as a manual step.
- [ ] str-35vtk.35 is closed as subsumed, or completed here.

## Suggested approach

Replace the explicit list in `meta` with `python3 -m unittest discover -s scripts -p 'test_*.py'`, plus the same for `demo/`, and keep the meta test as a guard against future non-discoverable layouts. Alternatively, keep the explicit list and let the meta test enforce it.

## Out of scope

- Fixing test failures uncovered by wiring, beyond filing them.
- Rewriting the gauntlet checker (`gauntlet-scan-checker-consumes-json`).

## Metadata

- Priority: P2. Type: task. Size: S-M.
- Labels: testing, quality-gates, audit.
- Parent epic: Epic: Audit 2026-09-22 findings.
- Blocked by: none.
- Related: str-35vtk.35, str-35vtk.24, str-35vtk.25, str-qwua7.7.
- Source findings: tests-ci-10 (verifier correction applied: gate-receipt.py's tests do run in meta), protocol-parity-05. Draft shatter-agent/19.

---

<!-- file: 07-gauntlet-scan-checker-consumes-json.md -->

---
slug: gauntlet-scan-checker-consumes-json
kind: new
title: "Gauntlet scan-failure checker has matched nothing since 2026-05-13: consume scan JSON `failed[]`, regenerate test fixtures from the CLI, fix allowlist and CLAUDE.md"
priority: P1
type: bug
labels: [gauntlet, quality-gates, testing, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Gauntlet scan-failure checker has matched nothing since 2026-05-13; its tests pin the dead format

## Problem

The gauntlet's per-step output screen can no longer detect scan failures. Both regexes it relies on target output formats the CLI stopped producing. Its unit tests use hand-written strings in the old format, so they stay green, and nothing runs them anyway. A scan with 4 failed and 7 interrupted functions passes the checker. The Gauntlet-gate paragraph in CLAUDE.md describes a guarantee that does not hold. The checker and the CLI output share no contract, so changing a user-facing format triggered no check on downstream parsers.

Closed issues str-jeen.57 (gauntlet fails on scan FAIL/error rows) and str-jeen.59 (allowlist) delivered this check. It went dead five days after str-jeen.57 closed. A reopen-note on str-jeen.57 points here (`gauntlet-checker-reopen-note`). The open str-qwua7.10 covers bad sanity checks in the demo gates generally. This issue is the concrete fix for the scan-failure part.

## Evidence (re-verified 2026-09-23 against the audit snapshot, unchanged on main)

- `demo/gauntlet_check_output.py:30-35`: `SCAN_ERROR_SUMMARY_RE` requires `Scan complete: ... N error(s)`, and `FAIL_ROW_RE` requires `| FAIL |` markdown rows.
- Commit 00124c84 (2026-05-13, str-izhn) changed the scan summary to `Scan complete: {} completed, {} failed, {} unsupported, {} interrupted, {} skipped ({} worker(s))` (`shatter-core/src/scan_orchestrator.rs:6120`). Production code never emits `error(s)`.
- `shatter-core/src/report.rs:2302-2315` (str-4ad5) emits only PASS, WARN and LOW rows, and a test at `report.rs:4886` asserts `!md.contains("| FAIL |")`. As a result, every `outcome: FAIL` entry in `demo/gauntlet-scan-allowlist.yaml` (lines 29-64 and following) is dead as well.
- `demo/test_gauntlet_check_output.py:28-39` pins `Scan complete: **43 function(s)** tested, **0 skipped**, **2 error(s)**` and hand-written `| FAIL |` rows. No gate runs this module (see `wire-every-test-module`). It appears only as a path trigger at `scripts/affected-gates.py:177`.
- `expected_scan_errors` in the allowlist (`demo/gauntlet-scan-allowlist.yaml:108-112`) names `11-opaque-types.ts` and `12-external-deps.ts`, which are absent from the pinned examples checkout. CLAUDE.md's Gauntlet-gate paragraph (line 41) repeats those names and the dead "`FAIL` rows / `N error(s)`" description.
- There are four divergent copies of the step check: `demo/gauntlet.sh:353-374` (which calls the Python helper, with an inline regex fallback), `demo/walkthrough.sh:261`, `demo/walkthrough-docker.sh:144` and `demo/gauntlet-docker.sh:167`. The Docker gauntlet has no FAIL or summary check at all.
- Repro: `python3 demo/gauntlet_check_output.py --allowlist demo/gauntlet-scan-allowlist.yaml --output audits/2026-09-22/artifact-samples/scan-mix.stdout --step x` exits 0 with no output. That stdout reads `**1 completed**, **4 failed**, **0 unsupported**, **7 interrupted**`, and the matching `scan-mix.json` has `codebase.failed_functions == 4` and a non-empty `codebase.failed` array.

## Acceptance criteria

- [ ] The checker detects failed and interrupted scan functions from scan's JSON output (`--format json`: `codebase.failed_functions`, `codebase.failed[]`), or from a single documented machine-readable summary line. It no longer parses markdown prose. The gauntlet's scan steps emit that JSON, or write it to a file the checker reads.
- [ ] The checker's tests get fixtures from the real CLI: either by running it on a small example with a known-failing function, or from a checked-in fixture produced by the CLI with the regeneration command documented next to it. A test fails if the checker reports 0 failures on that fixture. The tests pinning `N error(s)` and `| FAIL |` are removed. `demo/test_gauntlet_check_output.py` is run by the `gauntlet` gate or by `meta`.
- [ ] Proof: the new test fails against the current checker, with the command and output recorded in the close reason, and passes after the rewrite. Running the checker on `audits/2026-09-22/artifact-samples/scan-mix.*` reports the 4 failed functions.
- [ ] The allowlist names only functions and files that exist in the pinned examples checkout, each with a tracker issue id. Stale entries (`11-opaque-types.ts`, `12-external-deps.ts`, dead `outcome: FAIL` rows) are removed or rewritten against the JSON fields.
- [ ] The CLAUDE.md Gauntlet-gate paragraph describes the new contract (JSON-based, allowlist semantics) and drops the removed fixture names.
- [ ] All demo scripts (`gauntlet.sh`, `gauntlet-docker.sh`, `walkthrough.sh`, `walkthrough-docker.sh`) use the one shared checker for scan steps.
- [ ] `task gauntlet` on current main either passes with an allowlist that matches reality, or fails listing the real failures. Record the evidence (run log path or excerpt) in the close reason.

## Suggested approach

Have each gauntlet scan step also write `--format json` output, for example with `-o <step>.json`. The checker loads it, diffs `codebase.failed[]` (function + file + reason) against the allowlist, and flags anything new. Keep `PROCESS_ERROR_RE` for process-level markers. Generate the test fixture with a documented `shatter scan --format json` command over a tiny example containing one deliberately failing function.

## Out of scope

- Fixing the underlying scan failures themselves.
- A CLI golden-output contract suite (`golden-and-consumer-suite`, shatter-reports-and-specs bucket, which is blocked by this issue).
- General demo-gate sanity checks beyond scan failures (str-qwua7.10).

## Metadata

- Priority: P1. This overrides the P2 in the report tables, per report §15.1 and §14 item 25. Type: bug. Size: M.
- Labels: gauntlet, quality-gates, testing, audit.
- Parent epic: Epic: Audit 2026-09-22 findings.
- Blocked by: none.
- Related: str-jeen.57 (closed), str-jeen.59 (closed), str-qwua7.10, str-izhn, str-4ad5.
- Source findings: artifacts-07. Draft shatter-agent/09.

---

<!-- file: 08-gauntlet-checker-reopen-note.md -->

---
slug: gauntlet-checker-reopen-note
kind: reopen-note
title: "Note on closed str-jeen.57: gauntlet scan-failure check has matched nothing since 2026-05-13"
priority: P1
type: bug
labels: [gauntlet, quality-gates, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [gauntlet-scan-checker-consumes-json]
existing_id: str-jeen.57
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on closed str-jeen.57: the scan-failure check it delivered has been dead since 2026-05-13

**Target:** closed issue `str-jeen.57`. **Action:** add the comment below. **Do not reopen.** The fix is tracked by the new issue `gauntlet-scan-checker-consumes-json`. The filer substitutes that issue's real id for `<gauntlet-scan-checker-consumes-json>`, which is why `blocked_by` lists it: it must be filed first.

## Comment text (post verbatim, after substituting the new id)

> **Audit 2026-09-22 (finding artifacts-07): this check has matched nothing since 2026-05-13.**
>
> The gauntlet's scan-failure screen that this issue delivered, together with the allowlist from str-jeen.59, can no longer fire:
> - `demo/gauntlet_check_output.py` `SCAN_ERROR_SUMMARY_RE` requires `Scan complete: ... N error(s)`. Commit 00124c84 (2026-05-13, str-izhn) changed the summary to `Scan complete: N completed, M failed, ... interrupted, ...` (`shatter-core/src/scan_orchestrator.rs:6120`). Production code never emits `error(s)`.
> - `FAIL_ROW_RE` requires `| FAIL |` rows. `shatter-core/src/report.rs:2302-2315` (str-4ad5) emits only PASS, WARN and LOW, and a test at `report.rs:4886` asserts no `| FAIL |` row. That makes every `outcome: FAIL` allowlist entry dead too.
> - `demo/test_gauntlet_check_output.py` still pins the old format, and no gate runs it.
> - Repro: the checker exits 0 on a scan stdout reporting 4 failed and 7 interrupted functions (`audits/2026-09-22/artifact-samples/scan-mix.stdout`).
>
> This issue stays closed. The fix (consume scan JSON `codebase.failed[]`, fixtures generated by the CLI, allowlist and CLAUDE.md cleanup, one shared checker) is tracked in **<gauntlet-scan-checker-consumes-json>**. Related: str-jeen.59 (allowlist, closed) and str-qwua7.10 (open, demo gate sanity checks).

## Filer notes

- Also add a one-line comment on str-qwua7.10: "Scan-failure part of the demo-gate sanity problem is filed as <gauntlet-scan-checker-consumes-json> (audit 2026-09-22, artifacts-07)."
- str-jeen.59 needs no separate comment. The note above names it.

---

<!-- file: 09-verifier-per-language-evidence.md -->

---
slug: verifier-per-language-evidence
kind: new
title: "Landing evidence must cover each changed language: land verifier runs no TS/Go/rust-fe tests, hides output, reports no executed flag; /pre-completion has no coverage or PBT row"
priority: P2
type: task
labels: [landing, quality-gates, agents, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Landing evidence must cover each changed language: the verifier runs no TS/Go/rust-fe tests and hides output

## Problem

The land-work verifier gates landings with checks that do not cover the changed code, and it throws their output away. `/pre-completion` has no row checking that the gate evidence covers each changed language or crate. It also has no property-test row, even though CLAUDE.md Completion Checklist item 2 requires one, and it never says that `Task "X" is up to date` means the leaf did not run. Together with Task checksum caching, a landing can go green without the relevant tests running. There are three definitions of "the gate" (verifier, CI, pre-completion), and nothing checks free-text close reasons against the diff.

Related: str-qwua7.55 (open) covers the verifier's false "same gates as CI" claim and a verifier.json timeout. str-35vtk.24 (open) makes the verifier run one exact `task check`. str-qwua7.2 (open) covers executed-vs-cached receipts. bento already ships per-check `executed` (bento-rdtn.6) and verifier log persistence (bento-rdtn.4), but shatter has not adopted them. This issue owns the per-language coverage rows, the output and `executed` reporting, and the adoption of rdtn.4 and rdtn.6.

## Evidence (re-verified 2026-09-23 against the audit snapshot, unchanged on main)

- `scripts/land_work_verifier.sh`, last changed in d01a22db on 2026-08-05:
  - Header (`:3`) claims "Runs the same gates ci.yml uses". CI runs `task check` plus the shatter-llm steps (`.github/workflows/ci.yml:88-103`).
  - It runs `task test-standard`, `task parity` and `task conformance` only (`:25-27`).
  - `run_check` (`:14-23`) runs each check as `"$@" >/dev/null 2>&1`, so bento-rdtn.4's persisted log is empty on failure.
  - Per check it emits only `{name, status}`: no `executed`, no `wall_seconds`. bento's all-cached guard therefore cannot engage.
- `test-standard` (`Taskfile.yml:103-111`) = `frontends-built` + `workspace-clippy` + `workspace-test` (`cargo test` for core, cli and llm). It runs no `ts:test`, `go:test`, `rust-fe:test` or `rust-rt:test`.
- `.agent-plugins/bento/bento/land-work/verifier.json` has only `schema_version` and `command`, with no timeout.
- `.claude/skills/pre-completion/SKILL.md`: a grep for `property`, `PBT` or `up to date` finds nothing.
- Sessions since 2026-09-04 saw 65 `task ... is up to date` results in 24 sessions (finding agent-repo-11).

## Acceptance criteria

- [ ] `/pre-completion` (`.claude/skills/pre-completion/SKILL.md`) adds three rows:
  1. Gate evidence covers every language or crate in the diff, using a mapping table: shatter-core and shatter-cli → core:test-ignored/cli:test, shatter-ts → ts:test, shatter-go → go:test, shatter-rust → rust-fe:test, shatter-rust-runtime → rust-rt:test + rust-fe:test, shatter-llm → llm tests.
  2. Property-test adequacy per CLAUDE.md Completion Checklist item 2.
  3. Any `Task "X" is up to date` line for a required leaf means the leaf did not run. Re-run it with `--force` or a fresh `TASK_TEMP_DIR` before claiming it.
- [ ] `scripts/land_work_verifier.sh` tees each check's output to stdout/stderr (bento-rdtn.4 adoption). For each check it emits `executed`, parsed from Task's `is up to date` lines (bento-rdtn.6 adoption), and `wall_seconds`. It also either runs `task affected` for the landing diff, or has its header and CLAUDE.md corrected to say what it actually runs.
- [ ] `verifier.json` declares a timeout.
- [ ] A unit test wired into `meta` runs `run_check` against a stub command that prints `Task "cli:test" is up to date`. It asserts `executed: false` and that the output reaches stdout. The test fails on today's script and passes after.
- [ ] Proof: one real landing after the change shows per-check `executed` and `wall_seconds` in the verifier's final JSON line. Paste the line into the close reason.

## Suggested approach

Rewrite `run_check` to run the command through `tee` into a temp log, `grep -c 'is up to date'` the log to set `executed`, and time the run. Add the three `/pre-completion` rows with the mapping table. Decide with str-35vtk.24 whether the verifier runs `task affected` or one exact `task check`, and do not duplicate that work here.

## Out of scope

- The root-cause fix for meta-stage checksum poisoning (str-qwua7.3).
- The full receipt system (str-qwua7.2, str-35vtk.24/.25).
- bento-side verifier changes (already shipped in bento-rdtn.4 and bento-rdtn.6).

## Filer note (companion one-liner on str-qwua7.55)

Do not file a separate note for finding agent-repo-11, which duplicates open str-qwua7.55. Add this one line on str-qwua7.55 instead:

> Audit 2026-09-22 (agent-repo-11): the output-tee, per-check `executed`/`wall_seconds` (bento-rdtn.4/.6 adoption) and per-language evidence rows are tracked in <verifier-per-language-evidence>.

## Metadata

- Priority: P2. Type: task. Size: M.
- Labels: landing, quality-gates, agents, audit.
- Parent epic: Epic: Audit 2026-09-22 findings.
- Blocked by: none.
- Related: str-qwua7.55, str-35vtk.24, str-qwua7.2, bento-rdtn.4, bento-rdtn.6.
- Source findings: tests-ci-06 (verifier correction applied: the str-qwua7.4 example was dropped because the changed Go test was run directly), with agent-repo-11 as context. Draft shatter-agent/18.

---

<!-- file: 10-gate-telemetry-executed-vs-cached.md -->

---
slug: gate-telemetry-executed-vs-cached
kind: new
title: "Gate telemetry can't tell executed from cached runs (budgets computed from no-ops); sccache installed but unused; machine-wide slot covers only shatter gates"
priority: P2
type: task
labels: [quality-gates, performance, sccache, resource-governance, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [task-list-json-poisons-checksums]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Gate telemetry can't tell executed from cached runs; sccache is installed but unused; the machine-wide slot covers only shatter gates

## Problem

Between 18% and 28% of recorded gate runs fail, and a cold full check takes more than 40 minutes on the shared machine under contention. Meanwhile the recorded median `check` wall time mostly reflects cached no-ops. The budgets in `docs/perf/gate-budgets.md` are computed from rows that mix leaves that executed with leaves that were cached, so both slowness and hollowness are invisible in the telemetry. Governance was designed per project, so other agents' cargo and jest runs are not bounded by the shatter semaphore.

## Evidence (re-verified 2026-09-23)

- `~/.cache/shatter/gate-times.csv` (since 2026-08-10; columns `timestamp,worktree,label,wall_seconds,exit_code,loadavg_1min,slot,wait_seconds`, per `scripts/gate-wrapper.sh:9-10,55`): non-zero exits are affected 44/156, check-fast 36/151 and check 26/143. There is no executed-vs-cached column.
- In the audit run, workspace clippy alone took 14m48s and 22m01s at load 100-190, and stage 2 alone took 1558 s (finding gates-11; not re-timed).
- `/usr/bin/sccache` is installed, `RUSTC_WRAPPER` is unset in the environment, and `.cargo/config.toml` has no `rustc-wrapper`. str-35vtk.2 (closed) documented a host-level sccache setup and explicitly rejected a shared `CARGO_TARGET_DIR`.
- The `scripts/gate-wrapper.sh` semaphore (`SHATTER_HEAVY_SLOTS` flock slots, header `:1-15`) governs shatter gates only. Other projects' cargo and jest runs, and shatter work outside gate-wrapper (hooks, see `fast-hermetic-precommit`), are unbounded.

## Acceptance criteria

- [ ] `gate-wrapper.sh` records, for each leaf, whether it executed or was cached (parsed from Task's `Task "X" is up to date` lines, or from a flag passed by the task) as a new CSV column. The format change is documented in the script header.
- [ ] A unit test wired into `meta` feeds the wrapper's parser a sample log with one cached and one executed leaf and asserts the column values.
- [ ] `docs/perf/gate-budgets.md` is recomputed from executed rows only, by a committed script. The doc names the script and the date range used.
- [ ] Either sccache is enabled for gate runs, with a measured before/after cold `task check-unit` time recorded in AGENTS.md, or the reason for not enabling it is recorded in this issue's close reason.
- [ ] Either gate-wrapper's slot is honoured by bento's `run-heavy` (or vice versa), so cross-project heavy runs share one budget, or this is explicitly deferred to bento-dyp7 in the close reason.

## Suggested approach

Have gate-wrapper tee the wrapped command's output (it currently runs it without capturing output), parse `is up to date` lines from it, and add the column. Write `scripts/gate-budgets.py` to derive budgets from executed rows. Try `RUSTC_WRAPPER=sccache` in gate-wrapper, or in `.cargo/config.toml` behind an env guard, and measure. Do this after str-qwua7.3's fix lands. Before then, nearly every stage-2/3 row is a cached no-op and the recomputed budgets would be meaningless.

## Out of scope

- Host-wide admission control across projects (bento-dyp7), beyond the interoperability decision above.
- Receipts proving gates ran (str-qwua7.2, str-35vtk.24/.25).
- The CI guard (`ci-executed-leaf-guard`).

## Metadata

- Priority: P2. Type: task. Size: S-M.
- Labels: quality-gates, performance, sccache, resource-governance, audit.
- Parent epic: Epic: Audit 2026-09-22 findings.
- Blocked by: task-list-json-poisons-checksums (the str-qwua7.3 fix).
- Related: str-35vtk.10, str-35vtk.24, str-35vtk.2, str-0f6ze, bento-dyp7.
- Source findings: gates-11. Draft shatter-code/10.
