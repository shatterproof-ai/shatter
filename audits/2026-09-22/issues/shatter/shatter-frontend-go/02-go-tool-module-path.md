---
slug: go-tool-module-path
kind: new
title: "Documented `go get -tool github.com/shatterproof-ai/shatter/go-tool/cmd/shatter` cannot resolve: module path says go-tool/ but the directory is shatter-go-tool/"
priority: P1
type: bug
labels: [go-tool, distribution, install, docs, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Documented `go get -tool github.com/shatterproof-ai/shatter/go-tool/cmd/shatter` cannot resolve: module path says go-tool/ but the directory is shatter-go-tool/

## Problem

The Go tool wrapper's module path does not match its directory, so the install command in `docs/distribution.md` fails for every user. The repo has no root `go.mod`, so the Go module system resolves `github.com/shatterproof-ai/shatter/go-tool` to a `go-tool/` directory at the repo root, which has never existed.

str-fl9g.2 ("Go tool wrapper", P1) was closed "9dc76c85 landed on main" with exactly this command as its acceptance check; that check could never have passed.

No tag changes are needed. `continuous-*` tags are not semantic versions, so `@continuous-...` is a revision query: Go resolves the revision in the repository and gives the nested module a pseudo-version. Directory-prefixed tags (`go-tool/vX.Y.Z`) are needed only for semantic-version tags of a nested module ([Go module reference, version queries](https://go.dev/ref/mod#version-queries); [VCS version tags for subdirectory modules](https://go.dev/ref/mod#vcs-version)).

## Evidence

Re-verified against the audit worktree at commit 56c86168:

- `shatter-go-tool/go.mod:1`: `module github.com/shatterproof-ai/shatter/go-tool`. `ls go-tool` and `ls go.mod` at the repo root: no such file. `git log --all -- go-tool/go.mod` is empty.
- `docs/distribution.md:64`: `go get -tool github.com/shatterproof-ai/shatter/go-tool/cmd/shatter@continuous-20260512-1735-abc123def456`. The Renovate regex at `docs/distribution.md:110` targets the same path.
- `Taskfile.yml:467`: `task meta` only runs `cd shatter-go-tool && go test ./...`, which cannot detect module-path/directory drift.
- Audit run in a fresh temp module (network, `GOPROXY=direct`):
  ```
  go: module github.com/shatterproof-ai/shatter@main found (v0.0.0-20260922164214-16794cef9e10),
      but does not contain package github.com/shatterproof-ai/shatter/go-tool/cmd/shatter
  ```
  The error is a path error (no such package in the repo), not a version-resolution error.

## Acceptance criteria

- [ ] The module path and directory agree: either the directory is renamed to `go-tool/`, or the module path, docs and Renovate regex all change to `.../shatter/shatter-go-tool`. Every reference (Taskfile `meta`, CI, `docs/distribution.md`, Renovate regex, README/QUICKSTART if they mention it) is updated in the same change.
- [ ] **Pre-merge check (gates the PR).** A test in `task meta` resolves the documented module path against the local checkout, with no network: a temp module with `require <documented path> v0.0.0` plus `replace <documented path> => <repo>/<module dir>`, then `go build <documented path>/cmd/shatter`, and a check that the `module` line of the wrapper's `go.mod` equals the path in `docs/distribution.md` and the Renovate regex. It fails on current main (show the failing output in the close note) and passes on the branch.
- [ ] **Post-merge check (proves the documented command).** A job that runs on push to main (the release workflow or a small workflow of its own) creates a temp module and runs the exact documented `go get -tool <path>/cmd/shatter@<the continuous tag of that run, or the pushed commit SHA>` with `GOPROXY=direct`, then `go tool shatter --shatter-wrapper-help`. Proof at close: the URL of a green run of that job after the fix landed, with its output pasted into the close note.
- [ ] `docs/distribution.md` states that `@continuous-...` resolves to a pseudo-version, and does not ask for prefixed tags.
- [ ] str-fl9g.2 carries a comment linking this issue as the fix (see the companion reopen-note draft `go-tool-reopen-note`).
- [ ] `task affected` passes with `Gates selected` recorded.

## Suggested approach

Renaming the directory to `go-tool/` is the smallest change that keeps the published path in the docs. The post-merge job can use the commit SHA instead of the continuous tag if the release workflow is not yet green, so this issue does not wait on the release fixes.

## Out of scope

- Wrapper robustness (atomic extract, HTTP timeouts, API caching): `go-tool-wrapper-robustness`.
- Semantic-version tagging of the nested module (not needed for `continuous-*` revisions).

## Dependencies

- Blocked by: none.
- Blocks: `go-tool-wrapper-robustness` (paths move).
- Related: str-fl9g.2 (closed; acceptance never passed), str-wnyzy (closed; CI Setup Go step and root go.mod).

## Size

S

## References

- Finding frontend-go-02 (audit 2026-09-22, `audits/2026-09-22/findings.json`; evidence `audits/2026-09-22/areas/frontend-go.md` go-02). Old draft: `drafts/shatter-code/51-go-tool-module-path.md`. Revised after the Codex cross-check: the earlier requirement to push `go-tool/<tag>` tags was wrong and is removed.
