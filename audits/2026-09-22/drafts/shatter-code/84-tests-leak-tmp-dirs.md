# Shatter tests leak per-run directories into shared /tmp (hundreds of crate-bridge harness dirs, GBs) — widen filesystem isolation

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | bug |
| priority | P2 |
| labels | tests,tempdir,rust-frontend,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | str-dl2pj, str-ri1z, str-jeen.64 |
| source findings | sessions-08 |

<!-- body -->
## Problem

Tests fall back to `std::env::temp_dir()` for harness caches and flag files and never clean up; the same shared-/tmp coupling caused the discover_configs flake (str-dl2pj, user asked for filesystem isolation).

## Current code facts / evidence

- `shatter-rust/src/executor.rs:856` `shatter-bin-only-` and `:3247` `shatter-crate-bridge-{key:016x}` fallbacks under temp_dir().
- `shatter-core/src/scan_orchestrator.rs` ~9489 test flag `shatter-id-mismatch-injected-{pid}` under temp_dir().
- /tmp counts at audit time: 264 shatter-crate-bridge-* (earlier 127 totalling 6.0 GB), 74 each shatter-id-mismatch-injected-*/execute-exits-twice-*/dead-after-handshake-*, 44 shatter-bin-only-*.

## Acceptance criteria

- Tests use tempfile::TempDir or a per-test harness_cache_root; no test writes shared temp_dir() paths.
- Test-hygiene check (CI or drift-patrol) fails if a test run leaves new /tmp/shatter-* entries.
- str-dl2pj linked (widened or closed by this).

## Suggested approach

Implementer's choice within the acceptance criteria above.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: M

## References

- Audit findings: sessions-08 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-dl2pj, str-ri1z, str-jeen.64
