---
slug: qwua7-22-audit-land-before-file-note
kind: note-to-existing
title: "Note on str-qwua7.22: amend the /audit Phase 10 order to land the report before filing, and drop the bd sync sentence"
priority: P2
type: task
labels: [agents, audit, skills, audit-2026-09-22]
parent_epic: "(existing issue; parent str-qwua7)"
blocked_by: []
existing_id: str-qwua7.22
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-qwua7.22: land the audit report before filing

**Target:** `str-qwua7.22` (open, P2, "Rewrite /audit Phase 10 to be
worktree- and tracker-safe; add a patrol check for stale audit issues").

**Action:** post the comment below with `bd comments add str-qwua7.22`. Do not
close it and do not change its priority. str-qwua7.22 stays the single owner
of the `/audit` skill rewrite; no 2026-09-22 issue duplicates it.

## Comment text

**Audit 2026-09-22 note: amended acceptance order (supersedes the first
acceptance check's order)**

The 2026-09-22 audit found that filing before landing leaves filed issues
citing evidence that exists only on a local branch, and that branch can be
deleted: `audit-2026-09-04` was deleted after its issues were filed, so the
50 open str-qwua7 children cite `audits/2026-09-04...` paths that are not on
main (being recovered by <audit-2026-09-04-report-recovery>). The 2026-07-10
report was lost the same way (str-sff87).

Replace this issue's first acceptance check with this order:

1. `bd create` the audit-labelled tracking issue.
2. `bento:launch-work` on it; write `audits/<date>.md` and the evidence in that
   worktree.
3. Findings are **drafted** in the worktree (not filed).
4. `bento:land-work` lands the report and evidence on main.
5. A check proves every evidence path cited by a draft exists on origin/main
   (`git cat-file -e origin/main:<path>`; reuse the script added by
   <publish-audit-reports>).
6. Only then are issues filed, with `--parent <audit-id>`; the report's final
   table lists them in a follow-up commit or a comment on the audit issue.
7. The audit issue is closed with reason `report: audits/<date>.md`.

Also: the skill must not mention `bd sync` (it does not exist in bd 1.1.0 and
is retired by maintainer decision D4). The line-level removal of the
`SKILL.md:385` sentence is done by <beads-jsonl-consumers-drop-bd-sync>;
this issue must not reintroduce it. Tracker sync, if any, follows the
AGENTS.md Dolt-remote procedure from <beads-retire-jsonl-import-dolt-remote>.

The patrol check and the str-sff87 close in this issue are unchanged.
Evidence: `audits/2026-09-22/findings.json` agent-repo-04, docs-04.
