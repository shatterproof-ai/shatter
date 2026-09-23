---
slug: drift-patrol-workflow-go-mod
kind: new
title: "Fix the scheduled Drift Patrol workflow (setup-go points at a nonexistent root go.mod; 7/7 scheduled runs red) and test every workflow path"
priority: P1
type: bug
labels: [ci, github-actions, drift, agents, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Fix the scheduled Drift Patrol workflow (setup-go points at a nonexistent root go.mod; 7/7 scheduled runs red) and test every workflow path

## Problem

The weekly Drift Patrol workflow has never run its patrol step on a schedule. `.github/workflows/drift-patrol.yml:85` sets `go-version-file: go.mod`, but the repo has no `go.mod` at its root, so the `patrol` job fails in `actions/setup-go` within about 20 seconds. The workflow's only green runs are the 2026-08-07 pull_request runs, and on pull requests the `patrol` job is skipped (`if: github.event_name != 'pull_request'`, `:60`). str-u394l.1 ("Scheduled drift patrol") was closed on that PR evidence. The patrol meant to catch drift has been red for seven weeks and nobody was alerted. This is a same-class recurrence of str-wnyzy, which fixed the identical bug in `ci.yml` only.

`docs/DRIFT-PATROL.md` also overstates the protection. It says the PR self-test means "the patrol cannot rot in place", but the self-test only unit-tests the Python. Its "What it checks" table also omits the `tracker-server` check that `AGENTS.md:22` tells agents to run (docs-23).

## Evidence

Re-verified 2026-09-23 against the worktree at `56c86168`:

- `.github/workflows/drift-patrol.yml:82-85`: `uses: actions/setup-go@v5` / `go-version-file: go.mod` (wrong, and no `cache-dependency-path`). The other workflows use `shatter-go/go.mod`: `ci.yml:53`, `perf-ci.yml:36`, `release.yml:131-132` (only release also sets `cache-dependency-path: shatter-go/go.sum`).
- `gh run list --workflow drift-patrol.yml` → `{"failure":7,"success":2}`. The failures are the schedule runs of 08-10, 08-17, 08-24, 08-31, 09-07, 09-14 and 09-21, and the two successes are the 08-07 PR runs. `gh run view 35620815498` (09-21 schedule) shows: `The specified go version file at: go.mod does not exist`.
- The patrol job installs no system packages. `ci.yml:61-64` installs `libclang-dev z3`. Once setup-go is fixed, the `task ts:build go:build rust-fe:build` step (`:115-116`) or the `--require-conformance` patrol (`:118-126`) may surface the next missing dependency.
- `scripts/test_ci_workflow_structure.py` never mentions `drift-patrol.yml`, and it checks only `ci.yml`'s shape. str-35vtk.35 (open) wires that script into a local Taskfile task.
- `docs/DRIFT-PATROL.md:27-28`: "...so the patrol cannot rot in place." `:24` Owner: "The maintainer on the weekly triage rotation; if there is no rotation, whoever is landing work that week". No rotation exists.
- `docs/DRIFT-PATROL.md:33-43` "What it checks" lists 7 checks. `scripts/drift-patrol.py:757-766` `CHECKS` registers 8 (the extra one is `("tracker-server", check_tracker_server)`, defined at `:686`, added by str-qwua7.16). `AGENTS.md:22` references `--only tracker-server`.
- Tracker data in CI: `scripts/drift-patrol.py:183-221` `load_tracker()` falls back to the committed `.beads/issues.jsonl` when `bd` is absent, as it is in CI. D4 (2026-09-23) retires the JSONL import. `beads-jsonl-consumers-drop-bd-sync` (shatter-tracker-and-beads bucket) owns changing that data source.
- Run locally, `python3 scripts/drift-patrol.py` does run and reports a tracker-hygiene FAIL. The script works; only the workflow is broken.

## Acceptance criteria

- [ ] `drift-patrol.yml` uses `go-version-file: shatter-go/go.mod` and `cache-dependency-path: shatter-go/go.sum`.
- [ ] A test fails on the current tree and passes after the fix. It lives in `scripts/test_ci_workflow_structure.py` or a new module that `task meta` runs; coordinate with str-35vtk.35. For every `.github/workflows/*.yml` it asserts that each `go-version-file`, `cache-dependency-path`, `working-directory` and `hashFiles(...)` literal path exists in the repo. Paste the failing and then passing output in the close reason.
- [ ] One `workflow_dispatch` or scheduled run of Drift Patrol reaches and executes the `Run drift patrol` step, with its report in the run summary. Cite the run URL in the close reason. A PR run is not sufficient. A patrol FAIL on real drift (for example tracker-hygiene) is acceptable here. A setup or build failure before the patrol step is not.
- [ ] `docs/DRIFT-PATROL.md` changes:
  - adds a `tracker-server` row to the "What it checks" table;
  - adds a unit test in `scripts/test_drift_patrol.py` asserting that every id in `CHECK_IDS` appears in that table;
  - drops the "cannot rot in place" claim, or makes it true by pointing to the path test and to `workflow-health-patrol`;
  - replaces the owner line with a concrete action, for example "the lead of the next landing session reads the last patrol summary and files a `drift` issue for each FAIL".
- [ ] No change to the tracker data source in this issue. If the workflow still reads `.beads/issues.jsonl`, leave a pointer comment to `beads-jsonl-consumers-drop-bd-sync` (D4).

## Suggested approach

Make the one-line workflow fix, add the generic path-existence test, and add the doc row and its test. After merge, run `gh workflow run drift-patrol.yml`, then `gh run watch`. Fix any further setup failure the first real run shows, for example by adding the `ci.yml` apt step.

## Out of scope

- Fixing what the patrol reports (tracker hygiene and so on). Those are separate issues.
- Workflow-health checks for other workflows (`workflow-health-patrol`, which this issue blocks).
- Moving tracker-hygiene off the JSONL (`beads-jsonl-consumers-drop-bd-sync`, D4).

## Dependencies

- Blocks: `workflow-health-patrol`.
- Coordinate with: `beads-jsonl-consumers-drop-bd-sync` (D4), str-35vtk.35.
- Related: str-u394l.1 (closed; see `drift-patrol-reopen-note`), str-wnyzy (closed; same bug in ci.yml), str-qwua7.16 (added tracker-server).

Priority: P1 · Type: bug · Labels: ci, github-actions, drift, agents, audit · Parent: Epic: Audit 2026-09-22 findings · Sources: shatter-agent/02, prior-01, agent-repo-02, docs-23
