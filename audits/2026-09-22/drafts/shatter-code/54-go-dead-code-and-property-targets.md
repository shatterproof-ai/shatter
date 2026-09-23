# ~55 unreachable Go functions incl. the whole reconstruct package; re-scope str-qwua7.48 property tests to live generators

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | chore |
| priority | P2 |
| labels | go,cleanup,testing,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | str-qwua7.48 |
| source findings | frontend-go-06, frontend-go-11 |

<!-- body -->
## Problem

deadcode reports ~55 unreachable production functions in shatter-go. Open str-qwua7.48 asks for property tests on the dead reconstruct package while the live source generators (wrapper, launcher) and config matcher have none.

## Current code facts / evidence

- `deadcode ./...` (57 lines) includes reconstruct.Value/Inputs/toInt64 (no importers; the go Taskfile keeps `go build ./...` just to compile it), loader/legal_anchor.go (102 lines), launcher.OpenSession (session.go:136; 6 test users), planner.PlanAggregate (aggregate.go:51), planner.ResolveMockSpecs (plan.go:129; CLAUDE.md:273 says it emits MockSpecs), Workspace.NewRun (workspace/run.go), wrapper.BuildWrapperTargets (wrapper.go:1548,1833), protocol.AnalyzeFile/NewHandler (analyzer.go:189...).
- Rapid coverage: wrapper 3219 lines / 1 rapid file; launcher 1114 / 0; config 524 / 0; setup/ 158 lines / no tests.
- GenerateWrapper (wrapper.go:290) and GenerateLauncherMain (launcher.go:666,716) build Go source by concatenation.

## Acceptance criteria

- Each unreachable function is deleted or moved to _test.go / internal/testutil (case by case for test-used APIs).
- deadcode check with an allowlist runs in `task meta` (or check-static).
- str-qwua7.48 re-scoped (note appended): rapid properties that generated wrapper/launcher source parses and gofmt-formats for random param/receiver/generic shapes; config MatchTarget determinism and 'anchored beats fallback'; basic setup/loader_test.go.

## Suggested approach

Implementer's choice within the acceptance criteria above.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: M

## References

- Audit findings: frontend-go-06, frontend-go-11 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-qwua7.48
