---
slug: git-guard-bypasses-and-false-positives
kind: new
title: "Git guard: close /usr/bin/git, wrapper-prefix, -C, cd and GIT_CONFIG bypasses; stop false positives on quoted/heredoc text"
priority: P1
type: bug
labels: [audit, hooks, safety]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# Git guard: close /usr/bin/git, wrapper-prefix, -C, cd and GIT_CONFIG bypasses; stop false positives on quoted/heredoc text

Follow-up to closed bento-rdtn.15, whose goal is not met. Source findings: sessions-02, bento-04, sessions-15 (shatter audit 2026-09-22). Related: bento-rdtn.15 (a reopen note points here).

## Problem

bento-rdtn.15 added `require-worktree-git-guard.py`, a PreToolUse Bash hook. It blocks branch-mutating git in the primary checkout and blocks hook bypasses (`--no-verify`, `-c core.hooksPath=`). The guard only inspects a command segment whose first non-`VAR=value` token is literally `git`, so:

- **Bypasses (false negatives).** The spellings agents use most are not inspected. In shatter session c1689435 (2026-09-21 23:03 to 09-22 02:00), after the guard shipped, about 22 commits and 18 pushes were spelled `/usr/bin/git -c core.hooksPath=/dev/null commit|push ...`. Shatter's memory tells agents to use `/usr/bin/git`, and the global RTK guidance tells them to prefix commands with `rtk`.
- **False positives.** `_SEGMENT_SPLIT_RE` splits on `;`, `|`, `&` and newlines even inside quotes and heredoc bodies. A read-only `python3 - <<'EOF' ... re.search(r'...|git merge|...') ... EOF` run from the primary checkout was blocked with "Blocked: 'git merge' mutates the checkout in the primary checkout".

## Evidence

Synthetic PreToolUse payloads piped into the guard (exit 2 = blocked).

Blocked (exit 2), as intended:
- `git commit --no-verify -m x`
- `git -c core.hooksPath=/dev/null push`
- `git merge feature` (cwd = primary checkout)

Allowed (exit 0), but should be blocked:
- `/usr/bin/git commit --no-verify -m x`
- `/usr/bin/git -c core.hooksPath=/dev/null commit -m x`
- `timeout 60 git push --no-verify`
- `rtk git commit --no-verify -m x`
- `env git commit --no-verify`
- `git -C <primary> merge feature` (cwd elsewhere)
- `cd <primary> && git merge feature`
- `GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.hooksPath GIT_CONFIG_VALUE_0=/dev/null git commit`
- `git update-ref refs/heads/main <sha>`
- `git switch main` (in the primary checkout)

False positive: a heredoc or quoted string containing `git merge` is blocked.

## Current code facts (bento origin/main @ b1bb787, re-verified 2026-09-23)

- Source: `catalog/hooks/bento/claude/scripts/require-worktree-git-guard.py`. The generated copy is `plugins/claude/bento/hooks/scripts/require-worktree-git-guard.py`; do not edit it by hand, rebuild it with `scripts/build-plugins`.
- Line 44: `_SEGMENT_SPLIT_RE = re.compile(r"&&|\|\||[;&|\n]")`.
- Lines 132-146: `_find_git_segments` skips leading `VAR=value` tokens without inspecting them, then requires `tokens[idx] == "git"`.
- The repo comes from the payload cwd. The path after `-C` is skipped, not resolved, and an earlier `cd` in the same chain is ignored.
- Line 207: any command containing `LAND_WORK_MARKER` (`BENTO_LAND_WORK=1`, line 42) is exempted wholesale. Keep that, but document it.
- Lines 229-236: the hook-bypass block message points to "the launch-work skill's dependency-bootstrap guidance for slow-hook fixes". That pointer is fixed by git-hook-latency-visibility, not here.

## Acceptance criteria

- Each "allowed but should be blocked" command above exits 2, with the existing message style.
- A heredoc or quoted string containing `git merge`, `--no-verify` or `core.hooksPath` is not blocked when no git process is invoked. At least three negative cases: a python heredoc, a `grep 'git merge'` pattern, and an `echo "... --no-verify ..."`.
- A table-driven test covers every case above, and the existing cases still pass.
- Proof at close: the close note names the new test file and records a failing run of the new cases before the fix and a passing run after, plus a passing `scripts/build-plugins` check that the generated copy matches the source.

## Suggested approach

- Tokenize with quote and heredoc awareness: strip heredoc bodies before splitting, then split with `shlex.shlex(punctuation_chars=True)`.
- Treat `os.path.basename(tok) == "git"` as git.
- Skip wrapper commands and their arguments before the git token: `timeout N`, `nice [-n N]`, `ionice ...`, `env [VAR=..]`, `command`, `rtk`, `sudo`.
- Resolve the effective repo from `-C <path>` and from the last `cd <path>` earlier in the chain.
- Inspect `GIT_CONFIG_KEY_n`/`GIT_CONFIG_VALUE_n` and `GIT_CONFIG_PARAMETERS` for `core.hooksPath`.
- Add `switch <primary-branch>` and `update-ref refs/heads/<primary>` to the mutation list.

## Out of scope

- Changing which operations are policy-forbidden.
- The block message's slow-hook pointer (git-hook-latency-visibility).
- The per-checkout scope of the `hook_bypass=allow` opt-out (doctor-state-per-worktree).

## Priority / Type / Labels

P1 / bug / audit, hooks, safety

## Parent epic

Epic: Audit 2026-09-22 findings (bento)

## Dependencies

None.
