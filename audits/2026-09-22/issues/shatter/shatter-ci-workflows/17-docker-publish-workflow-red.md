---
slug: docker-publish-workflow-red
kind: new
title: "Docker image build fails (7/7): Dockerfile never copies shatter-llm, so `cargo build -p shatter-cli` cannot load the workspace"
priority: P2
type: bug
labels: [ci, github-actions, docker, distribution, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Docker image build fails (7/7): Dockerfile never copies shatter-llm, so `cargo build -p shatter-cli` cannot load the workspace

## Problem

`.github/workflows/docker-publish.yml` builds the root `Dockerfile` on pull requests to main and pushes the image on `v*` tags. It has failed on all 7 recorded runs. The builder stage copies only some workspace members. `shatter-llm` (a workspace member, and a dependency of shatter-core and shatter-cli) is never copied, so `cargo build --release -p shatter-cli` fails while loading the manifest.

The workflow has no push-to-main or schedule trigger. The last run was a 2026-08-07 PR, and no `v*` tag has ever been built successfully, so nobody notices this failure. `workflow-health-patrol` does not cover it either, because it watches only main and scheduled workflows.

This draft replaces the "file a docker-publish follow-up" step that `workflow-health-patrol` used to carry. Under D6, the maintainer's filer creates it.

## Evidence

Re-verified 2026-09-23:

- `gh run list --workflow docker-publish.yml` gives failure 7. The latest is run 31210298465 (pull_request, 2026-08-07): job `build-and-push`, step `Build and push`.
- Log of that job, via `gh api repos/shatterproof-ai/shatter/actions/jobs/<id>/logs`:
  - `[linux/amd64 builder 16/19] RUN sed -i 's/features = \["gh-release"\]/features = []/' shatter-core/Cargo.toml`
  - `[builder 17/19] RUN cargo build --release -p shatter-cli`, which fails with `error: failed to load manifest for workspace member /build/shatter-core ... failed to load manifest for dependency shatter-llm ... failed to read /build/shatter-llm/Cargo.toml ... No such file or directory`
  - `buildx failed with: ... exit code: 101`
- `Dockerfile:32-43`: the builder copies `Cargo.toml`, `Cargo.lock`, `shatter-core/`, `shatter-cli/`, `shatter-ts/`, `shatter-go/` and `shatter-rust/`. There is no `shatter-llm/`. The Dockerfile's `sed` also rewrites a `features = ["gh-release"]` line in `shatter-core/Cargo.toml` that no longer exists (shatter-core now depends on `z3 = "0.19"` with default features), so that step is a silent no-op.
- `docker-publish.yml:3-7`: the triggers are `push: tags: ['v*']` and `pull_request: branches: [main]`, with `platforms: linux/amd64,linux/arm64` (`:48`).

## Acceptance criteria

- [ ] The Dockerfile copies every workspace member that the build needs. Prefer a single `COPY . .` with a `.dockerignore` over a hand-maintained per-crate list, because the per-crate list is how this broke. Remove the stale `sed` step, or replace it with an explicit, working Z3 strategy for the image.
- [ ] A regression check fails if a workspace member listed in root `Cargo.toml` `[workspace] members` is not copied by the Dockerfile. This can be a test in `scripts/test_ci_workflow_structure.py`, or it is unnecessary if `COPY . .` is used. Show that it fails before the fix.
- [ ] The built image runs `shatter --version` in a workflow step, for both `linux/amd64` and `linux/arm64`, or the arm64 platform is removed with a documented reason. If Windows and aarch64 release issues change the Z3 strategy, keep the image consistent with them.
- [ ] Either the workflow gains a schedule or `workflow_dispatch` trigger so `workflow-health-patrol` can watch it, or the close reason explains why PR-only coverage is enough.
- [ ] **Close-time proof.** The close reason contains the URL of a green `docker-publish.yml` run (a PR run is fine, since it builds without pushing).

## Out of scope

- Publishing a `v*` tag or changing image tagging policy.
- The GitHub release binaries (the release drafts in this bundle).

## Dependencies

- None blocking.
- Related: `workflow-health-patrol`, `release-windows-z3-build` (Z3 linkage options), `workflow-action-versions`.

Priority: P2 · Type: bug · Labels: ci, github-actions, docker, distribution, audit · Parent: Epic: Audit 2026-09-22 findings · Sources: tests-ci-03, agent-repo-03 (split out of workflow-health-patrol in revision) · Decision: D6
