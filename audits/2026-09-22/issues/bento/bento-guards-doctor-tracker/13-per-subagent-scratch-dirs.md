---
slug: per-subagent-scratch-dirs
kind: new
title: "swarm/audit prompt templates: give each parallel subagent its own scratch subdirectory"
priority: P3
type: task
labels: [audit, swarm, skills]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# swarm/audit prompt templates: give each parallel subagent its own scratch subdirectory

Related: bento-btv. Source finding: cli-ux-20 (shatter audit 2026-09-22). The verifier noted the prompt that caused the incident came from the shatter audit workflow script; this issue is scoped to bento's own swarm and audit templates, which should carry the rule so generated workflows inherit it.

## Problem

Parallel subagents inherit the parent session's scratchpad path, so they share one directory. During the shatter audit on 2026-09-22, one reviewer replaced `scratchpad/proj` while another was using it: a later `cp` failed with "cannot stat .../proj/01-arithmetic.ts". An earlier `rm -rf scratchpad/proj` by one reviewer may have deleted another reviewer's files. Prompts that use generic names such as `proj/` make collisions likely.

## Current code facts (bento origin/main @ b1bb787)

- bento-btv (closed) added hygiene rules to the swarm worker prompt but said nothing about scratch dirs.
- The swarm skill is `catalog/skills/swarm/` (SKILL.md, references/, scripts/); the audit skill is `catalog/skills/audit/` (SKILL.md, references/, scripts/).

## Acceptance criteria

- The swarm worker prompt template and the audit skill's subagent prompt tell each subagent to write only inside `<scratchpad>/<agent-or-area-name>/`, created at start.
- Both say not to `rm -rf` outside that subdirectory.
- Example paths in both skills use area-specific names, not `proj/`.
- A grep-based test asserts the rule appears in both templates.
- Proof at close: the close note names the test with a failing-then-passing run.

## Out of scope

- Claude Code harness scratchpad behaviour.
- The shatter audit workflow script itself.

## Priority / Type / Labels

P3 / task / audit, swarm, skills

## Parent epic

Epic: Audit 2026-09-22 findings (bento)

## Dependencies

None.
