---
slug: go-dead-code-and-property-targets
kind: new
title: "~57 unreachable shatter-go functions incl. the whole reconstruct package; add a deadcode gate and re-scope str-qwua7.48 property tests to live generators"
priority: P2
type: chore
labels: [go-frontend, cleanup, testing, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# ~57 unreachable shatter-go functions incl. the whole reconstruct package; add a deadcode gate and re-scope str-qwua7.48 property tests to live generators

## Problem

`deadcode ./...` reports ~57 unreachable production functions (57 at 56c86168; the count moves with every commit) in shatter-go, including the entire `reconstruct` package, which has no importers. Nothing gates new dead code. Meanwhile open str-qwua7.48 asks for new rapid property tests *for* `reconstruct` (dead) while the live code generators that build Go source by string concatenation (wrapper, launcher) and the config matcher have no property tests, and `setup/` has no tests at all.

## Evidence

Re-verified against the audit worktree at commit 56c86168:

- `cd shatter-go && deadcode ./...` prints 57 lines. Selected:
  - `reconstruct/reconstruct.go:18 Value`, `:113 Inputs`, `:122 toInt64`, `:136 errorString.Error`. `grep -rn 'shatter-go/reconstruct"' --include=*.go shatter-go` finds no importer; `shatter-go/Taskfile.yml:13` keeps a `go build ./...` partly to compile such packages; `shatter-go/CLAUDE.md` calls it "historical, no current callers".
  - `loader/legal_anchor.go:21 LegalAnchor`, `:65 LauncherPackagePath`, `:93`, `:97`.
  - `launcher/session.go:136 OpenSession` (used only by tests).
  - `planner/aggregate.go:51 PlanAggregate`, `planner/classify.go:42 Classify`, `planner/plan.go:129 ResolveMockSpecs` (`shatter-go/CLAUDE.md:273` claims "The planner still emits ... via `planner.ResolveMockSpecs`").
  - `workspace/run.go:59 Workspace.NewRun`, `wrapper/wrapper.go:1548 BuildWrapperTargets`, `protocol/analyzer.go:189 AnalyzeFile`, `protocol/handler.go:99 NewHandler`.
  - `instrument/flow.go` and `instrument/flowwalk.go` (9 functions): owned by str-qwua7.35, see below.
- Property-test census (non-test lines / rapid files): wrapper 3219 / 1 (error_sentinel only); launcher 1114 / 0; config 524 / 0; setup 158 / no test files; reconstruct 136 / 0 (dead).
- `wrapper.GenerateWrapper` (`wrapper/wrapper.go:290`) and `launcher.GenerateLauncherMain` / `GenerateHarnessLauncherMain` (`launcher/launcher.go:666, 716`) emit Go source by string building; no property asserts the output parses.

## Acceptance criteria

- [ ] Each function in the deadcode report taken at branch start (paste that report, with its commit SHA, into the issue) is deleted, or moved to `_test.go` / `internal/testutil` if tests need it, case by case. `reconstruct/` is deleted. `shatter-go/CLAUDE.md` claims about removed code (e.g. `ResolveMockSpecs` at :273, reconstruct) are corrected.
- [ ] Exceptions: `instrument/flow*.go` stays until str-qwua7.35 removes it; `planner/classify.go Classify` is not deleted until `go-cgo-refusal-covers-bodies` decides whether cgo refusal is wired through it. Both are listed in the allowlist with the owning issue id.
- [ ] A `deadcode` check with a checked-in allowlist runs in `task meta` (or `check-static`) and fails on new unreachable production functions. Proof at close: the gate output from a forced (non-cached) run, and a demonstration that adding an unused exported function makes it fail.
- [ ] str-qwua7.48 is re-scoped by the comment below (filer posts it; the implementer of str-qwua7.48 does the tests, not this issue).
- [ ] `go test ./...` in shatter-go passes. `task e2e-go` passes (it runs `cargo test --test e2e_concolic_go -- --include-ignored`; plain `cargo test --test e2e_concolic_go` skips every case because all are `#[ignore]`); paste the cargo summary line showing `0 ignored`, or run the cargo command directly if the gate reports a cache hit. `task affected` passes (`Gates selected` recorded).

## Comment for str-qwua7.48 (post with `bd comments add str-qwua7.48`)

> **Audit 2026-09-22 (findings frontend-go-06, frontend-go-11):** `reconstruct/` is unreachable production code (no importers; `deadcode ./...` lists all of it) and is being deleted by **<id of go-dead-code-and-property-targets>**. Please re-scope this issue to live code: (a) rapid properties that `wrapper.GenerateWrapper` and `launcher.GenerateLauncherMain`/`GenerateHarnessLauncherMain` output parses (`go/parser`) and is gofmt-stable for random param/receiver/generic shapes (a compile check can go in a slow tier); (b) config `MatchTarget` determinism under map iteration and "anchored beats fallback" (the str-cl19s tie-break); (c) a basic `setup/loader_test.go`. Drop reconstruct from the acceptance criteria.

## Suggested approach

Run `deadcode ./...`, walk the list package by package, and keep each deletion in its own commit so reviewers can check test-only users. Wire the check as a small script that diffs `deadcode` output against `shatter-go/.deadcode-allow`.

## Out of scope

- Writing the str-qwua7.48 property tests.
- golangci-lint gating and gofmt drift (the audit's separate Go-lint issue).
- The four-builder unification (str-qwua7.35).

## Dependencies

- Blocked by: none.
- Related: str-qwua7.48 (re-scoped by this issue's comment), str-qwua7.35, `go-cgo-refusal-covers-bodies` (decides `planner.Classify`).

## Size

M

## References

- Findings frontend-go-06 and frontend-go-11 (audit 2026-09-22, `audits/2026-09-22/findings.json`; evidence `audits/2026-09-22/areas/frontend-go.md` go-06, go-13). Old draft: `drafts/shatter-code/54-go-dead-code-and-property-targets.md`.
