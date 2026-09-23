---
slug: go-small-correctness-tidy
kind: new
title: "Go frontend housekeeping: dead `_ = anchorImport` / `_ = fresh` assignments, and unchecked lock-file PID writes that silently weaken stale-lock detection"
priority: P3
type: chore
labels: [go-frontend, cleanup, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Go frontend housekeeping: dead assignments and unchecked lock-file PID writes

## Problem

Two small Go-frontend housekeeping items from the 2026-09-04 audit (rec 13) that were never filed:

1. **Dead assignments** that hide intent: `_ = anchorImport`, `_ = fresh`.
2. **Unchecked lock-file PID writes**: `_, _ = fmt.Fprintf(lockFile, ...)`. `lockIsStale` reads the PID back and falls back to a ModTime timeout when it cannot, so a failed write silently degrades stale-lock detection.

The other two items originally grouped here have their own issues: phantom line-0 coverage records (`go-line-zero-records`) and generated mock code (`go-mock-codegen-json`).

## Evidence

Re-verified against the audit worktree at commit 56c86168:

- `shatter-go/launcher/launcher.go:391` `_ = anchorImport`; `shatter-go/build/builder.go:277` `_ = fresh`.
- `shatter-go/build/builder.go:188` and `shatter-go/launcher/launcher.go:616`: `_, _ = fmt.Fprintf(lockFile, "%d\n", os.Getpid())`.
- Prior-audit rec 13 (2026-09-04) named both items. str-qwua7.32 (closed) covered errcheck on frontend code but left these.

## Acceptance criteria

- [ ] `_ = anchorImport` and `_ = fresh` are removed, or the variables are used for what they were meant for; the commit message says which, per site.
- [ ] Lock-file PID write errors are handled at both sites (returned, or logged at warn level), with the existing ModTime fallback kept. A unit test forces the write to fail (e.g. a read-only file handle) and asserts the error is surfaced and the lock is still usable via the fallback.
- [ ] `go test ./...` in shatter-go passes; `task affected` passes with `Gates selected` recorded.

## Out of scope

- Splitting the large `protocol` package (prior audit P2; not filed here).
- golangci-lint gating (the audit's Go-lint issue, `go-lint-and-gofmt-gated`).

## Dependencies

- Blocked by: none.
- Related: str-qwua7.32, `go-line-zero-records`, `go-mock-codegen-json`.

## Size

XS

## References

- Finding frontend-go-15 (audit 2026-09-22, `audits/2026-09-22/findings.json`; evidence `audits/2026-09-22/areas/frontend-go.md` go-14 and `areas/prior-audit-regress.md`). Old draft: `drafts/shatter-code/56-go-small-correctness-tidy.md`. Split after the Codex cross-check (finding 13): coverage and mock-generation fixes moved to their own issues.
