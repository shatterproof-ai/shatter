---
slug: completion-checklist-spec-docs
kind: new
title: "Completion checklist and /pre-completion must require SPEC/QUICKSTART/changelog updates for CLI-visible changes"
priority: P2
type: task
labels: [agents, docs, skills, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Completion checklist and /pre-completion must require SPEC/QUICKSTART/changelog updates for CLI-visible changes

## Problem

CLI-visible changes land without SPEC, QUICKSTART or changelog updates because
no agent checklist asks for them. SPEC.md itself asks for a changelog row per
CLI-visible change, but that rule lives inside SPEC, which the landing flow
never reads. This audit found the results:

- changelog rows claiming section updates that were never made;
- missing rows for the 2026-09-14 and 2026-09-19 CLI changes;
- `--failure-threshold` still documented after it was removed;
- a stale "Last updated" header.

## Evidence

Re-verified 2026-09-23 in `/home/ketan/.local/share/worktrees/shatter/audit-2026-09-22`:

- `grep -nE 'SPEC|changelog|QUICKSTART|stories' CLAUDE.md .claude/skills/pre-completion/SKILL.md`
  -> no matches.
- The Completion Checklist is `CLAUDE.md:43-53` (items 1-7, all test or
  parity gates). The only doc rule is `CLAUDE.md:80`: "Update README.md when
  build/run/config procedures change."
- `SPEC.md:7` holds the rule: "Any CLI-visible change (new command,
  new/renamed/removed flag, changed default, changed output shape) should add
  a row to the changelog". The changelog is `SPEC.md:1173` (`## 8. Changelog`),
  and CLI commands are `## 2.` at `:68`.
- `SPEC.md:3` "Last updated: 2026-09-09", but
  `git log --format='%h %ad' --date=short -- SPEC.md` shows edits on
  2026-09-14 (21981b1d) and 2026-09-19 (2de05fd9).
- `storystore:stories-impact-check` (a hard-trigger skill) is not referenced
  in CLAUDE.md, AGENTS.md or `.claude/`.
- The flag source of truth is `shatter-cli/src/args.rs`.
- Audit source: docs-10 (`audits/2026-09-22/findings.json`,
  `audits/2026-09-22/areas/docs.md`).

## Acceptance criteria

1. The CLAUDE.md Completion Checklist gains item 8: "**CLI-visible change**
   (`shatter-cli/src/args.rs`, output format, exit codes) → matching SPEC §2
   section + §8 changelog row + `Last updated` bump; QUICKSTART/README if
   first-run behaviour is affected; run `storystore:stories-impact-check` once
   `docs/stories/` exists."
2. `/pre-completion` (`.claude/skills/pre-completion/SKILL.md`) adds a
   diff-based check. If the branch diff against `origin/main` touches
   `shatter-cli/src/args.rs` or the report/output renderers (list the paths in
   the skill) and does not touch `SPEC.md`, the check reports FAIL, unless the
   completion message carries an explicit waiver line with a reason.
3. Demonstration in the close reason: on a scratch branch, a trivial
   `args.rs` help-text change without a SPEC edit makes the check FAIL; adding
   a SPEC changelog row (or a waiver) makes it PASS.
4. `task affected` passes, with `Gates selected` recorded.

## Suggested approach

Keep the check in the skill's existing summary table (one row, "Spec docs"),
so teammates' completion messages carry it automatically. Put the exact
path list in one place, the skill, and have CLAUDE.md point to it.

## Out of scope

- The mechanical CLI-surface drift gate that compares SPEC flag tables with
  clap (str-wurp).
- Fixing the existing SPEC drift (other audit docs issues).

## Dependencies

None. Related: str-wurp, str-u394l.4, str-qwua7.2.
