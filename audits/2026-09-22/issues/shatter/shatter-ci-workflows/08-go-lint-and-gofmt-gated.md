---
slug: go-lint-and-gofmt-gated
kind: new
title: "Go lint is red on main (10 golangci-lint issues, 10 non-gofmt files) and ungated: fix the findings and gate golangci-lint and gofmt in check-static"
priority: P2
type: bug
labels: [go, shatter-go, quality-gates, agents, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Go lint is red on main (10 golangci-lint issues, 10 non-gofmt files) and ungated: fix the findings and gate golangci-lint and gofmt in check-static

## Problem

`shatter-go` has a golangci-lint configuration, and the go-conventions skill describes lint as enforced. In practice no gate and no CI job runs it. The `go:lint` task has an "optional" precondition and silently no-ops when the tool is absent, and nothing depends on it. The tree now has 10 lint findings, including a tautological nil check in the execute handler and 7 dead functions, and 10 files that are not gofmt-clean. Two issues were closed on the claim that lint passed: str-2tyfk (empty close reason, residuals listed in its own body) and str-qwua7.32 (its acceptance "task go:lint passes" was false at close).

This merges audit drafts shatter-code/57 and shatter-agent/25 (report §15.1).

## Evidence

Re-verified 2026-09-23 in the worktree at `56c86168` with golangci-lint 2.12.2:

- `cd shatter-go && golangci-lint run --timeout 9m ./...` → exit 1, `10 issues:`
  - `protocol/handler.go:1325:32: nilness: tautological condition: nil == nil (govet)`, on the line `if preparedExec == nil && err == nil {`
  - `instrument/property_test.go:544:18: SA5011: possible nil pointer dereference (staticcheck)` (related `:541:7`)
  - unused: `protocol/analyzer.go:592` analyzeFunc, `:799` extractParams, `:1518` mapTypeInfo, `:1534` structTypeInfo; `protocol/handler.go:1964` (*Handler).lookupAnalyzedByTargetID; `protocol/prepared_launcher.go:474` toWrapperConstructors, `:506` toWrapperConstructorParams
- `cd shatter-go && gofmt -l . | grep -v testdata` → 10 files:
  - non-test: `instrument/symextract.go`, `protocol/analysis_cache.go`, `workspace/run.go`
  - test: `instrument/mockfingerprint_test.go`, `instrument/overlay_test.go`, `launcher/launcher_buildvcs_test.go`, `protocol/analysis_cache_handler_test.go`, `protocol/generated_enums_test.go`, `protocol/invocation_plan_test.go`, `protocol/property_test.go`
- `shatter-go/Taskfile.yml:61-71`: `lint` precondition `command -v golangci-lint` with msg `golangci-lint not installed (optional)`, then `golangci-lint run ./...`.
- Root `Taskfile.yml:184-186`: `lint` deps `[workspace-clippy, rust-fe:clippy, rust-rt:clippy, go:vet]` (no go:lint). `:535-546` `check-static` has no Go lint or format step. `:548-554` `check-unit` runs `go:test` and `go:vet` only. `.github/workflows/ci.yml` installs no golangci-lint.
- `shatter-go/.golangci.yml:3-4`: "Runs via: task go:test" (false). `:7` `timeout: 3m` (too short under load; the audit needed 8 minutes).
- `shatter-go/Taskfile.yml:13-16`: comment says CI "runs build via parity/conformance deps but never go:vet/go:test". This is stale, since `check-unit` runs both.
- str-2tyfk closed 2026-09-08 with an empty close reason. str-qwua7.32 closed with reason "Closed".

## Acceptance criteria

- [ ] All 10 golangci-lint findings are fixed: delete the dead functions, fix the tautology at `handler.go:1325` (decide what the second condition was meant to test), and fix the test nil dereference. `golangci-lint run ./...` in `shatter-go` exits 0.
- [ ] `gofmt -l` prints nothing for non-testdata files.
- [ ] `check-static` runs `go:lint` and a gofmt check (`test -z "$(gofmt -l $(git ls-files '*.go' | grep -v /testdata/))"` or equivalent). Under `CI=1` a missing golangci-lint fails the task and does not skip it. `ci.yml` installs a pinned golangci-lint version (for example `golangci/golangci-lint-action` with `install-only`, or `go install ...@v2.x.y`).
- [ ] Proof the gate executes: introduce an unused function and an unformatted file on a scratch branch, force the gate (`task check-static --force` or delete the checksum), show it failing, then revert. Paste both outputs in the close reason. Also cite a CI run URL in which the lint step executed.
- [ ] The `.golangci.yml` header (and timeout: 10m) and the stale comment at `shatter-go/Taskfile.yml:13-16` are corrected.
- [ ] The repo `check-go` skill runs `task go:lint`, and go-conventions no longer describes an unenforced rule.

## Suggested approach

Fix the findings first in one commit, and run the gofmt-only reformat as its own commit. Wire the gate in a third commit. Check whether the `task-sources-cover-real-inputs` issue (shatter-gates-integrity bucket) needs `.golangci.yml` added to the task's `sources:`.

## Out of scope

- Dead-code deletion beyond what the linter reports.
- The equivalent optional-linter problems for ESLint, clippy lint and markdownlint (str-qwua7.30/.31/.46).
- Rust formatting (`rustfmt-gate`).

## Dependencies

- None blocking.
- Related: str-2tyfk, str-qwua7.32 (closed; see `go-lint-reopen-note`), `rustfmt-gate`, `task-sources-cover-real-inputs`.

Priority: P2 · Type: bug · Labels: go, shatter-go, quality-gates, agents, audit · Parent: Epic: Audit 2026-09-22 findings · Sources: shatter-code/57, shatter-agent/25, prior-08, frontend-go-05
