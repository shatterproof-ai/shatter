---
slug: git-guard-reopen-note
kind: reopen-note
title: "Note on closed bento-rdtn.15: guard is bypassed by /usr/bin/git, wrapper prefixes, -C, cd and GIT_CONFIG env"
priority: P1
type: note
labels: [audit, hooks, safety]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: [git-guard-bypasses-and-false-positives]
existing_id: bento-rdtn.15
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# Note on bento-rdtn.15

Target: bento-rdtn.15 (closed). Action: add a comment (`bd comments add bento-rdtn.15 ...`). Do not reopen; the follow-up work is tracked in the new issue. Post it after git-guard-bypasses-and-false-positives is filed, and replace the slug with that issue's id.

## Comment text

Audit 2026-09-22 (shatter; findings sessions-02, bento-04): the goal of this issue is not met in practice. `require-worktree-git-guard.py` only inspects segments whose first non-`VAR=value` token is literally `git`, so these all exit 0:

- `/usr/bin/git -c core.hooksPath=/dev/null commit ...`
- `rtk git commit --no-verify`, `timeout 60 git push --no-verify`, `env git ...`, `nice -n 5 git commit --no-verify`
- `git -C <primary> merge ...` and `cd <primary> && git merge ...`
- `GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.hooksPath GIT_CONFIG_VALUE_0=/dev/null git commit`

In shatter session c1689435 (2026-09-21/22), after this guard shipped, about 22 commits and 18 pushes used the `/usr/bin/git -c core.hooksPath=/dev/null` form (re-probed at bento 0b8d488: all of the above still exit 0).

Where the fixes live: the quote/heredoc false positives and the `rtk`/`command`/`env` wrappers are bento-l01v; new primary-branch verbs (including `switch` and `update-ref`) are bento-i76i; `/usr/bin/git`, `timeout`/`nice`/`ionice`/`sudo`, `-C`, earlier `cd` and `GIT_CONFIG_*` env are <id of git-guard-bypasses-and-false-positives>.
