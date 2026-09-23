---
slug: workflow-action-versions
kind: new
title: "Workflows use deprecated Node-20 action majors, unpinned ubuntu-latest (Ubuntu 26 from 2026-10-19), and setup-go cache that cannot find go.sum"
priority: P3
type: chore
labels: [ci, github-actions, maintenance, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Workflows use deprecated Node-20 action majors, unpinned ubuntu-latest (Ubuntu 26 from 2026-10-19), and setup-go cache that cannot find go.sum

## Problem

GitHub Actions annotations warn about three things that will degrade or break CI without any repo change:

1. The Node-20 runtime is deprecated for several action majors the workflows use.
2. `ubuntu-latest` starts migrating to Ubuntu 26 on 2026-10-19. The workflows depend on distro packages (`libclang-dev`, `z3`) that have not been verified there.
3. `actions/setup-go` caching silently fails because it looks for `go.sum` at the repo root.

These annotations appear only in the web UI, so no agent has seen them.

## Evidence

Re-verified 2026-09-23 in the worktree at `56c86168`:

- Run 35756993223 (CI) annotations:
  - "Node.js 20 is deprecated ... actions/cache@v4, actions/checkout@v4, actions/setup-go@v5, actions/setup-node@v4, arduino/setup-task@v2"
  - "The ubuntu-latest label will migrate to Ubuntu 26 beginning October 19, 2026"
  - "Restore cache failed: Dependencies file is not found ... Supported file pattern: go.sum"
- Action usage across `.github/workflows/*.yml`: `actions/checkout@v4` ×11, `actions/cache@v4` ×4, `actions/setup-go@v5` ×4, `actions/setup-node@v4` ×4, `actions/setup-python@v5` ×3, `actions/upload-artifact@v4` ×3, `actions/download-artifact@v4` ×1, `arduino/setup-task@v2` ×2, plus docker/*, `devcontainers/ci@v0.3`, `dtolnay/rust-toolchain@stable`.
- `runs-on: ubuntu-latest` appears 10 times, in `ci.yml`, `drift-patrol.yml`, `perf-ci.yml`, `release.yml`, `devcontainer.yml`, `docker-publish.yml`, `parity-expiry.yml` and `cleanup-continuous-releases.yml`. `release.yml` also uses `windows-latest` and macOS labels through `matrix.os`.
- setup-go without `cache-dependency-path`: `ci.yml:51-53`, `drift-patrol.yml:83-85` (fixed separately by `drift-patrol-workflow-go-mod`) and `perf-ci.yml:34-36`. Only `release.yml:131-132` sets `cache-dependency-path: shatter-go/go.sum`.

## Acceptance criteria

- [ ] Every action whose major runs on Node 20 is bumped to a Node-24-based major. Check each action's releases for the current major; do not guess version numbers.
- [ ] Linux runners are pinned to `ubuntu-24.04` until a trial run on Ubuntu 26 (`ubuntu-26.04` label, or `ubuntu-latest` after the migration) proves that `apt-get install libclang-dev z3` and `task check` work. Record that follow-up as an issue.
- [ ] Every `actions/setup-go` step sets `cache-dependency-path: shatter-go/go.sum`.
- [ ] Proof at close: link one post-change run each of `ci.yml` and `release.yml` whose annotations show no Node-20 deprecation warning and no "Restore cache failed ... go.sum" warning.

## Suggested approach

Make one mechanical PR across all workflow files. If `drift-patrol-workflow-go-mod` has landed first, `drift-patrol.yml`'s setup-go is already fixed. Dependabot for `github-actions` (`.github/dependabot.yml`) is an optional way to keep the majors current, and is the implementer's call.

## Out of scope

- Fixing workflows that are red for other reasons (the release, drift-patrol, perf-ci and devcontainer issues in this bucket).
- `dtolnay/rust-toolchain@stable` pinning (see `rustfmt-gate` for the fmt toolchain).

## Dependencies

- None blocking.
- Related: `drift-patrol-workflow-go-mod`, `workflow-health-patrol`.

Priority: P3 · Type: chore · Labels: ci, github-actions, maintenance, audit · Parent: Epic: Audit 2026-09-22 findings · Sources: shatter-code/77, tests-ci-18
