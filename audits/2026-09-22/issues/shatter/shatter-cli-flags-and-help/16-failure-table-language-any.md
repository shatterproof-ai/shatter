---
slug: failure-table-language-any
kind: new
title: "explore failure-impact table shows Rust (and other unclassified) failures only under language `any`"
priority: P3
type: bug
labels: [cli, explore, report, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# explore failure-impact table shows Rust (and other unclassified) failures only under language `any`

## Problem

The "Failure impact" table in the explore report has a `Lang` column. For a Rust timeout, the only row is `| \`timed_out\` | \`any\` | ...`, which reads as "any language" rather than "Rust". The `any` row is a deliberate cross-language outcome rollup; language-specific rows exist only for Go build failures and TS build/runtime failures. When no language row exists (all Rust failures, all timeouts, Go runtime failures), the table never names the language.

## Evidence

- `audits/2026-09-22/artifact-samples/rust-explore.md:14`: `| \`timed_out\` | \`any\` | 1 | 1 | 13 | 13 | 100.0% |`.
- `shatter-cli/src/commands/explore.rs:3244-3277`: per-language rows only for `(BuildFailed, go)` and `(BuildFailed|RuntimeFailed, ts)`; every failure also adds an `("any", outcome)` rollup row.
- Finding artifacts-16 (verified).

## Acceptance criteria

- [ ] Every failure contributes to a row labelled with its target's real language (`rust`, `go`, `ts`) for its outcome category, in addition to or instead of the rollup. The rollup row, if kept, is labelled unambiguously (for example `all`) and documented.
- [ ] A unit test on the failure-impact builder feeds a Rust timed-out summary and asserts a `rust` / `timed_out` row; it fails on current HEAD. The existing `any`-row test (`explore.rs:~11280`) is updated to the new label.
- [ ] `task affected` passes. The close comment records the gates selected.

## Dependencies

- Blocked by: none.

## Source

Audit 2026-09-22 finding artifacts-16. Split from cli-minor-output-and-help-polish item 8 (evidence corrected: `any` is a rollup row, not a mislabel of the Rust row).
