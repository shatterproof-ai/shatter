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
