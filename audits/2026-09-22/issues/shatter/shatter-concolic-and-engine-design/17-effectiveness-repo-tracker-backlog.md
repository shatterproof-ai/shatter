---
slug: effectiveness-repo-tracker-backlog
kind: new
title: "If the effectiveness benchmark lives in shatter-effectiveness: initialize a tracker there and file the remaining plan tasks"
priority: P3
type: task
labels: [audit-2026-09-22, effectiveness, tracker]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [effectiveness-benchmark-holdout]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# If the effectiveness benchmark lives in shatter-effectiveness: initialize a tracker there and file the remaining plan tasks

## Problem

`~/project/shatter-effectiveness` holds a 13-task implementation plan (`docs/superpowers/plans/2026-08-30-effectiveness-benchmark.md`) but no tracker, so the plan's remaining tasks are invisible to any issue-driven workflow. effectiveness-benchmark-holdout decides where the benchmark lives and delivers its first slice. If that decision is shatter-effectiveness, the remaining work needs a home there. If the decision is shatter, this issue is closed as not applicable with a link to the decision.

## Acceptance criteria

- [ ] If the location decision (recorded in effectiveness-benchmark-holdout) is **shatter**: this issue is closed as not applicable, linking the decision. Otherwise:
- [ ] A tracker (bd or GitHub Issues) is initialized in shatter-effectiveness, and its repo guidance says which tracker it uses.
- [ ] Each plan task not delivered by effectiveness-benchmark-holdout is filed there, one issue per task, each with acceptance criteria copied or adapted from the plan and a link back to the plan section. The close note lists the plan task numbers and the new issue IDs, and names every plan task deliberately not filed, with the reason.
- [ ] The delivered first slice is recorded there as done, with a link to its result file.

## Out of scope

- Implementing the plan tasks.
- holdout (holdout-disposition).

## Metadata

- Priority: P3
- Type: task
- Labels: audit-2026-09-22, effectiveness, tracker
- Parent epic: Epic: Audit 2026-09-22 findings
- Blocked by: effectiveness-benchmark-holdout
- Source findings: goals-10 (split from draft other-first-party/50)
