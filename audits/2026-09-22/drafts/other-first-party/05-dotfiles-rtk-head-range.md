# rtk still shows summarized content for `head -N` inside compound commands (follow-up to #11)

## Filing metadata

- tracker/repo: dotfiles
- action: create new issue
- type: bug
- priority: P2
- labels: bug
- parent: repo epic (see INDEX)
- dedupe relation: duplicate-closed-but-unfixed (dotfiles#11) -> new follow-up issue
- source findings: plugins-13

## Readiness precheck

- review_mode: local-fallback (this drafting runtime exposed no subagent/Task tool; re-run bento:issue-readiness-check with a fresh reviewer before filing)
- ready: yes
- too_broad: no

<!-- BODY -->
## Problem

Closed issue #11 narrowed the rtk rewrite hook (`claude/rtk_prefilter.py`) and added a byte-identity smoke test for redirects, `find` and git ref reads. Exact-range reads shown to the agent are still rewritten, and the agent receives the wrong bytes.

## Reproduction

Run it in a Claude Code session with the rtk hook active:

```
wc -l ~/.claude/hooks/bento/require-worktree.sh; true; head -5 ~/.claude/hooks/bento/require-worktree.sh
```

The displayed `head` output was `#!/usr/bin/env bash / # See hooks/... / } / import json, os, sys / [164 more lines]`. `/usr/bin/head -5` on the same 168-line file prints the true first five lines, including `set -euo pipefail`. Reproduced twice (2026-09-22).

## Acceptance criteria

- [ ] `rtk_prefilter.py` passes `head`/`tail`/`sed -n`/`cat` with explicit ranges through unmodified, including when they appear as segments of `;`/`&&`/`|` chains.
- [ ] A byte-identity test covers the compound forms `a; head -5 f; b`, `a && tail -n 3 f`, `sed -n '1,4p' f | cat`.
- [ ] Optionally, an upstream rtk issue is filed so rtk never replaces requested ranges with summaries.

## Source

Shatter audit 2026-09-22 finding plugins-13. Prior issue: #11.
