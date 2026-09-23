---
slug: revalidate-return-values
kind: new
title: "`shatter revalidate` ignores return values: changed outputs on replayed inputs are reported as confirmed and exit 0"
priority: P1
type: bug
labels: [regression, revalidate, behavior-map, cli, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# `shatter revalidate` ignores return values: changed outputs on replayed inputs are reported as confirmed and exit 0

## Problem

SPEC §2.7 (SPEC.md:459-465) says `shatter revalidate <SOURCE>` replays each cached input "and compare[s] observed against cached behavior. Exit `0` = no regressions, `1` = issues found". The verdict does not use the return value or the thrown error value. It uses only whether the branch path matched and the error *severity*. When the code has changed, a path mismatch is classified `ExpectedDrift`, and that verdict counts as confirmed for both the summary and the exit code. A function whose output changed on the same input therefore passes revalidation.

`spec-diff` on the same change does report `[CHANGED] zero -> nil`, so Shatter's two regression paths disagree. Revalidation was built and closed in str-kab3 (verdict loop) and str-3lob (CLI and CI integration) under epic str-76z6, and neither checked outputs.

## Evidence

Line numbers re-checked against `56c86168` (branch `audit-2026-09-22`):

- `shatter-core/src/revalidation.rs:87-120` `classify_verdict(code_changed, path_matches, expected_severity, observed_severity)`. No return value or error value is passed in. Same severity with a path match gives `Confirmed`. Same severity with a path mismatch and `code_changed` gives `ExpectedDrift`.
- `shatter-cli/src/commands/revalidate.rs:143-147`: `has_issues` is set only for verdicts other than `Confirmed` and `ExpectedDrift`, so drift exits 0.
- `revalidate.rs:180-186`: the "N/M behaviors confirmed" count includes `ExpectedDrift`.
- The nondeterminism mask already exists for paths (`revalidation.rs:127-160`, `NondeterministicField` from the behavior map). It is not applied to outputs because outputs are not compared.
- Repro 1 (audit verifier): explore `01-arithmetic.ts`, change `return "zero"` to `return "nil"`, run `shatter revalidate 01-arithmetic.ts`. Every behavior prints `[ok] ... (confirmed)`, the summary is `6/6 behaviors confirmed.`, and the exit code is 0. `--json` shows verdict `confirmed` for input `[0]`, whose return changed from `zero` to `nil`.
- Repro 2 (`audits/2026-09-22/goals-runs/regress2/`): the same change plus swapped even/odd branches printed `[drift] classifyNumber (expected drift)` twice, then `4/4 behaviors confirmed.`, and exit 0.
- Unit tests exercise only the verdict matrix on synthetic severity/path inputs.

## Acceptance criteria

- [ ] The return value and the thrown error value are part of the verdict. The comparison uses the behavior map's nondeterministic-field mask for output fields. On a replayed input, an output change is a regression (a new verdict such as `OutputChanged`, counted as an issue) whether or not the fingerprint changed.
- [ ] `ExpectedDrift` no longer counts as confirmed in the "N/M confirmed" line. It fails the exit code by default. If a way to accept drift is needed, add an explicit `--allow-drift` flag, document it in SPEC §2.7, and default it to off.
- [ ] Unit tests: `classify_verdict` (or its replacement) with a changed output and a matching path returns a regression. The same with the output difference masked as nondeterministic returns confirmed.
- [ ] E2E known-answer test: explore a TS fixture, mutate one return value, run `shatter revalidate`. It exits 1 and names the function and the input. At close, show the test failing on current `main` and passing after the fix (test name plus before/after output in the close note).
- [ ] SPEC §2.7 states what is compared (path, severity, output, error) and which verdicts fail the exit code.
- [ ] `task affected` passes, and its `Gates selected` output is recorded.

## Suggested approach

Carry the expected `return_value`/`thrown_error` from each `Behavior` into `revalidate_behaviors` and compare them with the observed execute response after masking. Reuse the `NondeterministicField` paths that str-kab3 introduced (prefix `return`/`error` instead of `branch`). Keep `classify_verdict` a pure function by adding an `output_matches: bool` parameter, which keeps the existing matrix tests easy to extend.

## Out of scope

- Behavior maps written with empty `behaviors` for some functions (a symptom of the path under-count; see float-probe-paths-uncounted). Revalidate can only check the behaviors a map contains. The E2E fixture here must use a function whose map is populated.
- Behavior-map cache keying across files (behavior-map-cache-keys). That issue also adds the rule that revalidate refuses a map from a different source file.
- spec-diff false negatives (str-qwua7.38).

## Priority

P1: the command documented to catch regressions reports a changed output as confirmed and exits 0.

## Type

bug

## Dependencies

- Blocked by: none.
- Related: str-kab3 and str-3lob (closed; built revalidation without output comparison; reopen-note revalidate-reopen-note points here), str-76z6 (closed epic), float-probe-paths-uncounted, behavior-map-cache-keys.

## References

Audit 2026-09-22 finding goals-02 (verified P1). Source draft: `drafts/shatter-code/39-revalidate-ignores-return-values.md`.
