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
- The nondeterminism mask already exists for paths (`branch_paths_match`, `revalidation.rs:127-154`), which masks on `field_path == "branch"` or a `branch.` prefix. It is not applied to outputs because outputs are not compared.
- The mask vocabulary for outputs is fixed by the producer, `detect_within_run_nondeterminism` (`shatter-core/src/nondeterminism.rs:486-600`): `return` (the whole primitive return value), `return.<path>` (a nested field of the return value), `thrown_error` (`FIELD_PATH_THROWN_ERROR`, :490; set when the error's `error_type` or `message` varied) and `<outcome>` (`FIELD_PATH_OUTCOME`, :487; set when one execution returned and another threw). Error comparison there uses only `error_type` and `message`, never stack traces or locations.
- Repro 1 (audit verifier): explore `01-arithmetic.ts`, change `return "zero"` to `return "nil"`, run `shatter revalidate 01-arithmetic.ts`. Every behavior prints `[ok] ... (confirmed)`, the summary is `6/6 behaviors confirmed.`, and the exit code is 0. `--output-format json` shows verdict `confirmed` for input `[0]`, whose return changed from `zero` to `nil`.
- Repro 2 (`audits/2026-09-22/goals-runs/regress2/`): the same change plus swapped even/odd branches printed `[drift] classifyNumber (expected drift)` twice, then `4/4 behaviors confirmed.`, and exit 0.
- Unit tests exercise only the verdict matrix on synthetic severity/path inputs.

## Acceptance criteria

- [ ] The return value and the thrown error are part of the verdict. On a replayed input, an output change is a regression (a new verdict such as `OutputChanged`, counted as an issue for the exit code and not as confirmed) whether or not the fingerprint changed.
- [ ] Output comparison uses the existing mask vocabulary exactly as `nondeterminism.rs` writes it: a `return` field masks the whole return value; `return.<path>` masks that nested path (compared with the same path syntax `structural_similarity` produces); `thrown_error` masks the error's type and message; `<outcome>` masks a return-vs-throw flip. No new prefix (such as `error`) is introduced. Errors are compared by `error_type` and `message` only; stack traces, file paths and line numbers are never compared.
- [ ] `ExpectedDrift` is reported separately from confirmed: the summary line reads like `N confirmed, D expected drift, R regressed of M`, and JSON output carries the counts separately. Whether `ExpectedDrift` alone fails the exit code is **not** changed by this issue (it stays exit 0 when outputs match); once outputs are compared, a drifted path with a changed output is already caught as `OutputChanged`. Any later change to the drift exit policy needs a maintainer decision.
- [ ] Unit tests:
  - a changed return value with a matching path returns the regression verdict;
  - the same with a `return` mask (primitive) and with a `return.<field>` mask on the differing field returns confirmed; a `return.<other_field>` mask does not hide a change in a different field;
  - a changed error message with a `thrown_error` mask returns confirmed; without it, a regression;
  - two errors with equal `error_type`/`message` but different stack text are equal;
  - a behavior map persisted by current code (with its real `nondeterministic_fields`) round-trips through the comparison, so a mask written by the producer is honoured by the consumer.
- [ ] E2E known-answer test: explore a TS fixture, mutate one return value, run `shatter revalidate`. It exits 1 and names the function and the input. At close, show the test failing on current `main` and passing after the fix (test name plus before/after output in the close note).
- [ ] SPEC §2.7 states what is compared (path, severity, return value, error type and message), how nondeterministic masks apply, how drift is reported, and which verdicts fail the exit code.
- [ ] `task affected` passes, and its `Gates selected` output is recorded.

## Suggested approach

Carry the expected `return_value`/`thrown_error` from each `Behavior` into `revalidate_behaviors` and compare them with the observed execute response after masking. Reuse the `NondeterministicField` paths exactly as `detect_within_run_nondeterminism` emits them (`return`, `return.<path>`, `thrown_error`, `<outcome>`); put the output-vs-mask comparison next to that producer in `nondeterminism.rs` so the two cannot drift apart. Keep `classify_verdict` a pure function by adding an `output_matches: bool` parameter, which keeps the existing matrix tests easy to extend.

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
