---
slug: protocol-rs-doc-comments
kind: new
title: "Core protocol.rs doc comments misstate who emits `outcome`, what `runtime_crypto_boundaries` does, and the error-code count"
priority: P3
type: bug
labels: [protocol, docs, core, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Core protocol.rs doc comments misstate who emits `outcome`, what `runtime_crypto_boundaries` does, and the error-code count

## Problem

Three doc comments on the core wire types make false cross-frontend or behavioral claims. Readers and agents take doc comments on `shatter-core/src/protocol.rs` as the contract, so a wrong comment steers work. For example, it can lead someone to treat `outcome: None` from TS/Rust as normal, or to assume that crypto-boundary splitting exists.

## Evidence (re-verified 2026-09-23 at 56c86168)

- `outcome`: `protocol.rs:1176-1184` says "TS / Rust frontends do not currently emit this field". Both do: TS `shatter-ts/src/executor.ts:3252` (`response.outcome = deriveOutcome(rawResult);`) and Rust `shatter-rust/src/handler.rs:1085` (`resp.outcome = Some(derive_execute_outcome(&result));`). The parity matrix says all three frontends support it.
- `runtime_crypto_boundaries`: `protocol.rs:1169-1173` says "The core engine uses this to apply boundary splitting: solve constraints on the plaintext then re-encrypt". The only consumers are `tracing::debug!` calls: `orchestrator.rs:3188-3191`, and `explorer.rs:1604-1610`, whose comment says it "will be used for boundary splitting in a future solver integration pass".
- `protocol.rs:2027`: "Canonical error code list (11 codes)" sits on `const ALL_ERROR_CODES: [(ErrorCode, &str); 12]` (:2030). The next lines claim "This match is exhaustive — adding a variant … causes a compiler error", but an array literal is not a match.
- Verifier correction (protocol-parity-10, partially confirmed, P2 → P3): the separate test `error_code_enum_is_exhaustive` (:2064-2086) already uses an exhaustive `match`, so a new `ErrorCode` variant does fail compilation. No new exhaustiveness mechanism is needed. Only the comments are wrong.

## Acceptance criteria

- [ ] The `outcome` comment says all three frontends emit it on execute responses, and that `None` means "not reported" (older or third-party frontends).
- [ ] The `runtime_crypto_boundaries` comment says the field is currently logged only and has no consumer in the solver.
- [ ] The `ALL_ERROR_CODES` comment gives the right count, or no count, and points to `error_code_enum_is_exhaustive` as the compile-time guard instead of claiming the array is one.
- [ ] Whether `runtime_crypto_boundaries` stays on the wire before it has a consumer is recorded: as a note on the relevant parity-matrix entry, or in a filed issue linked from the comment.
- [ ] `cargo test -p shatter-core --lib protocol` passes (comment-only change, no behavior change). The diff contains no code changes other than comments and the matrix note.

## Out of scope

Implementing crypto-boundary splitting, and changing the error-code tests.

## Dependencies

- Blocked by: none.
- Related: protocol-md-execute-fields (documents the same fields for frontend authors).

Size: XS. Priority: P3. Type: bug. Labels: protocol, docs, core, audit. Parent: Epic: Audit 2026-09-22 findings.
