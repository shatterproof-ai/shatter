# `shatter diff` has no producer: no command writes a Snapshot, and diff rejects every JSON Shatter emits

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | bug |
| priority | P1 |
| labels | diff,cli,regression,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | str-6k6.1, str-81xiw, str-qwua7.9.2 |
| source findings | artifacts-12, docs-02, goals-01 |

<!-- body -->
## Problem

README/QUICKSTART/SPEC present `shatter diff <snapshot> <current>` as the behaviour-regression workflow, but no command writes a Snapshot. str-6k6.1 ('JSON snapshot export and comparison') closed with reason 'Closed' having shipped only the reader.

## Current code facts / evidence

- `shatter-core/src/snapshot.rs:70-110` `Snapshot::from_behavior_map(s)` and `write_to_file` have no callers outside snapshot.rs (only a test at :603).
- `shatter-cli/src/commands/diff.rs:17-30` only reads.
- `shatter diff` on an explore artifact → `missing field created_at`; on `.shatter-cache/behavior-maps/*.json` → `missing field version`; on a --spec-out bundle → `missing field function_id`; all exit 2.
- QUICKSTART.md:160 shows `shatter diff snapshots/shipping.json current/shipping.json` without saying how to create either; SPEC.md:864-885 documents the format.
- spec-diff does detect return-value changes on the same data.

## Acceptance criteria

- Decision recorded in this issue: (a) add a producer (`explore/scan --snapshot-out PATH` via `Snapshot::from_behavior_maps`) or (b) retire `diff` in favour of spec-diff.
- If (a): E2E explore → snapshot → edit source → re-explore → `shatter diff` exits 1 and names the change.
- If (b): `diff` is removed/deprecated and README, QUICKSTART §5 and SPEC §2.6/§5.5 point at spec-diff.
- Either way no documented workflow references a file nothing produces.

## Suggested approach

Needs a maintainer decision (a vs b) first; the rest is mechanical. Note str-81xiw plans `diff-explore` and says not to repurpose `diff` incompatibly.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: M

## References

- Audit findings: artifacts-12, docs-02, goals-01 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-6k6.1, str-81xiw, str-qwua7.9.2
