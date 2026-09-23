---
slug: tool-precedence-vs-harness-mode
kind: new
title: "Tool-Specific Notes never say the Read/Grep-first rule overrides the harness's bypass-mode Bash guidance (missed #10 acceptance check)"
priority: P3
type: enhancement
labels: [documentation]
parent_epic: "Epic: Audit 2026-09-22 findings (global agent guidance and hooks)"
blocked_by: []
existing_id: ""
tracker: "gh -R ketang/dotfiles (GitHub Issues; no .beads in the repo)"
---

# Tool-Specific Notes never say the Read/Grep-first rule overrides the harness's bypass-mode Bash guidance (missed #10 acceptance check)

Part of #<epic>. Priority: P3. Type: enhancement. Follow-up to closed **#10** ("Reconcile subagent, tools-vs-Bash, and RTK guidance into one paragraph"). Cross-references to other drafts use their slugs; the filer posts a slug-to-issue map on the epic.

## Problem

Closed #10 had this acceptance check: the "How to act" section "states explicitly that this rule wins over harness auto-mode prompts that prefer Bash for reads, because it is user instruction (which the harness ranks above its defaults)". #10 was closed on 2026-09-07 as landed (`af7864c`). The current text does not contain that statement.

The two sources do not strictly contradict each other: the harness permits shell reads, and the user rule prefers dedicated tools. But the harness text is specific and recent in context, the user rule does not say it takes precedence, and agents follow the harness. #10 already decided the precedence; this issue only lands the missing sentence.

## Evidence

- `codex/AGENTS.md:131-138` (dotfiles @ `81f35e1`), Tool-Specific Notes: "prefer `Read`/`Grep`/`Glob` over Bash `cat`/`grep`/`find` pipelines for anything those tools cover natively" and "dedicated harness tools (`Read`, `Grep`, `Glob`, `Edit`) still take precedence over any shell equivalent". `grep -n -i "auto-mode\|auto mode\|bypass" codex/AGENTS.md` finds nothing. The only "harness" match (`:137`) refers to the tools, not to harness mode text.
- The harness's bypass-permissions text, as it appears in a Claude Code session's system context on 2026-09-23: "While bypass permissions mode is active: You can do much of your work through the Bash tool when it is the simpler route: read files with cat, head, or sed -n, search with grep and find … The choice is yours".
- Usage, indicative only (the verifier did not recount, and these counts do not show that each shell call could have used a dedicated tool): in Shatter transcripts since 2026-09-04, 655 Bash calls led by grep, cat, sed, find or head across 34 sessions, against Read 379, Grep 128 and Glob 11. #10 itself measured 886 non-piped `grep/cat/sed/find` calls where Read/Grep/Glob fit.
- The rtk `find -not/-exec` rejections cited by the source finding all predate #11 (`88e9cb5`, 2026-09-07) and are excluded. The live exact-range problem (`head`/`sed -n` summarised inside compound commands) is tracked in `rtk-head-range-compound`.

## Acceptance criteria

- [ ] Tool-Specific Notes (or its successor after #23's rewrite) states that the Read/Grep/Glob preference is user instruction and takes precedence over harness mode text that permits or encourages shell reads, including bypass-permissions mode. This implements #10's acceptance check and does not change the rule itself.
- [ ] The wording fits dotfiles#23's ≤ 150-word target for Tool-Specific Notes. If #23 has landed, `test/global-instructions-budget-test.sh` still passes.
- [ ] Relaxing the rule (for example "plain shell is acceptable for one-off reads") is **not** part of this issue. It would reverse #10's decision and needs a separate maintainer decision.

## Proof at close

The closing comment shows the new text, `wc -w codex/AGENTS.md`, and `grep -n -i "bypass\|harness mode" ~/.codex/AGENTS.md` finding the sentence in the rendered Codex file (not only `agents-sync.sh status`).

## Out of scope

- rtk `find` handling (fixed in #11).
- The shatter-local Tool-rules block (str-qwua7.26).
- Hook enforcement of tool choice.

## Maintainer decisions that apply

D6: nothing from this audit is filed by agents; the maintainer runs the filer.

## Dependencies

None blocking. Coordinate with dotfiles#23, which rewrites the same section under a word budget. Whichever lands second rebases onto the other. Related: `rtk-head-range-compound`.

## Source

Shatter audit 2026-09-22 finding sessions-16 (verifier: partially confirmed, P3). Reframed during the Codex cross-check of 2026-09-23.
