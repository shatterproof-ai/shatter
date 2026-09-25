---
repo: bento
tracker: beads
type: feature
priority: 2
labels: hygiene
existing: none
---
# require-worktree hook: block branch-mutating git in the primary checkout, so policy does not depend on the permission allowlist

## Decision (2026-09-06)
The shatter maintainer keeps the global Bash allowlist (which auto-approves `git checkout/merge/rebase/reset/clean`) as friction control and wants the "never mutate the primary branch outside land-work" doctrine enforced mechanically instead. The allowlist is explicitly not policy.

## Problem
The only mechanical guardrail today is the PreToolUse require-worktree hook, which blocks Edit/Write/NotebookEdit on the primary branch. Git mutations are not covered: an agent in the primary checkout can `git merge`, `git rebase`, `git reset` or `git checkout main` with auto-approval. Shatter's transcripts show 100 `--no-verify` and 23 `core.hooksPath=/dev/null` uses in 14 sessions, six sessions hitting "must be run in a work tree" after the primary was left with `core.bare=true`, and edits into primary-checkout paths from a recovery session.

## Current code facts
- `~/project/bento/catalog/hooks/bento/claude/scripts/require-worktree.sh` (registered by `register-require-worktree-hook` at SessionStart) matches only the Edit/Write/NotebookEdit tools; opt-out is `require_worktree=false` in `.agent-mode.local`.
- The Bash PreToolUse path runs `auto-allow.py`; there is no bento hook that inspects git subcommands.
- land-work's own scripts run merge/push from the preview worktree, so a rule keyed on "cwd is the primary checkout and command mutates a branch" would not affect them.

## Acceptance checks
- A PreToolUse Bash hook (or an extension of require-worktree) denies `git merge|rebase|reset|clean|checkout <primary>|branch -D <primary>|push --force*` when cwd resolves to the primary checkout and the command is not invoked by a land-work/launch-work script (env marker), printing the doctrine line and the land-work pointer.
- `--no-verify` and `core.hooksPath=` in a git command are denied with a message pointing at the hook-timeout fix, unless `hook_bypass=allow` is set in `.agent-mode.local`.
- Opt-out and allow-marker documented in the bento README; tests for allow, deny, and the land-work marker path.

## Scope
In: the hook, registration, docs, tests. Out: changing any consumer's settings.json allowlist.

## Size
small

## Provenance
Shatter audit 2026-09-04 (audits/2026-09-04/agent-system.md contradictions 8–9, session-retro.md §3 anti-pattern 2; decision recorded 2026-09-06).
