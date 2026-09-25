# closure: add an apply mode for orphan worktree directories the doctor flags as "safe to remove"

- Filing action: new issue
- Priority: P2
- Type: feature
- Labels: audit, closure, hygiene
- Parent: epic
- Links: related bento-rdtn.1
- Source findings: bento-10

---BODY---
## Problem

bento-rdtn.1 made the doctor detect worktree directories that are no longer registered git worktrees, and it explicitly left removal to closure. No closure mode was ever added. In shatter the doctor has flagged the same five directories every session since the 2026-09-04 audit, and they are still there:

- `~/.local/share/worktrees/shatter/str-6q1i`
- `str-hszo-tmpfix`
- `str-k6e61-scm-followups`
- `str-mambd-enum-variant-gen`
- `str-yhsp-concolic-run`

Together they hold about 700 MB (109M, 573M, and 3 x 16K), with dates from June to July 2026. Shatter's AGENTS.md also forbids agents from deleting worktree dirs themselves, so nobody acts on the warning.

## Current code facts (bento @ 1c0c1e6)

- `agent-env-doctor.py` about lines 851-882: orphan detection with the message "... safe to remove".
- `closure-scan.py:1660`: the only apply modes are delete-local-merged and delete-local-patch-equivalent.

## Acceptance criteria

- `closure --apply remove-orphan-worktree-dirs`:
  - is dry-run by default, listing each path with its size and newest mtime;
  - on confirmation, removes only directories that are not registered worktrees and have no process with cwd inside them;
  - has tests.
- The doctor message names this exact command.

## Out of scope

- Changing shatter's AGENTS.md rule. That is shatter str-qwua7.23.
