---
slug: publish-audit-reports
kind: new
title: "Recover and land the 2026-09-04 audit report (branch deleted, commits unreachable), land the 2026-09-22 report, and make /audit land before it files"
priority: P1
type: task
labels: [agents, audit, skills, docs, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Recover and land the 2026-09-04 audit report (branch deleted, commits unreachable), land the 2026-09-22 report, and make /audit land before it files

## Problem

Audit reports are never published to main, so the issues filed from them cite
evidence that fresh agents cannot read.

- The 2026-09-04 report and evidence (62 str-qwua7 children, 68 open issues
  cite `audits/2026-09-04...`) lived only on the local branch
  `audit-2026-09-04`. **Since the audit, that branch has been deleted.** Its
  commits are now unreachable and will be lost at the next `git gc` prune.
- The 2026-07-10 audit was lost the same way (str-sff87: "no 2026-07-10
  report was committed").
- The 2026-06-09 audit commits are also unreachable.
- This 2026-09-22 report exists only on the local, unpushed branch
  `audit-2026-09-22`.

The repo `/audit` skill ends with "commit report" on main, which the
require-worktree hook blocks. It relies on `bd sync`, which does not exist in
bd 1.1.0 and is retired by D4. It has no landing step.

## Evidence (re-verified 2026-09-23)

- In `/home/ketan/project/shatter`:
  - `git for-each-ref | grep -i audit` lists only `refs/heads/audit-2026-09-22`
    and `refs/remotes/origin/audit-kapow-2026-05-24`. **`audit-2026-09-04` is
    gone.** (The 2026-09-22 findings still saw it: "102 behind / 7 ahead".)
  - `git fsck --unreachable` lists the 09-04 chain as unreachable commits:
    `e067979d` (tip, 2026-09-07 "audit: 2026-09-04 record ids of decided
    issues"), `032c1319`, `a21108b2`, `ce8c1f04`, `e577d697`, `6eb87f9d`,
    `42c112cd` (2026-09-04 "audit: 2026-09-04"). The parent is 84941b37.
    `git ls-tree -r e067979d -- audits` has 98 files under
    `audits/2026-09-04/` (gates, triage-drafts, ui) plus the report.
  - Unreachable 2026-06-09 audit commits: `f9dad247` (latest, 2026-06-12),
    `a7a27ea3`, `f307ac3a`, `8b8bcc14`.
- `git ls-remote --heads origin 'audit*'` returns only
  `audit-kapow-2026-05-24`. `audit-2026-09-22` is unpushed; it has one commit
  (56c86168), 507 tracked files, and 16 of them are session-transcript files
  under `audits/2026-09-22/sessions/`.
- `git ls-tree origin/main -- audits/` lists only `2026-02-28.md`,
  `2026-05-21.md` and `kapow-2026-05-21`. `docs/audits/` now exists on main
  and holds `2026-04-19-go-planner-parity.md`.
- `.claude/skills/audit/SKILL.md:380-387`: step 4 says to write
  `audits/YYYY-MM-DD.md`; step 6 says "Commit report ... Do NOT commit beads
  issue changes — those are handled by `bd sync`". There is no launch-work or
  land-work step.
- `docs/INDEX.md` has no "Audits" section.
- Related open issues: str-qwua7.22 (P2, "Rewrite /audit Phase 10 to be
  worktree- and tracker-safe; add a patrol check for stale audit ...") and
  str-qwua7.44 (P2, docs IA; it decided to move `audits/` to `docs/audits/`).
  Neither recovers or lands the 09-04 report.
- Findings: agent-repo-04 (verified P1), docs-04. Source draft:
  `drafts/shatter-agent/06-publish-audit-reports-and-audit-skill-landing.md`.

## Acceptance criteria

- [ ] **First, before anything else:** a ref preserves the 09-04 tip
      (for example `git branch audit-2026-09-04-recovered e067979d`, run by
      the maintainer or with their approval), and the 06-09 tip if it is
      wanted. The commands used are in the close reason.
- [ ] The cause of the deletion is identified (bento closure, a
      cleanup script, or a manual delete) and recorded. If a tool deleted an
      unmerged, unpushed branch, a bug is filed against that tool.
- [ ] The 2026-09-04 report and its evidence are on origin/main at the path
      str-qwua7.44 chose (`docs/audits/2026-09-04/`), or at
      `audits/2026-09-04/` if .44 has not landed. They land through
      launch-work/land-work. A script checks every evidence path cited by an
      open str-qwua7 child with `git cat-file -e origin/main:<path>` (after
      any path rewrite), and its zero-failure output is in the close reason.
- [ ] The 2026-09-22 report (`audits/2026-09-22.md` plus the tracked
      evidence) lands on origin/main the same way **before** the maintainer's
      D6 filer script files issues that cite it. The maintainer first reviews
      the `sessions/` transcript files for content that should not be on main.
      Untracked `goals-runs/` (about 255 MB) stays out.
- [ ] `.claude/skills/audit/SKILL.md` post-audit steps are, in order: create
      the audit issue; launch-work a worktree; write the report there; land
      the report; then file issues. The skill states the check command that
      proves issue evidence paths exist on origin/main. No step mentions
      `bd sync` (D4); tracker sync follows the AGENTS.md Dolt-remote procedure.
- [ ] `docs/INDEX.md` has an "Audits" section that links every landed report.

## Suggested approach

Recover the refs first; that is time-critical. Then coordinate with
str-qwua7.22. Either land this issue first and mark .22's Phase-10 part done,
or post a note on .22 that this issue carries the skill rewrite. Link both
str-qwua7.22 and str-qwua7.44 when filing.

## Out of scope

- The drift-patrol "audit follow-through" burn-down check
  (triage-policy-and-audit-epic-waves).
- Filing the 2026-09-22 issues themselves (D6: the maintainer's filer script).
- Moving older reports (02-28, 05-21) unless str-qwua7.44 does it.

## Priority / type / labels

P1, task. Labels: agents, audit, skills, docs, audit-2026-09-22.

## Parent epic

Epic: Audit 2026-09-22 findings (shatter).

## Dependencies

- Blocked by: none. This must run before the D6 filer script.
- Related: str-qwua7.22, str-qwua7.44, str-sff87 (closed), and
  beads-jsonl-consumers-drop-bd-sync (SKILL.md:385 `bd sync` line).
