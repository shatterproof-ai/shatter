---
slug: ci-runs-user-paths
kind: new
title: "Smoke, walkthrough, gauntlet and E2E user paths never run in CI; the only CI gauntlet path (Perf CI) has 0/13 successes"
priority: P2
type: task
labels: [ci, smoke, walkthrough, gauntlet, e2e, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Smoke, walkthrough, gauntlet and E2E user paths never run in CI; the only CI gauntlet path (Perf CI) has 0/13 successes

## Problem

`ci.yml` runs `task check` plus the shatter-llm steps, and nothing else. `task check` does not include `smoke`, `walkthrough`, `gauntlet` or `e2e`. Those gates, which exercise what a user actually runs, run only when an agent chooses to run them. The one workflow that touches the gauntlet, the weekly Perf CI (`gauntlet-auto-warm` is in its stable scenario list), has failed every run with its reason suppressed. A regression in the demo or user path can therefore land on main and stay there until someone happens to run the gate locally.

## Evidence

Re-verified 2026-09-23 in the worktree at `56c86168`:

- `.github/workflows/ci.yml:88-110`: the steps are `task check`, `cargo clippy -p shatter-llm`, `cargo test -p shatter-llm`, and `python3 scripts/test_ci_workflow_structure.py`. No workflow mentions `smoke`, `walkthrough` or `gauntlet` (`grep -rn` over `.github/workflows/`).
- `Taskfile.yml:648-660` `smoke` (about 15 s; TS + two Go explores + `scripts/test_empty_report_regression.sh`). `:680-689` `walkthrough` / `walkthrough-governed` (`demo/walkthrough.sh --auto --delay 0`). `:701-710` `gauntlet`. `:577-589` `e2e` / `e2e-governed` (TS, Go, Rust). `:535-575` `check-static`/`check-unit`/`check-integration` include none of them.
- `perf-ci.yml:58` runs `perf_runner.py run --scenario-file perf/stable-scenarios.txt`. `perf/stable-scenarios.txt` lists `gauntlet-auto-warm`, `explore-ts-arithmetic-warm`, `scan-standalone-ts-warm` and `go-frontend-instrument-tests`. `gh run list --workflow perf-ci.yml` → `{"failure":13}`. The latest run, 35620979400, fails in `Run stable perf scenarios`. The audit's log read showed "gauntlet-auto-warm failed on run 1 with exit code 1", with the underlying stderr not printed.
- Separately, CI's `task check` test leaves have reported "up to date" since about 2026-08-29 (see the str-qwua7.3 note `task-list-json-poisons-checksums` and `ci-executed-leaf-guard` in the shatter-gates-integrity bucket). The E2E coverage that `check-integration`'s `core:test-ignored` provides in CI is therefore hollow as well.

## Acceptance criteria

- [ ] A CI job on push to main (or nightly on schedule) runs `task smoke` and a bounded walkthrough (`task walkthrough`, or a documented subset if it exceeds the job budget) and uploads their output as workflow artifacts.
- [ ] The same job or a sibling runs `task e2e`, or the issue records why `check-integration`'s E2E coverage is sufficient once `ci-executed-leaf-guard` makes it real.
- [ ] Each new job fails on a real regression. On a branch, show a deliberate break (for example making `demo/walkthrough.sh` exit 1, or breaking a smoke target) producing a red job, then revert, and cite both run URLs in the close reason. If str-qwua7.10's content assertions have landed, the walkthrough job enforces them.
- [ ] perf-ci's `Run stable perf scenarios` step prints the failing scenario's stderr. `perf_runner.py` surfaces the child output on failure. Then either the gauntlet-auto-warm failure is fixed, with a green `perf-ci.yml` run URL cited, or the scenario/job is disabled with the reason and a tracking issue id written in the workflow file.
- [ ] `scripts/test_ci_workflow_structure.py` asserts the new job and steps exist, so they cannot be dropped silently.

## Suggested approach

Add a separate `user-paths` job in `ci.yml` (push to main only, not PRs, to keep PR latency down), or a new nightly workflow. Reuse the apt/toolchain setup from the `test` job. `workflow-health-patrol` will then cover the nightly run. Combine with str-qwua7.10 so the walkthrough job checks content, not just the exit code.

## Out of scope

- The Task checksum problem itself (str-qwua7.3 / `ci-executed-leaf-guard`).
- Gauntlet allowlist content.
- Moving perf-ci paths (str-qwua7.42).

## Dependencies

- None hard-blocking. The smoke/walkthrough job does not depend on the checksum fix.
- Related: str-qwua7.10 (open; demo gates fail on bad content), str-qwua7.42 (perf-ci paths), `ci-executed-leaf-guard`, `workflow-health-patrol` (links this issue as the perf-ci follow-up).

Priority: P2 · Type: task · Labels: ci, smoke, walkthrough, gauntlet, e2e, audit · Parent: Epic: Audit 2026-09-22 findings · Sources: shatter-code/73, tests-ci-11
