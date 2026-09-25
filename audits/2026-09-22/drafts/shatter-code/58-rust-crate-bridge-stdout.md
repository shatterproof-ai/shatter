# Rust crate-bridge harness shares stdout with user code: any function that prints fails as internal_error

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | bug |
| priority | P1 |
| labels | rust-frontend,crate-bridge,harness,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | none |
| source findings | frontend-rust-01 |

<!-- body -->
## Problem

The crate-bridge driver (automatic fallback for files inside a crate) writes its JSON result with `println!` on the same stdout the target prints to, and redirects no descriptors. A `println!` in user code corrupts the protocol line.

## Current code facts / evidence

- `shatter-rust/src/executor.rs:5195` crate-bridge driver emits via `println!(serde_json::to_string(&exec_result))`; dup2 redirection exists only in standalone/dispatch generators (`executor.rs:2524-2561`, `2720-2881`).
- `executor.rs:642-647`, `:6293-6329` PersistentHarness::execute treats the first stdout line as the response.
- Repro: `pub fn noisy(n:i64)->i64{println!("hello from user code {n}"); n+1}` under harness_mode crate_bridge → `internal_error output parse error: ... line: hello from user code 1`, outcome runtime_failed.
- parity-matrix notes (~line 491) record only that crate-bridge does not capture console output.

## Acceptance criteria

- Crate-bridge protocol output uses a private channel (dup original stdout to a private fd and redirect fd 1 around calls, or sentinel-prefixed lines with a skipping reader).
- Regression test: printing function in crate_bridge mode returns its value; console output is captured or discarded consistently with parity-matrix.
- shatter-rust/CLAUDE.md side-effect contract and parity-matrix updated.

## Suggested approach

Implementer's choice within the acceptance criteria above.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: M

## References

- Audit findings: frontend-rust-01 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: none
