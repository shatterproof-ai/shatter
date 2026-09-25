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
