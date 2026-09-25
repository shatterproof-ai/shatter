---
slug: duplicate-value-shrinkers
kind: new
title: "Two value shrinkers: str-55ep/str-ddxe/tuple fixes landed in the dead input_gen copy; live shrink.rs removes required fields and proposes -1 for unsigned ints"
priority: P2
type: bug
labels: [shrinking, shatter-core, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Two value shrinkers: str-55ep/str-ddxe/tuple fixes landed in the dead input_gen copy; live shrink.rs removes required fields and proposes -1 for unsigned ints

## Problem

Both engines shrink through `crate::shrink::shrink_candidates`. Three shrinker fixes were applied to a different, unused function with the same name, `input_gen::shrink_candidates`. The live shrinker in `shrink.rs` therefore still:

- removes every object field, including required ones (str-55ep claimed to fix this);
- proposes `-1` for every integer, including unsigned and range-limited params (str-ddxe claimed range-aware ints);
- does not shrink positional objects (tuples); `input_gen` has `shrink_positional_object`.

The open issue str-v0yjq (enum_values shrink work) also targets the dead copy.

## Evidence

Line numbers were re-checked against `56c86168`:

- Production callers use the live shrinker: `shatter-core/src/orchestrator.rs:3716` and `shatter-core/src/explorer.rs:1850` call `crate::shrink::shrink_candidates`, and `explorer.rs:1802` calls `crate::shrink::grouped_shrink_candidates`.
- `shatter-core/src/input_gen.rs:3574` `pub fn shrink_candidates` (the body runs to about :3790) has no non-test caller. `input_gen.rs:3752` `shrink_positional_object` exists only there.
- `shatter-core/src/shrink.rs:512-540` `shrink_object` loops over all `fields` and removes each one ("Remove each field one at a time"), with no optional-only check.
- `shatter-core/src/shrink.rs:395-412` `shrink_int(value)` takes no type or range and always pushes `SHRINK_INT_NEG_ONE` (`-1`, defined at :199). `shrink_candidates` at `:363-380` dispatches `TypeInfo::Int { .. } => shrink_int(value)` and discards `int_width`/`int_signed`.
- f40facf1 (str-55ep, "shrink_object removes only optional fields") touched only `input_gen.rs`. 3d458b2a (str-ddxe, range-aware ints) edited `shrink.rs` but did not add range awareness there.
- Required-field removal is partly masked by the str-kn3f repair funnel in `shatter-core/src/planner_consumer.rs:312-330` (`repair_required_fields`). The audit found, but the verifier did not confirm, that the accepted witness (`current = bulk_trial`, `explorer.rs:1790`) may be stored unrepaired.

## Acceptance criteria

- [ ] `shrink.rs` keeps required object fields (removes only optional ones), respects `int_range()`/signedness (no negative candidates for unsigned params, and all candidates within range), and shrinks positional objects (tuples) while keeping their arity.
- [ ] `input_gen::shrink_candidates` and its private helpers are deleted, along with any tests that only exercise the dead copy. Tests worth keeping are ported to `shrink.rs`.
- [ ] Proptests in `shrink.rs`: for any value and `TypeInfo::Int` with a range, every candidate stays within `int_range`. For any object type, every candidate keeps all required fields. For tuples, arity is preserved. At close, show the new required-field and unsigned tests failing on current `main` and passing after the fix.
- [ ] Check whether the accepted shrink witness is stored with required-field repair applied, in both `explorer.rs` and `orchestrator.rs`. Fix it if not, and state the finding in the close note.
- [ ] A comment is added on open str-v0yjq, retargeting its enum_values shrink work at the live `shrink.rs` instead of `input_gen` (and the dead `recursive.rs`).
- [ ] `task affected` passes, with `Gates selected` recorded.

## Suggested approach

Port the three fixes (optional-only field removal, range-aware ints, positional objects) from `input_gen.rs` into `shrink.rs`, then delete the copy and add the proptests. Give `shrink_int` the `TypeInfo` (or the range) instead of the bare value.

## Out of scope

- The enum_values shrink work itself (str-v0yjq). This issue only retargets it.
- Extracting a shared shrink phase between the engines (str-qwua7.6.1).
- A crate-wide dead-code check (core-reachability-gate, bucket shatter-concolic-and-engine-design; split from core-dead-code-removal).

## Priority

P2

## Type

bug

## Dependencies

- Blocked by: none.
- Related: str-v0yjq (open; retarget), str-55ep (closed but not fixed), str-ddxe (closed but not fixed), str-f4sow, str-kn3f, str-duens.

## References

Audit 2026-09-22 finding core-05 (verified, P2). Report §14 says to retarget str-v0yjq. Source draft: `drafts/shatter-code/15-duplicate-value-shrinkers.md`.
