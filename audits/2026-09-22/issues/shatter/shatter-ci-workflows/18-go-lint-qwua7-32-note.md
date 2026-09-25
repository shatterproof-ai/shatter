---
slug: go-lint-qwua7-32-note
kind: reopen-note
title: "Comment on closed str-qwua7.32: acceptance 'task go:lint passes' was false at close; golangci-lint still reports 10 issues"
priority: P2
type: note
labels: [go, quality-gates, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-qwua7.32
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Comment on closed str-qwua7.32

Target: **str-qwua7.32** (closed, close reason "Closed"). Post as a comment only. Do not reopen.

Comment text:

> Audit 2026-09-22 follow-up. One of this issue's acceptance checks was "The encoding/json.Unmarshal line is removed; task go:lint passes." It was closed with the reason "Closed" and no lint output cited. At that point `task go:lint` could not pass: str-2tyfk, filed from this issue's own work, recorded pre-existing unused and govet findings that were deferred and never fixed. On main as of 2026-09-23, `golangci-lint run ./...` in shatter-go still exits 1 with 10 issues:
> - govet nilness at `protocol/handler.go:1325` (`nil == nil`)
> - staticcheck SA5011 at `instrument/property_test.go:544`
> - 7 unused functions
>
> Whether the json.Unmarshal exclusion removal and its call-site fixes are complete was not re-audited. This note concerns only the lint acceptance.
>
> The gap that let this through is that no gate or CI job invokes `go:lint`, so "lint passes" was never checked mechanically. The fix, gating golangci-lint and gofmt in check-static with forced-gate proof at close, is tracked in `<id of go-lint-and-gofmt-gated>`.

(Filer: replace the `<id of ...>` placeholder.)
