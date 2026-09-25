# Lint tracker IDs out of CLI help and file the unfiled 2026-09-04 UI findings

- Priority: P2
- Type: task
- Labels: agents,cli,usability,audit
- Tracker: shatter (bd, /home/ketan/project/shatter)
- Relation: new (related str-9ee5, str-qwua7.15, str-qwua7.22)
- Source findings: cli-ux-17
- Parent: 01 (epic)
- Blocked by: none
- Readiness: drafted to the issue-readiness-check standard; fresh-reviewer precheck still required before filing (see INDEX.md)

<!-- body -->
## Problem
Tracker IDs keep appearing in user-facing `--help` text, including one added
after the 2026-09-04 audit flagged the pattern. That audit's filing step also
dropped several UI findings, which were never filed.

## Current Code Facts
- Rendering every subcommand's `--help` finds 11 distinct `str-*` IDs
  (13 including nested pages); `str-gg9v` appears ~19 times.
  Sites include `shatter-cli/src/args.rs:106,165,796-833,977,1148`;
  `str-0m0vn` (scan `--seed`) was added 2026-09-05.
- str-qwua7.15 explicitly left "tracker-ID removal from help text" out of scope;
  it was never filed.
- 2026-09-04 usability-ui.md items 9, 12 (termination reason), 16 (run report
  header/metrics), 17, 18 (`--format text` not stripped) and 20 have no issues;
  `bd search` for "tracker id", "termination", "Scan Results", "format text"
  finds nothing.
- `.claude/skills/rust-conventions/SKILL.md` has no rule on tracker IDs in
  user-facing strings.

## Acceptance Criteria
- A unit test in shatter-cli renders every subcommand's long help (clap
  `Command::render_long_help` over `Cli::command()` recursively) and fails on
  `/str-[a-z0-9.]+/`; existing IDs moved to code comments.
- rust-conventions rule: no tracker IDs in `///` docs on clap args or in
  user-facing strings.
- The unfiled 09-04 UI items filed (deduplicated against str-9ee5 and this
  audit's CLI findings), IDs listed in this issue's close reason.

## Out of Scope
Help-heading grouping (str-9ee5).
