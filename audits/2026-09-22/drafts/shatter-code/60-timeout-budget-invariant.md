# Request timeout (30 s) is not longer than the build timeout (30/120 s): cold Rust builds surface as a generic request timeout

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | bug |
| priority | P2 |
| labels | timeout,rust-frontend,cli,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | str-qe9pp, str-da35, str-jyxr |
| source findings | frontend-rust-04 |

<!-- body -->
## Problem

The first execute of a Rust target builds and runs inside one request. The CLI request timeout defaults to the same 30 s as the build timeout, so a cold build under load shows only `request timed out after 30s` with 0 iterations.

## Current code facts / evidence

- `shatter-cli/src/args.rs:561-563` --request-timeout default 30; CLI build-timeout default 30 (PARITY.md:82/84).
- `shatter-rust/src/executor.rs:1017` DEFAULT_BUILD_TIMEOUT_SECS=120 (eeceb7b4) while PARITY.md:96 says 30 s.
- Repro under load: explore two trivial Rust fns → `concolic observe failed: frontend error: request timed out after 30s` at 32.7 s; `--request-timeout 180 --build-timeout 170` succeeds.
- str-da35 notes said cold builds need manual timeout bumps that 'should be auto-set'.

## Acceptance criteria

- Invariant enforced: request timeout for requests that may build > build_timeout + exec_timeout (derived, or prepare/first-execute gets its own budget).
- A cold-build timeout produces a diagnostic naming --build-timeout / --request-timeout.
- PARITY.md:96 corrected; unit test for the invariant.

## Suggested approach

Implementer's choice within the acceptance criteria above.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: S

## References

- Audit findings: frontend-rust-04 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-qe9pp, str-da35, str-jyxr
