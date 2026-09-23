---
slug: invariant-markdown-blank-subjects
kind: new
title: "Spec markdown renders invariants with blank subjects and duplicate lines (\"-  != null [1] (100/100)\")"
priority: P2
type: bug
labels: [spec, report, invariants, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Spec markdown renders invariants with blank subjects and duplicate lines ("-  != null [1] (100/100)")

## Problem

`shatter explore --invariants --spec ts/01-arithmetic.ts:classifyNumber` prints this in the markdown spec:

```
**Function invariants:**
-  != null [1] (100/100)
-  != null [1] (100/100)
-  is non-empty [1] (100/100)
```

The subject is missing, so the reader cannot tell what is non-null. Two lines are identical. The JSON output of the same run has usable labels (for example `input.age is non-null`), and YAML renders `input is non-null`. Only the markdown renderer is broken. The feature is opt-in (`--invariants`), which is why this is P2: the audit finding artifacts-05 was filed at P1, and the verifier lowered it to P2.

## Evidence

Re-checked against the audit worktree (`/home/ketan/.local/share/worktrees/shatter/audit-2026-09-22`, HEAD 56c86168) on 2026-09-23:

- `shatter-core/src/spec.rs:568-577` (function-wide invariants) and `shatter-core/src/spec.rs:617-626` (per-class invariants) both format `"- {} [{}] ({}/{})"` from `ci.invariant.description, ci.confidence, ci.satisfied_count, ci.total_count`.
- `shatter-core/src/invariants.rs:144-146`: `format_path` is `path.join(".")`. For a scalar parameter or a return value the path is empty, so the description starts with an empty subject.
- `shatter-core/src/invariants.rs:638`: `ClassifiedInvariant.label` exists and holds the subject-qualified text. The markdown renderer does not use it.
- The spec.rs unit tests use synthetic descriptions such as `x > 0`, so no test runs the real detector through the markdown renderer.
- Captured sample: `audits/2026-09-22/artifact-samples/ts-spec-invariants.md:21-24` (the three lines above). The JSON of the same run is `ts-spec-invariants.json`. These paths are on branch `audit-2026-09-22` until the audit reports land.

Repro: `shatter explore --invariants --spec <examples>/standalone/ts/01-arithmetic.ts:classifyNumber`, then read the "Function invariants" block.

## Acceptance criteria

- [ ] The markdown renderer uses `ClassifiedInvariant.label` (or an equivalent subject-qualified string). For a scalar-parameter function every invariant line names its subject (`input`, `return`, or the parameter name).
- [ ] Duplicate invariant lines are removed. Invariants that only restate the class precondition (for example `input == 0` inside the zero class) are suppressed.
- [ ] A golden test runs the real invariant detector on `classifyNumber` with `--invariants --spec` and pins the markdown output. The test fails on current code (blank subject, duplicate line) and passes after the fix. Record the failing run in the close comment.
- [ ] The `[1] (n/n)` confidence suffix follows str-qwua7.61 (remove the confidence score). Whichever lands second rebases onto the other; the close comment says which.
- [ ] JSON and YAML invariant output are unchanged, or the change is recorded in the spec changelog.

## Suggested approach

Switch both render sites in `spec.rs` to `ci.label`. De-duplicate by label before rendering. Add a precondition-restatement filter keyed on the class's preconditions. Put the golden test next to the existing spec tests, or in the CLI golden suite if golden-and-consumer-suite has landed.

## Out of scope

- Removing the confidence score itself (str-qwua7.61).
- Changing invariant detection.

## Related

str-qwua7.61 (confidence score removal), str-qwua7.47. Source finding: artifacts-05 (confirmed; P1 lowered to P2 by the verifier).
