---
slug: go-release-relocation-smoke
kind: new
title: "Release smoke must prove the shipped Go frontend works where its build checkout does not exist (fresh path, no shatter-go/ source, cold caches)"
priority: P1
type: task
labels: [go-frontend, release, ci, distribution, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [go-harness-runtime-embed, release-publish-and-install-smoke]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Release smoke must prove the shipped Go frontend works where its build checkout does not exist

## Problem

`go-harness-runtime-embed` fixes the Go frontend's dependence on its compile-time source path and proves it with a local test. The release pipeline needs its own proof, because the published binary is what users run.

`release-publish-and-install-smoke` adds a smoke job to `release.yml` that installs the published binary and runs `shatter explore` on `examples/go/05-conditional-merge.go:Categorize` from a checkout. As drafted, that smoke would **not** catch the relocation bug. A GitHub-hosted runner checks the repo out at the same path the build job used (`/home/runner/work/shatter/shatter`), so `shatter-go/harness/go.mod` exists at the compiled-in path, and the pre-fix binary would pass.

## Evidence

- `shatter-go/instrument/executor.go:112-133` resolves the harness runtime via `runtime.Caller(0)` (the compiled-in source path); see `go-harness-runtime-embed` for the full evidence and repro.
- `.github/workflows/release.yml:175` builds the Go frontend in the runner checkout (`go build -o "../staging/$GO_BINARY" .`).
- `release-publish-and-install-smoke` (audit bucket shatter-ci-workflows) runs its explore smoke "from a checkout with Go set up".
- `shatter-go/workspace/workspace.go:13` `SHATTER_GO_WORKSPACE_ROOT` selects the workspace root; `workspace.GoEnv` (`:194-214`) pins `GOCACHE` under it, so a fresh workspace root gives cold launcher and Go build caches.

## Acceptance criteria

- [ ] The Linux x86_64 smoke leg in `release.yml` runs the installed binary in a state where the build checkout's harness source is unavailable: the examples are checked out to a different path from the build job's (e.g. `actions/checkout` with `path: smoke-src`) **and** `shatter-go/` is deleted from that checkout before the explore runs. `SHATTER_GO_WORKSPACE_ROOT` points at a fresh empty directory. The step asserts that `shatter-go/harness/go.mod` does not exist at the path baked into the binary (`go version -m` or `strings` on the frontend binary gives the path; `test ! -e` on it).
- [ ] The explore in that state exits 0 and reports at least one explored branch.
- [ ] Negative control, recorded once: the same step run against a pre-fix build (a `workflow_dispatch` run on a branch that reverts `go-harness-runtime-embed`, or a build from the parent of that fix) fails with the harness-runtime error. Paste the run URL and the failing log line in the close note.
- [ ] Close-time proof (D1): the URL of a green push-to-main `release.yml` run in which this step executed, with every matrix leg (including Windows and aarch64) `success`. No matrix leg is dropped or made `continue-on-error` to get there.

## Suggested approach

Add the steps to the smoke job that `release-publish-and-install-smoke` creates, rather than a new job. If that job is not yet merged when this is picked up, wait: this issue is blocked by it.

## Out of scope

- The runtime fix itself (`go-harness-runtime-embed`).
- The publish, install.sh and action.yml smoke (`release-publish-and-install-smoke`), and the Windows/aarch64 build fixes.

## Dependencies

- Blocked by: `go-harness-runtime-embed`, `release-publish-and-install-smoke` (which is itself blocked by `release-windows-z3-build`, `release-aarch64-openssl-cross` and `release-publish-guard-and-target`).

## Size

S

## References

- Finding frontend-go-01 (audit 2026-09-22). Split from `go-harness-runtime-embed` after the Codex cross-check (findings 3 and 4 on 01: undeclared release dependency; same-path runner checkout does not prove relocation).
