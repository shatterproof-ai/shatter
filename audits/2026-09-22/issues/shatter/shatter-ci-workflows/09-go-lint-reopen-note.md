---
slug: go-lint-reopen-note
kind: reopen-note
title: "Comment on closed str-2tyfk: the unused/govet residuals it deferred were never filed, and its 'silently no-ops' diagnosis was wrong (nothing invokes go:lint)"
priority: P2
type: note
labels: [go, quality-gates, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-2tyfk
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Comment on closed str-2tyfk

Target: **str-2tyfk** (closed). Post as a comment only. Do not reopen. str-qwua7.32 gets a different comment (`go-lint-qwua7-32-note`), because its acceptance criteria differ.

Comment text:

> Audit 2026-09-22 follow-up. This issue was scoped to the 22 `fmt.Fprint*` errcheck findings, and its body allowed the "unrelated unused/govet findings" to be cleaned up "or file[d] separately". The errcheck cleanup landed (c1364378). The deferred residuals were never filed, and they are still on main as of 2026-09-23. `golangci-lint run ./...` in shatter-go exits 1 with 10 issues:
> - govet nilness at `protocol/handler.go:1325` (`nil == nil`)
> - staticcheck SA5011 at `instrument/property_test.go:544`
> - 7 unused functions: analyzeFunc, extractParams, mapTypeInfo, structTypeInfo, lookupAnalyzedByTargetID, toWrapperConstructors, toWrapperConstructorParams
>
> `gofmt -l` also lists 10 files.
>
> One correction to this issue's diagnosis. The `go:lint` precondition ("golangci-lint not installed (optional)") does not silently no-op: a failed Task precondition fails the task. The real gap is that no gate or CI job invokes `go:lint` at all, and CI does not install golangci-lint. The residual findings, and gating golangci-lint and gofmt in check-static, are tracked in `<id of go-lint-and-gofmt-gated>`. That issue requires forced-gate output at close.

(Filer: replace the `<id of ...>` placeholder.)
