# Remove tracker IDs and internal status notes from --help, SPEC contracts and README

- Priority: P3
- Type: task
- Labels: cli,docs,ux
- Tracker action: new issue (str-qwua7.45 covers only README's "Executing Target Functions Safely" section)
- Related: str-qwua7.45, str-9ee5, str-qwua7.15
- Source findings: audit 2026-09-22 docs-18 (confirmed). The agent-side lint rule is AGENT finding cli-ux-17.

<!-- body -->
## Problem
User-facing text cites internal tracker IDs and process status. `--help` across all subcommands references 13 distinct `str-*` IDs (e.g. `--observer-pool … See str-frc.3 / str-frc.6`; `str-gg9v` appears about 19 times). SPEC has 10 `str-` references. SPEC §2.11 (`SPEC.md:643-648`) contains "Do not wait for that issue to land before relying on this table". README.md:292 is headed "Breaking change (str-gg9v)".

## Evidence
`for c in $(shatter --help | …subcommands); do shatter $c --help; done | grep -o 'str-[a-z0-9.]*' | sort -u` lists 13 IDs. They come from `///` doc comments on clap args in `shatter-cli/src/args.rs` (e.g. around lines 106, 165, 796-833, 977, 1148).

## Acceptance criteria
- No `str-[a-z0-9]+` string appears in rendered `--help` for any subcommand. A unit test renders every subcommand's long help and asserts this.
- SPEC and README user-facing sections carry no tracker IDs outside the §8 changelog. Where a date matters, use dated notes ("since 2026-07").
- The internal-status paragraph in SPEC §2.11 is removed, or rewritten as a "Known deviations" list.
- Tracker references move to code comments (`//`, not `///`) or the changelog.

## Scope
In: help text, SPEC and README wording, one unit test. Out: help grouping (str-9ee5).
