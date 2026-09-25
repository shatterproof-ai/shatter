# Make .claude/ and .codex/ agent config trackable despite the global gitignore

- Priority: P2
- Type: task
- Labels: agents,git,skills
- Tracker: shatter (bd, /home/ketan/project/shatter)
- Relation: new (partially covered by str-qwua7.54, str-qwua7.1)
- Source findings: agent-repo-10
- Parent: 01 (epic)
- Blocked by: none
- Readiness: drafted to the issue-readiness-check standard; fresh-reviewer precheck still required before filing (see INDEX.md)

<!-- body -->
## Problem
The user's global gitignore ignores `.claude/` and `.codex/`. The repo
`.gitignore` has no negations, so any new skill under `.claude/skills/` or the
decided `.codex/AGENTS.md` (str-qwua7.54) would be silently left untracked;
existing tracked files exist only because they were force-added.

## Current Code Facts
- `git check-ignore -v --no-index .claude/skills/newskill/SKILL.md` ->
  `/home/ketan/.config/git/ignore:35:.claude/`; line 36 ignores `.codex/`.
- Repo `.gitignore` lines ~130, ~165 touch these dirs; no `!` negation.
- 17 files under `.claude/` are tracked.
- Orphan `/home/ketan/project/shatter/.claude/worktrees/str-umw3/` (issue
  closed 2026-04-11) still exists.

## Acceptance Criteria
- Repo `.gitignore` negates `/.claude/` and re-ignores
  `/.claude/settings.local.json` and `/.claude/worktrees/`; negates
  `/.codex/AGENTS.md` (and any other intended tracked codex files).
- A meta test (wired into `task meta`) asserts
  `git check-ignore` is false for `.claude/skills/x/SKILL.md` and
  `.codex/AGENTS.md`, and true for `.claude/settings.local.json`.
- The str-umw3 orphan worktree dir is removed (operator-confirmed).

## Out of Scope
Narrowing the global gitignore (dotfiles tracker).
