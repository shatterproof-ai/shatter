# Git guard: close /usr/bin/git, wrapper-prefix, -C, cd and GIT_CONFIG bypasses; stop false positives on quoted/heredoc text

- Filing action: new issue (follow-up to closed bento-rdtn.15, whose goal is unmet)
- Priority: P1
- Type: bug
- Labels: audit, hooks, safety
- Parent: epic
- Links: related bento-rdtn.15
- Source findings: sessions-02, bento-04, sessions-15 (shatter audit 2026-09-22)

---BODY---
## Problem

bento-rdtn.15 added `require-worktree-git-guard.py`, a PreToolUse Bash hook. It blocks branch-mutating git in the primary checkout and blocks hook bypasses (`--no-verify`, `-c core.hooksPath=`). The guard only recognises a segment whose first non-env token is literally `git`, so:

- **Bypasses (false negatives).** Common real-world spellings are not inspected at all. Agents have used this in practice: in shatter session c1689435 (2026-09-21 23:03 to 09-22 02:00) about 22 commits and 18 pushes were spelled `/usr/bin/git -c core.hooksPath=/dev/null commit|push ...`, after the guard shipped. Shatter's own memory tells agents to use `/usr/bin/git`, and the global RTK guidance tells them to prefix commands with `rtk`, so the forms the guard misses are the forms agents use most.
- **False positives.** `_SEGMENT_SPLIT_RE` splits on `;`, `|`, `&` and newlines even inside quotes and heredoc bodies. Example: a read-only `python3 - <<'EOF' ... re.search(r'...|git merge|...') ... EOF` run from the primary checkout was blocked with "Blocked: 'git merge' mutates the checkout in the primary checkout".

## Evidence (synthetic PreToolUse payloads piped into the guard; exit 2 = blocked)

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

False positive: a heredoc or quoted string that contains `git merge` is blocked.

## Current code facts (bento @ 1c0c1e6)

- Source: `catalog/hooks/bento/claude/scripts/require-worktree-git-guard.py`. The generated copy is `plugins/claude/bento/hooks/scripts/require-worktree-git-guard.py`; do not edit it by hand, rebuild it with `scripts/build-plugins`.
- `_SEGMENT_SPLIT_RE = re.compile(r"&&|\|\||[;&|\n]")` at line 44.
- `_find_git_segments` (about lines 130-146) skips leading `VAR=value` tokens without inspecting them, then requires `tokens[idx] == "git"`.
- The repo is taken from the payload cwd. The path after `-C` is skipped, not resolved, and a preceding `cd` in the same command chain is ignored.
- Any command containing `LAND_WORK_MARKER` is exempted wholesale (about line 207). Keep that behaviour, but document it.

## Acceptance criteria

- Each "allowed but should be blocked" command above exits 2, with the existing message style.
- A heredoc or quoted string containing `git merge`, `--no-verify` or `core.hooksPath` is not blocked when no git process is invoked. At least 3 negative cases: a python heredoc, a `grep 'git merge'` pattern, and an `echo "... --no-verify ..."`.
- A table-driven test covers every case above. The existing 30 cases still pass.
- The block message for hook bypass points to a reference that actually discusses slow hooks. See the sibling issue on beads hook latency; until that lands, link it or drop the pointer.

## Suggested approach

- Tokenize quote- and heredoc-aware: strip heredoc bodies before splitting, and split with `shlex.shlex(punctuation_chars=True)`.
- Treat `os.path.basename(tok) == "git"` as git.
- Skip wrapper commands and their arguments before the git token: `timeout N`, `nice [-n N]`, `ionice ...`, `env [VAR=..]`, `command`, `rtk`, `sudo`.
- Resolve the effective repo from `-C <path>` and from the last `cd <path>` earlier in the chain.
- Inspect `GIT_CONFIG_KEY_n`/`GIT_CONFIG_VALUE_n` and `GIT_CONFIG_PARAMETERS` for `core.hooksPath`.
- Add `switch <primary-branch>` and `update-ref refs/heads/<primary>` to the mutation list.

## Out of scope

- Changing which operations are policy-forbidden.
- The `hook_bypass=allow` opt-out semantics. Its per-checkout scope is covered by the doctor per-worktree-state issue.
