---
slug: tool-precedence-vs-harness-mode
kind: new
title: "Global Read/Grep-first rule contradicts the harness bypass-mode text: state one rule that acknowledges both"
priority: P3
type: enhancement
labels: [documentation]
parent_epic: "Epic: Audit 2026-09-22 findings (global agent guidance and hooks)"
blocked_by: []
existing_id: ""
tracker: "gh -R ketang/dotfiles (GitHub Issues; no .beads in the repo)"
---

# Global Read/Grep-first rule contradicts the harness bypass-mode text: state one rule that acknowledges both

Part of #<epic>. Priority: P3. The verifier notes this is mostly a conflict of preferences with little effect on correctness. Type: enhancement.

## Problem

The global rule and the harness instructions give opposite defaults:

- `codex/AGENTS.md` "Tool-Specific Notes" (lines 129-137 at dotfiles @ `81f35e1`) says to prefer `Read`/`Grep`/`Glob` over Bash `cat`/`grep`/`find`.
- Claude Code's bypass-permissions harness text explicitly allows reading and searching with `cat`, `grep`, `sed` and `find` through Bash.

Agents follow the harness. Neither source says which one wins, and the global rule reads as ignored boilerplate.

## Evidence

Counts from Shatter transcripts since 2026-09-04 (the verifier did not recount them):

- 655 Bash calls led by grep, cat, sed, find or head, across 34 sessions: grep 372, cat 151, sed 70, find 58.
- Dedicated tools: Read 379, Grep 128, Glob 11.
- All-time totals: 1,470 shell-led calls against 1,429 dedicated-tool calls.

The source finding also cited rtk rejecting `find -not/-exec`. **That part is resolved.** Every occurrence of `rtk find does not support compound predicates` in the Shatter transcripts is dated between 2026-08-27 and 2026-09-07, before #11 landed (`88e9cb5`, 2026-09-07). `claude/rtk_prefilter.py:50-91` now passes those forms through, so it is excluded here.

The exact-range read problem, where `head`/`sed -n` output is summarized in compound commands, is a live reason to prefer `Read`. It is tracked in `rtk-head-range-compound`.

## Acceptance criteria

- [ ] The Tool-Specific Notes bullet (or its successor after #23's rewrite) acknowledges the harness's bypass mode and states the rule that matters:
  - use `Read` for files you will edit, or when you need exact byte ranges;
  - use `Grep`/`Glob` for multi-file search;
  - plain shell is acceptable for one-off reads and for pipelines that the dedicated tools cannot express.
- [ ] The wording fits dotfiles#23's ≤ 150-word target for Tool-Specific Notes. If #23 has landed, it must not break `test/global-instructions-budget-test.sh`.
- [ ] Proof at close: the closing comment shows the new bullet, the output of `wc -w codex/AGENTS.md`, and `codex/agents-sync.sh status` reporting `render vs snapshot: ok`.

## Out of scope

- rtk `find` handling (fixed in #11).
- The shatter-local Tool-rules block (str-qwua7.26).
- Hook enforcement of tool choice.

## Dependencies

None blocking. Coordinate with dotfiles#23, which rewrites the same section under a word budget. Whichever lands second rebases onto the other. Related: `rtk-head-range-compound`.

## Source

Shatter audit 2026-09-22 finding sessions-16 (verifier: partially confirmed, P3).
