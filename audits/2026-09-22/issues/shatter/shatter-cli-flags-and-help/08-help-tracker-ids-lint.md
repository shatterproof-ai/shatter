---
slug: help-tracker-ids-lint
kind: new
title: "Remove tracker IDs and internal status notes from --help, SPEC and README; add a lint"
priority: P2
type: task
labels: [cli, docs, usability, agents, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Remove tracker IDs and internal status notes from --help, SPEC and README; add a lint

## Problem

User-facing text cites internal tracker IDs and process status. `///` doc comments on clap args double as internal notes, so IDs such as `str-gg9v` and `str-frc.6` appear in `--help`. The 2026-09-04 audit flagged this, and a new ID (`str-0m0vn`, scan `--seed`) was added on 2026-09-05, the day after. SPEC and README carry the same pattern, including one paragraph of internal status ("Do not wait for that issue to land ...") inside the exit-code contract. Nothing lints for it. str-qwua7.15 explicitly left "tracker-ID removal from help text" out of scope, and it was never filed.

The other unfiled 2026-09-04 UI items are handled separately by unfiled-0904-ui-items-reconcile.

## Evidence

Re-verified 2026-09-23 with `target/debug/shatter` built at `56c86168` of `audit-2026-09-22`:

- Rendering `--help` for every top-level subcommand and running `grep -o 'str-[a-z0-9.]*[a-z0-9]' | sort | uniq -c` finds **13 distinct IDs**: str-gg9v (19 times), str-jeen.13 (2), str-v01r (2), str-0m0vn, str-1fwt, str-1wcl, str-d6hj, str-frc.3, str-frc.5, str-frc.6, str-izhn, str-o09e and str-p2rz. The verifier counted 11-13 depending on whether nested pages and dotted IDs are counted separately.
- Sources in `shatter-cli/src/args.rs` (`///` comments): lines 106, 165, 796, 801, 811, 818, 833, 977 (str-0m0vn), 1014, 1144, 1148, 1168, 1589, 1773, 1775, 2200, 2225, 2587, 2608. `args.rs` also has IDs in `//` comments (for example 283, 321, 3491), which are fine.
- `SPEC.md` has 10 lines containing `str-`. `SPEC.md:645-647` (§2.11): "str-qwua7.12 tracks bringing the remaining commands into line. Do not wait for that issue to land before relying on this table." SPEC §8 changelog starts at `SPEC.md:1173`.
- `README.md:293`: `> **Breaking change (str-gg9v).** ...`
- `.claude/skills/rust-conventions/SKILL.md` has no rule on tracker IDs in user-facing strings.
- str-qwua7.45 covers only README's "Executing Target Functions Safely" section.
- Findings cli-ux-17 (P2) and docs-18 (P3), merged here at P2 (report section 15.1).

## Acceptance criteria

- [ ] A unit test in `shatter-cli` walks `Cli::command()` recursively, renders every subcommand's long help (`render_long_help`), and fails on `/str-[a-z0-9]+/`. It fails on current HEAD and passes after the fix; the close comment records both runs (with the failing ID list from the first run).
- [ ] Every tracker ID now in `///` on clap args is moved to a `//` code comment next to the arg, or dropped. No help text loses user-relevant meaning. For example, the `str-gg9v` sentences become a plain description of the host-write default.
- [ ] `SPEC.md` and `README.md` user-facing sections contain no tracker IDs outside the SPEC §8 changelog. Where timing matters, use dated notes ("since 2026-07"). A check run by an existing gate (a script called from `check-static`, or a Rust test that reads the files) greps these files, excluding the changelog, and fails on `str-` IDs. It is shown failing on current HEAD.
- [ ] The internal-status paragraph in SPEC §2.11 (`SPEC.md:645-647`) is removed, or rewritten as a short "Known deviations" list that states each deviation, with no tracker references.
- [ ] `.claude/skills/rust-conventions/SKILL.md` gains a rule: no tracker IDs in `///` docs on clap args or in any user-facing string; put them in `//` comments.
- [ ] `task affected` passes, and the close comment records the gates selected.

## Suggested approach

Write the help-rendering test first to get the inventory, then rewrite each `///` line.

## Out of scope

- Reconciling the unfiled 2026-09-04 UI items (unfiled-0904-ui-items-reconcile).
- Help-heading grouping and flag vocabulary (str-9ee5).
- Hiding execution-only flags (help-hides-execution-flags).
- The README "Executing Target Functions Safely" wording already owned by str-qwua7.45. Coordinate so both do not rewrite the same lines.
- The status of str-qwua7.12 itself (exit-codes-qwua7-12-note).

## Dependencies

- Blocked by: none.
- Related: str-qwua7.15, str-qwua7.45, str-9ee5, help-hides-execution-flags, unfiled-0904-ui-items-reconcile.

## Source

Audit 2026-09-22 findings cli-ux-17 (areas/cli-ux.md F17) and docs-18 (areas/docs.md). Merges the old drafts `drafts/shatter-agent/28-help-tracker-ids-lint-and-unfiled-ui-items.md` (primary; its reconciliation half is now unfiled-0904-ui-items-reconcile) and `drafts/shatter-docs-ui/08-tracker-ids-in-help-and-docs.md`.
