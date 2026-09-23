---
slug: memory-lifecycle-rule
kind: new
title: "Agent memory stands in for a tracker and goes stale: add an issue-first, retire-on-close memory rule"
priority: P2
type: enhancement
labels: [documentation, enhancement]
parent_epic: "Epic: Audit 2026-09-22 findings (global agent guidance and hooks)"
blocked_by: []
existing_id: ""
tracker: "gh -R ketang/dotfiles (GitHub Issues; no .beads in the repo)"
---

# Agent memory stands in for a tracker and goes stale: add an issue-first, retire-on-close memory rule

Part of #<epic>. Priority: P2. Type: enhancement. Cross-references to other drafts use their slugs; the filer posts a slug-to-issue map on the epic.

## Problem

The global Self-Improvement Loop says only "update memory with the lesson". Nothing says a tooling or product bug belongs in the owning repo's tracker, and nothing says a memory must be retired when the underlying issue is fixed. Agent auto-memory therefore becomes a shadow tracker. Its entries outlive the bugs they describe, and later agents quote them as authority.

## Motivating example (partly cleaned up)

The Shatter project memory (`~/.claude/projects/-home-ketan-project-shatter/memory/`) shows the failure mode. State re-checked on 2026-09-23:

- `project_shatter_git_hook_test_corrupts_worktree.md` told agents to commit and push with `--no-verify` after the underlying bug (shatter str-jttrf) had been fixed. Agents quoted it verbatim to justify bypassing hooks (audit finding sessions-03). **Corrected on 2026-09-23**: it is now marked historical and no longer gives that advice.
- `project_shatter_gate_cache_and_bare_primary.md` (lines 3, 18 and 25) and its `MEMORY.md` index line **still** say the primary checkout has `core.bare = true` and tell agents not to run git there. `git -C /home/ketan/project/shatter config --show-origin core.bare` returns `file:.git/config false`. The audit's own prompt repeated the stale claim.
- `project_audit_2026_07_10_gate_state.md` **still** exists and is **still** missing from `MEMORY.md` (`grep -c 07_10 MEMORY.md` returns 0).

This issue is about the global rule that would have stopped the drift. Correcting the two remaining Shatter memories is the rule's first application and is part of closing it (see below).

## Evidence (current rule)

Re-verified on dotfiles @ `81f35e1`:

- `codex/AGENTS.md:57-62` "Self-Improvement Loop" says: "After corrections that reveal a recurring pattern, update memory with the lesson." It has no issue-first rule and no retirement rule.
- The memory audit toolchain was specified in #5, #6 and #7 (with #8 as packaging). All four were **closed as overscoped on 2026-09-07 without being built**: `claude/memory-index-audit.py` does not exist. No automated memory check exists today.
- dotfiles#23 caps Self-Improvement Loop at ≤ 12 words, so the full rule cannot live in `codex/AGENTS.md`.

## Acceptance criteria

- [ ] A leaf `~/dotfiles/docs/agent-guidance/memory.md` states these rules:
  1. A tooling or product bug goes to the owning repo's tracker first.
  2. Memory holds at most a one-line pointer with the issue ID and the date.
  3. When the issue closes, the memory is deleted or rewritten.
  4. A workaround memory, especially one containing bypass instructions such as `--no-verify` or `core.hooksPath`, must name the fixing issue and is removed when that issue closes.
- [ ] `memory.md` says where project facts belong: global per-project auto-memory or the repo's tracker memory (`bd remember`). It gives a one-line criterion for each.
- [ ] The Self-Improvement Loop in `codex/AGENTS.md` points to `memory.md` within #23's word budget.
- [ ] If `docs/agent-guidance/core.md` (from `global-guidance-actually-loads`) is on `main` when this lands, this issue adds the rule's one-line summary to it. If not, that issue adds it.
- [ ] A small check script (not the overscoped #5-#7 toolchain) scans `~/.claude/projects/*/memory/*.md`. It prints any file that:
  - contains `--no-verify` or `core.hooksPath` without naming an issue ID; or
  - is not linked from its directory's `MEMORY.md`.

  Its test, `claude/tests/test_memory_check.py` (run with `python3 -m pytest claude/tests/test_memory_check.py -q`), seeds one file of each kind plus one clean indexed file in a temp directory, and asserts that exactly the two bad files are reported.
- [ ] A real run of the check over `~/.claude/projects/*/memory/` reports `project_audit_2026_07_10_gate_state.md` as unindexed if that has not been fixed yet. After the fix, the stale `core.bare` claim is removed from `project_shatter_gate_cache_and_bare_primary.md` and its `MEMORY.md` line, and the 07_10 file is either indexed or deleted.

## Out of scope

- Editing Shatter memory files beyond the two still-stale ones named above.
- Checking whether a referenced issue is closed. That needs per-tracker access. If wanted, file it separately.

## Proof at close

The closing comment includes the pytest output, one real run of the check before and after the Shatter memory fixes, and the `memory.md` rule text.

## Maintainer decisions that apply

D6: nothing from this audit is filed by agents; the maintainer runs the filer.

## Dependencies

None blocking. Related: `global-guidance-actually-loads`, and dotfiles#23 (the word budget).

## Source

Shatter audit 2026-09-22 findings plugins-15 (P2; verifier: partially confirmed) and sessions-03.
