# agent-env-doctor and require-worktree hooks read .agent-mode.local per checkout, so linked-worktree sessions never see collapsed state

- Filing action: new issue (bento-rdtn.2 closed but goal unmet in linked worktrees; alternatively reopen rdtn.2)
- Priority: P2
- Type: bug
- Labels: audit, hooks
- Parent: epic
- Links: related bento-rdtn.2
- Source findings: bento-08, plugins-08 (context)

---BODY---
## Problem

bento-rdtn.2 stores the doctor's seen/decided/remind_after keys in `.agent-mode.local` at `git rev-parse --show-toplevel`, which is per checkout. Launch-work requires work in linked worktrees, so every working session:

- gets the full dormancy paragraphs and the "prints once per repo" superpowers notice again;
- gets a fresh `.agent-mode.local` written into its worktree root;
- misses settings recorded in the primary checkout's copy, such as the `dangerous` launcher line and any `agent_env_doctor_skip_plugin` or `hook_bypass=allow` decision.

## Evidence (shatter)

- Primary `/home/ketan/project/shatter/.agent-mode.local`: `dangerous`, `agent_env_doctor_seen=bugshot,storystore`, `agent_env_doctor_superpowers_pointer_seen=true`.
- Linked worktree `audit-2026-09-22/.agent-mode.local` (created 2026-09-22 12:16): only the seen keys that session wrote.
- Running the doctor in the worktree prints the full text. In the primary it prints one-liners.

## Current code facts (bento @ 1c0c1e6)

- `catalog/hooks/bento/claude/scripts/agent-env-doctor.py`: root resolved with `rev-parse --show-toplevel` (about line 198); reads and writes `root/.agent-mode.local` (about lines 588, 775, 946).
- `require-worktree-git-guard.py` `_read_agent_mode_keys` has the same per-checkout scope.

## Acceptance criteria

- The doctor and both require-worktree hooks resolve `.agent-mode.local` via `git rev-parse --git-common-dir`, meaning the main checkout's copy, for both reads and writes.
- Test: seen state written from a linked worktree collapses output in another linked worktree of the same repo.
- Migration: an existing worktree-local `.agent-mode.local` is merged into the common one, or ignored with a one-line notice.

## Out of scope

- New doctor checks.
