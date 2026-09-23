---
slug: release-publish-guard-and-target
kind: new
title: "Release: guard the publish job to push-on-main and pass --target $GITHUB_SHA, so branch runs of release.yml build without publishing"
priority: P1
type: bug
labels: [release, ci, distribution, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Release: guard the publish job to push-on-main and pass --target $GITHUB_SHA, so branch runs of release.yml build without publishing

## Problem

The Windows and aarch64 fixes (`release-windows-z3-build`, `release-aarch64-openssl-cross`) have to be iterated with `gh workflow run release.yml --ref <branch>`, because release.yml has no pull_request trigger. Once the matrix goes green, a dispatched branch run can publish a `continuous-*` prerelease from unreviewed code. Nothing restricts the `release` job to main:

- `release.yml:3-6` triggers on `push: branches: [main]` and on `workflow_dispatch`, and dispatch accepts any ref.
- The `release` job (`:183-186`) has only `needs: [build-ts, build]`. It has no `if:` on event or ref.
- `gh release create "$RELEASE_TAG"` (`:295-310`) passes no `--target`. When the tag does not exist yet, `gh release create` makes it on the repository's default branch, not on the commit that was built (see the `--target` flag of `gh release create`). A branch run would therefore publish branch binaries under a tag that points at main's HEAD. A push run could also tag a later main commit if main moved during the build.

`install.sh` picks the newest `continuous-*` prerelease, so a published branch build would be what users install.

## Evidence

Re-verified 2026-09-23 against the audit worktree (main 70465921 plus audit files). The line numbers above were read from `.github/workflows/release.yml`. `permissions: contents: write` is set for the whole workflow (`:8-9`).

## Acceptance criteria

- [ ] The `release` job runs only when `github.event_name == 'push' && github.ref == 'refs/heads/main'`. As an option, dispatch from main may also publish when an explicit `publish: true` input is set. A dispatch from any other ref runs the full build matrix, and the release job shows `skipped`.
- [ ] `gh release create` passes `--target "$GITHUB_SHA"`, so the tag points at the built commit.
- [ ] `scripts/test_ci_workflow_structure.py` (or a sibling test that `task meta` runs) parses `release.yml` and asserts both properties: the `if:` guard on the release job, and `--target` in the create step. Show the test failing on the current tree and passing after the fix, and paste both outputs in the close reason.
- [ ] **Close-time proof.** Cite a `gh workflow run release.yml --ref <non-main branch>` run URL in which the build jobs ran and `Create continuous GitHub Release` concluded `skipped`. The build jobs may still fail, since the Windows and aarch64 legs are fixed elsewhere.

## Out of scope

- Fixing any build leg.
- The install smoke (`release-publish-and-install-smoke`).
- Release retention (`cleanup-continuous-releases.yml`).

## Dependencies

- Blocks: `release-windows-z3-build`, `release-aarch64-openssl-cross`. Both iterate through branch dispatch.
- Related: `release-publish-and-install-smoke`, str-lj7s (see `release-reopen-note`).

Priority: P1 · Type: bug · Labels: release, ci, distribution, audit · Parent: Epic: Audit 2026-09-22 findings · Sources: crosscheck codex #5 (new in revision) · Decision: D1
