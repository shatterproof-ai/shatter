---
slug: explore-report-printed-twice
kind: new
title: "explore -o FILE --stdout prints the report twice; -o FILE without --stdout leaks '# Shatter Explore' to stdout"
priority: P2
type: bug
labels: [cli, explore, report, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# explore -o FILE --stdout prints the report twice; -o FILE without --stdout leaks '# Shatter Explore' to stdout

## Problem

The str-zt4v output contract is: with no `-o`, the report goes to stdout. With `-o FILE`, it goes to the file and stdout stays empty, unless `--stdout` is also given, in which case it goes to both exactly once. Explore breaks this in two ways:

1. **Double print.** `explore <file> -o x.json --stdout` streams the full report to stdout while exploring, then replays it after the run. stdout therefore holds the report twice, plus extra `## <fn> / **Status:** completed / exploration completed` blocks between the two copies.
2. **Header leak.** `explore <file> -o report.html -o bundle.json -q` with no `--stdout` leaves `# Shatter Explore` and a blank line (19 bytes) on stdout. `-o x.html` without `-q` does the same. stdout should be empty.

Either one breaks piping (`shatter explore ... -o r.json --stdout | tool`) and any script that checks that stdout is empty.

## Evidence

Re-verified against `audit-2026-09-22` (HEAD `56c86168`).

- The streaming path prints if `should_print_report = opts.report_outputs_empty || opts.stdout` (`shatter-cli/src/commands/explore.rs:3610`), and the `# Shatter Explore` header is emitted before that gate is consulted (`:3607-3661`).
- The replay path prints again: `explore.rs:4028-4029` (`// Replay to stdout if report files were also written.` / `if !report_outputs.is_empty() && stdout`) and the parallel path at `:6687-6688`.
- Transcripts in `audits/2026-09-22/cli-ux-transcripts/`:
  - `explore-o2.out` (`explore 01-arithmetic.ts -o x.json --stdout`): the full streamed report ending `**Summary:** 6 path(s) across 2 function(s)`, then `## classifyNumber` / `**Status:** \`completed\`` / `exploration completed`, then the tables a second time.
  - `explore-o.out` and `explore-o3.out` (`-o` without `--stdout`): exactly `# Shatter Explore\n\n` (19 bytes).
- Verifier (findings.json cli-ux-03) reproduced both and lowered the priority from P1 to P2: the output is duplicated or cosmetic, not incorrect.
- Existing issues cover neighbouring behavior only: str-zt4v (output matrix, closed), str-6c6p (`--quiet` hid reports, closed) and str-xve (stdout/stderr mixing, closed).

## Acceptance criteria

- [ ] CLI output tests in `shatter-cli/tests/` cover every combination of `-o FILE` (absent or present), `--stdout` (absent or present) and `-q` (absent or present) for `explore` on a small TS fixture, eight cases in all. They assert:
  - no `-o`: stdout holds the report exactly once;
  - `-o` without `--stdout`: stdout is empty (0 bytes), with or without `-q`;
  - `-o` with `--stdout`: stdout holds the report exactly once (for example, the `**Summary:**` line appears once), and the file is written;
  - `-q` never suppresses the report when stdout is the sink (keep str-6c6p's behavior).
- [ ] At least the double-print and header-leak cases are shown failing on the current code and passing after the fix. Record both runs in the close comment.
- [ ] The parallel path (`explore.rs:~6687`) and the sequential path (`~4028`) behave the same. The tests exercise both, for example with a multi-function file that triggers the parallel path, or by whatever switch selects each path.
- [ ] `task affected` passes. The close comment records its `Gates selected` line.

## Suggested approach

Pick one stdout emitter per run: either stream when stdout is the sink and skip the replay, or buffer and emit once. Gate header emission on the same `should_print_report` condition as the body. Delete the replay branch, or make it the only branch. This is the same code explore-format-flag-ignored changes, so land the two together or one after the other.

## Out of scope

- Which format stdout uses (explore-format-flag-ignored).
- What the JSON bundle written by `-o x.json` contains (explore-o-json-empty-bundle).
- scan and run stdout behavior, unless the same helper is shared (then note it in the close comment).

## Dependencies

- Blocked by: none.
- Related: explore-format-flag-ignored (same emitter code), explore-o-json-empty-bundle, str-zt4v, str-6c6p, str-xve.

## Source

Audit 2026-09-22 finding cli-ux-03 (areas/cli-ux.md F3). No prior draft (report section 15.1).
