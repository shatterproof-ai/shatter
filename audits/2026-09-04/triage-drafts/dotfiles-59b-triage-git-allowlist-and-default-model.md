---
repo: dotfiles
tracker: github
type: task
priority: 3
labels: question
existing: none
---
# Triage: global git-mutation allowlist vs worktree doctrine, and the default model

Triage: maintainer decisions required.

## Decisions
1. **Git mutations in the global allowlist.** `claude/settings.json` L90–104 auto-approves `Bash(git checkout:*)`, `git clean`, `git merge`, `git rebase`, `git reset` (and L188–202 the `rtk git …` twins); `ask` (L~230) only covers `git reset --hard`, `git push --force`, `git filter-branch`. The entire guidance chain (`docs/agent-guidance/branches-and-worktrees.md`) and the bento require-worktree hook exist to keep agents off `main`, yet none of the commands that actually move `main` prompt. Evidence from the shatter repo: a recovery session made 12 edits into the primary checkout, ran 23 `hooksPath=/dev/null` and 100 `--no-verify` git commands, and the primary now has `core.bare=true` — no prompt fired. Options: (a) move the five to `ask` (costs one prompt per rebase/merge in normal landing flows — land-work runs them from a linked worktree, so ~3–5 prompts per landing); (b) keep them allowed and document why, adding a `deny`/`ask` only for `git checkout main`/`git switch main` patterns; (c) leave as is.
2. **Default model.** `claude/settings.json` L82: `"model": "sonnet"` with no host override; `effortLevel: medium`. Swarm leads, land-work and audits therefore run on the smaller model unless a session overrides. Options: (a) keep, and document it in `AGENTS.md` "claude/settings.json" bullet with where per-host overrides go; (b) set the default to the larger model and rely on per-task `/model`; (c) leave undocumented.

## Current code facts
- `~/dotfiles/claude/settings.json` L82 (`model`), L90–104 and L188–202 (allow list), `ask` block; `claude/hosts/*/settings.json` do not override `model` or permissions.
- `~/dotfiles/docs/agent-guidance/branches-and-worktrees.md` states the never-edit-main rule but does not say which git commands prompt.

## Acceptance checks
- One decision recorded per item above (comment on this issue), then either a follow-up PR or closing this issue as "keep as is, documented".
- If (1a) or (1b): settings change rendered; `branches-and-worktrees.md` gains one sentence listing which git mutations prompt.
- If (2a)/(2b): `AGENTS.md` bullet updated.

## Scope
In: the two decisions and their one-line doc consequences.
Out: per-project `.claude/settings.json`; the `bd prime` hook mismatch (separate issue).

## Size
small

## Provenance
Shatter audit 2026-09-04 (shatter repo, branch audit-2026-09-04, audits/2026-09-04.md item 59); evidence summary inline above.
