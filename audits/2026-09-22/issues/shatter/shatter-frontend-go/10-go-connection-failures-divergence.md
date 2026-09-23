---
slug: go-connection-failures-divergence
kind: new
title: "Go frontend never emits connection_failures or runtime_crypto_boundaries, so LiveFirst fallback never fires for Go; no divergence recorded"
priority: P2
type: bug
labels: [go-frontend, parity, protocol, mocking, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Go frontend never emits connection_failures or runtime_crypto_boundaries, so LiveFirst fallback never fires for Go; no divergence recorded

## Problem

The core's LiveFirst mock policy (try the live dependency, fall back to a mock when it is unreachable) is driven solely by `connection_failures` in the execute result. The Go frontend's `Response` type has neither `connection_failures` nor `runtime_crypto_boundaries`, and no Go code produces them, so for Go targets LiveFirst can never fall back and runtime crypto boundary splitting never happens. The parity matrix records this gap only for Rust; its Rust entry says vaguely that "TypeScript and Go emit the subset each supports", which hides the Go gap.

## Evidence

Re-verified against the audit worktree at commit 56c86168:

- `git grep -n -i "connection_failures\|ConnectionFailures\|runtime_crypto_boundaries\|RuntimeCryptoBoundaries" -- shatter-go ':!*_test.go'` returns nothing.
- `shatter-go/protocol/types.go:206-267` `type Response struct` has no such fields.
- `shatter-core/src/explorer.rs:891` `update_live_first_states` is driven only by `result.connection_failures`; called at `explorer.rs:1602` (random explorer) and `:2545`, and at `shatter-core/src/orchestrator.rs:3185` (concolic).
- `protocol/parity-matrix.yaml:1172-1190` `rust-execute-response-fields-partial` is the only record; there is no Go entry.
- Related tracker state: str-2fjn (open) notes the missing Go type fields only as registry-vs-types drift; str-924ca (open) is the Rust counterpart; str-3ky9.9.2 is the TS reference implementation (dial / connection-refused classification).

## Acceptance criteria

Implement (a), which is preferred, or record (b):

- [ ] (a) Go mocks/adapters detect connection failures (dial errors, connection refused, DNS failure on outbound calls; mirroring the TS classification) and report them in `connection_failures`; the Go `Response` carries both fields with the core's shapes. A Go E2E or conformance case with an unreachable live dependency under LiveFirst shows the fallback to a mock on the next iteration; the case fails on current main and passes on the branch.
- [ ] (b) Otherwise, a `go-execute-response-fields-partial` divergence entry in `protocol/parity-matrix.yaml` naming both fields and the LiveFirst consequence, with its own tracking issue, mirrored in `shatter-go/CLAUDE.md` per the matrix rules; the Rust entry's "TypeScript and Go emit the subset each supports" sentence is corrected.
- [ ] Either way: `task parity` and `task conformance` pass, and `task affected` passes with `Gates selected` recorded.

## Suggested approach

Look at how the TS frontend classifies connection failures (str-3ky9.9.2) and at where the Go frontend intercepts outbound calls for side-effect capture; add the classification there. `runtime_crypto_boundaries` can be declared as a divergence even if connection failures are implemented.

## Out of scope

- The Rust counterpart (str-924ca).
- The registry-vs-implementation cross-validation (str-2fjn).

## Dependencies

- Blocked by: none.
- Related: str-2fjn, str-924ca, str-3ky9.9.2.

## Size

M for (a), S for (b)

## References

- Finding protocol-parity-11 (audit 2026-09-22, `audits/2026-09-22/findings.json`; evidence `audits/2026-09-22/areas/protocol-parity.md`). Old draft: `drafts/shatter-docs-ui/27-go-connection-failures-divergence.md`.
