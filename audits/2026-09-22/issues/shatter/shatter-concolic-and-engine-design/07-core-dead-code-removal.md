---
slug: core-dead-code-removal
kind: new
title: "Remove ~3,400 lines of production-dead shatter-core code (recursive, array_mutation, reporter+clustering, sequential scan(), shrink_witness)"
priority: P2
type: chore
labels: [audit-2026-09-22, cleanup, shatter-core, tech-debt]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Remove ~3,400 lines of production-dead shatter-core code (recursive, array_mutation, reporter+clustering, sequential scan(), shrink_witness)

Scope note: the slug is kept. The reachability gate that was part of this draft is now core-reachability-gate.

## Problem

Several shatter-core modules and functions have no non-test callers, yet they attract fixes, audit grades and planned proptests as if they were live, and they make parity reasoning harder. Examples:

- str-8q1b4 cites the dead sequential `scan()` as a fix site.
- str-qwua7.47 plans proptests for `array_mutation`.
- The prior audit graded clustering "Solid".

Only `export.rs` (1,743 lines) is tracked for deletion (str-qwua7.59).

## Evidence (re-verified at audit HEAD 56c86168)

| Item | Size | Callers outside own file / tests |
|---|---|---|
| `shatter-core/src/recursive.rs` | 675 lines | none |
| `shatter-core/src/array_mutation.rs` | 397 lines | none |
| `shatter-core/src/reporter.rs` | 1,326 lines | none |
| `shatter-core/src/clustering.rs` | 530 lines | only `reporter.rs` |
| `scan_orchestrator::scan()` (`scan_orchestrator.rs:1480`) | 482 lines | only the test at `scan_orchestrator.rs:7510` (helper doc at `:1223` says "Used by the non-parallel `scan()` path") |
| `shrink::shrink_witness` (`shrink.rs:40`) | — | tests only |
| `input_gen::mutate_mock_values` (`input_gen.rs:4241`) | — | tests only (`:8057`, `:8523`) |

The total is about 3,410 lines, excluding `export.rs`.

Check used, over the whole workspace (every crate's `src/`, `tests/` and `benches/`, and `examples/`, excluding `target/`): `grep -rln "recursive::\|array_mutation::\|reporter::\|clustering::\|shrink_witness\|mutate_mock_values" --include=*.rs`. It matches only `shatter-core/src/reporter.rs` (which uses clustering), `shrink.rs` and `input_gen.rs` (the items' own files). Test-only use inside those files is in `#[cfg(test)]` blocks.

## Acceptance criteria

- [ ] Each item above is either deleted, or wired into production behind a tracked issue whose ID appears in a comment at the item.
- [ ] `mutate_mock_values`: coordinate with concolic-mock-variation-regression. If that fix revives it as the production mock-variation path, it stays. Otherwise it is deleted.
- [ ] The workspace-wide grep above, re-run on the final branch, finds no reference to a deleted item. The output is in the close note.
- [ ] Note appended to str-qwua7.47 (open): drop `array_mutation` from its proptest list, because the module is deleted here.
- [ ] Proof at close: `cargo build --workspace --all-targets` and forced (uncached) `task check` output after the deletions.

## Suggested approach

One commit per module. Delete `reporter.rs` and `clustering.rs` together. Delete `scan()` together with its test and the doc reference at `:1223`.

## Out of scope

- The reachability gate (core-reachability-gate).
- Deleting `export.rs` (str-qwua7.59).
- Refactors of live code in the same files.

## Metadata

- Priority: P2
- Type: chore
- Labels: audit-2026-09-22, cleanup, shatter-core, tech-debt
- Parent epic: Epic: Audit 2026-09-22 findings
- Blocked by: none
- Related: core-reachability-gate, str-qwua7.59, str-qwua7.47, str-8q1b4, concolic-mock-variation-regression, engine-parity-e2e
- Source findings: core-08, core-17 (array_mutation part) (draft shatter-code/18)
