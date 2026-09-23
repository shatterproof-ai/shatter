---
slug: revalidate-reopen-note
kind: reopen-note
title: "Reopen-note on closed str-kab3 (and str-3lob): revalidate ignores return values; see revalidate-return-values"
priority: P1
type: note
labels: [audit-2026-09-22]
parent_epic: ""
blocked_by: [revalidate-return-values]
existing_id: str-kab3
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Reopen-note on closed str-kab3 (and str-3lob): revalidate ignores return values; see revalidate-return-values

## Tracker action

- Add the comment below to **str-kab3** (`bd comments add str-kab3 …`), and the same comment to **str-3lob**.
- Do not reopen either issue. The fix is tracked in the new issue.
- Filing order: file revalidate-return-values first, then replace `<revalidate-return-values id>` with its real id. (`blocked_by` in the front matter records only this ordering.)

## Comment text

> Audit 2026-09-22 (finding goals-02, verified P1): the revalidation shipped here does not detect output regressions.
>
> - `shatter-core/src/revalidation.rs:87-120` `classify_verdict` takes only `code_changed`, `path_matches` and error severity. The return value and the thrown error value are never compared.
> - A path mismatch under a code change maps to `ExpectedDrift`. `shatter-cli/src/commands/revalidate.rs:143-147` and `:180-186` treat `ExpectedDrift` as confirmed for both the exit code and the "N/M behaviors confirmed" line.
> - Repro: explore `01-arithmetic.ts`, change `return "zero"` to `return "nil"`, run `shatter revalidate 01-arithmetic.ts`. The output is `6/6 behaviors confirmed.` and the exit code is 0. `--json` reports verdict `confirmed` for input `[0]`. `spec-diff` on the same change reports `[CHANGED] zero -> nil`.
>
> This contradicts SPEC §2.7 ("compare observed against cached behavior. Exit 0 = no regressions"). The unit tests cover only the verdict matrix on synthetic path/severity inputs.
>
> Not reopening: the fix is tracked in **<revalidate-return-values id>** ("`shatter revalidate` ignores return values: changed outputs on replayed inputs are reported as confirmed and exit 0"). That issue adds output and error comparison under the existing nondeterminism mask, stops counting ExpectedDrift as a pass, and adds an E2E known-answer test.
