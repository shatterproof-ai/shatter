---
slug: ci-runs-user-paths
kind: new
title: "Smoke, walkthrough and E2E user paths never run in CI: add a push-to-main user-paths job that fails on a real regression"
priority: P2
type: task
labels: [ci, smoke, walkthrough, e2e, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Smoke, walkthrough and E2E user paths never run in CI: add a push-to-main user-paths job that fails on a real regression

## Problem

`ci.yml` runs `task check` plus the shatter-llm steps, and nothing else. `task check` does not include `smoke`, `walkthrough`, `gauntlet` or `e2e`. Those gates, which exercise what a user actually runs, run only when an agent chooses to run them. The only workflow that touches the gauntlet is the weekly Perf CI, which has failed every run. Its fix is a separate issue, `perf-ci-stable-scenarios-red`, so that a perf-runner failure does not hold this one open. A regression in the demo or user path can therefore land on main and stay there until someone happens to run the gate locally.

## Evidence

Re-verified 2026-09-23 in the audit worktree (main 70465921 plus audit files):

- `.github/workflows/ci.yml:88-110`: the steps are `task check`, `cargo clippy -p shatter-llm`, `cargo test -p shatter-llm`, and `python3 scripts/test_ci_workflow_structure.py`. No workflow mentions `smoke`, `walkthrough` or `gauntlet` (`grep -rn` over `.github/workflows/`).
- `Taskfile.yml:648-660` `smoke` (about 15 s; TS + two Go explores + `scripts/test_empty_report_regression.sh`). `:680-689` `walkthrough` / `walkthrough-governed` (`demo/walkthrough.sh --auto --delay 0`). `:701-710` `gauntlet`. `:577-589` `e2e` / `e2e-governed` (TS, Go, Rust). `:535-575` `check-static`/`check-unit`/`check-integration` include none of them.
- Perf CI (`perf-ci.yml`, 13/13 failures) is covered by `perf-ci-stable-scenarios-red`.
- Separately, CI's `task check` test leaves have reported "up to date" since about 2026-08-29 (see the str-qwua7.3 note `task-list-json-poisons-checksums` and `ci-executed-leaf-guard` in the shatter-gates-integrity bucket). The E2E coverage that `check-integration`'s `core:test-ignored` provides in CI is therefore hollow as well.

## Acceptance criteria

- [ ] A CI job on push to main (or nightly on schedule), which also has `workflow_dispatch` so it can be run from a branch, runs `task smoke` and a bounded walkthrough (`task walkthrough`, or a documented subset if it exceeds the job budget) and uploads their output as workflow artifacts.
- [ ] The same job or a sibling runs `task e2e`, or `ci-executed-leaf-guard` has landed and the close reason cites a `ci.yml` run log in which `core:test-ignored` actually executed the three `e2e_concolic*` suites (test names visible in the log). A claim without that log does not satisfy this item.
- [ ] Each new job fails on a real regression. On a branch, show a deliberate break (for example making `demo/walkthrough.sh` exit 1, or breaking a smoke target) producing a red job, then revert, and cite both run URLs in the close reason. If str-qwua7.10's content assertions have landed, the walkthrough job enforces them.
- [ ] `scripts/test_ci_workflow_structure.py` asserts the new job and steps exist, so they cannot be dropped silently.

## Suggested approach

Add a separate `user-paths` job in `ci.yml` (push to main only, not PRs, to keep PR latency down), or a new nightly workflow. Reuse the apt/toolchain setup from the `test` job. `workflow-health-patrol` will then cover the nightly run. Combine with str-qwua7.10 so the walkthrough job checks content, not just the exit code.

## Out of scope

- The Task checksum problem itself (str-qwua7.3 / `ci-executed-leaf-guard`).
- Gauntlet allowlist content, and the gauntlet in CI. The gauntlet is broad and slow; add it here only if it fits the job budget.
- Perf CI (`perf-ci-stable-scenarios-red`).
- Moving perf-ci paths (str-qwua7.42).

## Dependencies

- None hard-blocking. The smoke/walkthrough job does not depend on the checksum fix.
- Related: str-qwua7.10 (open; demo gates fail on bad content), `perf-ci-stable-scenarios-red` (split from this issue), `ci-executed-leaf-guard`, `workflow-health-patrol` (watches the new job once it runs on push to main or on a schedule).

Priority: P2 · Type: task · Labels: ci, smoke, walkthrough, e2e, audit · Parent: Epic: Audit 2026-09-22 findings · Sources: shatter-code/73, tests-ci-11
