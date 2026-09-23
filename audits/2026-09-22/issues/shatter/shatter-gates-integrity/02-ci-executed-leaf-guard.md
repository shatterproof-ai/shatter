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
