# 38 shatter-rust tests print 'skipping' and pass when cargo cannot reach the network

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | bug |
| priority | P2 |
| labels | rust-frontend,tests,quality-gates,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | none |
| source findings | frontend-rust-11 |

<!-- body -->
## Problem

Tests that build fixture crates treat offline compile errors as a skip by printing and returning, which nextest (status-level fail) records as a pass. A networkless CI or sandbox run silently tests nothing.

## Current code facts / evidence

- `shatter-rust/src/executor.rs:7611-7616` `is_offline_compile_error_message` ('spurious network error', 'Could not resolve host', ...); 15+ match arms `Err(CompilationFailed(msg)) if is_offline_compile_error_message(&msg) => { eprintln!("skipping ...") }` (e.g. :8088, :8276); 38 'skipping' sites.
- `.config/nextest-standalone.toml:12,20` status-level = "fail".

## Acceptance criteria

- Offline skips are real skips: env gate (e.g. SHATTER_OFFLINE=1 → #[ignore]/filterset) or failure when CI=1.
- Dependencies prefetched (`cargo fetch` step) so CI never hits the offline path.
- rust-fe:test prints the skip count; CI asserts 0.

## Suggested approach

Implementer's choice within the acceptance criteria above.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: S

## References

- Audit findings: frontend-rust-11 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: none
