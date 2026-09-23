---
slug: closure-orphan-worktree-dirs
kind: new
title: 'closure: add an apply mode for orphan worktree directories the doctor flags as "safe to remove"'
priority: P2
type: feature
labels: [audit, closure, hygiene]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# closure: add an apply mode for orphan worktree directories the doctor flags as "safe to remove"

Related: bento-rdtn.1. Source finding: bento-10 (shatter audit 2026-09-22).

## Problem

bento-rdtn.1 made the doctor detect worktree directories that are no longer registered git worktrees, and explicitly left removal to closure. No closure mode was added. In shatter the doctor has flagged the same five directories every session since the 2026-09-04 audit, and they are still there:

- `~/.local/share/worktrees/shatter/str-6q1i`
- `~/.local/share/worktrees/shatter/str-hszo-tmpfix`
- `~/.local/share/worktrees/shatter/str-k6e61-scm-followups`
- `~/.local/share/worktrees/shatter/str-mambd-enum-variant-gen`
- `~/.local/share/worktrees/shatter/str-yhsp-concolic-run`

Together they hold about 700 MB (109M, 573M and 3 x 16K), dated June to July 2026. Shatter's AGENTS.md forbids agents from deleting worktree dirs themselves, so nobody acts on the warning.

## Current code facts (bento origin/main @ b1bb787, re-verified 2026-09-23)

- `catalog/hooks/bento/claude/scripts/agent-env-doctor.py` lines 851-887 (`check_worktree_root_orphans`): orphan detection with the message "... worktree — dead directory left behind, safe to remove".
- `catalog/skills/closure/scripts/closure-scan.py` line 1660: the only apply modes are delete-local-merged and delete-local-patch-equivalent.

## Acceptance criteria

- `closure --apply remove-orphan-worktree-dirs`:
  - is dry-run by default, listing each path with its size and newest mtime;
  - on confirmation, removes only directories that are not registered worktrees of any repo and have no process with cwd inside them;
  - has tests covering a registered worktree (kept), an orphan (removed), and an orphan with a live process cwd inside (kept).
- The doctor message names this exact command.
- Proof at close: the close note names the tests with failing-then-passing runs and includes a dry-run listing from a real repo.

## Out of scope

- Changing shatter's AGENTS.md rule (shatter str-qwua7.23).

## Priority / Type / Labels

P2 / feature / audit, closure, hygiene

## Parent epic

Epic: Audit 2026-09-22 findings (bento)

## Dependencies

None.
