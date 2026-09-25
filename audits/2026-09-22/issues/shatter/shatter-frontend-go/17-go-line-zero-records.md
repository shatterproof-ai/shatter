---
slug: go-line-zero-records
kind: new
title: "Go instrumenter records line 0 on every execution (synthetic call_enter/call_exit statements), so lines_executed always contains phantom zeros"
priority: P3
type: bug
labels: [go-frontend, coverage, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Go instrumenter records line 0 on every execution

## Problem

The Go instrumenter emits a line-record call for every statement, including the synthetic `call_enter`/`call_exit` statements it prepends, whose position resolves to line 0. Only the denominator (`instrumentableLines`) is guarded, so every execution reports `lines_executed: [0, 0, ...]` and every consumer must filter the zeros.

## Evidence

Re-verified against the audit worktree at commit 56c86168:

- `shatter-go/instrument/visitor.go:131-139`: `newList = append(newList, makeLineRecordCall(line))` at :132 is unconditional; the `if line > 0` guard at :137 covers only `instrumentableLines`, and the comment there says synthetic statements resolve to line 0.
- Audit explore artifact: `"lines_executed": [0, 0, 4, 7, 10, 13]`.
- str-qo1.12 (closed) fixed a different line-coverage gap.

## Acceptance criteria

- [ ] No line-record call is emitted for `line <= 0`.
- [ ] `instrument/visitor_test.go` asserts that instrumenting a function with a body produces no line-record call with line 0 (fails on main, passes on the branch).
- [ ] An execute-level check: the `lines_executed` of a Go execute response for a small fixture contains no `0` (Go test through the handler, or an assertion added to an existing case in `shatter-core/tests/e2e_concolic_go.rs`). Fails on main, passes on the branch.
- [ ] Any consumer that filters line 0 for Go specifically is left alone or simplified, with the change listed in the close note.
- [ ] `go test ./...` in shatter-go passes. If the E2E file is touched, `task e2e-go` passes (it runs `cargo test --test e2e_concolic_go -- --include-ignored`; paste the summary line showing `0 ignored`). `task affected` passes with `Gates selected` recorded.

## Out of scope

- Other coverage-metric issues (engine-correctness bucket).

## Dependencies

- Blocked by: none.
- Related: str-qo1.12, `go-small-correctness-tidy`.

## Size

XS

## References

- Finding frontend-go-14 (audit 2026-09-22, `audits/2026-09-22/findings.json`; evidence `audits/2026-09-22/areas/frontend-go.md` go-12). Split out of `go-small-correctness-tidy` after the Codex cross-check (finding 13).
