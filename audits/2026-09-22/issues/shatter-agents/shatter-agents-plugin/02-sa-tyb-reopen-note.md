---
slug: sa-tyb-reopen-note
kind: reopen-note
title: "Comment on closed sa-tyb: only the skill text landed; the documented command does not exist"
priority: P1
type: note
labels: [audit-2026-09-22]
parent_epic: ""
blocked_by: []
existing_id: sa-tyb
tracker: "bd in /home/ketan/project/shatter-agents (prefix sa)"
---

# Comment on closed sa-tyb

Target: **sa-tyb** ("shatter diff: first-class diff-scoped exploration command"), CLOSED with the reason "a850d93b0f2d012143f88e12eaeb021cd8c8c4b5 landed on main". Post the comment below. Do not reopen: the follow-up work is tracked in the new issue.

## Comment text

Audit 2026-09-22 (finding plugins-01): this issue was closed when the `shatter-diff` skill text landed, but the command the skill documents does not exist. `shatter diff --staged` fails with `error: unexpected argument '--staged' found`, and `Usage: shatter diff [OPTIONS] <SNAPSHOT> <CURRENT>` exits 2. `shatter diff-explore` is an unrecognized subcommand. As a result, the skill's pre-commit hook recipe (`catalog/skills/shatter-diff/SKILL.md:122-136`) aborts every commit.

Current state:
- The engine work for diff-scoped exploration is shatter epic **str-81xiw** (open). It has not shipped.
- On 2026-09-23 the shatter maintainer decided to retire the snapshot-comparison `shatter diff`. `shatter spec-diff` is the supported regression tool. Once the retirement lands, the `diff` name is free, and str-81xiw decides what the diff-scoped command is called.

Follow-up: **<withdraw-shatter-diff-skill id>** withdraws the skill and its hook recipe from the published payload and points readers to `shatter spec-diff`. A new skill for diff-scoped exploration should be filed once str-81xiw ships a command, and should be closed only after the skill's commands have been run against a real build.
