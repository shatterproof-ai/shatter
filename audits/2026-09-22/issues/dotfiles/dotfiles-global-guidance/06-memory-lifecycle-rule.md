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

Part of #<epic>. Priority: P2. Type: enhancement.

## Problem

The global Self-Improvement Loop says only "update memory with the lesson". Nothing says a tooling or product bug belongs in the owning repo's tracker, and nothing says a memory must be retired when the underlying issue is fixed. Agent auto-memory therefore becomes a shadow tracker. Its entries outlive the bugs they describe, and later agents quote them as authority.

## Motivating example (already cleaned up, no action requested)

The Shatter project memory (`~/.claude/projects/-home-ketan-project-shatter/memory/`) showed the failure mode during the 2026-09-22 audit:

- `project_shatter_git_hook_test_corrupts_worktree.md` still told agents to commit and push with `--no-verify` after the underlying bug (shatter str-jttrf) had been fixed. Agents quoted it verbatim to justify bypassing hooks (audit finding sessions-03).
- `project_shatter_gate_cache_and_bare_primary.md` claimed the primary checkout has `core.bare=true`. `git -C /home/ketan/project/shatter config --show-origin core.bare` returned `file:.git/config false`. The audit's own prompt repeated the stale claim.
- `project_audit_2026_07_10_gate_state.md` was not indexed in `MEMORY.md`.

These files were corrected on 2026-09-23. This issue is about the global rule that would have stopped the drift, not about those files.

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
- [ ] The Self-Improvement Loop in `codex/AGENTS.md` points to `memory.md` within #23's word budget. The rule's summary line is in the core-rules file from `global-guidance-actually-loads`.
- [ ] A small check script (not the overscoped #5-#7 toolchain) scans `~/.claude/projects/*/memory/*.md`. It prints any file that:
  - contains `--no-verify` or `core.hooksPath` without naming an issue ID; or
  - is not linked from its directory's `MEMORY.md`.

  Proof at close: its test seeds one file of each kind and asserts that both are reported, and the closing comment includes one real run.

## Out of scope

- Editing specific Shatter memory files. That was done on 2026-09-23.
- Checking whether a referenced issue is closed. That needs per-tracker access. If wanted, file it separately.

## Dependencies

None blocking. Related: `global-guidance-actually-loads`, and dotfiles#23 (the word budget).

## Source

Shatter audit 2026-09-22 findings plugins-15 (P2; verifier: partially confirmed) and sessions-03.
