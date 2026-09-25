---
slug: go-connection-failures-impl
kind: new
title: "Go frontend: detect outbound connection failures during execute and emit connection_failures so LiveFirst can fall back to mocks"
priority: P3
type: feature
labels: [go-frontend, parity, protocol, mocking, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Go frontend: detect outbound connection failures during execute and emit connection_failures

## Problem

The core's LiveFirst policy falls back to a mock only when the execute result reports `connection_failures`. The Go frontend never reports them (see `go-connection-failures-divergence`, which records the gap in the parity matrix and cites this issue as its tracker). So a Go target with an unreachable live dependency under LiveFirst keeps failing live instead of switching to a mock.

Unlike TS, the Go frontend has no live outbound-call interception point today. Its side-effect handling is a pre-execution classification (`shatter-go/protocol/policy.go:28-36`, `SideEffectClass` values such as `network` and `database`) plus mock substitution at call sites (`shatter-go/instrument/mocksubst.go`); nothing observes a real `net.Dial` failing at runtime. This issue therefore starts by choosing that interception point.

## Evidence

- `git grep -n -i "connection_failures\|ConnectionFailures" -- shatter-go ':!*_test.go'`: no hits (audit worktree, 56c86168).
- `shatter-go/protocol/types.go:206-267` `Response` has no `connection_failures` field.
- `shatter-core/src/explorer.rs:891` `update_live_first_states` reads only `result.connection_failures` (callers `explorer.rs:1602`, `:2545`; `orchestrator.rs:3185`).
- str-3ky9.9.2 (TS reference implementation: dial / connection-refused / DNS classification).

## Acceptance criteria

- [ ] Design note in the issue (before code): where the Go harness observes outbound connection attempts (candidates: wrapping `net/http.DefaultTransport` and `net.Dialer` in the generated harness, or instrumenting call sites that the policy already classifies as `network`/`database`), what it can and cannot see (e.g. a target that builds its own `net.Dialer`), and how failures map to the core's `connection_failures` shape.
- [ ] The Go `Response` carries `connection_failures` with the core's shape, populated for dial errors, connection refused and DNS failures on the intercepted paths, mirroring the TS classification.
- [ ] Known-answer test: a Go fixture under `examples/go/` whose target calls an unreachable live dependency under LiveFirst. The first execute reports a connection failure and a later iteration runs with the mock. Added to `shatter-core/tests/e2e_concolic_go.rs` (or a conformance case); fails on current main and passes on the branch.
- [ ] Both explorer paths are covered (random `explorer.rs` and concolic `orchestrator.rs`), per the parallel-parity rule.
- [ ] `go-connection-failures-divergence`'s matrix entry is updated: `connection_failures` removed from the Go gap (the entry stays for `runtime_crypto_boundaries` unless that is also implemented); `shatter-go/CLAUDE.md` matches.
- [ ] `task e2e-go` passes (it runs `cargo test --test e2e_concolic_go -- --include-ignored`; paste the summary line showing `0 ignored` and the new test name). `task parity`, `task conformance` and `task affected` pass (`Gates selected` recorded).

## Out of scope

- `runtime_crypto_boundaries` for Go (may stay a recorded divergence).
- The Rust counterpart (str-924ca).

## Dependencies

- Blocked by: none. (`go-connection-failures-divergence` should land first so the matrix entry exists to update, but the two can proceed in parallel.)
- Related: `go-connection-failures-divergence`, str-3ky9.9.2, str-924ca, str-2fjn.

## Size

L

## References

- Finding protocol-parity-11 (audit 2026-09-22; evidence `audits/2026-09-22/areas/protocol-parity.md`). Split out of `go-connection-failures-divergence` after the Codex cross-check (finding 12) and the same-runtime review (no live-call hook exists in Go).
