# Completion checklist and /pre-completion must require SPEC/QUICKSTART/changelog updates for CLI-visible changes

- Priority: P2
- Type: task
- Labels: agents,docs,skills
- Tracker: shatter (bd, /home/ketan/project/shatter)
- Relation: new (related str-wurp, str-u394l.4)
- Source findings: docs-10
- Parent: 01 (epic)
- Blocked by: none
- Readiness: drafted to the issue-readiness-check standard; fresh-reviewer precheck still required before filing (see INDEX.md)

<!-- body -->
## Problem
CLI-visible changes land without SPEC, QUICKSTART or changelog updates because
no agent checklist asks for them. SPEC.md itself asks for a changelog row per
CLI-visible change, but the landing flow never reads SPEC. Result (this audit):
changelog rows claiming section updates that were never made, missing rows for
09-14/09-19 CLI changes, `--failure-threshold` still documented after removal.

## Current Code Facts
- `grep -n 'SPEC.md\|changelog\|QUICKSTART' CLAUDE.md
  .claude/skills/pre-completion/SKILL.md` -> no matches.
- The only doc rule: CLAUDE.md:~80 "Update README.md when build/run/config
  procedures change".
- `SPEC.md:7` states the changelog-row rule; `SPEC.md:3` "Last updated:
  2026-09-09" though SPEC was edited 09-14 and 09-20.
- `storystore:stories-impact-check` (a hard trigger skill) is referenced nowhere
  in CLAUDE.md, AGENTS.md or `.claude/`.

## Acceptance Criteria
- CLAUDE.md Completion Checklist item 8: "CLI-visible change (args.rs, output
  format, exit codes) → matching SPEC §2 section + §8 changelog row +
  QUICKSTART/README if first-run affected; run stories-impact-check once
  docs/stories exists".
- `/pre-completion` checks the diff: if `shatter-cli/src/args.rs` or report
  renderers changed and `SPEC.md` did not, it flags the item (waiver allowed
  with a stated reason).

## Out of Scope
The mechanical CLI-surface drift gate (str-wurp, duplicate-open).
