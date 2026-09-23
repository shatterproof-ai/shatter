---
slug: snapshot-diff-reopen-note
kind: reopen-note
title: "Reopen-note on closed str-6k6.1: only the snapshot reader shipped; snapshot `shatter diff` is being retired (D2); see retire-snapshot-diff"
priority: P1
type: note
labels: [audit-2026-09-22]
parent_epic: ""
blocked_by: [retire-snapshot-diff]
existing_id: str-6k6.1
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Reopen-note on closed str-6k6.1: only the snapshot reader shipped; snapshot `shatter diff` is being retired (D2); see retire-snapshot-diff

## Tracker action

- Add the comment below to **str-6k6.1** (`bd comments add str-6k6.1 …`).
- Do not reopen it. Under D2 the missing producer will not be built.
- Filing order: file retire-snapshot-diff first, then replace `<retire-snapshot-diff id>` with its real id.

## Comment text

> Audit 2026-09-22 (findings goals-01, artifacts-12, docs-02): this issue ("JSON snapshot export and comparison") closed with reason "Closed" after shipping only the comparison half.
>
> - The reader and differ exist: `Snapshot::read_from_file` and `snapshot::diff`, used by `shatter diff` (`shatter-cli/src/commands/diff.rs`).
> - The export half never shipped. `Snapshot::from_behavior_map(s)` and `write_to_file` (`shatter-core/src/snapshot.rs:70-110`) have no callers outside the module, and no command or flag writes a snapshot.
> - `shatter diff` exits 2 on every JSON Shatter emits: `missing field created_at` (explore artifact), `missing field version` (behavior-map cache) and `missing field function_id` (`--spec-out` bundle). README, QUICKSTART §5 and SPEC §2.6/§5.5 still document the workflow.
>
> Maintainer decision D2 (2026-09-23): the snapshot `shatter diff` command and the unused Snapshot module are being retired, and `shatter spec-diff` is the regression tool. No snapshot producer will be added. The removal and the doc changes are tracked in **<retire-snapshot-diff id>** ("Retire the snapshot `shatter diff` command and the unused Snapshot module; make spec-diff the documented regression tool"). Not reopening this issue.
