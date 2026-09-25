# Spec markdown renders invariants with blank subjects and duplicate lines ("-  != null [1] (100/100)")

- Priority: P2
- Type: bug
- Labels: spec,report
- Tracker action: new issue (str-qwua7.61 covers only removing the confidence score)
- Related: str-qwua7.61, str-qwua7.47
- Source findings: audit 2026-09-22 artifacts-05 (confirmed; downgraded P1→P2)

<!-- body -->
## Problem
`shatter explore --invariants --spec ts/01-arithmetic.ts:classifyNumber` prints:
```
**Function invariants:**
-  != null [1] (100/100)
-  != null [1] (100/100)
-  is non-empty [1] (100/100)
```
The JSON output has good labels (`input.age is non-null`), and YAML renders `input is non-null`. Only the markdown is broken.

## Current code facts
- `shatter-core/src/spec.rs:566-578` and `:615-627` render `ci.invariant.description` followed by `[{confidence}] (sat/total)`.
- `format_path` (`shatter-core/src/invariants.rs:144-146`) is `path.join(".")`, so a scalar param or return value has an empty subject.
- `ClassifiedInvariant.label` exists (`invariants.rs:638`) but markdown does not use it.
- The unit tests use synthetic descriptions such as `x > 0`.

## Acceptance criteria
- Markdown renders `ClassifiedInvariant.label`. For a scalar-param function, every invariant line names its subject (`input`, `return`).
- Duplicate lines are removed. Invariants that only restate the class precondition (e.g. `input == 0` in the zero class) are suppressed.
- A golden test runs the real detector on `classifyNumber` with `--invariants --spec`.
- The `[1] (n/n)` suffix is handled per str-qwua7.61; coordinate so the two changes do not conflict.
