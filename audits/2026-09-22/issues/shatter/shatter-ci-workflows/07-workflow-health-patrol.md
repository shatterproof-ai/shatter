---
slug: workflow-health-patrol
kind: new
title: "Surface persistently red GitHub workflows to agents: an authenticated, self-safe drift-patrol workflow-health check plus a landing step"
priority: P1
type: task
labels: [agents, ci, github-actions, drift, landing, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [drift-patrol-workflow-go-mod]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Surface persistently red GitHub workflows to agents: an authenticated, self-safe drift-patrol workflow-health check plus a landing step

## Problem

Several main-branch and scheduled workflows fail on every run. Until this audit, none of them had a tracker issue. Landing and all agent checks look only at the local verifier and, at most, `ci.yml`, so a workflow that stays red is invisible to agents. The persistently red workflows are Build and Release (0 successes in 267 runs), Drift Patrol (7/7 scheduled runs), Perf CI (13/13) and Devcontainer (19 failures; the last success was in February). The patrol that should report drift is itself one of the broken workflows (`drift-patrol-workflow-go-mod`).

This issue adds the missing feedback loop on the shatter side:

- a read-only `workflow-health` drift-patrol check;
- a landing instruction to run it.

The per-workflow fix issues already exist as drafts in this audit bundle and are filed by the maintainer's filer (D6). This issue files nothing.

The bento-side counterpart, which polls CI for the landed main SHA inside `land-work`, is `land-work-post-push-workflow-health` in the bento tracker. The cross-repo split is intentional.

Two design constraints come from how the patrol itself runs:

1. **The check must not be self-perpetuating.** The Drift Patrol workflow goes red whenever any patrol check FAILs; that is its reporting mechanism (`drift-patrol.yml:118-126`, "Explain a red run"). If `workflow-health` counted Drift Patrol's own red runs as "workflow broken", then after any three weeks of real drift findings, including `workflow-health` flagging release.yml, the check would FAIL on Drift Patrol too. That FAIL would keep the patrol red, the next run would count it again, and fixing every underlying defect would not break the loop until three clean weeks passed. A red run whose failing step is the patrol-report step is a report, not a broken workflow.
2. **The check needs explicit authentication in CI.** `drift-patrol.yml`'s patrol job exports neither `GH_TOKEN` nor `GITHUB_TOKEN` to its shell steps (`:58-126`). The `repo-token` passed to `arduino/setup-task` (`:87-91`) is consumed by that action only. The job's `permissions:` block grants only `contents: read`. As written, `gh run list` inside the patrol would be unauthenticated, and a check that SKIPs on missing auth would pass silently forever.

## Evidence

Re-verified 2026-09-23 with `gh run list --workflow <w> -L 300 --json conclusion` and `gh run view <id> --json jobs`:

| Workflow | Trigger | Conclusions | Failing step (latest run) |
|---|---|---|---|
| `release.yml` | push main, dispatch | failure 169, cancelled 98, success 0 | Windows `z3.h` not found; aarch64 `openssl-sys` (run 35773969737). Drafted: `release-publish-guard-and-target`, `release-windows-z3-build`, `release-aarch64-openssl-cross`, `release-publish-and-install-smoke` |
| `drift-patrol.yml` | schedule Mon 09:00, dispatch, PR | failure 7, success 2 (PR only) | `Setup Go`: `go.mod does not exist`. Drafted: `drift-patrol-workflow-go-mod` |
| `perf-ci.yml` | schedule Mon 09:00, dispatch | failure 13 | `perf` / `Run stable perf scenarios` (run 35620979400). Drafted: `perf-ci-stable-scenarios-red` |
| `devcontainer.yml` | push main (`.devcontainer/**`), schedule, dispatch | failure 19, success 2 (2026-02) | `build-and-test` / `Build devcontainer and run tests` (run 35619881868, schedule, 2026-09-21). Drafted: `devcontainer-workflow-red` |
| `docker-publish.yml` | push tags `v*`, PR to main | failure 7 | `build-and-push` / `Build and push` (run 31210298465, pull_request, 2026-08-07). It has no main or schedule trigger, so this check does not cover it. Drafted: `docker-publish-workflow-red` |
| `parity-expiry.yml` | schedule | success 9, failure 3 (08-31, 09-07, 09-14; green 09-21) | Not persistently red; the check must not flag it |
| `ci.yml` | push/PR main | success 98, failure 72, cancelled 4 (latest green) | n/a |

- `scripts/drift-patrol.py:757-767`: the `CHECKS` registry has no workflow-health check. `run_command` is at `:110`.
- `.github/workflows/drift-patrol.yml:58-68`: the `patrol` job has `permissions: contents: read` and an `env:` block with only `STALE_DAYS` and `STRICT_PENDING`.
- AGENTS.md mentions `gh run` only in an rtk example (`AGENTS.md:581`). No AGENTS.md, CLAUDE.md or skill instruction tells agents to check workflow conclusions after landing.
- `bd search` for windows, aarch64, "release workflow", perf-ci, devcontainer and docker found no open issue at audit time (agent-repo-03, prior-02). str-qwua7.42 (open) only moves `perf-ci.yml` paths during the benchmarks merge and does not address the red runs.

## Acceptance criteria

- [ ] **The check.** A new drift-patrol check, `workflow-health`, is registered in `CHECKS` and documented in `docs/DRIFT-PATROL.md` (the table test added by `drift-patrol-workflow-go-mod` enforces the documentation):
  - It covers each workflow in `.github/workflows/` whose `on:` includes `push` to main, `schedule` or `workflow_run`. Any smoke job inside `release.yml` is covered through `release.yml`.
  - It reads the last N completed runs on `main` (default 3, flag `--workflow-health-runs`) with `gh run list --workflow <file> --branch main --json conclusion,databaseId,url,createdAt`.
  - It classifies each failed run by its first failing job and step (`gh run view <id> --json jobs`).
  - It reports FAIL when all N runs failed, printing the workflow name, the last run URL, and the first failing job and step.
- [ ] **Self-safety.** A failed run of `drift-patrol.yml` whose first failing step is the patrol-report step (`Run drift patrol`) counts as a report, not a failure. Only a Drift Patrol failure in setup or build steps (for example `Setup Go`) counts toward FAIL. Identify the report step by its step `id: patrol`, or by a name constant shared with the workflow file, not by a hard-coded string that can drift silently. The structure test from `drift-patrol-workflow-go-mod` asserts that the step still exists.
- [ ] **Authentication.**
  - `drift-patrol.yml`'s patrol job grants `permissions: { contents: read, actions: read }` and exports `GH_TOKEN: ${{ github.token }}` to the patrol step.
  - Locally, the check reports SKIP when `gh` is missing or unauthenticated.
  - When `GITHUB_ACTIONS=true`, missing or unauthenticated `gh` is a FAIL, not a SKIP, so the scheduled patrol cannot pass silently.
  - A workflow with fewer than N completed runs on main reports SKIP with a reason.
- [ ] **Unit tests** in `scripts/test_drift_patrol.py`, using canned `gh` JSON:
  - all-red → FAIL;
  - mixed → PASS;
  - `gh` unavailable locally → SKIP;
  - `gh` unavailable with `GITHUB_ACTIONS=true` → FAIL;
  - fewer than N runs → SKIP;
  - **recovery:** drift-patrol.yml's last 3 runs all failed at `Run drift patrol` → no FAIL for drift-patrol.yml;
  - drift-patrol.yml's last 3 runs all failed at `Setup Go` → FAIL.
- [ ] **Current data.** Run locally, the check FAILs on release, perf-ci and devcontainer (and on drift-patrol while the `Setup Go` failure persists), and PASSes on ci and parity-expiry. Paste the output.
- [ ] **Landing step.** The AGENTS.md landing section says: after pushing main, run `python3 scripts/drift-patrol.py --only workflow-health`. For each FAIL, confirm that a tracker issue exists, and name it in the landing notes. If none exists, report it to the maintainer.
- [ ] Release targets are not disabled. Under D1 they are fixed by the release drafts in this bundle, and this issue only links them.
- [ ] **Close-time proof:**
  - (a) the local authenticated output of `python3 scripts/drift-patrol.py --only workflow-health`;
  - (b) the URL of a scheduled or `workflow_dispatch` Drift Patrol run on main whose step summary shows `workflow-health` rows with PASS or FAIL results (not SKIP), proving that the CI token wiring works.

## Suggested approach

Reuse the existing `Result` / PASS-FAIL-SKIP-PENDING structure and the `run_command` helper in `scripts/drift-patrol.py`. Keep the check read-only: it reports and never files. Put the report-step exemption in a small table keyed by workflow file, so a future reporter workflow can opt in the same way.

## Out of scope

- Fixing perf-ci (`perf-ci-stable-scenarios-red`), devcontainer (`devcontainer-workflow-red`), docker-publish (`docker-publish-workflow-red`) or the release builds (the release drafts).
- Bento `land-work` CI polling (bento tracker: `land-work-post-push-workflow-health`).

## Dependencies

- Blocked by: `drift-patrol-workflow-go-mod`. The check is only useful once the patrol itself runs on schedule.
- Related: `release-publish-guard-and-target`, `release-windows-z3-build`, `release-aarch64-openssl-cross`, `release-publish-and-install-smoke`, `perf-ci-stable-scenarios-red`, `devcontainer-workflow-red`, `docker-publish-workflow-red`, `ci-runs-user-paths`, str-qwua7.42, bento `land-work-post-push-workflow-health` (cross-repo; link in body text only).

Priority: P1 · Type: task · Labels: agents, ci, github-actions, drift, landing, audit · Parent: Epic: Audit 2026-09-22 findings · Sources: shatter-agent/03, agent-repo-03, tests-ci-03 · Decision: D1, D6
