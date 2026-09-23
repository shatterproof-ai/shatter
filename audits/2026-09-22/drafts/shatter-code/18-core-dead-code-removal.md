# ~5,100 lines of production-dead code in shatter-core (recursive, array_mutation, reporter+clustering, sequential scan(), shrink_witness, mutate_mock_values); add a reachability check

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | chore |
| priority | P2 |
| labels | cleanup,shatter-core,tech-debt,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | str-qwua7.59, str-qwua7.47, str-8q1b4 |
| source findings | core-08 |

<!-- body -->
## Problem

Several shatter-core modules and functions have no non-test callers, attract fixes and audit grades as if live, and make parity reasoning harder. Only export.rs is tracked (str-qwua7.59).

## Current code facts / evidence

- No non-test references outside their own files: `recursive.rs` (675 lines), `array_mutation.rs` (397), `reporter.rs` (1,326), `clustering.rs` (530; only reporter uses it).
- `scan_orchestrator::scan()` (:1480, 482 lines) is only called by the test at :7510 (str-8q1b4 cites it as a fix site).
- `shrink_witness` (shrink.rs:40) and `input_gen::mutate_mock_values` have only test callers (see draft 14 for the latter).
- str-qwua7.47 plans proptests for array_mutation.

## Acceptance criteria

- Each item is deleted, or wired into production behind a tracked issue.
- A module-reachability check in `task check` lists pub modules/functions with zero references outside their own tests (allowlist permitted) and fails on new ones.
- str-qwua7.47 scope drops array_mutation (note appended).

## Suggested approach

Delete in one PR per module; coordinate mutate_mock_values with draft 14.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: S-M

## References

- Audit findings: core-08 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-qwua7.59, str-qwua7.47, str-8q1b4
