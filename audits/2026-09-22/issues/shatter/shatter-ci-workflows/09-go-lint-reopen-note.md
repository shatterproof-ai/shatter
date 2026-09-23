---
slug: go-lint-reopen-note
kind: reopen-note
title: "Comment on closed str-2tyfk and str-qwua7.32: closed with residuals and a false 'lint passes'; golangci-lint still reports 10 issues"
priority: P2
type: note
labels: [go, quality-gates, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-2tyfk
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Comment on closed str-2tyfk (and str-qwua7.32)

Targets: **str-2tyfk** (closed) and **str-qwua7.32** (closed). Post the same comment on both. Do not reopen either.

Comment text:

> Audit 2026-09-22 follow-up. This issue was closed without its lint acceptance being true. str-2tyfk has an empty close reason and was closed on a narrowed "changed files only" check. str-qwua7.32 was closed as "Closed" with the acceptance "task go:lint passes". On main as of 2026-09-23, `golangci-lint run ./...` in shatter-go still exits 1 with 10 issues:
> - govet nilness at `protocol/handler.go:1325` (`nil == nil`)
> - staticcheck SA5011 at `instrument/property_test.go:544`
> - 7 unused functions: analyzeFunc, extractParams, mapTypeInfo, structTypeInfo, lookupAnalyzedByTargetID, toWrapperConstructors, toWrapperConstructorParams
>
> str-2tyfk's own body listed several of these residuals. `gofmt -l` also lists 10 files.
>
> The root cause is that `go:lint` is optional ("golangci-lint not installed (optional)") and no gate or CI job runs it, so "lint passes" was never checked by anything. The fix, gating golangci-lint and gofmt in check-static, is tracked in `<id of go-lint-and-gofmt-gated>`. That issue requires forced-gate output at close.

(Filer: replace the `<id of ...>` placeholder.)
