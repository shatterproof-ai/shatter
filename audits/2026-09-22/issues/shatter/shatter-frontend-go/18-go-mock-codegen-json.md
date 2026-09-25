---
slug: go-mock-codegen-json
kind: new
title: "Generated Go mock harness embeds mock JSON in a backtick raw string (a backtick in a mock value breaks the build) and ignores json.Unmarshal errors"
priority: P3
type: bug
labels: [go-frontend, mocking, codegen, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Generated Go mock harness embeds mock JSON in a backtick raw string and ignores json.Unmarshal errors

## Problem

The mock harness generator embeds the mock return-value JSON inside a Go raw string literal (backticks). `json.Marshal` does not escape a backtick, so a mock value containing one ends the raw string early and the generated harness fails to compile. The generated code also ignores `json.Unmarshal` errors, so a malformed value silently becomes a zero value and the target runs with a mock return it was never given.

## Evidence

Re-verified against the audit worktree at commit 56c86168:

- `shatter-go/instrument/executor.go:225-231`: `retValsJSON, _ := json.Marshal(...)` (error discarded), then line 231 emits the Go source ``json.Unmarshal([]byte(`<json>`), &vals)`` via `fmt.Fprintf` (JSON inside a backtick raw string; the Unmarshal result is unchecked).
- `executor.go:299`: `b.WriteString("\t\tjson.Unmarshal(retvals[idx], &retVal)\n")` (unchecked).
- The backtick breakage is by reasoning, not executed. The first AC below executes it.
- str-qwua7.32 (closed) covered errcheck on frontend code, not on generated code.

## Acceptance criteria

- [ ] First, confirm the defect: a test that generates, compiles and runs a mock harness whose mock return value is a string containing a backtick. Record its failure on main in the issue.
- [ ] Generated mock code embeds the JSON via `strconv.Quote` (an interpreted string literal) and checks both `json.Unmarshal` results; a failure panics with a message naming the mocked function, or is reported through the harness error channel (pick one and say which in `shatter-go/CLAUDE.md`). The generator's own `json.Marshal` error is returned, not discarded.
- [ ] The backtick test passes on the branch. A second test feeds a malformed return-value payload at runtime and asserts the error is reported, not silently zeroed.
- [ ] rapid property: for random strings (including backticks, quotes, backslashes, newlines and non-UTF-8 bytes where JSON allows), the generated mock source parses with `go/parser` and the decoded value round-trips.
- [ ] `go test ./...` in shatter-go passes. `task e2e-go` passes (it runs `cargo test --test e2e_concolic_go -- --include-ignored`; paste the summary line showing `0 ignored`). `task affected` passes with `Gates selected` recorded.

## Out of scope

- Property tests for the other code generators (str-qwua7.48, re-scoped by `go-dead-code-and-property-targets`).

## Dependencies

- Blocked by: none.
- Related: str-qwua7.32, str-qwua7.48, `go-small-correctness-tidy`.

## Size

S

## References

- Finding prior-22 (audit 2026-09-22, `audits/2026-09-22/findings.json`; evidence `audits/2026-09-22/areas/prior-audit-regress.md`). Split out of `go-small-correctness-tidy` after the Codex cross-check (finding 13).
