---
slug: orphan-worktree-dirs-cleanup
kind: new
title: "Review the six orphan worktree directories (five doctor-flagged under ~/.local/share/worktrees/shatter plus .claude/worktrees/str-umw3) and remove or retain each with operator approval"
priority: P3
type: chore
labels: [agents, git, tooling, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Review the six orphan worktree directories and remove or retain each with operator approval

## Problem

Six directories look like worktrees but are no longer registered with git:

- five under `~/.local/share/worktrees/shatter/`, which the bento
  agent-env-doctor reports as orphan worktree dirs at every SessionStart;
- `/home/ketan/project/shatter/.claude/worktrees/str-umw3/` (issue str-umw3
  closed 2026-04-11), which recursive greps from the primary still match.

Removing them is destructive, so it needs explicit operator approval. This
cleanup was previously bundled into three other issues (str-qwua7.1's repair
half, and the audit drafts `env-doctor-decisions` and `agent-config-gitignore`),
where a declined deletion left those issues without a defined outcome. It now
lives here alone, and **retaining a directory is a valid, documented outcome.**

## Evidence

Re-verified 2026-09-23:

- `~/.local/share/worktrees/shatter/str-6q1i` (109 MB),
  `str-hszo-tmpfix` (573 MB), `str-k6e61-scm-followups` (16 KB),
  `str-mambd-enum-variant-gen` (16 KB), `str-yhsp-concolic-run` (16 KB). None
  is a git repo any more, and none appears in `git worktree list`.
- `/home/ketan/project/shatter/.claude/worktrees/str-umw3/` (9.0 MB), not in
  `git worktree list`.
- Audit sources: agent-repo-10, agent-repo-15, prior-04
  (`audits/2026-09-22/findings.json`).

## Acceptance criteria

1. For each of the six directories the issue records: size (`du -sh`),
   `git worktree list` showing it unregistered, whether it contains a `.git`
   file or dir, and whether it holds any file not present on origin/main that
   someone might want (a quick listing of top-level contents and any
   uncommitted-looking source files).
2. Each directory gets a disposition: `delete` or `retain`. For `delete`, the
   operator's explicit approval is quoted in the issue before removal; after
   removal `ls` no longer lists it. For `retain`, the reason is recorded.
3. The close reason includes the agent-env-doctor output from a SessionStart
   run in the primary checkout: deleted dirs are no longer reported; any
   retained dir that the doctor still reports is named with its retain
   reason. The issue can close with retained directories.

## Out of scope

- The bento doctor's detection logic or a way to silence a retained dir
  (bento tracker).
- The git-state check in str-qwua7.1.

## Dependencies

None. Detection of dead worktree dirs and stale previews landed in str-qwua7.1 (closed
2026-09-24, drift-patrol git-state check); this issue is only the operator-confirmed removal.
