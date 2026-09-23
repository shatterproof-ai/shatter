---
slug: analyze-only-output-detail
kind: new
title: "--analyze-only output shows only counts: no parameter names/types, no branch conditions, ignores --format"
priority: P3
type: feature
labels: [cli, explore, report, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# --analyze-only output shows only counts: no parameter names/types, no branch conditions, ignores --format

## Problem

`explore --analyze-only` prints one line per function, for example `classifyNumber  (arithmetic-v1.ts:11)` / `  params: 1, branches: 3`, or `computeArea (05-unions.ts:17) params: 1, branches: 6`. It shows no parameter names or types and no branch conditions, although the walkthrough promises "types and conditions". Output is un-headed plain text whatever `--format` says. The missing parameter type hid the string-typed discriminant behind goals-07.

## Evidence

- `audits/2026-09-22/cli-ux-transcripts/err-analyze-only.out` and `audits/2026-09-22/artifact-samples/analyze-only-ts.out`.
- findings.json goals-18 (recommendation: "show parameter types in analyze-only output") and artifacts-16.

## Acceptance criteria

- [ ] Diagnosis recorded first, in a comment on this issue before implementation: whether the analyze response already carries parameter names/types and branch condition text for TS, Go and Rust (cite the protocol type and a sample response per frontend). If any frontend lacks them, the implementation follows `protocol/GOVERNANCE.md`, updates `protocol/parity-matrix.yaml` and runs `task parity` and `task conformance`.
- [ ] `--analyze-only` output lists each parameter with its name and type, and each branch with its condition text, rendered in the active `--format` (markdown by default, with a heading; text and html per explore-format-flag-ignored if it has landed).
- [ ] Snapshot tests cover TS and at least one of Go or Rust; they fail on current HEAD (no types in output) and pass after.
- [ ] `task affected` and `task walkthrough` pass (walkthrough output changes). The close comment records the gates selected.

## Out of scope

- The sandbox refusal (analyze-only-sandbox-refusal).

## Dependencies

- Blocked by: none. Simpler after explore-format-flag-ignored.
- Related: explore-format-flag-ignored, analyze-only-sandbox-refusal.

## Source

Audit 2026-09-22 findings goals-18 (output half) and artifacts-16. Split from cli-minor-output-and-help-polish item 2.
