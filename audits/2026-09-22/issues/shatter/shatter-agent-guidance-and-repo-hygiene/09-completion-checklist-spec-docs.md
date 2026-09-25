---
slug: completion-checklist-spec-docs
kind: new
title: "Require SPEC section + changelog + Last-updated updates for CLI-visible changes (checked semantically by /pre-completion), and run stories-impact-check before behavioural edits"
priority: P2
type: task
labels: [agents, docs, skills, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Require SPEC section + changelog + Last-updated updates for CLI-visible changes (checked semantically by /pre-completion), and run stories-impact-check before behavioural edits

## Problem

CLI-visible changes land without SPEC, QUICKSTART or changelog updates because
no agent checklist asks for them. SPEC.md itself asks for a changelog row per
CLI-visible change, but that rule lives inside SPEC, which the landing flow
never reads. This audit found the results:

- changelog rows claiming section updates that were never made;
- missing rows for the 2026-09-14 and 2026-09-19 CLI changes;
- `--failure-threshold` still documented after it was removed;
- a stale "Last updated" header.

A check that only asks "did `SPEC.md` change?" would not have caught the
first item (a row was added, the section was not), and a check that only
watches `args.rs` misses exit-code and output-shape changes made in
`main.rs`, `helpers.rs`, the renderers or `commands/`.

Separately, `storystore:stories-impact-check` is a hard-trigger skill that
must run **before** behavioural edits to user-facing surfaces. Listing it
only in a completion checklist would surface protected intent after the
change is already written.

## Evidence

Re-verified 2026-09-23 in `/home/ketan/.local/share/worktrees/shatter/audit-2026-09-22`:

- `grep -nE 'SPEC|changelog|QUICKSTART|stories' CLAUDE.md .claude/skills/pre-completion/SKILL.md`
  -> no matches.
- The Completion Checklist is `CLAUDE.md:43-53` (items 1-7, all test or
  parity gates). The only doc rule is `CLAUDE.md:80`: "Update README.md when
  build/run/config procedures change."
- `SPEC.md:7` holds the rule: "Any CLI-visible change (new command,
  new/renamed/removed flag, changed default, changed output shape) should add
  a row to the changelog". The changelog is `SPEC.md:1173` (`## 8. Changelog`,
  a `| Date | Change | Section |` table, newest row first), and CLI commands
  are `## 2.` at `:68` with `### 2.N` subsections.
- `SPEC.md:3` "Last updated: 2026-09-09", but
  `git log --format='%h %ad' --date=short -- SPEC.md` shows edits on
  2026-09-14 (21981b1d) and 2026-09-19 (2de05fd9).
- CLI-visible code lives in more than `args.rs`: exit codes are set in
  `shatter-cli/src/main.rs`, `shatter-cli/src/helpers.rs` and
  `shatter-cli/src/commands/build_frontend.rs` (`process::exit` /
  `ExitCode`); output is rendered in `shatter-cli/src/render.rs`,
  `shatter-cli/src/commands/*.rs`, `shatter-core/src/report.rs` and
  `shatter-core/src/reporter.rs`.
- `storystore:stories-impact-check` is not referenced in CLAUDE.md, AGENTS.md
  or `.claude/`. Its skill description makes it a hard trigger "before any
  behavioral change to user-facing surfaces", and it keys on
  `docs/stories/INDEX.md`, which does not exist yet (adoption is
  str-qwua7.52).
- `/pre-completion` already emits a summary table
  (`.claude/skills/pre-completion/SKILL.md:110-129`).
- Audit source: docs-10 (`audits/2026-09-22/findings.json`,
  `audits/2026-09-22/areas/docs.md`).

## Acceptance criteria

1. **Pre-edit rule.** CLAUDE.md's Agent Workflow section states: before
   editing a user-facing CLI surface (the path list in item 3), if
   `docs/stories/INDEX.md` exists, run `storystore:stories-impact-check` for
   the planned change and resolve any locked/accepted-story conflict before
   editing. The completion step (item 4) verifies it happened; it does not
   replace it.
2. **Completion checklist item 8** in CLAUDE.md: "CLI-visible change → the
   matching SPEC §2 subsection, a §8 changelog row naming that subsection, a
   `Last updated` bump, QUICKSTART/README if first-run behaviour changes.
   Checked by `/pre-completion` (Spec docs row)." The path list lives only in
   the skill; CLAUDE.md points to it.
3. **Trigger paths** are listed once in `.claude/skills/pre-completion/SKILL.md`
   and include at least: `shatter-cli/src/args.rs`, `shatter-cli/src/main.rs`,
   `shatter-cli/src/helpers.rs`, `shatter-cli/src/render.rs`,
   `shatter-cli/src/commands/**`, `shatter-core/src/report.rs`,
   `shatter-core/src/reporter.rs`. Changes limited to `#[cfg(test)]` modules
   or test files do not trigger.
4. **Semantic Spec-docs check** in `/pre-completion` (a script, for example
   `scripts/spec-docs-check.py`, invoked by the skill and adding one "Spec
   docs" row to its table). When the branch diff against `origin/main`
   touches a trigger path, it reports PASS only if the `SPEC.md` diff:
   (a) adds at least one row to the `## 8. Changelog` table dated on or after
   the branch's first commit; (b) changes the `Last updated:` line to that
   row's date or later; and (c) has at least one hunk inside each `§2.N`
   subsection that the new row's Section column names. It also reports the
   stories-impact-check result recorded for the branch (item 1) or `N/A`
   when `docs/stories/INDEX.md` does not exist. Otherwise it reports FAIL,
   naming which of (a)-(c) is missing.
5. **Waiver** is machine-readable: a `Spec-Waiver: <reason>` trailer in any
   commit on the branch turns FAIL into `WAIVED (<reason>)`. No free-text
   waiver in a completion message counts.
6. **Unit tests** for the script (wired into `task meta` `cmds:` and
   `sources:`) cover: trigger path touched with no SPEC change -> FAIL;
   changelog row added but no §2 hunk -> FAIL (c); row and §2 hunk but stale
   `Last updated` -> FAIL (b); all three -> PASS; `main.rs` exit-code change
   alone -> FAIL; test-only change -> not triggered; `Spec-Waiver` trailer ->
   WAIVED.
7. **Demonstration in the close reason** on a scratch branch: a help-text
   change in `args.rs` with no SPEC edit -> FAIL; adding only a changelog row
   -> still FAIL; adding the §2 edit and the `Last updated` bump -> PASS.
8. `task affected` passes, with `Gates selected` recorded.

## Suggested approach

Parse `git diff -U0 origin/main...HEAD -- SPEC.md` hunks against the section
line ranges of the base and head `SPEC.md`. Keep the check a small,
dependency-free Python script so `task meta` can test it.

## Out of scope

- The mechanical CLI-surface drift gate that compares SPEC flag tables with
  clap (str-wurp).
- Fixing the existing SPEC drift (other audit docs issues).
- Adopting storystore (str-qwua7.52).

## Dependencies

None. Related: str-wurp, str-u394l.4, str-qwua7.2, str-qwua7.52.
