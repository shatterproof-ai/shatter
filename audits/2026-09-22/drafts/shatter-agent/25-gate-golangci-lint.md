# Gate golangci-lint and gofmt in check-static (lint ungated; str-2tyfk and str-qwua7.32 closed on false 'lint passes')

- Priority: P2
- Type: task
- Labels: go,quality-gates,agents,shatter-go
- Tracker: shatter (bd, /home/ketan/project/shatter)
- Relation: new (str-2tyfk and str-qwua7.32 closed-but-unfixed)
- Source findings: frontend-go-05
- Parent: 01 (epic)
- Blocked by: none
- Readiness: drafted to the issue-readiness-check standard; fresh-reviewer precheck still required before filing (see INDEX.md)

<!-- body -->
## Problem
golangci-lint is configured and the go-conventions skill describes lint
enforcement, but no gate or CI job runs it: the go `lint` task is optional and
unwired. Two issues were closed claiming lint passed when it did not.

## Current Code Facts
- `shatter-go/.golangci.yml:3-4` header: "Runs via: task go:test" (false).
- `shatter-go/Taskfile.yml` `lint` precondition: "golangci-lint not installed
  (optional)". Root `lint` depends on `go:vet` only (Taskfile.yml:~184-186);
  `check-unit` runs go:test + go:vet (:~548-554). `ci.yml` installs no
  golangci-lint.
- `golangci-lint run --timeout 8m ./...` in shatter-go -> 10 issues: govet
  `protocol/handler.go:1325` "nilness: tautological condition: nil == nil";
  staticcheck `instrument/property_test.go:544` SA5011; unused: analyzeFunc,
  extractParams, mapTypeInfo, structTypeInfo, lookupAnalyzedByTargetID,
  toWrapperConstructors, toWrapperConstructorParams.
- `gofmt -l` lists 4 non-test files (e.g. `instrument/symextract.go`,
  `protocol/analysis_cache.go`, `workspace/run.go`).
- str-2tyfk (closed, empty reason) listed the same residuals; str-qwua7.32
  acceptance "task go:lint passes" was false at closure.

## Acceptance Criteria
- The 10 findings fixed; `gofmt -l` clean for non-testdata files.
- `go:lint` and a `gofmt -l` check wired into `check-static`; under `CI=1` a
  missing golangci-lint fails (not skips); CI installs a pinned golangci-lint.
- `.golangci.yml` header and Taskfile comments corrected.
- `check-go` skill runs `task go:lint` (coordinate with draft 14).

## Out of Scope
Dead-code deletion beyond what the linter reports (product finding).
