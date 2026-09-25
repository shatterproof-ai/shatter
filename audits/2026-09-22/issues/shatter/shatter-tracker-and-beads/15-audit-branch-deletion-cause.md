---
slug: audit-branch-deletion-cause
kind: new
title: "Find out (time-boxed) what deleted the unmerged, unpushed audit-2026-09-04 branch, and file a bug on the tool if one did it"
priority: P2
type: task
labels: [agents, audit, git, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Find out (time-boxed) what deleted the unmerged, unpushed audit-2026-09-04 branch, and file a bug on the tool if one did it

## Problem

The local branch `audit-2026-09-04` (tip `e067979d`, 7 commits, never pushed,
never merged) was deleted between the 2026-09-22 audit run, which still saw
it ("102 behind / 7 ahead"), and 2026-09-23. The 2026-06-09 audit commits
(`f9dad247` and others) are unreachable too. If an automated tool (bento
closure, a cleanup script, land-work teardown) deletes unmerged, unpushed
branches, it will destroy more work. If it was a manual delete, the fix is
guidance instead.

Missing refs and surviving objects show that the work is recoverable. They do
not show who deleted the branch or why, and the trail may be gone. So this
investigation is bounded, and "cause undetermined" is an acceptable outcome.

## Evidence (re-verified 2026-09-23)

- `git for-each-ref | grep -i audit` in /home/ketan/project/shatter shows no
  `audit-2026-09-04`; `git cat-file -t e067979d` → `commit`; no reflog entry
  holds it.
- Candidate actors: bento `closure` (garbage-collects other agents' branches),
  bento `land-work` teardown, `scripts/cleanup-merged-remote-branches.sh`
  (remote only), `scripts/cleanup.sh`, manual `git branch -D`.
- Finding: agent-repo-04.

## Acceptance criteria

- [ ] At most one working session (about 2 hours) is spent. The sources
      searched are listed in the close reason: at least session transcripts
      under `~/.claude/projects/-home-ketan-project-shatter/` from 2026-09-22
      to 2026-09-23 (grep for `audit-2026-09-04`, `branch -D`, `closure`),
      bento closure/land-work logs if any exist, and `git reflog` of
      `HEAD` in each worktree.
- [ ] The outcome is one of: (a) cause identified, with the transcript line or
      log entry quoted; (b) cause undetermined, with the searches that came
      up empty.
- [ ] If (a) and a tool deleted an unmerged, unpushed branch, a bug is filed
      against that tool's tracker with a reproduction (for example a scratch
      repo where the tool deletes an unmerged local branch), and its id is in
      the close reason. If a manual delete, a one-line guidance change is
      proposed to the maintainer instead.
- [ ] This issue does not block recovering or landing any report.

## Out of scope

- Recovering the commits (the maintainer's bootstrap ref) and landing the
  report (audit-2026-09-04-report-recovery).

## Priority / type / labels

P2, task. Labels: agents, audit, git, audit-2026-09-22.

## Parent epic

Epic: Audit 2026-09-22 findings (shatter).

## Dependencies

- Blocked by: none.
- Related: audit-2026-09-04-report-recovery, publish-audit-reports.
