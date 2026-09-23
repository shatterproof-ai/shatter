# Go lint is red on main (10 issues) and no gate runs golangci-lint or gofmt; str-2tyfk and str-qwua7.32 closed with residuals

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | bug |
| priority | P2 |
| labels | go,quality-gates,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | str-2tyfk, str-qwua7.32 |
| source findings | prior-08, frontend-go-05 (AGENT context) |

<!-- body -->
## Problem

golangci-lint is configured but optional (Taskfile precondition), not wired into check or CI, and currently reports 10 issues including a tautological nil check. str-2tyfk (empty close reason) listed these residuals; str-qwua7.32's acceptance 'task go:lint passes' was false at close.

## Current code facts / evidence

- `golangci-lint run --timeout 8m ./...` in shatter-go: '10 issues: govet 1, staticcheck 2, unused 7' — `protocol/handler.go:1325` nilness `nil == nil`; `instrument/property_test.go:544` SA5011; unused analyzeFunc, extractParams, mapTypeInfo, structTypeInfo, lookupAnalyzedByTargetID, toWrapperConstructors, toWrapperConstructorParams (e.g. analyzer.go:592, prepared_launcher.go:474).
- `shatter-go/Taskfile.yml:61-65` lint precondition 'golangci-lint not installed (optional)'; root `Taskfile.yml:186` lint depends on go:vet only; `:554` check-unit runs go:test + go:vet.
- `.golangci.yml:3-4` header says 'Runs via: task go:test' (false).
- `gofmt -l` lists ~10 files incl. instrument/symextract.go, protocol/analysis_cache.go, workspace/run.go.

## Acceptance criteria

- The 10 findings fixed; gofmt clean.
- go:lint (golangci-lint, required under CI=1, --timeout 10m) and a `gofmt -l` check run in check-static and CI.
- .golangci.yml header and Taskfile comments corrected.

## Suggested approach

Implementer's choice within the acceptance criteria above.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: S

## References

- Audit findings: prior-08, frontend-go-05 (AGENT context) (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-2tyfk, str-qwua7.32
