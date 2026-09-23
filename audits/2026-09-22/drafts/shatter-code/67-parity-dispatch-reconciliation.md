# Parity gates never check dispatch: a command advertised but not dispatched passes every static gate

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | task |
| priority | P2 |
| labels | parity,protocol,quality-gates,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | str-qwua7.7, str-2fjn |
| source findings | protocol-parity-01 |

<!-- body -->
## Problem

validate-parity.py compares only handshake capability lists; validate-protocol-registry.py only warns on a missing command. A mutation removing `prepare` dispatch from all three frontends while still advertising it passed both.

## Current code facts / evidence

- `scripts/validate-parity.py:294-386` detectors read SUPPORTED_CAPABILITIES (TS), CommandCapabilities + handleHandshake (Go), handle_handshake (Rust).
- `scripts/validate-protocol-registry.py:641-669` warns (does not fail) on missing commands; TS extraction returns empty sets.
- Dispatch sites: `shatter-ts/src/handlers.ts:556`, `shatter-go/protocol/handler.go:273`, `shatter-rust/src/handler.rs:547`.
- Only prepare conformance case: `protocol/conformance/conformance_cases.yaml:167-168` frontends:[rust].

## Acceptance criteria

- validate-parity.py extracts dispatch arms for each frontend and hard-fails when a command the matrix marks implemented (or the handshake advertises) is not dispatched, and vice versa.
- One minimal conformance case per (frontend × implemented command) is generated or hand-written.
- Mutation test (scripted) shows the gate goes red.

## Suggested approach

Implementer's choice within the acceptance criteria above.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: M

## References

- Audit findings: protocol-parity-01 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-qwua7.7, str-2fjn
