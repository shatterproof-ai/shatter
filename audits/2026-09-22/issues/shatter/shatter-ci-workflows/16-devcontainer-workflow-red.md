---
slug: devcontainer-workflow-red
kind: new
title: "Devcontainer CI red since February: post-create.sh runs `bd init --from-jsonl`, which bd now refuses because origin has Dolt history"
priority: P2
type: bug
labels: [ci, github-actions, devcontainer, beads, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Devcontainer CI red since February: post-create.sh runs `bd init --from-jsonl`, which bd now refuses because origin has Dolt history

## Problem

`.github/workflows/devcontainer.yml` runs weekly and on pushes to `.devcontainer/**`. It has failed 19 times, and its last success was in 2026-02. The current failure happens before any test runs: `.devcontainer/post-create.sh` initializes beads from the committed JSONL, and bd refuses because the `origin` remote already carries Dolt history. The container never finishes `postCreateCommand`, so the `cargo test` / `clippy` / `npm test` / `go test` `runCmd` never executes. The devcontainer is therefore an untested onboarding path.

Maintainer decision D4 (2026-09-23) retires the JSONL import and moves tracker sync to a Dolt remote. The devcontainer's tracker bootstrap has to follow D4: adopt the remote rather than import JSONL. Do not add a timeout env var or any hook-bypass guidance.

This draft replaces the "file a devcontainer follow-up" step that `workflow-health-patrol` used to carry. Under D6, the maintainer's filer creates it.

## Evidence

Re-verified 2026-09-23:

- `gh run list --workflow devcontainer.yml -L 300` gives failure 19 and success 2 (2026-02). The last three runs are all `schedule` failures: 35619881868 (09-21), 34863054869 (09-14) and 34134350747 (09-07).
- Run 35619881868, job 106399983455, step `Build devcontainer and run tests`. The log, fetched with `gh api repos/shatterproof-ai/shatter/actions/jobs/106399983455/logs`, contains:
  - `==> Initializing beads issue tracker...`
  - `bd init refuses: remote 'origin' already has Dolt history (refs/dolt/data).` bd suggests `bd bootstrap` ("Adopt the remote (recommended)").
  - `postCreateCommand from devcontainer.json failed with exit code 10.`
- `.devcontainer/post-create.sh:43-51`:

  ```sh
  if [ -f .beads/issues.jsonl ] && [ ! -d .beads/dolt/beads_str ]; then
    bd init --prefix str --from-jsonl --quiet
    bd import -i .beads/issues.jsonl
  ```

  This is followed by `bd config set beads.role maintainer`.
- `.github/workflows/devcontainer.yml:13-24`: `devcontainers/ci@v0.3` with `runCmd` `cargo test`, `cargo clippy -- -D warnings`, `cd shatter-ts && npm test`, `cd ../shatter-go && go test ./...`. It runs bare `cargo test`, not `task` gates, and does not cover the standalone crates.
- Earlier failures (before about 2026-09) may have had other causes. Only the current first error is verified here.

## Acceptance criteria

- [ ] `post-create.sh` no longer imports `.beads/issues.jsonl`. It adopts the tracker the way D4's Dolt-remote design specifies (for example `bd bootstrap`). If CI has no credentials for the Dolt remote, the script skips tracker setup with an explicit log line; tracker setup is not needed to run tests. The script fails only on real errors. Do not add `|| true` around the whole block.
- [ ] Once post-create completes, the `runCmd` runs. Any test failures it then exposes are fixed, or each is recorded with the failing command and first error in the close reason, and this issue stays open until the workflow is green.
- [ ] The `runCmd` uses `task` targets consistent with the local gates (for example `task test-quick` or `task check-unit`), or a comment in the workflow explains why it runs bare commands.
- [ ] **Close-time proof.** The close reason contains the URL of a green `devcontainer.yml` run, `workflow_dispatch` or schedule, on main.

## Suggested approach

Coordinate with the tracker bucket's `beads-retire-jsonl-import-dolt-remote` and `beads-jsonl-consumers-drop-bd-sync`: this script is one more JSONL consumer. Iterate with `gh workflow run devcontainer.yml --ref <branch>`. The workflow publishes nothing, so branch runs are safe.

## Out of scope

- Designing the Dolt remote sync itself (tracker bucket, D4).
- Devcontainer feature changes that are not needed to go green.

## Dependencies

- None blocking in this bucket. Coordinate with `beads-retire-jsonl-import-dolt-remote` and `beads-jsonl-consumers-drop-bd-sync` (shatter-tracker-and-beads bucket).
- Related: `workflow-health-patrol` (flags this workflow), `workflow-action-versions`.

Priority: P2 · Type: bug · Labels: ci, github-actions, devcontainer, beads, audit · Parent: Epic: Audit 2026-09-22 findings · Sources: tests-ci-03, agent-repo-03 (split out of workflow-health-patrol in revision) · Decision: D4, D6
