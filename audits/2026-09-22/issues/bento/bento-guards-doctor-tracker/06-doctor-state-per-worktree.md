---
slug: doctor-state-per-worktree
kind: new
title: "agent-env-doctor and require-worktree hooks read .agent-mode.local per checkout, so linked-worktree sessions never see collapsed state or primary-checkout decisions"
priority: P2
type: bug
labels: [audit, hooks]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# agent-env-doctor and require-worktree hooks read .agent-mode.local per checkout, so linked-worktree sessions never see collapsed state or primary-checkout decisions

bento-rdtn.2 is closed but its goal is not met in linked worktrees (a note on rdtn.2 points here). Source findings: bento-08, plugins-08 (context), shatter audit 2026-09-22.

## Problem

bento-rdtn.2 stores the doctor's seen/decided/remind_after keys in `.agent-mode.local` at `git rev-parse --show-toplevel`, which is per checkout. Launch-work requires work in linked worktrees, so every working session:

- gets the full dormancy paragraphs and the "prints once per repo" superpowers notice again;
- gets a fresh `.agent-mode.local` written into its worktree root;
- misses settings recorded in the primary checkout's copy, such as the `dangerous` launcher line and any `agent_env_doctor_skip_plugin`, `require_pushed=false` or `hook_bypass=allow` decision.

## Evidence (shatter)

- Primary `/home/ketan/project/shatter/.agent-mode.local`: `dangerous`, `agent_env_doctor_seen=bugshot,storystore`, `agent_env_doctor_superpowers_pointer_seen=true`.
- Linked worktree `audit-2026-09-22/.agent-mode.local` (created 2026-09-22 12:16): only the seen keys that session wrote.
- Running the doctor in the worktree prints the full text; in the primary it prints one-liners.

## Current code facts (bento origin/main @ b1bb787, re-verified 2026-09-23)

- `catalog/hooks/bento/claude/scripts/agent-env-doctor.py` line 198: root resolved with `git -C <cwd> rev-parse --show-toplevel`; line 588: `config = root / ".agent-mode.local"`; seen/decided writes use the same root.
- `catalog/hooks/bento/claude/scripts/require-worktree-git-guard.py` line 85: `_read_agent_mode_keys(repo_root)` has the same per-checkout scope. `check-unpushed.py` reads `require_pushed` the same way.

## Acceptance criteria

- The doctor, the require-worktree hooks and check-unpushed resolve `.agent-mode.local` from the main working tree (via `git rev-parse --git-common-dir`), for both reads and writes.
- Test: seen state written from one linked worktree collapses output in another linked worktree of the same repo; a `hook_bypass=allow` set in the primary is honoured in a linked worktree.
- Migration: an existing worktree-local `.agent-mode.local` is merged into the common one, or ignored with a one-line notice. Test included.
- Proof at close: the close note names the new tests with failing-then-passing runs.

## Out of scope

- New doctor checks.

## Priority / Type / Labels

P2 / bug / audit, hooks

## Parent epic

Epic: Audit 2026-09-22 findings (bento)

## Dependencies

None.
