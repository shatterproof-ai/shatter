# Land the 2026-09-04 audit report on main and make /audit publish before filing

- Priority: P1
- Type: task
- Labels: agents,audit,skills,docs
- Tracker: shatter (bd, /home/ketan/project/shatter)
- Relation: new (partially covered by str-qwua7.22, str-qwua7.44)
- Source findings: agent-repo-04, docs-04
- Parent: 01 (epic)
- Blocked by: none
- Readiness: drafted to the issue-readiness-check standard; fresh-reviewer precheck still required before filing (see INDEX.md)

<!-- body -->
## Problem
The 2026-09-04 audit report and evidence live only on local branch
`audit-2026-09-04`, which was never pushed or merged. 62 str-qwua7 children
(68 open issues overall) cite `audits/2026-09-04...` paths that do not exist
on origin/main, so any fresh agent following those issues cannot read the
evidence. The 2026-07-10 audit was lost the same way (str-sff87). The repo
`/audit` skill ends at "commit on main", which the require-worktree hook
blocks, and has no landing step.

## Current Code Facts
- `git rev-list --left-right --count main...audit-2026-09-04` -> `102 7`;
  `git ls-remote --heads origin audit-2026-09-04` -> empty.
- `git ls-tree main -- audits` -> only `2026-02-28.md`, `2026-05-21.md`,
  `kapow-2026-05-21`.
- `.claude/skills/audit/SKILL.md:380-387` (post-audit step 6) commits on main
  and relies on `bd sync` (removed in bd 1.1.0; see draft 05).
- str-qwua7.22 (open) plans a Phase-10 rewrite; str-qwua7.44 (open) plans to
  move `audits/` to `docs/audits/`. Neither lands the existing 09-04 report.
- `docs/INDEX.md` has no Audits section.

## Acceptance Criteria
- The 2026-09-04 report and evidence are on origin/main (at the path chosen
  by str-qwua7.44, or `audits/2026-09-04/` if .44 has not landed), via
  launch-work/land-work. `git cat-file -e origin/main:<path>` succeeds for
  every evidence path cited by an open str-qwua7 child (script the check).
- `.claude/skills/audit/SKILL.md` post-audit steps: create audit issue →
  launch-work worktree → write report there → land report → then file issues;
  issue evidence paths must exist on origin/main (the skill states the check
  command).
- `docs/INDEX.md` has an "Audits" section linking landed reports.
- This 2026-09-22 audit is landed the same way before its issues are filed.

## Suggested Approach
Coordinate with str-qwua7.22/.44: either fold this into .22 (note to append)
or land it first and mark .22's Phase-10 part done.

## Out of Scope
The drift-patrol burn-down check for audit epics (draft 11).
