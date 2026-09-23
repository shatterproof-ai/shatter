---
slug: go-tool-reopen-note
kind: reopen-note
title: "NOTE on str-fl9g.2: documented `go get -tool .../go-tool/cmd/shatter` never resolved; fix tracked in go-tool-module-path"
priority: P1
type: note
labels: [go-tool, distribution, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [go-tool-module-path]
existing_id: str-fl9g.2
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# NOTE on str-fl9g.2 (closed): the acceptance command never resolved

Target: `str-fl9g.2` ("Go tool wrapper", closed, P1).

Action: add the comment below with `bd comments add str-fl9g.2`. Leave str-fl9g.2 closed; the work is carried by the new issue filed from `go-tool-module-path` (blocked_by lists it so the filer can substitute its real id into the comment). Do not re-close anything on the basis of this note.

## Comment text

> **Audit 2026-09-22 (finding frontend-go-02):** this issue was closed on "9dc76c85 landed on main", but its acceptance check ("a temp Go module can run `go get -tool` for the wrapper at a continuous tag and then `go tool shatter --version`") could never pass.
>
> - `shatter-go-tool/go.mod:1` declares `module github.com/shatterproof-ai/shatter/go-tool`, but the module lives in `shatter-go-tool/`; there is no `go-tool/` directory and no root `go.mod`, so the Go toolchain looks for `go-tool/` at the repo root.
> - `docs/distribution.md:64` (and the Renovate regex at :110) document `go get -tool github.com/shatterproof-ai/shatter/go-tool/cmd/shatter@...`.
> - Running it in a fresh temp module with `GOPROXY=direct` gives: `module github.com/shatterproof-ai/shatter@main found (v0.0.0-20260922164214-16794cef9e10), but does not contain package github.com/shatterproof-ai/shatter/go-tool/cmd/shatter`.
>
> The `continuous-*` tags themselves are fine: they are revision queries, which Go resolves to pseudo-versions for a nested module without directory-prefixed tags. Only the path is wrong.
>
> The fix (module path/directory alignment, a pre-merge local-resolution check, and a post-merge job that runs the documented command) is tracked in **<id of go-tool-module-path>**. Evidence: `audits/2026-09-22/areas/frontend-go.md` go-02.
