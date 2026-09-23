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
