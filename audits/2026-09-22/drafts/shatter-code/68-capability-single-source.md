# Protocol capability facts are hand-replicated in six places, four parity-matrix sections are validated by nothing, and codegen covers 6 of 13 registry enums

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | task |
| priority | P2 |
| labels | parity,protocol,architecture,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | str-qwua7.24, str-2fjn |
| source findings | protocol-parity-08, protocol-parity-19 |

<!-- body -->
## Problem

Capability lists are maintained by hand in frontend handshakes, registry.yaml frontends block, parity-matrix, golden handshake files, PARITY.md and CLAUDE.md/skills, with only pairwise checks. side_effect/feature/adapter/shared_wire_types matrix sections are read by no script. protocol-codegen emits only the legacy enums; error_category already disagrees (TS has 'unknown').

## Current code facts / evidence

- `git grep -l 'side_effect_capabilities|feature_capabilities|adapter_capabilities|shared_wire_types'` → only the matrix, docs, conformance comments, one prose mention in explore.rs.
- `scripts/protocol-codegen.py` → `shatter-ts/src/generated/protocol-enums.ts` exports PROTOCOL_VERSION, commands, statuses, error codes, setup levels, generator kinds, branch types; registry declares 13 enums incl. outcome_status, value_plan_kind, error_category, trace_event_type, crypto_boundary_kind.
- Registry error_category = [validation, runtime, infrastructure]; `shatter-ts/src/protocol.ts:631-635` adds 'unknown'.

## Acceptance criteria

- parity-matrix.yaml is the single capability source; registry frontends block, golden expectations and generated tables are derived with `--check` in `task parity`.
- Feature/side-effect sections get detectors or are labelled documentation-only.
- protocol-codegen emits every registry enum for TS, Go and Rust FE; core test asserts serde spellings equal the registry; error_category mismatch resolved.

## Suggested approach

Coordinate with str-qwua7.24 (generated doc tables).

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: L

## References

- Audit findings: protocol-parity-08, protocol-parity-19 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-qwua7.24, str-2fjn
