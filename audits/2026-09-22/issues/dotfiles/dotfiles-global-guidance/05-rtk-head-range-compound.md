---
slug: rtk-head-range-compound
kind: new
title: "rtk still replaces `head -N` output with a summary inside compound commands (follow-up to #11)"
priority: P2
type: bug
labels: [bug]
parent_epic: "Epic: Audit 2026-09-22 findings (global agent guidance and hooks)"
blocked_by: []
existing_id: ""
tracker: "gh -R ketang/dotfiles (GitHub Issues; no .beads in the repo)"
---

# rtk still replaces `head -N` output with a summary inside compound commands (follow-up to #11)

Part of #<epic>. Priority: P2. Type: bug. This is a follow-up to closed **#11** ("Narrow the rtk PreToolUse hook: skip find/pipelines/redirects/git ref reads; add a byte-identity smoke test").

## Problem

#11 added `claude/rtk_prefilter.py` (landed in `88e9cb5` on 2026-09-07). It passes redirects, `find` with `-not`/`-exec`, and git ref reads through unmodified. It does not cover exact-range reads whose output is shown to the agent. When `head -N` (and presumably `tail`, `sed -n` and similar reads) appears as a segment of a compound command, rtk still rewrites it. The agent then receives a token-compacted summary instead of the bytes it asked for, without being told.

## Reproduction

Re-reproduced on 2026-09-23, in a Claude Code session with the prefilter hook active. The hook is registered as `python3 $DOTFILES/claude/rtk_prefilter.py` at `~/dotfiles/claude/settings.json:285`.

```
wc -l ~/.claude/hooks/bento/require-worktree.sh; true; head -5 ~/.claude/hooks/bento/require-worktree.sh
```

Displayed output:

```
168
#!/usr/bin/env bash
# See hooks/references/hook-contract.md for the blocking contract
}
import json, os, sys
[164 more lines]
```

`/usr/bin/head -5` on the same file prints the true first five lines:

```
#!/usr/bin/env bash
# See hooks/references/hook-contract.md for the blocking contract
# (exit 2 = block, exit 1 = non-blocking failure, JSON decision shapes).
set -euo pipefail

```

It was reproduced twice on 2026-09-22 and once on 2026-09-23. The same summarising also happened to `head -20 claude/tests/run.sh` inside a `;` chain while this issue was being drafted.

## Acceptance criteria

- [ ] `claude/rtk_prefilter.py` passes these through unmodified, whether they are the whole command or any segment of a `;`, `&&`, `||` or `|` chain:
  - `head` or `tail` with an explicit `-n N` / `-N`;
  - `sed -n '<range>p'`;
  - `cat` of a named file.
- [ ] New cases in `claude/tests/test_rtk_prefilter.py`, run with `python3 -m pytest claude/tests/test_rtk_prefilter.py -q` (`claude/tests/run.sh` runs only `test_*.sh`), fail before the fix and pass after it. The closing comment includes both runs. The cases must cover at least:
  - `a; head -5 f; b`
  - `a && tail -n 3 f`
  - `sed -n '1,4p' f | cat`
  - `wc -l f; true; head -5 f` (the reproduction above)
- [ ] The reproduction command above prints the true first five lines in a live session. Paste the output into the closing comment.
- [ ] The closing comment says whether an upstream rtk report ("never replace requested ranges with summaries") is worth filing. The maintainer files it; the implementer does not.

## Suggested approach

`rtk_prefilter.py` has no shell tokenizer. `skip_reason()` (`:84-95`) runs word-bounded regexes over the whole raw command, deliberately unanchored to position so that chains, `$(...)` and multi-line scripts are covered (see the comment at `:66-82`). Follow that design: add one more whole-string regex for exact-range reads (`head`/`tail` with `-n N` or `-N`, `sed -n '...p'`, `cat <path>`) and return a skip reason when it matches. The prefilter then exits 0 with no output, so the command runs byte-identical, as it already does for its other skip cases. A false positive only costs rtk's token savings. Note that `|` pipelines are not currently a skip case (only `tee` is), so `sed -n '1,4p' f | cat` needs the new regex too.

## Out of scope

- The `find -not/-exec` case, which #11 already handles (`rtk_prefilter.py:50-91`).
- rtk's own rewrite logic in `~/.claude/hooks/rtk-rewrite.sh`, which the rtk tool owns.

## Maintainer decisions that apply

D6: nothing from this audit is filed by agents; the maintainer runs the filer.

## Dependencies

None blocking. Related: `hooks-dotfiles-env-unset`. The prefilter hook uses `$DOTFILES`, so in sessions where it is unset neither the prefilter nor rtk runs at all.

## Source

Shatter audit 2026-09-22 finding plugins-13. Prior issue: #11 (closed).
