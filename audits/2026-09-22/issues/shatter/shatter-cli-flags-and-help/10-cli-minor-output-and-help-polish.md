---
slug: cli-minor-output-and-help-polish
kind: new
title: "Help text is wrong: --dry-run says 'Requires --output' (it does not); target help omits .rs = Rust"
priority: P3
type: bug
labels: [cli, help, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Help text is wrong: --dry-run says 'Requires --output' (it does not); target help omits .rs = Rust

## Problem

Two explore help strings contradict actual behavior:

1. **`--dry-run`** help says "Requires --output", but `explore ... --dry-run` without `-o` works and exits 0.
2. **The target argument** help says "(.ts = TypeScript, .go = Go)". `.rs` (Rust) targets are supported.

This draft previously bundled ten other CLI output defects. They have separate implementations and completion conditions, so they are now separate drafts in this bucket: analyze-only-sandbox-refusal, analyze-only-output-detail, explore-function-not-found-diagnostics, spec-flag-dropped-with-spec-out, html-source-non-executable-lines, failure-table-language-any, demo-complete-with-errors-green, and exit-codes-qwua7-12-note (the `print_stdout` exit code and the str-qwua7.12 status).

## Evidence

Re-verified against `audit-2026-09-22` (source at `56c86168`). Transcripts are in `audits/2026-09-22/cli-ux-transcripts/`.

- `shatter-cli/src/args.rs:676`: `--dry-run` doc says "Requires --output". `err-dryrun.out`: `explore ... --dry-run` with no `-o` exits 0.
- `shatter-cli/src/args.rs:501`: target help "(.ts = TypeScript, .go = Go)".
- Finding artifacts-16 (P3).

## Acceptance criteria

- [ ] The `--dry-run` help string describes actual behavior (no "Requires --output"), and a CLI test asserts `explore <fixture> --dry-run` without `-o` exits 0, so the help cannot drift back unnoticed.
- [ ] The target-argument help lists `.rs = Rust` alongside TS and Go. A test asserts `explore --help` contains `.rs`.
- [ ] Other target-help strings that list languages (scan, run) are grepped and fixed the same way; the close comment lists what was checked.
- [ ] `task affected` passes. The close comment records the gates selected.

## Suggested approach

Edit the `///` doc comments on the two args. If help-tracker-ids-lint lands first, its help-rendering test can host these assertions.

## Out of scope

- All other CLI output defects listed above (their own drafts).
- Help grouping and flag vocabulary (str-9ee5).

## Dependencies

- Blocked by: none.
- Related: help-tracker-ids-lint, str-9ee5.

## Source

Audit 2026-09-22 finding artifacts-16 (items 4-5 of the merged old drafts `drafts/shatter-docs-ui/13-minor-output-defects.md` and `drafts/shatter-docs-ui/14-analyze-only-and-error-help-polish.md`).
