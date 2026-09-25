---
slug: go-connection-failures-divergence
kind: new
title: "Record the Go execute-response gap in the parity matrix: Go never emits connection_failures or runtime_crypto_boundaries, so LiveFirst fallback never fires for Go"
priority: P2
type: task
labels: [go-frontend, parity, protocol, mocking, docs, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Record the Go execute-response gap in the parity matrix: Go never emits connection_failures or runtime_crypto_boundaries

## Problem

The core's LiveFirst mock policy (try the live dependency, fall back to a mock when it is unreachable) is driven solely by `connection_failures` in the execute result. The Go frontend's `Response` type has neither `connection_failures` nor `runtime_crypto_boundaries`, and no Go code produces them, so for Go targets LiveFirst can never fall back and runtime crypto boundary splitting never happens. The parity matrix records this gap only for Rust; its Rust entry says vaguely that "TypeScript and Go emit the subset each supports", which hides the Go gap.

This issue has one deliverable: make the gap visible and tracked, per the matrix rules. Implementing the signals is a separate issue, `go-connection-failures-impl`, because the Go frontend has no live outbound-call interception point today and building one is a much larger job.

## Evidence

Re-verified against the audit worktree at commit 56c86168:

- `git grep -n -i "connection_failures\|ConnectionFailures\|runtime_crypto_boundaries\|RuntimeCryptoBoundaries" -- shatter-go ':!*_test.go'` returns nothing.
- `shatter-go/protocol/types.go:206-267` `type Response struct` has no such fields.
- `shatter-core/src/explorer.rs:891` `update_live_first_states` is driven only by `result.connection_failures`; called at `explorer.rs:1602` (random explorer) and `:2545`, and at `shatter-core/src/orchestrator.rs:3185` (concolic).
- `protocol/parity-matrix.yaml:1172-1190` `rust-execute-response-fields-partial` is the only record; its description ends "TypeScript and Go emit the subset each supports"; there is no Go entry.
- Tracker (checked with `bd show` on 2026-09-23): str-2fjn (open) notes the missing Go type fields only as registry-vs-types drift; str-924ca (open) is the Rust counterpart; str-3ky9.9.2 is the TS reference implementation (dial / connection-refused classification).

## Acceptance criteria

- [ ] A `go-execute-response-fields-partial` divergence entry in `protocol/parity-matrix.yaml` names both fields, states the LiveFirst consequence (Go targets never fall back to a mock), and cites the issue filed from `go-connection-failures-impl` (look up its id in the epic) as its tracking issue. It is mirrored in `shatter-go/CLAUDE.md` per the matrix rules.
- [ ] The Rust entry's "TypeScript and Go emit the subset each supports" sentence is replaced with an accurate statement of which of the five fields TS and Go each emit (checked against `shatter-ts` and `shatter-go` source; list the file:line for each field TS emits).
- [ ] `task parity` and `task conformance` pass; `task affected` passes with `Gates selected` recorded.

## Out of scope

- Implementing the fields in Go (`go-connection-failures-impl`).
- The Rust counterpart (str-924ca).
- The registry-vs-implementation cross-validation (str-2fjn).

## Dependencies

- Blocked by: none.
- Related: `go-connection-failures-impl` (the tracking issue the entry cites), str-2fjn, str-924ca, str-3ky9.9.2.

## Size

S

## References

- Finding protocol-parity-11 (audit 2026-09-22, `audits/2026-09-22/findings.json`; evidence `audits/2026-09-22/areas/protocol-parity.md`). Old draft: `drafts/shatter-docs-ui/27-go-connection-failures-divergence.md`. Split after the Codex cross-check (finding 12: implement-or-document left the deliverable open).
