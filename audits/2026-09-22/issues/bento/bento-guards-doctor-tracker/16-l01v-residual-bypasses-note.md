---
slug: l01v-residual-bypasses-note
kind: note-to-existing
title: "Note on bento-l01v: residual guard bypasses (/usr/bin/git, timeout/nice/ionice/sudo, -C, earlier cd, GIT_CONFIG_* env) are tracked separately and build on shell_segments.py"
priority: P1
type: note
labels: [audit, hooks, safety]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: bento-l01v
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# Note on bento-l01v

Target: bento-l01v (open, P2, "git guard: shared shell segmenter ..."). Action: add a comment. Do not change its scope. This draft exists so the filer adds the dependency edge git-guard-bypasses-and-false-positives blocked-by bento-l01v.

## Comment text

Audit 2026-09-22 (shatter; findings sessions-02, bento-04). The guard's most common real-world bypass is not in this issue's scope: in shatter session c1689435 (2026-09-21/22), after the guard shipped, about 22 commits and 18 pushes were spelled `/usr/bin/git -c core.hooksPath=/dev/null commit|push ...`, and shatter's memory tells agents to prefer `/usr/bin/git`. That, the `timeout`/`nice`/`ionice`/`sudo` wrappers, `-C <path>` and earlier-`cd` repo resolution (this issue's stated known limit), and hook disabling through `GIT_CONFIG_COUNT/KEY/VALUE` or `GIT_CONFIG_PARAMETERS` are tracked in <id of git-guard-bypasses-and-false-positives>, which is blocked by this issue and builds on `shell_segments.py` instead of adding a second tokenizer.

One API request for this issue so the follow-up does not need to change the corpus: have `shell_segments` expose the `VAR=value` assignments it strips (for example a `command_segments_with_env()` sibling returning `(env, argv)` per segment), since the follow-up must inspect `GIT_CONFIG_*` assignments.

Please add the follow-up to the shared-parser landing order after bento-i76i (and bento-nfi3). Because that P1 follow-up is blocked here, consider raising this issue to P1.
