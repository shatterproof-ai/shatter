---
slug: perf-ci-stable-scenarios-red
kind: new
title: "Perf CI red 13/13: gauntlet-auto-warm exits 1 with its output suppressed; surface child output in perf_runner.py, then fix the scenario"
priority: P2
type: bug
labels: [ci, github-actions, perf, gauntlet, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Perf CI red 13/13: gauntlet-auto-warm exits 1 with its output suppressed; surface child output in perf_runner.py, then fix the scenario

## Problem

The weekly Perf CI workflow has failed on every run. It fails in `Run stable perf scenarios`, where `scripts/perf_runner.py` reports only that `gauntlet-auto-warm failed on run 1 with exit code 1` and prints none of the scenario's own output. Nobody can diagnose the failure from the log. The workflow is also the only place the gauntlet runs in CI.

This was split out of `ci-runs-user-paths` so that fixing the perf runner does not hold up adding the user-path CI job, and the other way round. It is the perf-ci follow-up that `workflow-health-patrol` links.

## Evidence

Re-verified 2026-09-23:

- `gh run list --workflow perf-ci.yml -L 300` gives `{"failure":13}`.
- Latest run 35620979400 (schedule, 2026-09-21), job 106403653337. The log, fetched with `gh api repos/shatterproof-ai/shatter/actions/jobs/106403653337/logs`, contains:
  - `gauntlet-auto-warm failed on run 1 with exit code 1`
  - `##[error]Process completed with exit code 1.`

  No scenario stderr precedes these lines. The same log also shows `Restore cache failed: Dependencies file is not found ... go.sum`; `workflow-action-versions` owns that.
- `.github/workflows/perf-ci.yml:57-58`: `python3 scripts/perf_runner.py run --scenario-file perf/stable-scenarios.txt --results-dir "$RESULTS_DIR"`. `perf/stable-scenarios.txt` lists `gauntlet-auto-warm`, `explore-ts-arithmetic-warm`, `scan-standalone-ts-warm` and `go-frontend-instrument-tests`. The scenario is defined in `perf/scenarios.json:28`.
- str-qwua7.42 (open) only moves perf-ci paths during the benchmarks merge. It does not address the failure.

## Acceptance criteria

- [ ] **Diagnosis first.** `scripts/perf_runner.py` prints the failing scenario's command, exit code, and the last 200 lines of its stdout and stderr when a scenario fails. A unit test (`scripts/test_perf_runner.py` or similar, run by `task meta`) uses a stub scenario that exits 1 with known stderr, and asserts that the stderr appears in the runner's output. Show the test failing before the change.
- [ ] The real cause of the `gauntlet-auto-warm` failure is recorded in the issue, quoted from a CI run that includes the new output (cite the run URL).
- [ ] **Resolution.** The cause is fixed, and a `perf-ci.yml` run on main (dispatch or schedule) is green with `gauntlet-auto-warm` still in `perf/stable-scenarios.txt`.
  - Interim option: if the fix is blocked on another issue, temporarily remove `gauntlet-auto-warm` from the list, with a comment naming this issue's id and the blocking issue. This issue then stays open. A temporary removal never satisfies this item.
  - Disabling the whole workflow is not an acceptable resolution.
- [ ] **Close-time proof.** The close reason contains the URL of a green `perf-ci.yml` run whose log shows `gauntlet-auto-warm` executing.

## Out of scope

- Adding smoke, walkthrough or E2E jobs to CI (`ci-runs-user-paths`).
- Perf baseline policy and thresholds.
- Moving perf-ci paths (str-qwua7.42).

## Dependencies

- None blocking.
- Related: `ci-runs-user-paths` (split from it), `workflow-health-patrol`, `workflow-action-versions` (go.sum cache warning), str-qwua7.42.

Priority: P2 · Type: bug · Labels: ci, github-actions, perf, gauntlet, audit · Parent: Epic: Audit 2026-09-22 findings · Sources: shatter-code/73, tests-ci-11, tests-ci-03 (split from ci-runs-user-paths in revision)
