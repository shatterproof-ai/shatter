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

The Go tool wrapper's module path does not match its directory, so the install command in `docs/distribution.md` fails for every user. The repo has no root `go.mod`, so the Go module system resolves `github.com/shatterproof-ai/shatter/go-tool` to a `go-tool/` directory at the repo root, which has never existed. Even after a rename, a nested module needs `go-tool/`-prefixed tags for `@continuous-...` versions to resolve.

str-fl9g.2 ("Go tool wrapper", P1) was closed "9dc76c85 landed on main" with exactly this command as its acceptance check; that check could never have passed.

## Evidence

Re-verified against the audit worktree at commit 56c86168:

- `shatter-go-tool/go.mod:1`: `module github.com/shatterproof-ai/shatter/go-tool`. `ls go-tool` and `ls go.mod` at the repo root: no such file. `git log --all -- go-tool/go.mod` is empty.
- `docs/distribution.md:64`: `go get -tool github.com/shatterproof-ai/shatter/go-tool/cmd/shatter@continuous-20260512-1735-abc123def456`. The Renovate regex at `docs/distribution.md:110` targets the same path.
- `Taskfile.yml:467`: `task meta` only runs `cd shatter-go-tool && go test ./...`, which cannot detect module-path/directory drift.
- `release.yml` pushes no `go-tool/<tag>` tags.
- Audit run in a fresh temp module (network, `GOPROXY=direct`):
  ```
  go: module github.com/shatterproof-ai/shatter@main found (v0.0.0-20260922164214-16794cef9e10),
      but does not contain package github.com/shatterproof-ai/shatter/go-tool/cmd/shatter
  ```

## Acceptance criteria

- [ ] The module path and directory agree: either the directory is renamed to `go-tool/`, or the module path, docs and Renovate regex all change to `.../shatter/shatter-go-tool`. Every reference (Taskfile `meta`, CI, `docs/distribution.md`, Renovate regex, README/QUICKSTART if they mention it) is updated in the same change.
- [ ] Versions resolve: `release.yml` pushes a `<module-dir>/<tag>` tag alongside each continuous/release tag, or the docs switch to pseudo-versions and say so.
- [ ] A CI job (in `task check` or the release workflow) creates a temp module and runs the exact documented `go get -tool ...@<ref>` followed by `go tool shatter --shatter-wrapper-help`. Proof at close: the URL of a green CI run in which that job executed, plus the job's output pasted into the close note.
- [ ] str-fl9g.2 carries a comment linking this issue as the fix (see the companion reopen-note draft `go-tool-reopen-note`).

## Suggested approach

Renaming the directory to `go-tool/` is the smallest change that keeps the published path in the docs. Add the nested-module tag push to `release.yml` next to the existing tag creation.

## Out of scope

- Wrapper robustness (atomic extract, HTTP timeouts, API caching): `go-tool-wrapper-robustness`.

## Dependencies

- Blocked by: none.
- Blocks: `go-tool-wrapper-robustness` (paths move).
- Related: str-fl9g.2 (closed; acceptance never passed), str-wnyzy (closed; CI Setup Go step and root go.mod).

## Size

S

## References

- Finding frontend-go-02 (audit 2026-09-22, `audits/2026-09-22/findings.json`; evidence `audits/2026-09-22/areas/frontend-go.md` go-02). Old draft: `drafts/shatter-code/51-go-tool-module-path.md`.
