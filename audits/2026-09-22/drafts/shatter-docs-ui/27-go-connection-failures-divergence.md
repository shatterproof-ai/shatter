# Go frontend never emits connection_failures or runtime_crypto_boundaries, so LiveFirst fallback never fires for Go; no divergence recorded

- Priority: P2
- Type: bug
- Labels: go-frontend,parity,protocol,mocking
- Tracker action: new issue (the str-2fjn notes record only the missing Go type fields as registry drift)
- Related: str-2fjn, str-924ca (Rust counterpart), str-3ky9.9.2 (TS reference)
- Source findings: audit 2026-09-22 protocol-parity-11 (confirmed)

<!-- body -->
## Current code facts
- A `git grep` over non-test Go code finds no `connection_failures`, `ConnectionFailures`, `runtime_crypto_boundaries` or `RuntimeCryptoBoundaries`. The Go `Response` type (`shatter-go/protocol/types.go:227-245`) lacks both fields.
- `update_live_first_states` (`shatter-core/src/explorer.rs:885-910`, called at `:1602` and `:2545`) is driven only by `result.connection_failures`. The orchestrator uses the same path (`orchestrator.rs:3185`). So LiveFirst mock fallback can never trigger for Go targets.
- `protocol/parity-matrix.yaml:1172-1190` has only `rust-execute-response-fields-partial`, which says vaguely that "TypeScript and Go emit the subset each supports".

## Acceptance criteria
Either:
- (a) The Go mocks/adapters report connection failures (dial and connection-refused classification, mirroring TS), with a Go E2E or conformance case where a live dependency is unreachable and LiveFirst falls back. Or:
- (b) A `go-execute-response-fields-partial` divergence entry with a tracking issue that names the LiveFirst consequence, mirrored per the matrix rules, and `task parity` passing.

The choice between (a) and (b) is left to the implementer, with (a) preferred.
