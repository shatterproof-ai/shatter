---
slug: workflow-health-patrol
kind: new
title: "Surface persistently red GitHub workflows to agents: drift-patrol workflow-health check, landing step, and follow-up issues for perf-ci, devcontainer and docker-publish"
priority: P1
type: task
labels: [agents, ci, github-actions, drift, landing, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [drift-patrol-workflow-go-mod]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Surface persistently red GitHub workflows to agents: drift-patrol workflow-health check, landing step, and follow-up issues for perf-ci, devcontainer and docker-publish

## Problem

Several main-branch and scheduled workflows fail on every run, and until this audit none of them had a tracker issue. Landing and all agent checks look only at the local verifier and, at most, `ci.yml`, so a workflow that stays red is invisible to agents. The failures include Build and Release (0 successes in 267 runs), Drift Patrol (7/7 scheduled runs), Perf CI (13/13) and Devcontainer (19 failures, last success February). The patrol that should report drift is itself one of the broken workflows (`drift-patrol-workflow-go-mod`).

This issue adds the missing feedback loop on the shatter side: a read-only `workflow-health` drift-patrol check, plus a landing instruction to run it. It also files the per-workflow fix issues that do not exist yet. The bento-side counterpart, polling CI for the landed main SHA inside `land-work`, is `land-work-post-push-workflow-health` in the bento tracker. That is an intentional cross-repo split.

## Evidence

Re-verified 2026-09-23 with `gh run list --workflow <w> -L 300 --json conclusion`:

| Workflow | Trigger | Conclusions | Failing step (latest run) |
|---|---|---|---|
| `release.yml` | push main, dispatch | failure 169, cancelled 98, success 0 | Windows `z3.h` not found; aarch64 `openssl-sys` (run 35773969737). Now filed: `release-windows-z3-build`, `release-aarch64-openssl-cross`, `release-publish-and-install-smoke` |
| `drift-patrol.yml` | schedule Mon 09:00, dispatch, PR | failure 7, success 2 (PR only) | setup-go `go.mod does not exist`. Now filed: `drift-patrol-workflow-go-mod` |
| `perf-ci.yml` | schedule Mon 09:00, dispatch | failure 13 | `perf` / `Run stable perf scenarios` (run 35620979400); the audit saw "gauntlet-auto-warm failed on run 1 with exit code 1" with the reason not surfaced |
| `devcontainer.yml` | push main (`.devcontainer/**`), schedule, dispatch | failure 19, success 2 (2026-02) | `build-and-test` / `Build devcontainer and run tests` (run 35619881868) |
| `docker-publish.yml` | push tags `v*`, PR to main | failure 7 | `build-and-push` / `Build and push` (run 31210298465, cargo build exit 101). It has no main or schedule trigger, so the check below does not cover it; its last run was a 2026-08-07 PR |
| `parity-expiry.yml` | schedule | success 9, failure 3 (08-31, 09-07, 09-14; green 09-21) | not persistently red; the check must not flag it |
| `ci.yml` | push/PR main | success 98, failure 72, cancelled 4 (latest green) | n/a |

- `scripts/drift-patrol.py:757-766`: the `CHECKS` registry has no workflow-health check.
- AGENTS.md mentions `gh run` only in an rtk example (`AGENTS.md:581`). No AGENTS.md, CLAUDE.md or skill instruction tells agents to check workflow conclusions after landing.
- `bd search` for windows / aarch64 / "release workflow" / perf-ci / devcontainer / docker found no open issue at audit time (agent-repo-03, prior-02).
- str-qwua7.42 (open) only moves `perf-ci.yml` paths during the benchmarks merge. It does not address the red runs.

## Acceptance criteria

- [ ] New drift-patrol check `workflow-health`, registered in `CHECKS` and documented in `docs/DRIFT-PATROL.md` (the table test added by `drift-patrol-workflow-go-mod` enforces this):
  - It covers each workflow in `.github/workflows/` that triggers on `push` to main or on `schedule`.
  - It reads the last N (default 3, flag `--workflow-health-runs`) completed runs on `main` via `gh run list --workflow <file> --branch main --json conclusion,databaseId,url,createdAt`.
  - It reports FAIL when all N failed, printing the workflow name, the last run URL and the first failing job and step (`gh run view <id> --json jobs`).
  - It reports SKIP (not PASS) when `gh` is missing or unauthenticated, or when a workflow has fewer than N completed runs.
  - With the current data it FAILs on release, perf-ci and devcontainer (and on drift-patrol until that fix lands) and PASSes on ci and parity-expiry.
- [ ] Unit tests for the check in `scripts/test_drift_patrol.py` use canned `gh` JSON and cover: all-red → FAIL, mixed → PASS, `gh` unavailable → SKIP, fewer than N runs → SKIP.
- [ ] The AGENTS.md landing section says: after pushing main, run `python3 scripts/drift-patrol.py --only workflow-health`, and make sure a tracker issue exists for every FAIL before ending the session.
- [ ] Every persistently red workflow has a linked fix issue, each with the failing run URL and first error:
  - (a) perf-ci `Run stable perf scenarios` / gauntlet-auto-warm: already covered by `ci-runs-user-paths` in this bucket, which requires surfacing the suppressed stderr and fixing or disabling the job. Link it. Do not file a duplicate.
  - (b) devcontainer `Build devcontainer and run tests`: file a new child of the audit epic.
  - (c) docker-publish `Build and push` (cargo exit 101): file a new child of the audit epic.
  For perf-ci, devcontainer and docker-publish, the fix issue may choose to fix the workflow, or to disable it with the reason documented in the workflow file and the issue.
- [ ] Release targets are not disabled. Under D1 they are fixed in `release-windows-z3-build` / `release-aarch64-openssl-cross` / `release-publish-and-install-smoke`. This issue links those and files nothing more for release.
- [ ] Proof at close: paste the output of `python3 scripts/drift-patrol.py --only workflow-health` from a real, authenticated run, and include the ids of the devcontainer and docker-publish follow-up issues.

## Suggested approach

Reuse the existing `Result` / PASS-FAIL-SKIP-PENDING structure and `run_command` helper in `scripts/drift-patrol.py`. Keep the check read-only: it reports, it never files. Filing the devcontainer and docker-publish follow-ups is a manual step of this issue. In CI the scheduled patrol has `GITHUB_TOKEN`. Give the patrol job `actions: read` permission so `gh run list` works there too.

## Out of scope

- Fixing the perf-ci (`ci-runs-user-paths`), devcontainer or docker-publish builds (the follow-ups do that).
- Fixing the release builds (already filed in this bucket).
- Bento `land-work` CI polling (bento tracker: `land-work-post-push-workflow-health`).

## Dependencies

- Blocked by: `drift-patrol-workflow-go-mod`. The check is only useful once the patrol itself runs on schedule.
- Related: `release-windows-z3-build`, `release-aarch64-openssl-cross`, `release-publish-and-install-smoke`, `ci-runs-user-paths` (perf-ci gauntlet overlap), str-qwua7.42, bento `land-work-post-push-workflow-health` (cross-repo; link in body text only).

Priority: P1 · Type: task · Labels: agents, ci, github-actions, drift, landing, audit · Parent: Epic: Audit 2026-09-22 findings · Sources: shatter-agent/03, agent-repo-03, tests-ci-03 · Decision: D1
