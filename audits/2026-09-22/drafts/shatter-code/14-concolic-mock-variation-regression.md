# Concolic dynamic mock variation regressed (str-3ky9.4 undone by str-lebv/str-r59s); hidden behind `_`-prefixed params

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | bug |
| priority | P2 |
| labels | concolic,mocking,regression,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | str-3ky9.4, str-lebv, str-r59s |
| source findings | core-04 |

<!-- body -->
## Problem

str-3ky9.4 added per-worklist-entry mock variation to the concolic orchestrator. Later MetaStrategy wiring made the mock params unused and silenced the warning with `_` prefixes, so concolic now explores with fixed mocks while the random explorer still varies them.

## Current code facts / evidence

- `shatter-core/src/orchestrator.rs:2640-2647` generates `_initial_mocks` ('Retained for future use; currently unused') and discards it (still consumes RNG draws); introduced by 9f2d2a3d (str-r59s).
- `orchestrator.rs:2147` `solve_and_generate(.., _mock_params: &[MockParam], ..)` from 0293c35c (str-lebv).
- Strategy worklist entries carry `mock_values: vec![]` (:2223, :2259) and fall back to `config.mocks`.
- `input_gen::mutate_mock_values` has only test callers (input_gen.rs:8081, 8533); random explorer regenerates mocks per iteration (explorer.rs:1536-1537, 2467-2468).

## Acceptance criteria

- Concolic worklist entries carry varied mock values (via `mutate_mock_values` or equivalent), or concolic explicitly documents and warns that mocks are fixed.
- The dead `_initial_mocks` block is removed.
- Concolic E2E fixture whose branch depends on a mocked dependency's return value reaches both sides.

## Suggested approach

Restore variation in the MetaStrategy loop; reuse the random explorer's mock generation.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: S-M

## References

- Audit findings: core-04 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-3ky9.4, str-lebv, str-r59s
