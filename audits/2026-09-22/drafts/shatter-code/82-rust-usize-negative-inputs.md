# Rust frontend still generates negative integers for usize params (str-ddxe regression?); deserialization failures appear as behaviors

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | bug |
| priority | P2 |
| labels | rust-frontend,input-generation,regression,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | str-ddxe, str-qwua7.14, str-4yc9w |
| source findings | goals-15 |

<!-- body -->
## Problem

str-ddxe (closed) added sized int_width/int_signed and in-range generation for unsigned Rust ints, yet exploring `parse_language_preference` yields many `input 1 deserialization failed: invalid value: integer -998, expected usize` rows, reported as target throws.

## Current code facts / evidence

- Repro: `explore 18_accept_language.rs:parse_language_preference` → 23 rows, 19 'expected usize' deserialization failures (values -998, -44, -1, -644, -838, i64::MIN).
- Harness path that bypasses the str-ddxe fix not yet identified (check shatter-rust/src/analyzer.rs type mapping and shatter-core/src/input_gen.rs unsigned handling for this signature).
- Go counterpart of misclassification: str-4yc9w.

## Acceptance criteria

- usize (and u8/u16/u32/u64) params never receive negative values on any generation path (random, mutation, solver, seeds).
- Harness deserialization failures are classified as tool/input errors, not target throws.
- E2E known-answer test on parse_language_preference has zero deserialization-failure rows.

## Suggested approach

Start by diffing TypeInfo emitted for this function vs the u8 e2e fixture str-ddxe used.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: S

## References

- Audit findings: goals-15 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-ddxe, str-qwua7.14, str-4yc9w
