# land-work: report GitHub workflow conclusions for the landed SHA; doctor flags persistently red workflows

- Filing action: new issue
- Priority: P1
- Type: feature
- Labels: audit, land-work, hygiene
- Parent: epic
- Links: related bento-1qry, related bento-rdtn.14
- Source findings: tests-ci-03 (bento part), prior-01/02, agent-repo-03 (context)

---BODY---
## Problem

Nothing in the agent workflow looks at GitHub Actions results after a landing. In shatter, every workflow except ci.yml has been permanently red, and nobody noticed:

- Drift Patrol: 7/7 scheduled runs failed (08-10 to 09-21). setup-go points at a nonexistent root go.mod.
- Build and Release: 0 successes in 200 runs.
- Perf CI: 0/13.
- Devcontainer: 19 failures.
- Docker: 0/7.

Separately, ci.yml's `task check` has been hollow since 2026-08-30 because stage-2/3 tests are cached. Agents close issues citing a local verifier pass or a sibling PR job. The fixes to the workflows themselves are shatter issues; this issue is the missing feedback loop in bento.

## Current code facts

- `catalog/skills/land-work/scripts/land.py` ends after `merge_push` and `verify_landing`. It makes no `gh run` query.
- `land-work-verify-landing.py` checks that the ref landed, not CI.
- bento-1qry (in_progress) binds local gate evidence and a primary-red stop. It does not consult GitHub workflow conclusions.
- `agent-env-doctor.py` has no workflow-health check.

## Acceptance criteria

- After push, land.py (or verify-landing) optionally polls `gh run list --commit <landed-sha>` for up to N minutes (configurable in verifier.json). It prints each workflow's conclusion and includes them in the final JSON as `workflows: [{name, conclusion, url}]`. `failure` is reported as a warning. Landing is not rolled back.
- The step is skipped cleanly when `gh` is missing or unauthenticated, or the remote is not GitHub. The reason appears in the JSON.
- The doctor, or a closure report, lists workflows on the primary branch whose last 3 or more runs all failed, with the latest run URL. The output is collapsed to one line per workflow.
- Tests use a stubbed `gh`.

## Suggested approach

Reuse the landed SHA that verify-landing already has. Keep polling bounded (default about 10 min) and non-blocking. Include scheduled workflows in the doctor check, because they are the ones nobody watches.

## Out of scope

- Fixing shatter's workflows (drift-patrol.yml go-version-file, release.yml Windows Z3/aarch64 openssl, perf-ci). Those are shatter issues.
- Making CI a hard landing gate.
