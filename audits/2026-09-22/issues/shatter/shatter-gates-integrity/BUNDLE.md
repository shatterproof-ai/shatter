# Bundle: shatter-gates-integrity (audit 2026-09-22)

- **Bucket:** shatter-gates-integrity. Theme: gates must prove they executed. Covers Task checksum poisoning, sources, affected routing, pre-commit, verifier evidence, the dead gauntlet checker and unwired test modules.
- **Repo / tracker:** shatter, bd in /home/ketan/project/shatter (prefix str).
- **Parent epic:** Epic: Audit 2026-09-22 findings.
- **Status:** revised drafts after the Codex cross-check (2026-09-23; see REVISION.md). Nothing is filed. Evidence was re-verified on 2026-09-23 against main `70465921` and the audit snapshot (`56c86168`). Main differs from the snapshot in `shatter-core/Taskfile.yml`, `shatter-core/src/cache.rs` and `shatter-core/src/scan_orchestrator.rs` (str-6nul9, str-8q1b4); citations into those files are qualified by commit. Audit artifacts under `audits/2026-09-22/` live on branch `audit-2026-09-22` (retrieve with `git show 56c86168:<path>`); `*.log` files there are gitignored and local-only.

## Maintainer decisions (2026-09-23). These override the report and the old drafts.

- **D1 Releases:** keep Windows (x86_64-pc-windows-msvc) and aarch64-unknown-linux-gnu in the release matrix and fix them (Z3 header or static link on Windows; openssl-sys under cross for aarch64). Do not drop them. Release work closes only with a green release-run URL.
- **D2 shatter diff:** retire the snapshot-diff command (`shatter diff`) and the unused Snapshot writer path. spec-diff is the regression tool. Update SPEC, README and QUICKSTART. The `diff` name becomes free, and whether str-81xiw takes it is left to that epic. Correct the shatter-agents plugin's `shatter diff --staged` docs to describe what exists today.
- **D3 Concolic positioning:** measure first. P1: a controlled default-vs-concolic benchmark, reported per release. P1: fix concolic early termination. A follow-up decision issue, blocked by both, re-decides the "concolic-first" positioning. No doc softening now.
- **D4 Beads hook stall:** retire the JSONL import and move tracker sync to a Dolt remote. The first step checks whether the stale import has been clobbering newer DB state. AGENTS.md drops `bd sync`. str-qwua7.28 is superseded. bento's beads-issue-flow gets matching guidance. No BEADS_HOOK_TIMEOUT env var and no hook-bypass guidance anywhere.
- **D5 Git identity:** the leaked `[user]` section was already removed. Add a `.mailmap`, a git-state check (folded into str-qwua7.1), and a before/after `.git/config` snapshot in test_git_fixture_isolation.py.
- **D6 Filing:** after reconciliation and the Codex cross-check, the maintainer runs one filer script. Agents file nothing.

Decision touching this bucket: **D4**. `fast-hermetic-precommit` makes the hook fast and contains no bypass guidance; no draft in this bucket adds a timeout environment variable (the verifier timeout in `qwua7-55-verifier-timeout-note` is a command argument). **D6**: nothing here is filed by agents.

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
| 08 | gauntlet-checker-reopen-note | reopen-note | str-jeen.57 | P1 | gauntlet-scan-checker-consumes-json |
| 09 | verifier-per-language-evidence | new | - | P2 | ci-executed-leaf-guard |
| 10 | gate-telemetry-executed-vs-cached | new | - | P2 | task-list-json-poisons-checksums, ci-executed-leaf-guard |
| 11 | qwua7-55-verifier-timeout-note | note-to-existing | str-qwua7.55 | P2 | verifier-per-language-evidence |
| 12 | qwua7-2-scope-note | note-to-existing | str-qwua7.2 | P1 | ci-executed-leaf-guard |
| 13 | ci-first-real-run-triage | new | - | P1 | task-list-json-poisons-checksums, ci-executed-leaf-guard |
| 14 | sccache-for-gate-runs | new | - | P3 | task-list-json-poisons-checksums |

`task-list-json-poisons-checksums` as a blocker means the fix tracked on str-qwua7.3. Notes whose `blocked_by` names a new issue (08, 11, 12) list it so the filer files that issue first and substitutes its id.

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

**Target:** open issue `str-qwua7.3` ("Explain why stage-2/3 tasks report 'up to date' right after .task/checksum is deleted", P1; verified open with `bd show` on 2026-09-23).
**Action:** post the comment below on str-qwua7.3, then widen its acceptance criteria as shown. Keep str-qwua7.3 at P1. Do not file a new issue. The follow-ups `ci-executed-leaf-guard`, `ci-first-real-run-triage`, `gate-telemetry-executed-vs-cached` and `sccache-for-gate-runs` in this bucket are blocked by str-qwua7.3.

## Comment text (post verbatim)

> **Root cause found (audit 2026-09-22, findings gates-01 and tests-ci-01).**
>
> **Problem.** `task check` exits 0 in a fresh worktree, but no stage-2 or stage-3 test leaf runs: each one prints `Task "X" is up to date`. The cause is the meta-stage unit test `test_every_emitted_gate_is_a_real_task`. It runs `task --list-all --json` with cwd set to the repo root. On go-task 3.50 and 3.53, computing the JSON `up_to_date` field writes `.task/checksum/*` for every task that has `sources:`. Stage 1 runs `meta` first, so stages 2 and 3 find fresh checksums and skip their work. That is why deleting `.task/checksum` did not force the leaves to execute, which is the question this issue asked.
>
> **Evidence.** Re-verified 2026-09-23. The Taskfile and script citations below are identical on main `70465921` and on the audit snapshot `56c86168`. (Main has since changed `shatter-core/Taskfile.yml`, `shatter-core/src/cache.rs` and `shatter-core/src/scan_orchestrator.rs`; none of those is cited here.)
> - `scripts/test_affected_gates.py:203-212` (`test_every_emitted_gate_is_a_real_task`) runs `subprocess.run(["task", "--list-all", "--json"], cwd=ROOT, ...)`.
> - `Taskfile.yml:459` runs that module in `meta` (`python3 -m unittest scripts.test_affected_gates`). `meta` is a dep of `check-static` (`Taskfile.yml:535-546`), which runs before `check-unit` (`:548`) and `check-integration` (`:564`).
> - Repro with go-task 3.50.0 in a `git archive` copy of HEAD: `task --list-all` writes 0 checksum files, while `task --list-all --json` writes 39 (core-test-ignored, go-test, ts-test, cli-test, rust-fe-test, conformance, parity, ...). After that, `task sub:test` prints `is up to date` even when a source file's contents have been edited. Adding `--dry`, or running with `TASK_TEMP_DIR=$(mktemp -d)`, avoids the writes to the repo's `.task`. A scratch repro with `includes: sub: {taskfile: ./sub, dir: ./sub}` behaves the same way.
> - Durable CI evidence: in run 35756993223 (task v3.53.1), cli:test, conformance, core:test-ignored, docs-smoke, go:test, go:vet, parity, rust-fe:test, rust-rt:test, ts:install and ts:test all report up to date. All 4 `test result:` lines in the log come from the separate shatter-llm step. Retrieve with `gh run view 35756993223 --log --repo <shatter remote>`.
> - Local evidence: the poisoned cache is committed on the audit branch; retrieve it with `git show 56c86168:audits/2026-09-22/gates/poisoned-task-cache-snapshot/checksum/cli-test` (and siblings; `git ls-tree 56c86168 audits/2026-09-22/gates/poisoned-task-cache-snapshot/checksum/` lists them). The audit's `check.log` is gitignored and exists only on the audit machine; it is corroborating, not required.
> - Introduced 2026-08-29 in 8ae14f13 and b12e3054 (str-35vtk.8). CI wall time dropped from about 330-613 s to 154-225 s from 2026-08-30 onward.
>
> **Fix direction.** Remove the side effect from the meta stage. The simplest options are to run the listing with `TASK_TEMP_DIR` pointed at a throwaway `mktemp -d`, or to add `--dry`. Parsing the Taskfile YAML also works. Then add a regression test to `meta` that snapshots `.task/checksum` before and after it runs.
>
> **Effect on str-qwua7.2.** str-qwua7.2's `check-fresh` design (delete `.task/checksum`, then run) was itself defeated by this bug, because `meta` re-poisoned the cache after the deletion. Once this fix lands, `check-fresh` needs no further adjustment for this cause. The CI part of str-qwua7.2 is being moved to the audit issue `ci-executed-leaf-guard` (see the companion note on str-qwua7.2).
>
> **Widened acceptance for this issue** (replaces "explain why"):
> 1. No meta-stage command mutates the repo's `.task/`: no `task --list-all --json` against the live tree without `--dry` or an isolated `TASK_TEMP_DIR`. A grep test in `meta` fails if a new unguarded `task --list*` call appears in `scripts/test_*.py` or `demo/test_*.py`.
> 2. A new regression test, wired into `meta`, copies the repo (or a minimal Taskfile fixture with `includes:` and a `sources:` leaf) into a temp dir, runs the listing helper there, and asserts that `.task/checksum` has no new or changed entries. It then edits the leaf's source file **contents** (not just its mtime; Task's checksum method ignores mtime) and asserts the leaf runs its command (`task: [<leaf>] ...` echo line present, no `is up to date` line). It must fail on the current tree and pass after the fix. Record both runs (command + tail of output) in the close reason.
> 3. Execution proof on the real repo, independent of whether the tests pass: in a fresh worktree, `rm -rf .task && task meta`, then run each stage-2 and stage-3 test leaf **individually** (`task <leaf>` for cli:test, core:test-ignored, ts:test, go:test, go:vet, rust-fe:test, rust-rt:test, conformance, parity). For each, the output shows the leaf's `task: [<leaf>]` command echo and no `Task "<leaf>" is up to date` line. A leaf whose tests fail still counts as executed for this criterion. Attach the per-leaf echo lines (or a log path) to the close reason. Because `check` is staged and stops at the first failing stage, a full `task check` run is **not** required here; getting it green is `ci-first-real-run-triage`.
> 4. Optional: file an upstream go-task issue describing the `--list-all --json` side effect and record its link here.
>
> **Out of scope here.** The CI executed-leaf guard (`ci-executed-leaf-guard`) and triage of test failures that surface once leaves run (`ci-first-real-run-triage`). Executed-vs-cached receipts for the verifier and pre-completion (str-qwua7.2). Missing `sources:` globs (`task-sources-cover-real-inputs`).

## Filer notes

- Priority stays P1. Add the labels `quality-gates, ci, taskfile, audit` if they are missing.
- Add a `related` link to str-qwua7.2 and str-35vtk.8.
- Add a parent or related link to the epic "Epic: Audit 2026-09-22 findings" if the tracker allows a second parent. Otherwise mention the epic in the comment only.
- In the comment, replace the backticked slugs `ci-executed-leaf-guard` and `ci-first-real-run-triage` with their filed ids. Those issues are blocked by str-qwua7.3, so this note has no `blocked_by`; post it after they are filed in the same filer run.

---

<!-- file: 02-ci-executed-leaf-guard.md -->

---
slug: ci-executed-leaf-guard
kind: new
title: "CI 'Full landing gate' has run no product tests since 2026-08-29: require positive per-leaf execution evidence for an explicit expected leaf set"
priority: P1
type: bug
labels: [ci, quality-gates, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [task-list-json-poisons-checksums]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# CI 'Full landing gate' has run no product tests since 2026-08-29: require positive per-leaf execution evidence

## Problem

Because of the meta-stage checksum poisoning (root cause recorded on str-qwua7.3; see the `task-list-json-poisons-checksums` note), the CI `task check` step has run no unit, integration, E2E, conformance or parity tests since about 2026-08-29. Every merge to `main` since then passed on a hollow green. Nothing in CI checks that the leaves it depends on actually ran, so the next caching bug, a renamed task, or a leaf dropped from the `check` graph would go unnoticed the same way.

A guard that only rejects `is up to date` lines is not enough. It still passes when a required leaf disappears from the task graph entirely, or when a leaf's command never started. The guard must require **positive evidence** that each leaf in an explicit expected set executed.

Ownership: str-qwua7.2 (open, P1) lists "`ci.yml:89` calls `task check-fresh`" and "a live run asserts no `Task "<leaf>" is up to date` line" among its acceptance checks. This issue takes over that CI item (a companion note on str-qwua7.2, `qwua7-2-scope-note`, records the transfer). str-qwua7.2 keeps the verifier and `/pre-completion` receipts. The parser built here is the shared one those parts, and `gate-telemetry-executed-vs-cached`, reuse.

str-35vtk.21 ("CI runs full landing gate") was closed on run 33277936601, and the guarantee it recorded no longer holds.

## Evidence (re-verified 2026-09-23 on main `70465921`)

- `gh run view 35756993223 --log` shows `Task "go:test" is up to date`, and the same for `core:test-ignored`, `conformance`, `parity`, `cli:test`, `rust-fe:test`, `rust-rt:test`, `ts:test` and `go:vet`. All `test result:` lines come from the separate shatter-llm steps (`.github/workflows/ci.yml:91-103`).
- CI job durations on main were 330-613 s from 2026-08-10 to 08-27, and 154-225 s from 2026-08-30 onward.
- `.github/workflows/ci.yml:88-89` runs `task check`. Nothing inspects whether its leaves executed. `scripts/test_ci_workflow_structure.py` (run only in CI, `ci.yml:110`) checks workflow shape only.
- Task prints a `task: [<leaf>] <command>` echo line for each command it runs (no leaf in `Taskfile.yml` or `*/Taskfile.yml` sets `silent:`), and `task: Task "<leaf>" is up to date` for a skipped one. Both forms appear in the audit's local `check.log` (gitignored, audit machine only).

## Acceptance criteria

- [ ] A script, for example `scripts/task_leaf_evidence.py`, takes a Task output log and an explicit expected leaf list, and classifies each expected leaf as `executed` (at least one `task: [<leaf>]` echo line and no `is up to date` line for it), `cached` (an `is up to date` line), or `missing` (neither). It exits non-zero unless every expected leaf is `executed`. The expected list for CI lives in one checked-in file (for example `scripts/ci-required-leaves.txt`), not in YAML.
- [ ] A unit test wired into `meta` feeds the script fixtures for each case and asserts the verdict: all executed → pass; one leaf cached → fail naming it; one expected leaf absent from the log → fail naming it; a mixed log where a dependency is cached but every required leaf executed → pass; a leaf that executed and then failed (non-zero Task exit) → the script reports `executed` and the CI step still fails on the Task exit code.
- [ ] A meta test asserts that every leaf named in the expected-leaf file exists as a task (parsed from the Taskfiles as YAML, **not** via `task --list-all --json`) and is reachable from `check`. Removing a leaf from `check` without updating the file fails `meta`.
- [ ] `.github/workflows/ci.yml` **replaces** the existing `task check` invocation (`:88-89`) with one step that runs `task check` once, tees the output to a log, preserves Task's exit code, and then runs the evidence script on the log. There is no second `task check` run. `scripts/test_ci_workflow_structure.py` asserts that shape.
- [ ] Proof the guard fails on hollow work: link a CI run on a throwaway branch where the guard fails. Produce it either by reverting the str-qwua7.3 fix on that branch, or by pre-seeding `.task/checksum` before `task check`. Paste the guard's failure output (naming the cached or missing leaves) into the close reason.
- [ ] Proof the guard passes on real work: link a CI run on main after the str-qwua7.3 fix where the guard reports every expected leaf `executed`. If that run is red because of real test failures, the guard proof still counts; getting it green is `ci-first-real-run-triage`.

## Suggested approach

1. Wait for the str-qwua7.3 fix.
2. Write the evidence script and its fixtures. Keep the parser small and pure so the verifier (str-qwua7.2) and gate telemetry can import it.
3. Change the CI step to `set -o pipefail; task check 2>&1 | tee check.log; rc=${PIPESTATUS[0]}; python3 scripts/task_leaf_evidence.py --expected scripts/ci-required-leaves.txt check.log; exit $(( rc != 0 ? rc : $? ))`, or an equivalent script.
4. On CI, cargo-nextest is absent, so `core:test-ignored` runs its `cargo test` fallback (`shatter-core/Taskfile.yml:61-66` on main). The echo line is emitted on either path.

## Out of scope

- Fixing the test failures that surface once leaves run (`ci-first-real-run-triage`).
- The verifier and `/pre-completion` receipts (str-qwua7.2, str-35vtk.24).
- Missing `sources:` globs (`task-sources-cover-real-inputs`). Gate telemetry (`gate-telemetry-executed-vs-cached`).
- Adding cargo-nextest or a CI nextest profile (`nextest-ci-profile-and-stale-parity-fallback`, shatter-ci-workflows bucket).
- Correcting the CLAUDE.md CI claims (`test-tier-docs-overstate-coverage`, shatter-docs bucket).

## Metadata

- Priority: P1. Type: bug. Size: M.
- Labels: ci, quality-gates, audit.
- Parent epic: Epic: Audit 2026-09-22 findings.
- Blocked by: task-list-json-poisons-checksums (the str-qwua7.3 fix).
- Related: str-qwua7.2 (CI item transferred here), str-35vtk.21, str-6nul9.
- Source findings: gates-02 (audit 2026-09-22).

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

## Evidence (re-verified 2026-09-23; the cited Taskfiles other than `shatter-core/Taskfile.yml` are identical on main `70465921` and the audit snapshot `56c86168`, and the `shatter-core/Taskfile.yml` lines below were checked on main)

- **cli:test**, at `shatter-cli/Taskfile.yml:16-24`, and **cli:test-fast** (`:35`ff) list only `src/**/*.rs`, `Cargo.toml`, `../Cargo.lock`, core `src/**/*.rs`, core `Cargo.toml` and `../.config/nextest.toml`. They miss:
  - `shatter-cli/tests/**` (30 entries)
  - `shatter-cli/templates/**`: askama templates `explore_fn.md` and `scan.md`, used at `render.rs:17,40`
  - `shatter-cli/build.rs`, which embeds shatter-ts (`build.rs:90`) and shatter-go (`build.rs:163`)
  - the embedded frontend trees `shatter-ts/src/**`, `shatter-go/**/*.go` and `shatter-core/build.rs`
- **core:test-ignored**, at `shatter-core/Taskfile.yml:37-45`, runs the Go, TS and Rust E2E suites (`--run-ignored all` / `--include-ignored`), but lists no frontend source trees (`shatter-ts/src`, `shatter-go`, `shatter-rust/src`, `shatter-rust-runtime/src`). `test-ignored-fast` has the same gap.
- **workspace-test** (test-standard), at `Taskfile.yml:115-128`, runs plain `cargo test --workspace` (`:134`). Every test in `e2e_concolic.rs`, `e2e_concolic_go.rs` and `e2e_concolic_rust.rs` is `#[ignore]`d, so it does **not** run the E2E suites (the `test-standard` comment at `Taskfile.yml:105-108` saying the Rust and Go E2E suites remain in this tier is stale). It still builds `shatter-cli`, whose `build.rs` embeds the TS and Go frontends, and runs the CLI integration tests that render `shatter-cli/templates/**`. It lists `shatter-cli/**/*.rs` and `shatter-llm/**/*.rs`, but no frontend sources and no `shatter-cli/templates/**`.
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
- [ ] Cache-invalidation proof, without `--force` (Task's default checksum method hashes file contents, so `touch` alone does not and should not invalidate it). After the fix and after str-qwua7.3's fix, in a fresh worktree:
  1. Prime: run `task cli:test` and `task parity` until each exits 0, then run each again and confirm it prints `Task "<leaf>" is up to date` (the cache is warm).
  2. Change **contents**: append a blank line or comment to `shatter-cli/templates/scan.md`, and to `protocol/parity-matrix.yaml`.
  3. Re-run `task cli:test` and `task parity`. Each shows its `task: [<leaf>]` command echo and no `is up to date` line.
  Repeat step 2-3 for one file per newly added glob class (a `shatter-cli/tests/` file, `shatter-cli/build.rs`, a `shatter-ts/src/` file for cli:test and core:test-ignored, `shatter-rust/tests/codegen_parity.rs` and a `shatter-rust-runtime/src/` file for rust-fe:test, `shatter-ts/jest.config.js` for ts:test). Revert the edits afterwards. Record the command transcript (or a log path) in the close reason. Each of these must print `is up to date` on the current tree, which is the before-state to record.

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

## Evidence (re-verified 2026-09-23 by calling `select_gates` directly; `scripts/affected-gates.py` is identical on main `70465921` and the audit snapshot)

| Path | `select_gates([path])` today | Missing |
|---|---|---|
| `shatter-cli/templates/scan.md` | `['docs']` | cli:test, cli:clippy, gauntlet |
| `README.md` | `['docs']` | docs-smoke |
| `shatter-ts/CLAUDE.md` (parity contract) | `['docs']` | parity, conformance |
| `shatter-core/src/report.rs` | `['smoke','core:clippy','core:test']` | cli:test |
| `shatter-ts/src/analyzer.ts` | `['smoke','ts:typecheck','ts:test','e2e-ts','parity','conformance']` | cli:test (build.rs embeds the TS frontend) |
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
- [ ] Table-driven cases in `scripts/test_affected_gates.py` cover every row of the table above, using paths that exist in the tree (a test asserts each case path exists, so the table cannot drift to phantom files). They fail on today's tree (record this in the close reason) and pass after the fix. Any new task-name lookup in those tests must not run `task --list-all --json` against the live tree (str-qwua7.3).

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
title: "Pre-commit hook runs the full shatter-core/shatter-cli test suites on every commit: make it fast (<=30 s warm) and hermetic, with a measured budget"
priority: P1
type: task
labels: [git-hooks, quality-gates, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Pre-commit hook runs the full shatter-core/shatter-cli test suites on every commit: make it fast and hermetic

## Problem

On every commit that stages Rust changes, the pre-commit hook runs `cargo test` for all of shatter-core and/or shatter-cli (3,345 core lib tests plus the integration test binaries), then clippy. The pre-push hook then runs `task affected` (or `task check` for main), and the land verifier runs gates again on the merge preview. Pre-commit has no latency budget and no measurement. str-npdt (closed) added the pre-commit test run without a latency target, and the gate-dedup epic str-35vtk never covered pre-commit. Pre-commit failures unrelated to the diff (an unbuilt TS dist in fresh worktrees, ambient `/tmp/.shatter` config, Go build timeouts under load) came right before most of the commits that skipped hooks in recent sessions.

The goal is a pre-commit hook fast and deterministic enough that nobody needs to skip it. This issue must **not** add or document any way to bypass hooks (maintainer decision D4, 2026-09-23). The beads post-checkout hook stall is handled by the D4 beads issues (`beads-jsonl-import-clobber-check`, `beads-retire-jsonl-import-dolt-remote`) and is out of scope here.

## Evidence (re-verified 2026-09-23; the cited scripts are identical on main `70465921` and the audit snapshot)

- `scripts/precommit-rust.sh:32-37` runs `cargo test -p shatter-core` and/or `-p shatter-cli` at `:33`, then `cargo clippy ... -D warnings` at `:34`. It also runs `cargo test` plus clippy in `shatter-rust` (`:36`) and `shatter-rust-runtime` (`:37`) when those crates change. The E2E suites are all `#[ignore]`d, so they do not run here; the cost is the lib and integration tests.
- `scripts/setup-hooks.sh:92-94` installs `precommit-rust.sh` as the pre-commit body. It calls cargo directly, outside `scripts/gate-wrapper.sh`'s machine-wide semaphore. The pre-push body (`:96-190`) runs `task affected` or `task check`, and both of those Taskfile entries already run through `gate-wrapper.sh` (`Taskfile.yml:498`, `:514`). Only pre-commit is ungoverned.
- `shatter-cli/build.rs` runs `npm install`/`npm run bundle` for shatter-ts (`:113-117`) and `go build` for shatter-go (`:196-204`), with per-file `rerun-if-changed`. Any `cargo check`, `clippy` or `test` of shatter-cli therefore does frontend build work on a cold target dir, or when frontend sources changed. Replacing `cargo test` with `cargo check`/`clippy` removes the test cost, not the build-script cost.
- Session measurements since 2026-09-04 (audit finding sessions-04): a commit with hooks takes a median of 60 s (p90 124 s) against 2 s without hooks.
- Hook failures unrelated to the diff:
  - 9f13ca23, 2026-09-19T14:09: `TypeScript frontend not built: .../shatter-ts/dist/main.js does not exist` in a fresh worktree (a test that needs a prebuilt dist).
  - 87606e10, 2026-09-19T15:58: `discover_configs` tests failed on an ambient `/tmp/.shatter` (str-dl2pj).
  - b6375e4e: Go build timeout under a sustained load average of about 100-170.

## Definitions used below

- **Warm:** the worktree's cargo target dir already holds a build of the parent commit (produced by running the new pre-commit command once on `HEAD`), and no frontend source changed. On warm runs `build.rs` must not re-run npm or go.
- **Cold:** a fresh linked worktree with an empty target dir. `build.rs` frontend bundling is expected and allowed here; what is not allowed is a dependency on artifacts that some *other* command had to build first (for example a prebuilt `shatter-ts/dist/main.js`), on ambient config, or on an examples checkout.

## Acceptance criteria

- [ ] `precommit-rust.sh` runs no `cargo test` and no `cargo nextest`, only `cargo clippy -D warnings` (which subsumes `cargo check`) on the staged packages, plus optionally `cargo fmt --check` limited to staged files. A test in `meta` asserts the script contains no `cargo test`/`cargo nextest` invocation; it fails on today's script.
- [ ] The hook depends on no prebuilt artifacts, ambient config, or examples checkout. Proof: in a cold worktree with `HOME` and `TMPDIR` pointed at empty temp dirs, stage a one-line shatter-cli change and commit; the hook passes. Record the wall time (not held to the 30 s budget).
- [ ] Measured budget on warm runs: record wall time before and after for 5 commits (a core-only change, a cli-only change, a shatter-rust-only change, a frontend-only change, a docs-only change) on warm worktrees, with the load average at start. The after-median is at most 30 s. The close reason includes the exact commands used to prime and measure, so the numbers can be reproduced.
- [ ] Pre-commit's cargo invocation runs under `scripts/gate-wrapper.sh` (with its own label, for example `precommit`) or bento's `run-heavy`, so it takes a machine-wide slot like the pre-push gates. A `meta` test asserts this.
- [ ] The per-hook budget (pre-commit ≤30 s warm, and the existing pre-push behaviour) is documented in CONTRIBUTING.md and AGENTS.md. It is a budget, not bypass instructions.
- [ ] The tests removed from pre-commit remain covered on pre-push: for each crate `precommit-rust.sh` used to test, `select_gates` in `scripts/affected-gates.py` selects that crate's test leaf for a path in it. A `meta` assertion (or existing cases in `scripts/test_affected_gates.py`, cited in the close reason) proves it.

## Suggested approach

Replace the `cargo test` lines in `precommit-rust.sh` with `cargo clippy -p <staged pkgs> -- -D warnings` wrapped in `gate-wrapper.sh precommit`. Leave tests to pre-push's `task affected`. Keep `rerun-if-changed` precise so warm runs skip the frontend build. str-jttrf (closed, 2026-09-12) fixed the `GIT_DIR` leak for hook-run tests; with tests out of pre-commit, that class of failure disappears from this hook.

## Out of scope

- Pre-push receipt reuse (skipping a re-run for a tree a verifier already passed). That is str-35vtk.25 (shadow check only), str-35vtk.26 (evidence threshold) and str-35vtk.9 (batch landing). Reuse may not be enabled before str-35vtk.26's threshold is met, so it is not part of this issue.
- Any documented or scripted way to skip hooks (D4).
- The beads post-checkout JSONL import stall (the D4 beads issues in the shatter-tracker-and-beads bucket).
- Redesigning the landing verifier (str-qwua7.55, str-35vtk.24).
- Unrelated refactors in the touched files.

## Metadata

- Priority: P1. Type: task. Size: M.
- Labels: git-hooks, quality-gates, audit.
- Parent epic: Epic: Audit 2026-09-22 findings.
- Blocked by: none.
- Related: str-35vtk.25, str-35vtk.26, str-npdt, str-jttrf (closed), str-dl2pj.
- Source findings: sessions-04. Draft shatter-code/83.

---

<!-- file: 06-wire-every-test-module.md -->

---
slug: wire-every-test-module
kind: new
title: "Meta test: every scripts/test_*.py, scripts/test_*.sh and demo/test_*.py must be run by a gate reachable from `check` (8 modules, ~209 tests unwired today)"
priority: P2
type: task
labels: [testing, quality-gates, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Meta test: every script test module must be run by a gate reachable from `check`

## Problem

Adding a test file and wiring it into a gate are separate manual steps, and nothing checks the second step. Eight script test modules, about 209 test methods, are never run by `task check`, `task affected` or CI. They include the regression test that str-qwua7.7 added. Agents report "regression test added" and treat the work as done. The `meta` task lists its test modules one by one (`Taskfile.yml:449-466`) and lists its `sources:` file by file (`:396` onward), so a new module is neither run nor able to invalidate `meta`'s checksum until someone edits both lists by hand. A wiring guard that lives inside `meta` would itself be skipped as `up to date` when the only change is a new, unwired test file.

## Evidence (re-verified 2026-09-23; `Taskfile.yml` and the listed modules are identical on main `70465921` and the audit snapshot)

The following modules are referenced by no Taskfile, workflow or demo/scripts shell script. The check looked for the module basename in `Taskfile.yml`, `*/Taskfile.yml`, `.github/` and `demo/` or `scripts/` `*.sh`/`*.yml`. Test method counts come from `grep -c "def test_"`.

| Module | Tests | Note |
|---|---|---|
| `scripts/test_build_cache_doctor.py` | 57 | |
| `scripts/test_docs_smoke.py` | 54 | |
| `scripts/test_gate_pressure.py` | 30 | tests the pressure provider staged for str-35vtk.17/.19 |
| `scripts/test_gate_event_log.py` | 25 | tests the event store (str-35vtk.18, closed) that str-35vtk.25 depends on |
| `scripts/test_validate_parity.py` | 24 | includes the str-qwua7.7 regression test (4cf2165f) |
| `scripts/test_validate_protocol_registry.py` | 11 | only mentioned as a manual step at `protocol/GOVERNANCE.md:120-121` |
| `demo/test_gauntlet_check_output.py` | 6 | appears only as a path trigger at `scripts/affected-gates.py:177`; it pins a dead output format and is rewritten by `gauntlet-scan-checker-consumes-json` |
| `scripts/test_perf_compare.py` | 2 | |

- `scripts/test_ci_workflow_structure.py` runs only in CI (`.github/workflows/ci.yml:110`), never locally. str-35vtk.35 (open, P3; verified with `bd show`) covers this one file.
- `scripts/test_broad_run_validation_gate` and `scripts/test_kapow_refute_agent` run only in non-gate tasks (`Taskfile.yml:922`, `:932`).
- `scripts/gate-event-log.py` and `scripts/gate-pressure.py` are not invoked by any gate yet. That is intentional staging: str-35vtk.19 (open) owns the pressure-aware wait loop, and str-35vtk.25 (open) writes shadow events into the event store. Their tests must run; the scripts themselves stay.
- `scripts/test_gate_receipt.py` does run in `meta` (`Taskfile.yml:460`). `scripts/test_test_tier_wiring.py` also runs in `meta` (`:466`) and is the natural home for the new check.

## Acceptance criteria

- [ ] A meta test, for example in `scripts/test_test_tier_wiring.py`, discovers `scripts/test_*.py`, `scripts/test_*.sh` and `demo/test_*.py`. It fails for any module that no task reachable from `check` runs, whether through `python3 -m unittest scripts.<module>`, a direct path, or a `discover` pattern. Reachability is computed by parsing the Taskfiles as YAML (not via `task --list-all --json`; see str-qwua7.3). An allowlist with a reason per entry is permitted only for modules intentionally run by a named non-gate task (for example `test_broad_run_validation_gate`).
- [ ] The task that hosts the guard declares glob `sources:` that cover future additions: `scripts/test_*.py`, `scripts/test_*.sh`, `demo/test_*.py` (and the Taskfiles it parses). A meta test asserts those globs are present.
- [ ] Regression proof that the guard cannot be cached away: on a scratch branch, with the checksum-poisoning fix (str-qwua7.3) in place, run `task meta` until it exits 0, confirm a second run prints `Task "meta" is up to date`, then add an empty-but-valid `scripts/test_zz_unwired.py` containing one passing test and referenced by no task. `task meta` (without `--force`) must execute and fail naming that module. Record the transcript in the close reason and delete the scratch file.
- [ ] Proof on today's tree: the meta test fails, listing the 8 modules above (record the output in the close reason), and passes after wiring.
- [ ] Each listed module except `demo/test_gauntlet_check_output.py` is wired into `meta` (or a better-fitting gate reachable from `check`) and passes. Wiring failures are fixed or filed, with ids in the close reason. `scripts/test_gate_pressure.py` and `scripts/test_gate_event_log.py` are wired as tests only; this issue does not invoke, integrate or delete `gate-pressure.py` or `gate-event-log.py`.
- [ ] `demo/test_gauntlet_check_output.py`: if `gauntlet-scan-checker-consumes-json` has landed, it is already in `meta` and the guard sees it. If not, it is allowlisted with the reason "pins dead format; wired into meta by <gauntlet-scan-checker-consumes-json id>", and that issue's acceptance removes the allowlist entry.
- [ ] `protocol/GOVERNANCE.md:120-121` no longer describes the validator tests as a manual step.
- [ ] str-35vtk.35 is closed as subsumed (with a pointer to this issue), or completed here.

## Suggested approach

Replace the explicit module list in `meta` with `python3 -m unittest discover -s scripts -p 'test_*.py'` plus the same for `demo/`, and add the glob `sources:`. Keep the meta test as a guard against layouts `discover` would not reach. Alternatively, keep the explicit list and let the meta test enforce it.

## Out of scope

- Fixing test failures uncovered by wiring, beyond filing them.
- Rewriting the gauntlet checker (`gauntlet-scan-checker-consumes-json`).
- Integrating `gate-pressure.py` or `gate-event-log.py` into `gate-wrapper.sh` (str-35vtk.19, str-35vtk.25).

## Metadata

- Priority: P2. Type: task. Size: S-M.
- Labels: testing, quality-gates, audit.
- Parent epic: Epic: Audit 2026-09-22 findings.
- Blocked by: none. The cache-regression proof needs the str-qwua7.3 fix (`task-list-json-poisons-checksums`) to be meaningful; do it last.
- Related: str-35vtk.35, str-35vtk.19, str-35vtk.25, str-qwua7.7, `gauntlet-scan-checker-consumes-json`.
- Source findings: tests-ci-10 (verifier correction applied: gate-receipt.py's tests do run in meta), protocol-parity-05. Draft shatter-agent/19.

---

<!-- file: 07-gauntlet-scan-checker-consumes-json.md -->

---
slug: gauntlet-scan-checker-consumes-json
kind: new
title: "Gauntlet scan-failure checker has matched nothing since 2026-05-13: consume scan JSON (`codebase.failed[]` and interrupted `skipped_functions[]`), regenerate test fixtures from the CLI, fix allowlist and CLAUDE.md"
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

The gauntlet's per-step output screen can no longer detect scan failures or interruptions. Both regexes it relies on target output formats the CLI stopped producing. Its unit tests use hand-written strings in the old format, so they stay green, and nothing runs them anyway. A scan with 4 failed and 7 interrupted functions passes the checker. The Gauntlet-gate paragraph in CLAUDE.md describes a guarantee that does not hold. The checker and the CLI output share no contract, so changing a user-facing format triggered no check on downstream parsers.

Closed issues str-jeen.57 (gauntlet fails on scan FAIL/error rows) and str-jeen.59 (allowlist) delivered this check. It went dead five days after str-jeen.57 closed. A reopen-note on str-jeen.57 points here (`gauntlet-checker-reopen-note`). The open str-qwua7.10 covers bad sanity checks in the demo gates generally. This issue is the concrete fix for the scan-failure part.

## Evidence (re-verified 2026-09-23 on main `70465921`)

- `demo/gauntlet_check_output.py:30-35`: `SCAN_ERROR_SUMMARY_RE` requires `Scan complete: ... N error(s)`, and `FAIL_ROW_RE` requires `| FAIL |` markdown rows.
- Commit 00124c84 (2026-05-13, str-izhn) changed the scan summary to `Scan complete: {} completed, {} failed, {} unsupported, {} interrupted, {} skipped ({} worker(s))` (the format string in `shatter-core/src/scan_orchestrator.rs`; line 6301 on main, 6120 on the audit snapshot). Production code never emits `error(s)`.
- `shatter-core/src/report.rs` (str-4ad5) emits only PASS, WARN and LOW rows in the markdown scan table, and a test in the same file asserts `!md.contains("| FAIL |")`. As a result, every `outcome: FAIL` entry in `demo/gauntlet-scan-allowlist.yaml` is dead as well.
- In scan's JSON output, failures and interruptions live in different fields. Failed functions are in `codebase.failed[]` (count in `codebase.failed_functions`). Interrupted functions are **not** failures: they are entries in `codebase.skipped_functions[]` with `category == "interrupted"` (`report.rs`, str-smcx; the `category: "interrupted".into()` constructor). A checker that reads only `failed[]` misses every interruption.
- `demo/test_gauntlet_check_output.py:28-39` pins `Scan complete: **43 function(s)** tested, **0 skipped**, **2 error(s)**` and hand-written `| FAIL |` rows. No gate runs this module (see `wire-every-test-module`). It appears only as a path trigger at `scripts/affected-gates.py:177`.
- `expected_scan_errors` in the allowlist (`demo/gauntlet-scan-allowlist.yaml:108-112`) names `11-opaque-types.ts` and `12-external-deps.ts`, which are absent from the pinned examples checkout. CLAUDE.md's Gauntlet-gate paragraph repeats those names and the dead "`FAIL` rows / `N error(s)`" description.
- There are four divergent copies of the step check: `demo/gauntlet.sh:353-374` (which calls the Python helper, with an inline regex fallback), `demo/walkthrough.sh:261`, `demo/walkthrough-docker.sh:144` and `demo/gauntlet-docker.sh:167`. The Docker gauntlet has no FAIL or summary check at all.
- Some gauntlet scan steps are expected to produce non-standard results: `scan --timeout-total 120 --timeout-per-fn 30` (`demo/gauntlet.sh:641`) may legitimately interrupt functions, and `scan --core-sample 3 --dry-run` (`:585`) executes nothing.
- Audit repro sample (committed on the audit branch only, not on main): `git show 56c86168:audits/2026-09-22/artifact-samples/scan-mix.stdout` and `...scan-mix.json`. `python3 demo/gauntlet_check_output.py --allowlist demo/gauntlet-scan-allowlist.yaml --output <that stdout> --step x` exits 0 with no output. The stdout reads `**1 completed**, **4 failed**, **0 unsupported**, **7 interrupted**`; the JSON has `codebase.failed_functions == 4`, 4 `failed[]` entries, and 7 `skipped_functions[]` entries with category `interrupted`.
- Cheaper partial mechanism: `shatter scan --fail-on-failures[=PERCENT]` (`shatter-cli/src/args.rs`, str-izhn) exits non-zero on failed attempts. It cannot express a per-function allowlist and does not cover interruptions, so it complements rather than replaces the JSON check.

## Acceptance criteria

- [ ] The checker reads scan's JSON output (`--format json`). It flags (a) every `codebase.failed[]` entry and (b) every `codebase.skipped_functions[]` entry with `category == "interrupted"`, each unless allowlisted by function + file (+ reason for failures). It no longer parses markdown prose. The gauntlet's scan steps write that JSON to a file the checker reads.
- [ ] Intentional cases are explicit, not implicit: the `--timeout-total` step declares in the allowlist (or a per-step config) that interruptions are expected there, and the `--dry-run` step is marked as producing no scan JSON and skipped by the checker with a logged reason. A step that produces no JSON when one was expected is an error.
- [ ] The checker's tests use fixtures produced by the real CLI, with the regeneration command documented next to them: one fixture with at least one failed function and one with only interrupted functions (for example a tiny example plus `--timeout-per-fn 1` on a function that loops). Tests assert the checker reports the failure in the first and the interruption in the second, and reports 0 for an allowlisted copy of each. The tests pinning `N error(s)` and `| FAIL |` are removed.
- [ ] `demo/test_gauntlet_check_output.py` is wired into `meta` (reachable from `check`, as `wire-every-test-module` requires). If `wire-every-test-module` added a temporary allowlist entry for it, that entry is removed here.
- [ ] Proof: the new fixture tests fail against the current checker, with the command and output recorded in the close reason, and pass after the rewrite.
- [ ] The allowlist names only functions and files that exist in the pinned examples checkout, each with a tracker issue id. Stale entries (`11-opaque-types.ts`, `12-external-deps.ts`, dead `outcome: FAIL` rows) are removed or rewritten against the JSON fields.
- [ ] The CLAUDE.md Gauntlet-gate paragraph describes the new contract (JSON-based, failed + interrupted, allowlist semantics) and drops the removed fixture names.
- [ ] All demo scripts (`gauntlet.sh`, `gauntlet-docker.sh`, `walkthrough.sh`, `walkthrough-docker.sh`) use the one shared checker for scan steps.
- [ ] `task gauntlet` on current main either passes with an allowlist that matches reality, or fails listing the real failures and interruptions. Record the run log path or excerpt in the close reason.

## Suggested approach

Have each gauntlet scan step also write `--format json` output, for example with `-o <step>.json`. The checker loads it, collects `failed[]` and interrupted `skipped_functions[]`, diffs them against the allowlist, and flags anything new. Keep `PROCESS_ERROR_RE` for process-level markers. Optionally also pass `--fail-on-failures` so a non-zero exit backs up the JSON check on steps with no allowlisted failures.

## Out of scope

- Fixing the underlying scan failures themselves.
- A CLI golden-output contract suite (`golden-and-consumer-suite`, shatter-reports-and-specs bucket, which is blocked by this issue).
- General demo-gate sanity checks beyond scan failures (str-qwua7.10).

## Metadata

- Priority: P1. This overrides the P2 in the report tables, per report §15.1 and §14 item 25. Type: bug. Size: M.
- Labels: gauntlet, quality-gates, testing, audit.
- Parent epic: Epic: Audit 2026-09-22 findings.
- Blocked by: none.
- Related: str-jeen.57 (closed), str-jeen.59 (closed), str-qwua7.10, str-izhn, str-4ad5, str-smcx, `wire-every-test-module`.
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
> - `demo/gauntlet_check_output.py` `SCAN_ERROR_SUMMARY_RE` requires `Scan complete: ... N error(s)`. Commit 00124c84 (2026-05-13, str-izhn) changed the summary to `Scan complete: N completed, M failed, ... interrupted, ...` (format string in `shatter-core/src/scan_orchestrator.rs`, line 6301 on main `70465921`). Production code never emits `error(s)`.
> - `FAIL_ROW_RE` requires `| FAIL |` rows. `shatter-core/src/report.rs` (str-4ad5) emits only PASS, WARN and LOW scan rows, and a test in that file asserts no `| FAIL |` row. That makes every `outcome: FAIL` allowlist entry dead too.
> - Interrupted functions were never covered: scan JSON records them in `codebase.skipped_functions[]` with `category == "interrupted"`, not in `codebase.failed[]`.
> - `demo/test_gauntlet_check_output.py` still pins the old format, and no gate runs it.
> - Repro: the checker exits 0 on a scan stdout reporting 4 failed and 7 interrupted functions (`git show 56c86168:audits/2026-09-22/artifact-samples/scan-mix.stdout`, committed on the audit branch `audit-2026-09-22`, not on main).
>
> This issue stays closed. The fix (consume scan JSON `codebase.failed[]` plus interrupted `skipped_functions[]`, fixtures generated by the CLI, allowlist and CLAUDE.md cleanup, one shared checker) is tracked in **<gauntlet-scan-checker-consumes-json>**. Related: str-jeen.59 (allowlist, closed) and str-qwua7.10 (open, demo gate sanity checks).

## Filer notes

- Also add a one-line comment on str-qwua7.10: "Scan-failure part of the demo-gate sanity problem is filed as <gauntlet-scan-checker-consumes-json> (audit 2026-09-22, artifacts-07)."
- str-jeen.59 needs no separate comment. The note above names it.

---

<!-- file: 09-verifier-per-language-evidence.md -->

---
slug: verifier-per-language-evidence
kind: new
title: "/pre-completion must prove per-language gate execution: add a changed-crate -> required-leaf evidence row, a property-test row, and an 'up to date means not run' rule"
priority: P2
type: task
labels: [quality-gates, agents, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [ci-executed-leaf-guard]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# /pre-completion must prove per-language gate execution

## Problem

`/pre-completion` (`.claude/skills/pre-completion/SKILL.md`) tells agents to run `task affected` and copy its `Gates selected:` list. It never asks whether the leaves covering each changed crate or language actually **executed**, and it never says that `Task "X" is up to date` means the leaf did not run. With Task checksum caching (and the str-qwua7.3 poisoning), a completion report can list the right gates while none of the relevant tests ran. The skill also has no property-test row, although CLAUDE.md Completion Checklist item 2 requires one. Sessions since 2026-09-04 saw 65 `task ... is up to date` results in 24 sessions (finding agent-repo-11), and agents treated them as passes.

Scope, reconciled against open issues (checked with `bd show` on 2026-09-23):
- The **landing verifier** (`scripts/land_work_verifier.sh`, `verifier.json`) is not changed here. str-35vtk.24 (in progress, P1) replaces its trio with exactly one `task check` and writes receipts. str-qwua7.55 (open, P2) owns verifier honesty and the timeout. str-qwua7.2 (open, P1) owns executed-vs-cached reporting for the verifier. The audit's verifier findings are added to those issues as notes (`qwua7-55-verifier-timeout-note`, `qwua7-2-scope-note`), not re-filed.
- This issue owns only the `/pre-completion` skill rows. It reuses the per-leaf evidence parser built by `ci-executed-leaf-guard` rather than writing another regex.

## Evidence (re-verified 2026-09-23 on main `70465921`)

- `.claude/skills/pre-completion/SKILL.md` Phase 2 runs `task affected` and records `Gates selected:` only. A grep for `property`, `PBT` or `up to date` finds nothing.
- `task affected` selects gates per path (`scripts/affected-gates.py`), but a selected gate can be served from `.task/checksum` without running.
- CLAUDE.md "Completion Checklist" item 2 requires property-test adequacy for new or modified public functions.

## Acceptance criteria

- [ ] Phase 2 of `/pre-completion` tees `task affected` output to a log and runs the `ci-executed-leaf-guard` evidence script on it with the required-leaf set derived from the diff. The mapping lives in one checked-in table (shared with or derived from `scripts/affected-gates.py`, not duplicated prose): shatter-core → core:test (plus core:test-ignored when the selector adds it), shatter-cli → cli:test, shatter-ts → ts:test, shatter-go → go:test, shatter-rust → rust-fe:test, shatter-rust-runtime → rust-rt:test + rust-fe:test, shatter-llm → its llm test leaf once it exists (str-35vtk.36).
- [ ] The skill states that a required leaf reported `cached` or `missing` is **not** evidence. The agent re-runs that leaf directly (`task --force <leaf>`; `--force` on `task affected` does not propagate to nested leaves) and records the leaf's executed result, or reports the gap. The output table has one row per required leaf with its `executed`/`cached`/`missing` verdict.
- [ ] The skill gains a property-test row that restates CLAUDE.md Completion Checklist item 2 and asks for the names of the property tests covering each new or modified public function (or an explicit "none needed: <reason>").
- [ ] A `meta` test asserts the mapping table covers every crate directory that `scripts/affected-gates.py` classifies, so a new crate cannot be added without a required-leaf entry.
- [ ] Proof: in a scratch worktree (after the str-qwua7.3 fix), commit a one-line comment change to a `shatter-go` file, run `task go:test` once (it executes and warms the cache), then follow the updated skill's Phase 2. Paste the row showing `go:test` as `cached`, then the directed `task --force go:test` re-run showing `executed`. Record both in the close reason. Running the same procedure against today's skill text produces no per-leaf row at all; note that as the before-state.

## Suggested approach

Add the steps to Phase 2 and a row to the output table. Put the crate → leaf table next to the evidence script from `ci-executed-leaf-guard`, so CI, `/pre-completion` and later the verifier (str-qwua7.2) read one definition.

## Out of scope

- Any change to `scripts/land_work_verifier.sh` or `verifier.json` (str-35vtk.24, str-qwua7.55, str-qwua7.2; see the companion notes in this bucket).
- The root-cause fix for meta-stage checksum poisoning (str-qwua7.3).
- bento-side verifier changes (bento-rdtn.4 and bento-rdtn.6 have shipped).

## Metadata

- Priority: P2. Type: task. Size: S-M.
- Labels: quality-gates, agents, audit.
- Parent epic: Epic: Audit 2026-09-22 findings.
- Blocked by: ci-executed-leaf-guard (for the shared evidence script).
- Related: str-qwua7.2, str-qwua7.55, str-35vtk.24, str-35vtk.36, `qwua7-55-verifier-timeout-note`, `qwua7-2-scope-note`.
- Source findings: tests-ci-06 (pre-completion half; the verifier half moved to the two notes), agent-repo-11. Draft shatter-agent/18.

---

<!-- file: 10-gate-telemetry-executed-vs-cached.md -->

---
slug: gate-telemetry-executed-vs-cached
kind: new
title: "Gate telemetry can't tell executed from cached runs (budgets computed from no-ops): add a per-leaf event stream, aggregate it, and recompute budgets from new executed-only measurements"
priority: P2
type: task
labels: [quality-gates, performance, resource-governance, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [task-list-json-poisons-checksums, ci-executed-leaf-guard]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Gate telemetry can't tell executed from cached runs

## Problem

Between 18% and 28% of recorded gate runs fail, and a cold full check takes more than 40 minutes on the shared machine under contention. Meanwhile the recorded median `check` wall time mostly reflects cached no-ops. The budgets in `docs/perf/gate-budgets.md` are computed from rows that mix runs whose leaves executed with runs whose leaves were cached, so both slowness and hollowness are invisible in the telemetry.

The telemetry cannot be fixed by adding a column. `scripts/gate-wrapper.sh` writes **one row per outer invocation** (`check`, `affected`, ...). Nested wrapped tasks (`check` → `conformance`, `core:test-ignored`, ...) hit the `SHATTER_GATE_LOCK_HELD` pass-through (`gate-wrapper.sh:21-23`, `exec "$@"`) and record nothing. So there is no per-leaf identity or duration anywhere in the data, and historical rows cannot be reclassified after the fact.

## Evidence (re-verified 2026-09-23 on main `70465921`)

- `~/.cache/shatter/gate-times.csv` (since 2026-08-10; columns `timestamp,worktree,label,wall_seconds,exit_code,loadavg_1min,slot,wait_seconds`, per `scripts/gate-wrapper.sh:9-10` and the `csv_write` function): non-zero exits are affected 44/156, check-fast 36/151 and check 26/143. There is no executed-vs-cached information.
- `gate-wrapper.sh` runs the gate without capturing its output (`nice ... "$@" &`, then `wait`), so it has nothing to parse today.
- In the audit run, workspace clippy alone took 14m48s and 22m01s at load 100-190, and stage 2 alone took 1558 s (finding gates-11; not re-timed).

## Acceptance criteria

- [ ] **Event schema, documented in the `gate-wrapper.sh` header:** the outer (lock-holding) invocation gets an `invocation_id`. Each leaf reached under it produces one row in a separate file, for example `~/.cache/shatter/gate-leaves.csv`, with `invocation_id,timestamp,label_outer,leaf,outcome,wall_seconds`, where `outcome` ∈ `executed|cached|missing|failed`. The existing per-invocation CSV keeps its columns and gains `invocation_id`, so rows join.
- [ ] **Source of leaf data:** the outer invocation tees the gate's output to a temp log and classifies leaves with the shared parser from `ci-executed-leaf-guard` (not a new regex). Per-leaf `wall_seconds` comes from nested wrapped leaves recording their start/end into the invocation's temp dir before the pass-through `exec` (or an equivalent mechanism); leaves without a wrapper may record an empty duration. The chosen mechanism is stated in the header.
- [ ] A unit test wired into `meta` runs the wrapper around a stub command that prints one executed leaf, one cached leaf and one failing leaf, and asserts the rows written to both CSVs (including matching `invocation_id`).
- [ ] **Aggregation:** a committed script (for example `scripts/gate-budgets.py`) computes per-gate and per-leaf medians and p90s from `executed` rows only, and reports how many `cached`/`missing` rows it excluded.
- [ ] **New measurements, not history:** `docs/perf/gate-budgets.md` is regenerated only from rows recorded after this change and after the str-qwua7.3 fix, with at least 10 executed `check` invocations. The doc names the script, the date range and the row counts, and states that pre-change rows are excluded because they cannot distinguish execution from caching.

## Suggested approach

Tee inside `run_governed`, pass an `invocation_id` and a temp dir to nested calls through the environment the wrapper already sets for `SHATTER_GATE_LOCK_HELD`, and have the pass-through path append its leaf's timing there before `exec`. Parse the teed log after the child exits. Keep CSV writes under the existing `csv.lock`.

## Out of scope

- Enabling sccache for gate runs (`sccache-for-gate-runs`, split from this issue).
- Host-wide admission control across projects, including making gate-wrapper's slot and bento's `run-heavy` share one budget (bento-dyp7).
- Receipts proving gates ran (str-qwua7.2, str-35vtk.24/.25). The CI guard (`ci-executed-leaf-guard`).
- The pressure-aware wait policy and gate event log (str-35vtk.19, str-35vtk.18); this issue's CSV is the timing log, not that event store.

## Metadata

- Priority: P2. Type: task. Size: M.
- Labels: quality-gates, performance, resource-governance, audit.
- Parent epic: Epic: Audit 2026-09-22 findings.
- Blocked by: task-list-json-poisons-checksums (the str-qwua7.3 fix), ci-executed-leaf-guard (shared parser).
- Related: str-35vtk.10, str-35vtk.24, str-35vtk.19, str-0f6ze, bento-dyp7, `sccache-for-gate-runs`.
- Source findings: gates-11. Draft shatter-code/10.

---

<!-- file: 11-qwua7-55-verifier-timeout-note.md -->

---
slug: qwua7-55-verifier-timeout-note
kind: note-to-existing
title: "Note on str-qwua7.55: a `timeout` in verifier.json is not read by bento's land-work; enforce the verifier timeout in a way the installed runner honours, and keep one exact `task check` per str-35vtk.24"
priority: P2
type: task
labels: [landing, quality-gates, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [verifier-per-language-evidence]
existing_id: str-qwua7.55
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-qwua7.55: the timeout criterion as written cannot be met by editing verifier.json

**Target:** open issue `str-qwua7.55` ("Repo-owned landing fixes now (verifier honesty, timeout); orchestration driver comes from bento", P2; verified open with `bd show` on 2026-09-23).
**Action:** post the comment below and amend the acceptance as shown. Do not file a new issue. This replaces the one-line companion note that the earlier draft of `verifier-per-language-evidence` proposed.

## Comment text (post verbatim)

> **Audit 2026-09-22 (findings tests-ci-06, agent-repo-11): amendments to this issue's repo-owned verifier items.**
>
> **1. "verifier.json gains a timeout" would enforce nothing.** In the installed bento (2.3.88 inspected on 2026-09-23; the Codex cross-check found the same in 2.3.73), `skills/land-work/scripts/land-work-run-verifier.py` takes its timeout only from the `--timeout` CLI argument (`:208`, applied at `:344-383` with a process-group kill), and `land.py` forwards its own `--timeout` (`:77`, `:303-304`). Neither reads a timeout from `.agent-plugins/bento/bento/land-work/verifier.json`, which today holds only `schema_version` and `command`. A new field there would be ignored.
>
> **Amended acceptance for the timeout item** (replaces "verifier.json gains a timeout"):
> - The timeout is enforced by a mechanism the runner actually honours. Either (a) `scripts/land_work_verifier.sh` bounds its own gate run (for example `timeout --kill-after=30 <seconds> task check`, killing the whole process group, with the seconds passed as an argument in `verifier.json`'s `command` array, e.g. `["scripts/land_work_verifier.sh", "--timeout", "3600"]`, and a documented default when absent; no new environment variable) and reports `status: "failed"` with a timeout reason in its final JSON line; or (b) the repo's landing instructions (AGENTS.md and the landing section the agents follow) pass `--timeout <seconds>` to bento's `land.py`/`land-work-run-verifier.py`. If a manifest-level timeout is wanted instead, that is a bento feature request, not a repo-owned fix.
> - A runtime test, wired into `meta`, runs the verifier (or the exact wrapper used in option b) against a stub `task` on `PATH` that sleeps past a short timeout. It asserts the run ends within the timeout plus the kill grace period, no stub process survives, and the reported status is a timeout/failure. It fails on today's script.
>
> **2. What the verifier runs.** str-35vtk.24 (in progress) already replaces the `test-standard`/`parity`/`conformance` trio with exactly one exact-candidate `task check`. This issue's "runs task check (or states exactly what it runs)" item should be satisfied by .24's change plus correcting the header comment at `scripts/land_work_verifier.sh:3` ("Runs the same gates ci.yml uses") and any CLAUDE.md/AGENTS.md text that repeats it. Running `task affected` in the verifier is **not** an option; it would conflict with .24.
>
> **3. Output.** `run_check` (`scripts/land_work_verifier.sh:14-23`) runs each gate as `"$@" >/dev/null 2>&1`, so bento's persisted verifier log is empty on failure or kill. str-35vtk.24's notes already require the replacement to stop discarding output. Whichever of .24/.55 lands first must include a fixture where the stub gate prints stdout/stderr sentinels and exits non-zero, and assert both sentinels reach the verifier's output.
>
> Executed-vs-cached reporting for the verifier stays with str-qwua7.2 (see the companion note there). The `/pre-completion` per-language evidence rows are filed as <verifier-per-language-evidence>.

## Filer notes

- Replace `<verifier-per-language-evidence>` with that issue's filed id.
- Add `related` links to str-35vtk.24 and str-qwua7.2 if missing.
- Add the label `audit` if missing.
- `blocked_by` lists `verifier-per-language-evidence` only so it is filed first and its id can be substituted; the note can be posted immediately after.

---

<!-- file: 12-qwua7-2-scope-note.md -->

---
slug: qwua7-2-scope-note
kind: note-to-existing
title: "Note on str-qwua7.2: check-fresh was defeated by the str-qwua7.3 root cause; CI item moves to the audit CI guard; define execution evidence per required leaf (mixed/missing/cached/failed), not 'executed list non-empty'"
priority: P1
type: task
labels: [quality-gates, landing, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [ci-executed-leaf-guard]
existing_id: str-qwua7.2
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-qwua7.2: re-scope after the str-qwua7.3 root cause, and tighten the execution-evidence definition

**Target:** open issue `str-qwua7.2` ("Add task check-fresh and make verifier, CI and pre-completion prove gates actually ran", P1; verified open with `bd show` on 2026-09-23).
**Action:** post the comment below and amend the acceptance as shown. Do not file a new issue. `blocked_by` lists `ci-executed-leaf-guard` only so the filer can substitute its real id; the note itself can be posted as soon as that issue is filed.

## Comment text (post verbatim, after substituting ids)

> **Audit 2026-09-22 (findings gates-01, gates-02, tests-ci-06): re-scope and tighten this issue.**
>
> **1. Root cause of the "still up to date after deleting checksums" mystery.** It is recorded on str-qwua7.3: a `meta`-stage test runs `task --list-all --json`, which rewrites `.task/checksum/*` for every task with `sources:`. `check-fresh` as specified here (delete checksums, then call the governed task) would have been defeated the same way, because `meta` runs first and re-poisons the cache. Once str-qwua7.3's fix lands, `check-fresh` is viable again, but it is no longer the main defence: positive per-leaf evidence (below) catches hollow runs whatever their cause.
>
> **2. The CI item moves to <ci-executed-leaf-guard>.** The acceptance checks "`.github/workflows/ci.yml:89` calls `task check-fresh`" and "a live run asserts no `Task "<leaf>" is up to date` line" are replaced by that issue, which runs `task check` once in CI and requires every leaf in an explicit expected set to show positive execution evidence. Remove the CI item from this issue's acceptance. This issue keeps the verifier and `/pre-completion` parts; the `/pre-completion` rows are filed as <verifier-per-language-evidence>.
>
> **3. Replace "exits non-zero when the executed list is empty".** That passes when one required leaf ran and the others were cached, missing from the graph, or never started. Amended acceptance for the verifier:
> - Execution evidence is defined **per required leaf** from an explicit expected-leaf list: `executed` = at least one `task: [<leaf>]` command echo line and no `Task "<leaf>" is up to date` line; `cached` = an up-to-date line; `missing` = neither. The verifier fails unless every required leaf is `executed`. It uses the shared parser from <ci-executed-leaf-guard> (for example `scripts/task_leaf_evidence.py`), not a new regex.
> - The verifier's final JSON line sets the bento-recognized per-check `executed` boolean (bento `land.py` validates it as a boolean on each `selected_checks[]` entry; bento-rdtn.6) to `true` only when every required leaf executed. No new top-level keys are added to Shatter's strict receipt schema (`scripts/gate-receipt.py::RESULT_KEYS`), per the str-35vtk.24 note.
> - A test wired into `meta` runs the verifier against a stub `task` that emits fixture logs for each case and asserts the verdict: all required leaves executed → pass; a mixed log where dependencies are cached but every required leaf executed → pass; one required leaf cached → fail naming it; one required leaf absent → fail naming it; everything cached → fail; a required leaf executed but its command failed → fail on the exit code, with the leaf reported `executed`.
> - Close-time proof: one real landing after the change whose verifier JSON line shows `executed: true`, and one forced hollow run (pre-seeded `.task/checksum`) showing the verifier failing with the cached leaves named. Paste both lines into the close reason.
>
> Coordination: str-35vtk.24 (in progress) makes the verifier run exactly one `task check`; build this on top of that, not on the old trio. The verifier timeout and output-tee amendments are on str-qwua7.55.

## Filer notes

- Substitute the filed ids for `<ci-executed-leaf-guard>` and `<verifier-per-language-evidence>`.
- Add `related` links to str-qwua7.3, str-qwua7.55 and str-35vtk.24 if missing.
- Add the label `audit` if missing. Priority stays P1.

---

<!-- file: 13-ci-first-real-run-triage.md -->

---
slug: ci-first-real-run-triage
kind: new
title: "Triage every test failure the first real CI `task check` run surfaces after the checksum-poisoning fix, until main's CI is green with all required leaves executed"
priority: P1
type: bug
labels: [ci, quality-gates, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [task-list-json-poisons-checksums, ci-executed-leaf-guard]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Triage the test failures that surface once CI's `task check` really runs its leaves

## Problem

CI's `task check` has run no stage-2/3 test leaves since about 2026-08-29 (str-qwua7.3 root cause; `ci-executed-leaf-guard` makes that visible). About a month of merges landed without those tests running. Once the poisoning is fixed and the guard is in place, the first real CI runs will fail on regressions that accumulated in the meantime. This issue owns getting main's CI green with every required leaf executing. It is split from `ci-executed-leaf-guard` so the guard can land and close on its own proof, independent of how many regressions the triage turns up.

## Evidence (re-verified 2026-09-23 on main `70465921`)

Failures already known when the leaves are forced to run locally (audit 2026-09-22):

- str-k7czv: Go loader test. Its root cause is filed as `go-config-discovery-unbounded` (shatter-frontend-go bucket).
- 10 TS handler timeouts under load: filed as `ts-handlers-test-timeouts` (shatter-test-hygiene bucket).
- `bench_frontier_ranking` timeout in `core:test-ignored`: str-6nul9 landed on main after the audit snapshot (20692b08, merged 70465921). It excludes the benchmark with a nextest `-E` filter and with `--skip bench_frontier_ranking` on the `cargo test` fallback (`shatter-core/Taskfile.yml:61-66` on main). CI has no cargo-nextest, so it takes the fallback path; this issue confirms the skip holds there.

The complete list is unknown until CI runs the leaves.

## Acceptance criteria

- [ ] Every failing leaf in the first CI run on main after `ci-executed-leaf-guard` lands is listed in this issue's notes with its run URL, and each failure is either fixed (commit SHA) or filed (issue id). Pre-existing issues (str-k7czv, `go-config-discovery-unbounded`, `ts-handlers-test-timeouts`) are linked rather than re-filed.
- [ ] Close-time proof: a CI run URL on main in which the `task check` step exits 0 **and** `ci-executed-leaf-guard`'s evidence script reports every expected leaf `executed`. Paste the script's summary line into the close reason.
- [ ] No failure is resolved by marking a test `#[ignore]`, `skip`, `.skip` or `t.Skip` unless the same commit links an open issue for re-enabling it. The close reason lists any such skip with its issue id.
- [ ] The close reason notes whether `bench_frontier_ranking` stayed excluded on CI's `cargo test` fallback path, citing the run log line.

## Suggested approach

Let the guard land, read the first real run's log, and file or fix per leaf. Batch trivial fixes; file anything larger than a small change as its own issue and link it here.

## Out of scope

- The guard itself (`ci-executed-leaf-guard`) and the root-cause fix (str-qwua7.3).
- Correcting the CLAUDE.md "Full = Landing, CI" and "verified in CI" claims. That is owned by `test-tier-docs-overstate-coverage` (shatter-docs bucket), which restores the stronger claim once this issue's green run exists.
- Adding cargo-nextest to CI (`nextest-ci-profile-and-stale-parity-fallback`, shatter-ci-workflows bucket).

## Metadata

- Priority: P1. Type: bug. Size: M (unknown until the first real run).
- Labels: ci, quality-gates, audit.
- Parent epic: Epic: Audit 2026-09-22 findings.
- Blocked by: task-list-json-poisons-checksums (the str-qwua7.3 fix), ci-executed-leaf-guard.
- Related: str-k7czv, str-6nul9, str-35vtk.21, `go-config-discovery-unbounded`, `ts-handlers-test-timeouts`, `test-tier-docs-overstate-coverage`.
- Source findings: gates-02 (triage half; split from `ci-executed-leaf-guard` in the 2026-09-23 Codex revision).

---

<!-- file: 14-sccache-for-gate-runs.md -->

---
slug: sccache-for-gate-runs
kind: new
title: "sccache is installed but unused by gate runs: measure a cold `task check-unit` with and without it, then enable it or record why not"
priority: P3
type: task
labels: [quality-gates, performance, sccache, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [task-list-json-poisons-checksums]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# sccache is installed but unused by gate runs

## Problem

Cold Rust builds dominate gate wall time on the shared machine (a cold full check took more than 40 minutes under contention in the audit). Every new linked worktree starts with an empty target dir. `sccache` is installed on the host but nothing in the repo or the gate environment uses it, and no measurement shows whether it would help. str-35vtk.2 (closed) documented a host-level sccache setup and rejected a shared `CARGO_TARGET_DIR`, but did not wire sccache into gates or measure it.

## Evidence (re-verified 2026-09-23)

- `/usr/bin/sccache` is installed. `RUSTC_WRAPPER` is unset in the agent environment, and `.cargo/config.toml` has no `rustc-wrapper`.
- In the audit run, workspace clippy alone took 14m48s and 22m01s at load 100-190 (finding gates-11; not re-timed).

## Acceptance criteria

- [ ] A reproducible measurement, with commands recorded in the close reason: in two fresh linked worktrees of the same commit (after the str-qwua7.3 fix, so the leaves actually execute), run `task --force check-unit` once without sccache and once with `RUSTC_WRAPPER=sccache` and a pre-warmed sccache cache (warmed by one prior run in a third worktree). Record wall time, load average at start, and `sccache --show-stats` hit rate.
- [ ] Decision recorded: either sccache is enabled for gate runs (in `gate-wrapper.sh`, or `.cargo/config.toml` behind a guard that falls back cleanly when sccache is absent, with a `meta` test for the fallback), with the before/after numbers in AGENTS.md; or the close reason records the numbers and why it was not enabled.
- [ ] If enabled, a cold `task check-unit` on a machine without sccache still passes (the fallback test above), so CI and other hosts are unaffected.

## Out of scope

- A shared `CARGO_TARGET_DIR` (rejected in str-35vtk.2).
- Gate telemetry (`gate-telemetry-executed-vs-cached`), which should supply the executed-only timings once it lands.

## Metadata

- Priority: P3. Type: task. Size: S.
- Labels: quality-gates, performance, sccache, audit.
- Parent epic: Epic: Audit 2026-09-22 findings.
- Blocked by: task-list-json-poisons-checksums (the str-qwua7.3 fix; before it, `check-unit` leaves may not execute).
- Related: str-35vtk.2, `gate-telemetry-executed-vs-cached`.
- Source findings: gates-11 (sccache part; split from `gate-telemetry-executed-vs-cached` in the 2026-09-23 Codex revision). Draft shatter-code/10.
