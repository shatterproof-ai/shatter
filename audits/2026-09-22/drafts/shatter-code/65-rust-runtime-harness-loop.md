# shatter-rust-runtime harness loop swallows malformed requests; branches on user-spawned threads are silently lost

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | bug |
| priority | P3 |
| labels | rust-frontend,runtime,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | str-dfnu2, str-oc67 |
| source findings | frontend-rust-15 |

<!-- body -->
## Problem

The runtime harness turns an unparseable request into Null and runs with empty inputs (misleading 'input 0 deserialization failed'), and branch tracking is thread-local so branches on std::thread/rayon/spawn_blocking threads are dropped with no documentation.

## Current code facts / evidence

- `shatter-rust-runtime/src/lib.rs:539-600` (e.g. :558, :612) `serde_json::from_str(line).unwrap_or_default()`.
- `lib.rs:340-345` flush_results fallback interpolates error text into JSON without escaping (unverified).
- `lib.rs:167` `thread_local!` STATE; str-dfnu2/str-oc67 fixed only tokio cross-worker awaits.

## Acceptance criteria

- Unparseable request → explicit protocol-error result.
- Fallback JSON built with serde_json::json!.
- Thread-local limitation documented in shatter-rust/CLAUDE.md + parity-matrix, or a process-global recorder keyed by execution id.

## Suggested approach

Implementer's choice within the acceptance criteria above.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: S

## References

- Audit findings: frontend-rust-15 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-dfnu2, str-oc67
