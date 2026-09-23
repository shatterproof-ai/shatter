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

1. **Double print.** `explore <file> -o x.json --stdout` streams the full report to stdout while exploring, then replays it after the run. stdout therefore holds every per-function report twice, plus extra `## <fn> / **Status:** completed / exploration completed` blocks between the copies.
2. **Header leak.** `explore <file> -o report.html -o bundle.json -q` with no `--stdout` leaves `# Shatter Explore` and a blank line (19 bytes) on stdout. `-o x.html` without `-q` does the same. stdout should be empty.

Either one breaks piping (`shatter explore ... -o r.json --stdout | tool`) and any script that checks that stdout is empty.

Explore has two replay sites, and they are reached by different paths, not by sequential vs parallel scheduling:

- `explore.rs:4028-4029` is in `finalize_explore` (defined at `:3773`), reached only through `--from-artifacts` (`run_explore` returns into it at `:4244`).
- `explore.rs:6687-6688` is in `run_explore` (`:4116`), the live exploration path. This is the one that produced `explore-o2.out`.

## Evidence

Re-verified against `audit-2026-09-22` (source at `56c86168`).

- The streaming path prints if `should_print_report = opts.report_outputs_empty || opts.stdout` (`shatter-cli/src/commands/explore.rs:3610`), and the `# Shatter Explore` header is emitted before that gate is consulted (`:3607-3661`).
- Replays: `explore.rs:4028-4029` (`// Replay to stdout if report files were also written.`) and `:6687-6688` (`// If files were written and --stdout was also requested, replay to stdout.`).
- Transcripts in `audits/2026-09-22/cli-ux-transcripts/`:
  - `explore-o2.out` (`explore 01-arithmetic.ts -o x.json --stdout`): `## \`classifyNumber\` *(01-arithmetic.ts:10-21)*` appears twice (lines 3 and 33), as do the table rows (`| 1 | \`classifyNumber(0)\` | returns \`"zero"\` |`) and the `compareMagnitudes` heading. `**Summary:**` appears only **once**: the replayed copy has no summary line, so counting summaries does not detect the bug.
  - `explore-o.out` and `explore-o3.out` (`-o` without `--stdout`): exactly `# Shatter Explore\n\n` (19 bytes).
- Verifier (findings.json cli-ux-03) reproduced both and lowered the priority from P1 to P2: the output is duplicated or cosmetic, not incorrect.
- Existing issues cover neighbouring behavior only: str-zt4v (output matrix, closed), str-6c6p (`--quiet` hid reports, closed) and str-xve (stdout/stderr mixing, closed).

## Acceptance criteria

- [ ] CLI output tests in `shatter-cli/tests/` cover every combination of `-o FILE` (absent or present), `--stdout` (absent or present) and `-q` (absent or present) for a live `explore` on a small TS fixture with at least two exported functions (the transcript used `01-arithmetic.ts` from the examples snapshot; `demo/fixtures/arithmetic-v1.ts` has only one function, so add a two-function fixture under `shatter-cli/tests/` if none exists), eight cases. They assert:
  - no `-o`: every per-function heading line (`## \`<fn>\``) and every result-table row occurs **exactly once** on stdout;
  - `-o` without `--stdout`: stdout is empty (0 bytes), with or without `-q`;
  - `-o` with `--stdout`: the same exactly-once predicate holds per heading and per table row, no `**Status:**`/`exploration completed` replay blocks appear, and the file is written;
  - `-q` never suppresses the report when stdout is the sink (keep str-6c6p's behavior).
- [ ] The exactly-once predicate is shown to be discriminating: run against current HEAD, the `-o --stdout` case fails because `## \`classifyNumber\`` occurs twice, and the `-o` without `--stdout` case fails on the 19-byte header. Both pass after the fix. The close comment records both runs (test names and failure messages).
- [ ] The `--from-artifacts` path (`finalize_explore`) is tested separately: first run explore with an artifact dir, then `explore --from-artifacts <dir>` with each of `-o r.md --stdout`, `-o r.md` and no `-o`, asserting the same exactly-once and empty-stdout predicates. If that path is already correct today, the close comment says so and the tests still land as regression guards.
- [ ] `task affected` passes. The close comment records its `Gates selected` line.

## Suggested approach

Pick one stdout emitter per run: either stream when stdout is the sink and skip the replay, or buffer and emit once. Gate header emission on the same `should_print_report` condition as the body. Delete the replay branches or make them the only emitter, and apply the same rule in `finalize_explore`. This is the same code explore-format-flag-ignored changes, so land the two together or one after the other.

## Out of scope

- Which format stdout uses (explore-format-flag-ignored).
- What the JSON bundle written by `-o x.json` contains (explore-o-json-empty-bundle).
- scan and run stdout behavior, unless the same helper is shared (then note it in the close comment).

## Dependencies

- Blocked by: none.
- Related: explore-format-flag-ignored (same emitter code), explore-o-json-empty-bundle, str-zt4v, str-6c6p, str-xve.

## Source

Audit 2026-09-22 finding cli-ux-03 (areas/cli-ux.md F3). No prior draft (report section 15.1).
