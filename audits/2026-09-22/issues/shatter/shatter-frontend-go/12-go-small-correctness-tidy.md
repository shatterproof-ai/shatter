---
slug: go-small-correctness-tidy
kind: new
title: "Go frontend small fixes: line-0 records on every execution, generated mock code ignores Unmarshal errors and embeds JSON in backtick raw strings, dead assignments, unchecked lock-PID writes"
priority: P3
type: bug
labels: [go-frontend, cleanup, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Go frontend small fixes: line-0 records on every execution, generated mock code ignores Unmarshal errors and embeds JSON in backtick raw strings, dead assignments, unchecked lock-PID writes

## Problem

Grouped small Go-frontend defects, each independently fixable:

1. **Phantom line-0 records.** The instrumenter emits a line-record call for every statement, including the synthetic `call_enter`/`call_exit` statements it prepends, whose position resolves to line 0. Only the denominator is guarded, so every execution reports `lines_executed: [0, 0, ...]` and consumers must filter.
2. **Generated mock code.** The mock harness generator embeds the mock return-value JSON inside a Go raw string (backticks). `json.Marshal` does not escape a backtick, so a mock value containing one breaks compilation of the harness. The generated code also ignores `json.Unmarshal` errors, so a malformed value silently becomes a zero value.
3. **Dead assignments** that hide intent: `_ = anchorImport`, `_ = fresh`.
4. **Unchecked lock-file PID writes**: `_, _ = fmt.Fprintf(lockFile, ...)`. `lockIsStale` reads the PID back and falls back to a ModTime timeout when it cannot, so a failed write silently degrades stale-lock detection.

## Evidence

Re-verified against the audit worktree at commit 56c86168:

- `shatter-go/instrument/visitor.go:131-139`: `newList = append(newList, makeLineRecordCall(line))` at :132 is unconditional; the `if line > 0` guard at :137 covers only `instrumentableLines`, and the comment there says synthetic statements resolve to line 0. Audit explore artifact: `"lines_executed": [0, 0, 4, 7, 10, 13]`.
- `shatter-go/instrument/executor.go:225-231`: `retValsJSON, _ := json.Marshal(...)`, then line 231 emits the Go source ``json.Unmarshal([]byte(`<json>`), &vals)`` via `fmt.Fprintf` (JSON inside a backtick raw string; the Unmarshal result is unchecked); `:299`: `b.WriteString("\t\tjson.Unmarshal(retvals[idx], &retVal)\n")` (unchecked). Backtick breakage is by reasoning, not executed.
- `shatter-go/launcher/launcher.go:391` `_ = anchorImport`; `shatter-go/build/builder.go:277` `_ = fresh`.
- `shatter-go/build/builder.go:188` and `shatter-go/launcher/launcher.go:616`: `_, _ = fmt.Fprintf(lockFile, "%d\n", os.Getpid())`.
- Prior-audit rec 13 (2026-09-04) named items 3-4 and was never filed. str-qo1.12 (closed) fixed a different line-coverage gap; str-qwua7.32 (closed) covered errcheck on frontend code, not generated code.

## Acceptance criteria

- [ ] No line-record call is emitted for `line <= 0`; a `visitor_test.go` assertion checks that no emitted record has line 0 (fails before, passes after).
- [ ] Generated mock code embeds the JSON via `strconv.Quote` (interpreted string literal) and checks both `json.Unmarshal` results (panic with a clear message or report through the harness error channel). A test generates, compiles and runs a mock whose return value contains a backtick; it fails before the fix and passes after.
- [ ] `_ = anchorImport` and `_ = fresh` are removed (or the variables are used for what they were meant for).
- [ ] Lock-file PID write errors are handled (returned or logged, with the existing ModTime fallback kept).
- [ ] `go test ./...` in shatter-go, `cargo test --test e2e_concolic_go`, and `task affected` pass (`Gates selected` recorded).

## Suggested approach

Four small commits, one per item.

## Out of scope

- Splitting the large `protocol` package (prior audit P2; not filed here).
- golangci-lint gating (the audit's Go-lint issue).

## Dependencies

- Blocked by: none.
- Related: str-qo1.12, str-qwua7.32.

## Size

S

## References

- Findings frontend-go-14, frontend-go-15, prior-22 (audit 2026-09-22, `audits/2026-09-22/findings.json`; evidence `audits/2026-09-22/areas/frontend-go.md` go-12, go-14 and `areas/prior-audit-regress.md`). Old draft: `drafts/shatter-code/56-go-small-correctness-tidy.md`.
