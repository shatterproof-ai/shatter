---
slug: land-work-post-push-workflow-health
kind: new
title: "land-work: report GitHub workflow conclusions for the landed SHA; doctor flags workflows that stay red on the primary branch"
priority: P1
type: feature
labels: [audit, land-work, hygiene, ci]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# land-work: report GitHub workflow conclusions for the landed SHA; doctor flags workflows that stay red on the primary branch

## Problem

Nothing in the bento agent workflow looks at GitHub Actions results after a landing. Agents close issues citing a local verifier pass, while workflows on the primary branch can stay red for weeks without anyone noticing. In shatter, every workflow except `ci.yml` is persistently red. Fixing those workflows is shatter work. This issue adds the missing feedback loop on the bento side, so that any consumer repo, not only shatter, sees workflow conclusions at landing time and at session start.

This is the bento counterpart of shatter's `workflow-health-patrol` (shatter epic, bucket shatter-ci-workflows). That issue adds a repo-side drift-patrol check. This one makes land-work and the doctor report the signal to every agent.

## Evidence

Re-verified 2026-09-23: `gh run list --workflow <w> -L 50 --json conclusion` in the shatter audit worktree (56c86168):

| Workflow | Last 50 runs |
|---|---|
| drift-patrol.yml | 7 failure, 2 success (last success 2026-08-07) |
| perf-ci.yml | 13 failure, 0 success |
| devcontainer.yml | 19 failure, 2 success (both 2026-02) |
| docker-publish.yml | 7 failure, 0 success |
| release.yml | 50 failure, 0 success (0 successes ever; 169 failures and 98 cancellations in the last 300) |
| ci.yml | 41 success, 7 failure, 2 cancelled |

Bento code, at origin/main b1bb787 (land-work scripts unchanged since the audit's 1c0c1e6):
- `catalog/skills/land-work/scripts/land.py:273-341`: `run()` ends after `verify_landing`. There is no `gh` call anywhere in land-work (`grep -rn "gh run" catalog/skills/land-work` finds nothing).
- `land-work-verify-landing.py` checks that the expected tree landed on the ref, not CI.
- `catalog/hooks/bento/{claude,codex}/scripts/agent-env-doctor.py` has no workflow-health check (no `gh` invocation).
- bento-1qry (in_progress) binds local gate evidence and adds a stop when the primary branch is red locally. It does not consult GitHub workflow conclusions.

## Acceptance criteria

- [ ] After a successful push, land.py (or verify-landing) runs `gh run list --commit <landed-sha> --json name,conclusion,status,url`, polling until all runs complete or a timeout expires. The timeout is configurable in verifier.json (`post_push_workflows: {enabled, timeout_s}`, default enabled with about 600 s; 0 disables it).
- [ ] land.py prints one line per workflow and adds `workflows: [{name, conclusion, url}]` plus `workflows_status: complete|timed_out|skipped` to the final JSON. A `failure` conclusion is reported as a warning. The landing is not rolled back and the exit code is unchanged.
- [ ] The step is skipped cleanly, with `workflows_skipped_reason` in the JSON, when `gh` is missing or unauthenticated or the remote is not GitHub.
- [ ] The SessionStart doctor lists each workflow on the primary branch, scheduled workflows included, whose last 3 or more completed runs all failed. It prints one collapsed line per workflow with the latest run URL, and caches the result so SessionStart makes at most one `gh` call per repo per hour.
- [ ] Tests use a stubbed `gh` on PATH and cover: all green, one failure (warning, exit 0), timeout, `gh` missing, non-GitHub remote, and the doctor's 3-consecutive-failures rule.
- [ ] Proof at close: the test names and passing output, plus one real land.py run against a GitHub repo whose final JSON contains a populated `workflows` array (paste it into the close reason). "Merged" is not sufficient.

## Suggested approach

- Reuse the landed SHA that land.py already has (`merge_sha`). Run the poll after `verify_landing`, so a slow CI never delays the landing verdict itself.
- Build the doctor check on `gh run list --branch <primary> --workflow <file> -L 3`, with the list of workflows taken from `gh workflow list`. Include scheduled workflows: they are the ones nobody watches.
- Keep the output collapsed. One line per red workflow is the ceiling.

## Out of scope

- Fixing shatter's workflows: drift-patrol `go-version-file` (`drift-patrol-workflow-go-mod`), the release.yml Windows Z3 and aarch64 openssl legs (`release-windows-z3-build` and `release-aarch64-openssl-cross`, which per maintainer decision D1 are fixed, not dropped), perf-ci, devcontainer and docker. Those are shatter issues.
- Making CI a hard landing gate.
- Filing tracker issues automatically for red workflows. That belongs to the shatter-side `workflow-health-patrol`.

## Dependencies

- Blocked by: none.
- Related: bento-1qry (in_progress; local gate evidence), bento-rdtn.14 (closed; land.py), shatter `workflow-health-patrol` (repo-side counterpart), `merge-push-observability`.

Priority: P1 · Type: feature · Labels: audit, land-work, hygiene, ci · Parent: Epic: Audit 2026-09-22 findings (bento) · Sources: bento/04, tests-ci-03 (verifier kept P1)
