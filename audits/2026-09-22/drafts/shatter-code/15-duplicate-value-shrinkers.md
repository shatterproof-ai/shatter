# Two value shrinkers: str-55ep/str-ddxe/tuple fixes landed in the dead input_gen copy; live shrink.rs lacks them

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | bug |
| priority | P2 |
| labels | shrinking,shatter-core,refactor,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | str-55ep, str-ddxe, str-f4sow, str-v0yjq, str-kn3f, str-duens |
| source findings | core-05 |

<!-- body -->
## Problem

Both engines shrink through `crate::shrink::shrink_candidates`, but three shrink fixes were applied to a same-named, unused copy in input_gen.rs. The live shrinker still removes required object fields, proposes -1 for unsigned ints, and does not shrink tuples; open str-v0yjq also targets the dead copy.

## Current code facts / evidence

- Production callers: `shatter-core/src/orchestrator.rs:3716`, `explorer.rs:1850` → `crate::shrink::shrink_candidates`. `input_gen::shrink_candidates` (input_gen.rs:3574-3790) has no non-test caller.
- f40facf1 (str-55ep, 'shrink_object removes only optional fields') touched only input_gen.rs; `shrink.rs:512-539` `shrink_object` still removes every field.
- `shrink.rs:363-412` `shrink_int` has no range and always proposes -1; 3d458b2a (str-ddxe range-aware ints) did not add range-awareness there.
- shrink.rs has no positional-object (tuple) handling; input_gen has `shrink_positional_object`.
- Required-field removal is partly masked by the str-kn3f repair in planner_consumer.rs:317-330, but (unverified) the accepted witness may be stored unrepaired.

## Acceptance criteria

- shrink.rs keeps required fields, respects int_range/signedness, and shrinks tuples.
- input_gen's duplicate shrinker is deleted; str-v0yjq is retargeted at shrink.rs (note added).
- Proptests: candidates stay within int_range and retain required fields.
- Check whether planner_consumer stores the repaired input as the witness; fix if not.

## Suggested approach

Port the three fixes into shrink.rs, delete the copy, add proptests.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: S-M

## References

- Audit findings: core-05 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-55ep, str-ddxe, str-f4sow, str-v0yjq, str-kn3f, str-duens
