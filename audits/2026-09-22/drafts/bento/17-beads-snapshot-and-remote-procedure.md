# beads-issue-flow: define the jsonl snapshot/export and Dolt-remote procedure for bd 1.x; doctor checks for stale snapshot and missing remote

- Filing action: new issue
- Priority: P2
- Type: task
- Labels: audit, beads-issue-flow
- Parent: epic
- Links: related bento-rdtn.12
- Source findings: bento-16, prior-03 (context)

---BODY---
## Problem

bd 1.1.0 no longer has `bd sync`, but consumer docs still require it: shatter's AGENTS.md mentions it 9 times, including landing step 5. The skills do not say how a tracked `.beads/issues.jsonl` snapshot is refreshed, or how to configure a Dolt remote. As a result:

- shatter's tracked jsonl has been frozen since 2026-09-07 (commit 134dd616). It has 1,733 issues against 1,775 live and about 19 status mismatches. CI drift-patrol and a branch-cleanup script both read it.
- hooks print "post-checkout JSONL import warning: no Dolt remote configured" 26 times.
- one session hand-rolled a "bd sync" commit and pushed it straight to main with --no-verify on 09-06.

## Evidence

- `bd sync --help` gives `Error: unknown command "sync" for "bd"` (bd 1.1.0).
- `bd export` has an `-o` flag. `bd backup sync` exists; confirm what it does before recommending it.

## Current code facts (bento @ 1c0c1e6)

- `catalog/skills/beads-issue-flow/SKILL.md` has no snapshot or Dolt-remote section. Its only related text is "Never read issues.jsonl" and one tracker-sync mention (about line 233).
- `catalog/skills/land-work/SKILL.md` about lines 783-789 treats jsonl as possibly untracked, and says not to commit it.
- bento-rdtn.12 (closed) documented the bd CLI surface but not export or remote setup.

## Acceptance criteria

- beads-issue-flow gains a "Snapshot and remote" section covering:
  - how to tell whether a repo tracks the jsonl;
  - the bd 1.x export command;
  - when to export (once at landing, by land.py or a documented hook);
  - how to check and configure a Dolt remote.
- The doctor warns when a tracked `.beads/issues.jsonl` is older than the newest issue update by more than N days, when the repo docs reference `bd sync`, or when no Dolt remote is configured and the repo docs expect one.
- If land.py is chosen as the exporter, land.py exports and commits the jsonl in the landing commit when the repo opts in via verifier.json.

## Decision left to implementer (with maintainer)

Whether land.py owns the export, or a documented per-repo hook does.

## Out of scope

- Editing shatter's AGENTS.md. That is a separate shatter issue.
