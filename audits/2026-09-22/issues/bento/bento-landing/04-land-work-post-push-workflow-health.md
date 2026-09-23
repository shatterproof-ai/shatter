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

Bento code, at origin/main 0b8d488 (land-work scripts unchanged since the audit's 1c0c1e6):
- `catalog/skills/land-work/scripts/land.py:273-341`: `run()` ends after `verify_landing`. There is no `gh` call anywhere in land-work (`grep -rn "gh run" catalog/skills/land-work` finds nothing).
- `land-work-verify-landing.py` checks that the expected tree landed on the ref, not CI.
- `catalog/hooks/bento/{claude,codex}/scripts/agent-env-doctor.py` has no workflow-health check (no `gh` invocation).
- bento-1qry (in_progress) binds local gate evidence and adds a stop when the primary branch is red locally. It does not consult GitHub workflow conclusions.

## Acceptance criteria

Post-push reporting (land.py):

- [ ] After `verify_landing` succeeds, land.py polls `gh run list --commit <merge_sha> --json databaseId,name,workflowName,event,status,conclusion,url --limit 100`. Polling has two phases with separate budgets from verifier.json `post_push_workflows: {enabled, discovery_s, timeout_s}` (defaults: enabled, `discovery_s` about 90, `timeout_s` about 600; `enabled: false` disables it):
  - **Discovery.** GitHub creates runs asynchronously, so an empty first response does not mean "no workflows". land.py keeps polling until at least one run appears or `discovery_s` expires. If none ever appears it reports `workflows_status: no_runs` (distinct from `complete`).
  - **Settling.** Once runs exist, land.py keeps polling until every run it has seen is `completed` and no new run has appeared for one further poll interval, or `timeout_s` expires (`workflows_status: timed_out`, with the incomplete runs listed).
  - **Pagination.** If a response returns exactly `--limit` rows, land.py reports `workflows_truncated: true` rather than silently treating the first page as the full set.
- [ ] land.py prints one line per workflow run and adds `workflows: [{name, event, conclusion, url}]` and `workflows_status: complete|no_runs|timed_out|skipped` to the final JSON. A `failure` conclusion is a warning: the landing is not rolled back and the exit code is unchanged.
- [ ] The step is skipped cleanly, with `workflows_status: skipped` and `workflows_skipped_reason`, when `gh` is missing or unauthenticated or the remote is not GitHub.

Doctor (SessionStart):

- [ ] The doctor fetches the primary branch's recent runs in **one** call, `gh run list --branch <primary> --status completed --limit 200 --json workflowName,conclusion,createdAt,url`, groups them by workflow client-side, and lists each workflow (scheduled ones included) whose 3 most recent completed runs all have conclusion `failure`. In-progress and queued runs are excluded by `--status completed`; `cancelled` and `skipped` runs are ignored when counting, so they neither break nor extend a streak. Output is one collapsed line per red workflow with the latest failed run URL.
- [ ] The result is cached per repo for one hour, so SessionStart makes at most one `gh` call per repo per hour. A workflow with fewer than 3 completed runs in the window is not flagged.

Tests and proof:

- [ ] Tests use a stubbed `gh` on PATH (a script that replays a sequence of responses) and cover: all green; one failure (warning, exit 0); **initially empty then runs appear** (reported `complete`, not `no_runs`); never any runs (`no_runs` after `discovery_s`); a run that appears after the first ones completed (included); timeout; truncated page; `gh` missing; non-GitHub remote; the doctor's streak rule with interleaved `cancelled` runs; and the doctor cache (a second invocation within the hour makes no `gh` call).
- [ ] Proof at close: the test names and passing output, plus one real land.py run against a GitHub repo whose final JSON contains a populated `workflows` array and `workflows_status: complete` (paste it into the close reason). "Merged" is not sufficient.

## Suggested approach

- Reuse the landed SHA that land.py already has (`merge_sha`). Run the poll after `verify_landing`, so a slow CI never delays the landing verdict itself.
- Use a fixed poll interval (for example 15 s) and keep a set of seen `databaseId`s so newly appearing runs are detected.
- Keep the doctor output collapsed. One line per red workflow is the ceiling.

## Out of scope

- Fixing shatter's workflows: drift-patrol `go-version-file` (`drift-patrol-workflow-go-mod`), the release.yml Windows Z3 and aarch64 openssl legs (`release-windows-z3-build` and `release-aarch64-openssl-cross`, which per maintainer decision D1 are fixed, not dropped), perf-ci, devcontainer and docker. Those are shatter issues.
- Making CI a hard landing gate.
- Filing tracker issues automatically for red workflows. That belongs to the shatter-side `workflow-health-patrol`.

## Dependencies

- Blocked by: none.
- Related: bento-2jo (open; the pre-push side: warn when pushing main triggers publishing workflows), bento-1qry (in_progress; local gate evidence), bento-rdtn.14 (closed; land.py), shatter `workflow-health-patrol` (repo-side counterpart), `merge-push-observability`.

Priority: P1 · Type: feature · Labels: audit, land-work, hygiene, ci · Parent: Epic: Audit 2026-09-22 findings (bento) · Sources: bento/04, tests-ci-03 (verifier kept P1)
