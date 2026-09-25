---
slug: invariant-markdown-blank-subjects
kind: new
title: "Spec markdown renders invariants with blank subjects (\"-  != null [1] (100/100)\"), so input and output invariants look like duplicates"
priority: P2
type: bug
labels: [spec, report, invariants, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Spec markdown renders invariants with blank subjects ("-  != null [1] (100/100)"), so input and output invariants look like duplicates

## Problem

`shatter explore --invariants --spec ts/01-arithmetic.ts:classifyNumber` prints this in the markdown spec:

```
**Function invariants:**
-  != null [1] (100/100)
-  != null [1] (100/100)
-  is non-empty [1] (100/100)
```

The subject is missing, so the reader cannot tell what is non-null. The first two lines look identical, but they are different invariants: one is about the input and one is about the output. The JSON output of the same run carries the correct subject-qualified text in `label` (`input is non-null`, `output is non-null`, `output is non-empty string`). The markdown renderer prints `invariant.description` instead, and for a scalar parameter or return value that description starts with an empty path.

The feature is opt-in (`--invariants`), which is why this is P2: the audit finding artifacts-05 was filed at P1 and the verifier lowered it to P2.

## Evidence

Re-checked on 2026-09-23 in the audit worktree (`/home/ketan/.local/share/worktrees/shatter/audit-2026-09-22`, HEAD 793f2b0b; code under `shatter-core/` and `shatter-cli/` is identical to 56c86168):

- `shatter-core/src/spec.rs:568-577` (function-wide invariants) and `shatter-core/src/spec.rs:617-626` (per-class invariants) both format `"- {} [{}] ({}/{})"` from `ci.invariant.description, ci.confidence, ci.satisfied_count, ci.total_count`.
- `shatter-core/src/invariants.rs:143-146`: `format_path` is `path.join(".")`. For a scalar parameter or a return value the path is empty, so `description` starts with an empty subject (for example `" != null"`).
- `shatter-core/src/invariants.rs:630-645`: `ClassifiedInvariant.label` holds the subject-qualified text. The markdown renderer does not use it.
- The spec.rs unit tests use synthetic descriptions such as `x > 0`, so no test runs the real detector through the markdown renderer.
- Matching markdown and JSON from one input, captured during this revision with `target/release/shatter` built from the audit worktree:
  - `shatter explore --allow-host-writes --invariants --spec <examples>/standalone/ts/01-arithmetic.ts:classifyNumber` prints the three blank-subject lines above.
  - `shatter explore --allow-host-writes --invariants --spec --spec-json <same target>` gives function-wide invariants with `description` / `label` pairs `" != null"` / `input is non-null`, `" != null"` / `output is non-null`, and `" is non-empty"` / `output is non-empty string`. Per-class invariants follow the same pattern (for example `" == 0"` / `input == 0` in the "zero" class).
- Correction to the earlier draft: the audit sample `audits/2026-09-22/artifact-samples/ts-spec-invariants.json` describes `categorizeUser`, not `classifyNumber`, so it is not the JSON of the same run as `ts-spec-invariants.md`. Use the commands above as the evidence.

Repro: run the two commands above (`<examples>` is `/home/ketan/project/examples`; `--allow-host-writes` is needed when the sandbox is unavailable), then compare the "Function invariants" markdown block with the `invariants[].label` values in the JSON.

## Acceptance criteria

- [ ] Both markdown render sites in `spec.rs` (function-wide and per-class) print `ClassifiedInvariant.label`, not `invariant.description`. For `classifyNumber`, the function-wide block contains exactly three lines, with the subjects `input is non-null`, `output is non-null` and `output is non-empty string`. No rendered invariant line starts with a space after the `- ` bullet.
- [ ] A test runs the real invariant detector (not a hand-built `ClassifiedInvariant`) on `classifyNumber` through the markdown renderer and asserts the three labelled lines above. This can be a CLI test that runs `explore --invariants --spec` on the example, or a core test that feeds recorded executions to the detector and then to `format_spec_markdown`. Close-time proof: the test output failing on the unfixed code (blank subjects) and passing after the fix, both pasted into the close comment.
- [ ] The `[1] (n/n)` confidence suffix follows str-qwua7.61 (remove the confidence score). Whichever issue lands second rebases onto the other; the close comment says which landed first.
- [ ] The fix changes only the markdown renderer: the diff touches no serde attribute, JSON/YAML view struct or serializer, and the existing spec JSON/YAML tests pass unchanged. If a JSON or YAML change turns out to be needed, the spec schema version is bumped under the bump policy in `shatter-core/src/spec.rs:236-252` (its doc comment is the schema changelog).

## Suggested approach

Switch both render sites to `ci.label`. Put the test next to the existing spec tests in `shatter-core`, or in the CLI golden suite if golden-and-consumer-suite has landed.

## Out of scope

- Removing the confidence score itself (str-qwua7.61).
- Suppressing invariants that only restate a class precondition (for example `input == 0` inside the "zero" class). File separately if wanted; it depends on spec-preconditions-from-path-constraints.
- Fixing the blank-subject `description` field in JSON (consumers should read `label`).
- Changing invariant detection.

## Related

str-qwua7.61 (confidence score removal), str-qwua7.47. Source finding: artifacts-05 (confirmed; P1 lowered to P2 by the verifier).
