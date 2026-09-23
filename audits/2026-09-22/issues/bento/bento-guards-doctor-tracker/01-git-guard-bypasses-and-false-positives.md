---
slug: git-guard-bypasses-and-false-positives
kind: new
title: "Git guard: close the bypasses bento-l01v leaves open (/usr/bin/git, timeout/nice/ionice/sudo wrappers, -C <path>, earlier cd, GIT_CONFIG_* env)"
priority: P1
type: bug
labels: [audit, hooks, safety]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: [l01v-residual-bypasses-note, i76i-switch-update-ref-note]
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# Git guard: close the bypasses bento-l01v leaves open (/usr/bin/git, timeout/nice/ionice/sudo wrappers, -C <path>, earlier cd, GIT_CONFIG_* env)

Follow-up to closed bento-rdtn.15, whose goal is not met. Source findings: sessions-02, bento-04, sessions-15 (shatter audit 2026-09-22).

Related, and the scope boundary for this issue:

- **bento-l01v** (open): adds `shell_segments.py`, a quote-, heredoc- and comment-aware segmenter that also strips the `rtk`, `command`, `exec` and `env` wrappers. It fixes every false positive and the `rtk`/`command`/`env` false negatives. It lists "a `cd` in an earlier segment does not change how paths are resolved later" as a known limit. **This issue builds on `shell_segments.py`; it must not add a second tokenizer.**
- **bento-i76i** (open): adds commit/cherry-pick/pull/am/revert on the primary branch and clustered `-n`. It explicitly keeps the `-C <path>` limitation ("repo_root always comes from the payload cwd"). The `switch <primary>` and `update-ref refs/heads/<primary>` verbs this audit found are handed to i76i by a note (i76i owns the primary-branch verb list); they are not in this issue.
- **Landing order** (l01v's shared-parser order): bento-l01v, then bento-1srw (Codex copy of the guard), then bento-i76i, then bento-nfi3, then this issue. If the Codex copy exists when this lands, update both runtime copies (`tests/test_vendored_parity.py`).
- bento-rdtn.15 (closed; a reopen note points here).

## Problem

bento-rdtn.15 added `require-worktree-git-guard.py`, a PreToolUse Bash hook that blocks branch-mutating git in the primary checkout and hook bypasses (`--no-verify`, `-c core.hooksPath=`) everywhere. After bento-l01v and bento-i76i land, the guard will still miss these spellings, which are the ones agents in shatter actually use:

- **Absolute git path.** In shatter session c1689435 (2026-09-21 23:03 to 09-22 02:00), after the guard shipped, about 22 commits and 18 pushes were spelled `/usr/bin/git -c core.hooksPath=/dev/null commit|push ...`. Shatter's own memory tells agents to use `/usr/bin/git` to avoid the rtk wrapper, so this is the common form, not an edge case. l01v requires `argv[0] == "git"`.
- **Wrappers l01v does not strip:** `timeout N`, `nice [-n N]`, `ionice [-c N] [-n N]`, `sudo [-u U]`.
- **Repo resolution.** `git -C <primary> merge x` run from elsewhere, and `cd <primary> && git merge x`, are judged against the payload cwd, not the repo git would act on.
- **Config via environment.** `GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.hooksPath GIT_CONFIG_VALUE_0=/dev/null git commit` and `GIT_CONFIG_PARAMETERS="'core.hookspath'='/dev/null'" git commit` disable hooks without any `-c` or `--no-verify` token. l01v strips `VAR=value` prefixes (including after `env`) before the guard sees argv, so the guard cannot inspect them today.

## Evidence

Synthetic PreToolUse payloads piped into the guard at bento origin/main 0b8d488 (the guard is unchanged since b1bb787). Exit 0 = allowed; each of these should be 2:

- `/usr/bin/git commit --no-verify -m x`
- `/usr/bin/git -c core.hooksPath=/dev/null commit -m x`
- `timeout 60 git push --no-verify`
- `nice -n 5 git commit --no-verify -m x`
- `git -C <primary> merge feature` (payload cwd = a linked worktree)
- `cd <primary> && git merge feature` (payload cwd = a linked worktree)
- `GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.hooksPath GIT_CONFIG_VALUE_0=/dev/null git commit -m x`

Controls, already exit 2: `git commit --no-verify -m x`, `git -c core.hooksPath=/dev/null push`, `git merge feature` with cwd = primary.

## Current code facts (bento origin/main @ 0b8d488; guard unchanged since b1bb787)

- Source: `catalog/hooks/bento/claude/scripts/require-worktree-git-guard.py`. The generated copy `plugins/claude/bento/hooks/scripts/require-worktree-git-guard.py` is rebuilt with `scripts/build-plugins`, never edited by hand.
- Lines 132-146: `_find_git_segments` skips leading `VAR=value` tokens without inspecting them, then requires `tokens[idx] == "git"`.
- Lines 101-129: `_parse_git_invocation` skips the path after `-C`; line 218 takes `repo_root` from the payload cwd.
- Line 207: any command containing `LAND_WORK_MARKER` (`BENTO_LAND_WORK=1`, line 42) is exempted wholesale. Unchanged by this issue.
- Lines 229-236: the hook-bypass block message's slow-hook pointer is fixed by git-hook-latency-visibility, not here.

## Acceptance criteria

- Each command in the Evidence list exits 2 with the existing message style; each control still exits 2.
- Negative cases exit 0: `/usr/bin/git status`, `timeout 60 git fetch`, `git -C <linked-worktree> merge x` from the primary checkout, `cd /tmp && git status`, `GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=user.name GIT_CONFIG_VALUE_0=x git commit -m x` in a linked worktree.
- `-C` resolution: a relative `-C` path resolves against the payload cwd (or the last effective `cd`), several `-C` flags compose as git composes them, and a path that does not exist fails open (exit 0), matching the guard's existing fail-open contract. Tests cover each.
- `cd` tracking applies only within one `&&`/`;` chain; a `cd` inside a subshell or `$(...)` does not leak out. Tests cover both.
- The environment rule reads the `VAR=value` assignments the segmenter strips. `shell_segments.command_segments` (or a sibling function) exposes them; l01v's corpus still passes unchanged.
- No second tokenizer: the diff imports `shell_segments.py` and adds no quote or heredoc parsing of its own (reviewer checks; a grep test asserts `require-worktree-git-guard.py` contains no `shlex.split` call).
- Proof at close: the close note names the new tests and records a run of them failing on the pre-fix guard (with l01v and i76i landed) and passing after, plus `scripts/build-plugins` reporting the generated copy in sync.

## Suggested approach

- Treat `os.path.basename(argv[0]) == "git"` as git.
- Extend l01v's prefix stripping with `timeout [opts] N`, `nice [-n N]`, `ionice [opts]`, `sudo [opts]`.
- Resolve the effective repo from `-C` and a tracked `cd` target, then run the existing primary-checkout test on that path.
- Treat `GIT_CONFIG_KEY_n=core.hooksPath` (case-insensitive key) and `core.hookspath` inside `GIT_CONFIG_PARAMETERS` like `-c core.hooksPath=`.

## Out of scope

- False positives and the `rtk`/`command`/`env` wrappers (bento-l01v).
- New primary-branch verbs, including `switch` and `update-ref` (bento-i76i; see the note filed there).
- The block message's slow-hook pointer (git-hook-latency-visibility).
- The per-checkout scope of the `hook_bypass=allow` opt-out (doctor-state-per-worktree).
- `bash -c '...'` and `eval` bodies (l01v known limit).

## Priority / Type / Labels

P1 / bug / audit, hooks, safety

## Parent epic

Epic: Audit 2026-09-22 findings (bento)

## Dependencies

Blocked by bento-l01v (supplies `shell_segments.py`) and ordered after bento-i76i (same file, shared landing order). Both edges come from the note drafts l01v-residual-bypasses-note and i76i-switch-update-ref-note, which resolve to those ids.
