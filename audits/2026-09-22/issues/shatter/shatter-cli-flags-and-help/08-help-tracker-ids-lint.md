---
slug: help-tracker-ids-lint
kind: new
title: "Remove tracker IDs and internal status notes from --help, SPEC and README; add a lint; file the unfiled 2026-09-04 UI items"
priority: P2
type: task
labels: [cli, docs, usability, agents, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Remove tracker IDs and internal status notes from --help, SPEC and README; add a lint; file the unfiled 2026-09-04 UI items

## Problem

User-facing text cites internal tracker IDs and process status. `///` doc comments on clap args double as internal notes, so IDs such as `str-gg9v` and `str-frc.6` appear in `--help`. The 2026-09-04 audit flagged this, and a new ID (`str-0m0vn`, scan `--seed`) was added on 2026-09-05, the day after. SPEC and README carry the same pattern, including one paragraph of internal status ("Do not wait for that issue to land ...") inside the exit-code contract. Nothing lints for it.

Separately, the 2026-09-04 audit's filing step dropped several UI findings, which were never filed. str-qwua7.15 also explicitly left "tracker-ID removal from help text" out of scope, and that item was never filed either.

## Evidence

Re-verified 2026-09-23 with `target/debug/shatter` built at HEAD `56c86168` of `audit-2026-09-22`:

- Rendering `--help` for every top-level subcommand and running `grep -o 'str-[a-z0-9.]*[a-z0-9]' | sort | uniq -c` finds **13 distinct IDs**: str-gg9v (19 times), str-jeen.13 (2), str-v01r (2), str-0m0vn, str-1fwt, str-1wcl, str-d6hj, str-frc.3, str-frc.5, str-frc.6, str-izhn, str-o09e and str-p2rz. The verifier counted 11-13 depending on whether nested pages and dotted IDs are counted separately.
- Sources in `shatter-cli/src/args.rs` (`///` comments): lines 106, 165, 796, 801, 811, 818, 833, 977 (str-0m0vn), 1014, 1144, 1148, 1168, 1589, 1773, 1775, 2200, 2225, 2587, 2608. `args.rs` also has IDs in `//` comments (for example 283, 321, 3491), which are fine.
- `SPEC.md` has 10 lines containing `str-`. `SPEC.md:645-647` (§2.11): "str-qwua7.12 tracks bringing the remaining commands into line. Do not wait for that issue to land before relying on this table."
- `README.md:293`: `> **Breaking change (str-gg9v).** ...`
- `.claude/skills/rust-conventions/SKILL.md` has no rule on tracker IDs in user-facing strings.
- Unfiled 2026-09-04 usability-ui.md items: 9 (strip tracker IDs, which this issue covers), 12 (surface the termination reason), 16 (scan double report and absolute paths), 17 (`Wrote ... artifact -> <abs path>` at info level), 18 (`--format text` not stripped, now filed as explore-format-flag-ignored) and 20. `bd search` for "tracker id", "termination", "Scan Results", "absolute path", "artifact path" and "format text" returned nothing (areas/cli-ux.md F17; the verifier did not re-run these searches).
- str-qwua7.45 covers only README's "Executing Target Functions Safely" section.
- Findings cli-ux-17 (P2) and docs-18 (P3), merged here at P2 (report section 15.1).

## Acceptance criteria

- [ ] A unit test in `shatter-cli` walks `Cli::command()` recursively, renders every subcommand's long help (`render_long_help`), and fails on `/str-[a-z0-9]+/`. It fails on current HEAD and passes after the fix; record both in the close comment.
- [ ] Every tracker ID now in `///` on clap args is moved to a `//` code comment next to the arg, or dropped. No help text loses user-relevant meaning. For example, the `str-gg9v` sentences become a plain description of the host-write default.
- [ ] `SPEC.md` and `README.md` user-facing sections contain no tracker IDs outside the SPEC §8 changelog. Where timing matters, use dated notes ("since 2026-07"). A check (script or test run by an existing gate) greps these files, excluding the changelog, and fails on `str-` IDs.
- [ ] The internal-status paragraph in SPEC §2.11 (`SPEC.md:645-647`) is removed, or rewritten as a short "Known deviations" list that states each deviation, with no tracker references.
- [ ] `.claude/skills/rust-conventions/SKILL.md` gains a rule: no tracker IDs in `///` docs on clap args or in any user-facing string; put them in `//` comments.
- [ ] The unfiled 2026-09-04 UI items (12, 16, 17, 20, and any other items from that report's usability-ui section that have no issue) are each either filed as a child of the audit epic (deduplicated against str-9ee5 and this audit's CLI issues) or recorded as already covered, with the covering id. The close comment lists every item and its outcome.
- [ ] `task affected` passes, and the close comment records the gates selected.

## Suggested approach

Write the help-rendering test first to get the inventory, then rewrite each `///` line. The SPEC/README check can be a small script called from `check-static`, or a Rust test that reads the files. For the 09-04 items, read the usability/UI section of the 2026-09-04 audit report. It is not in the `audit-2026-09-22` tree, so find it from the str-qwua7 epic description or the `audit-2026-09-04` branch or tag. Search `bd` for each item before filing.

## Out of scope

- Help-heading grouping and flag vocabulary (str-9ee5).
- Hiding execution-only flags (help-hides-execution-flags).
- The README "Executing Target Functions Safely" wording already owned by str-qwua7.45. Coordinate so both do not rewrite the same lines.
- Closing str-qwua7.12 (cli-minor-output-and-help-polish).

## Dependencies

- Blocked by: none.
- Related: str-qwua7.15, str-qwua7.45, str-9ee5, str-qwua7.22 (audit filing process), help-hides-execution-flags, explore-format-flag-ignored.

## Source

Audit 2026-09-22 findings cli-ux-17 (areas/cli-ux.md F17) and docs-18 (areas/docs.md). Merges the old drafts `drafts/shatter-agent/28-help-tracker-ids-lint-and-unfiled-ui-items.md` (primary) and `drafts/shatter-docs-ui/08-tracker-ids-in-help-and-docs.md`.
