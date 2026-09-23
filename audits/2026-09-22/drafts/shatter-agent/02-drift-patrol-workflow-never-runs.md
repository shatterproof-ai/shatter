# Fix scheduled Drift Patrol workflow (setup-go points at nonexistent root go.mod; 7/7 scheduled runs red)

- Priority: P1
- Type: bug
- Labels: agents,ci,github-actions,drift
- Tracker: shatter (bd, /home/ketan/project/shatter)
- Relation: new (str-u394l.1 closed-but-unfixed; same bug class as closed str-wnyzy)
- Source findings: agent-repo-02, prior-01 (related: tests-ci-03)
- Parent: 01 (epic)
- Blocked by: none
- Readiness: drafted to the issue-readiness-check standard; fresh-reviewer precheck still required before filing (see INDEX.md)

<!-- body -->
## Problem
The weekly Drift Patrol GitHub workflow has never executed its patrol step on
a schedule. `.github/workflows/drift-patrol.yml:85` sets
`go-version-file: go.mod`, and there is no `go.mod` at the repo root, so the
job dies in `actions/setup-go`. The only successful runs are the 2026-08-07
pull_request self-test runs, where the patrol job is skipped
(`if: github.event_name != 'pull_request'`). str-u394l.1 ("Scheduled drift
patrol") was closed on that PR evidence. The patrol that is meant to catch
drift has itself been red for 7 weeks with nobody alerted.

## Current Code Facts
- `.github/workflows/drift-patrol.yml:85`: `go-version-file: go.mod` (wrong).
- `ci.yml:53`, `perf-ci.yml:36`, `release.yml:131` all use
  `shatter-go/go.mod` (correct). str-wnyzy fixed the identical bug in
  `ci.yml` only.
- `gh run list --workflow drift-patrol.yml`: failures (schedule) on 08-10,
  08-17, 08-24, 08-31, 09-07, 09-14, 09-21; `gh run view 35620815498`:
  "The specified go version file at: go.mod does not exist".
- `scripts/test_ci_workflow_structure.py` never mentions `drift-patrol.yml`.
- `docs/DRIFT-PATROL.md:24-28` claims the PR self-test means "the patrol
  cannot rot in place"; the self-test only unit-tests the Python. The owner
  is a vague "whoever is landing work that week".
- Locally, `python3 scripts/drift-patrol.py` runs and reports a
  tracker-hygiene FAIL, so the script works; only the workflow is broken.

## Acceptance Criteria
- `drift-patrol.yml` uses `go-version-file: shatter-go/go.mod` (and any
  `cache-dependency-path` points at `shatter-go/go.sum`).
- `scripts/test_ci_workflow_structure.py` (or a new test wired into `meta`)
  asserts that every `go-version-file`, `cache-dependency-path` and
  `working-directory` in every `.github/workflows/*.yml` exists in the repo;
  the test fails on the current tree before the fix.
- One `workflow_dispatch` or scheduled run of Drift Patrol executes the patrol
  step (run URL cited in the close reason). A PR run is not sufficient.
- `docs/DRIFT-PATROL.md` names a concrete owner action (e.g. "the lead of the
  next landing session reads the last patrol summary") and drops the
  "cannot rot in place" claim or makes it true.

## Suggested Approach
One-line workflow fix plus the path-existence test. Trigger the workflow with
`gh workflow run drift-patrol.yml` after merge and paste the run URL.

## Out of Scope
Fixing the things the patrol reports (tracker hygiene etc.) — separate issues.
Workflow-health alerting for other workflows — see draft 03.
