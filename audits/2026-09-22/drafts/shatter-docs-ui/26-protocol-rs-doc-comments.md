# Core protocol.rs doc comments misstate who emits `outcome`, what `runtime_crypto_boundaries` does, and the error-code count

- Priority: P3
- Type: bug
- Labels: protocol,docs,core
- Tracker action: new issue
- Source findings: audit 2026-09-22 protocol-parity-10 (partially confirmed; the exhaustiveness test already exists)

<!-- body -->
## Current code facts
- `shatter-core/src/protocol.rs:1177-1184`: the `outcome` doc says TS and Rust frontends do not emit it. TS `shatter-ts/src/executor.ts` (around 3252) sets `response.outcome`, and Rust `shatter-rust/src/handler.rs:1085` sets `resp.outcome`. The matrix says all three frontends support it.
- `protocol.rs:1169-1175`: the `runtime_crypto_boundaries` doc says "The core engine uses this to apply boundary splitting". The only consumer is a `tracing::debug!` at `orchestrator.rs:3187-3196`, and `explorer.rs:1603-1616` says it "will be used … in a future solver integration pass".
- `protocol.rs:2027-2030` says "(11 codes)" on a `[_; 12]` array. The exhaustive-match test `error_code_enum_is_exhaustive` (around line 2064) already catches new variants.

## Acceptance criteria
- The three comments are corrected.
- Record in the parity matrix, or an issue, whether `runtime_crypto_boundaries` stays on the wire before it has a consumer.
