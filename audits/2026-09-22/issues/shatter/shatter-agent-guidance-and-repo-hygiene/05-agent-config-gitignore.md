---
slug: agent-config-gitignore
kind: new
title: "Make intended .claude/ and .codex/ agent config trackable: repo .gitignore negations plus a check-ignore meta test"
priority: P2
type: task
labels: [agents, git, skills, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Make intended .claude/ and .codex/ agent config trackable: repo .gitignore negations plus a check-ignore meta test

## Problem

New agent config files are silently left untracked:

- The maintainer's global gitignore (`~/.config/git/ignore:35-36`) ignores
  `.claude/` and `.codex/`.
- The repo's own `.gitignore:134` also ignores `.codex/`.

So a new skill under `.claude/skills/`, or the `.codex/AGENTS.md` decided in
str-qwua7.54, would not show up in `git status`. The 17 tracked files under
`.claude/` exist only because someone force-added them. Nothing tests that
agent config the project means to track is actually trackable.

## Evidence

Re-verified 2026-09-23 in `/home/ketan/.local/share/worktrees/shatter/audit-2026-09-22`:

- `git check-ignore -v --no-index .claude/skills/newskill/SKILL.md .codex/AGENTS.md .claude/settings.local.json` ->
  `/home/ketan/.config/git/ignore:35:.claude/  .claude/skills/newskill/SKILL.md`,
  `.gitignore:134:.codex/  .codex/AGENTS.md`,
  `/home/ketan/.config/git/ignore:35:.claude/  .claude/settings.local.json`.
- Repo `.gitignore:130` `.claude/settings.local.json`, `:134` `.codex/`,
  `:165` `.claude/worktrees/`. There is no `!` negation.
- `git ls-files .claude | wc -l` -> 17. `git ls-files .codex | wc -l` -> 0.
  The primary checkout's `.codex/` holds untracked symlinks (`skills`,
  `swarm-config.md`) and a `worktrees/` dir.
- An orphan checkout `/home/ketan/project/shatter/.claude/worktrees/str-umw3/`
  (9.0 MB; issue str-umw3 closed 2026-04-11) still exists. It is not
  registered in `git worktree list`, and greps still match it.
- `task meta` (`Taskfile.yml:396`) is the home for repo meta tests; its
  `sources:` list must include any new test file, or the checksum cache will
  skip it.
- Audit source: agent-repo-10 (`audits/2026-09-22/findings.json`,
  `audits/2026-09-22/areas/agent-repo.md`).

## Acceptance criteria

1. Repo `.gitignore` un-ignores `/.claude/` (`!/.claude/`) and re-ignores
   `/.claude/settings.local.json` and `/.claude/worktrees/`. It replaces the
   blanket `.codex/` ignore with rules that track `/.codex/AGENTS.md` (and any
   other codex files the project intends to track) and keep machine-local
   codex state ignored.
2. A meta test (for example `scripts/test_agent_config_trackable.py`) is wired
   into `task meta` `cmds:` and `sources:`. It asserts that
   `git check-ignore -q --no-index` **fails** (not ignored) for
   `.claude/skills/x/SKILL.md` and `.codex/AGENTS.md`, and **succeeds**
   (ignored) for `.claude/settings.local.json` and `.claude/worktrees/x`. It
   runs with the maintainer's real global excludes, and also with
   `core.excludesFile` pointing at a temp file containing `.claude/` and
   `.codex/`, so it holds on CI where the global file is absent.
3. Failing-then-passing proof in the close reason: the test run before the
   `.gitignore` change (fails) and after (passes).
4. `git status` in a worktree shows a newly created `.claude/skills/probe/SKILL.md`
   as untracked (recorded, then the probe is deleted).
5. The `.claude/worktrees/str-umw3/` orphan is removed **only after explicit
   operator confirmation**, with its size recorded. (This item moves here from
   str-qwua7.1.)
6. `task affected` passes, with `Gates selected` recorded.

## Suggested approach

A negation in the repo `.gitignore` overrides the global excludes file,
because repo `.gitignore` has higher precedence than `core.excludesFile`. Keep
the negations near the existing `.claude/` lines (130/165) so the intent is
visible.

**Possible alternative (not filed):** narrow the global ignore in dotfiles
(`~/.config/git/ignore:35-36`) to `.claude/settings.local.json`,
`.claude/worktrees/` and codex session state. That would fix every repo at
once. It is a dotfiles-side change, so it is mentioned here only as an option.
The repo-level negation plus test is still needed, so shatter does not depend
on each developer's global config.

## Out of scope

- Writing `.codex/AGENTS.md` itself (str-qwua7.54).
- Changing the dotfiles global gitignore (see the alternative above; no
  dotfiles issue is filed from this audit).

## Dependencies

None.
