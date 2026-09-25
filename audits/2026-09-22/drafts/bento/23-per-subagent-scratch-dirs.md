# swarm/audit prompt templates: give each parallel subagent its own scratch subdirectory

- Filing action: new issue (verifier noted the prompt that caused the incident came from a workflow script; bento owns the swarm and audit prompt templates that should carry the rule)
- Priority: P3
- Type: task
- Labels: audit, swarm, skills
- Parent: epic
- Links: related bento-btv
- Source findings: cli-ux-20

---BODY---
## Problem

Parallel subagents inherit the parent session's scratchpad path, so they share one directory. During the shatter audit on 2026-09-22, one reviewer replaced `scratchpad/proj` while another was using it: a later `cp` failed with "cannot stat .../proj/01-arithmetic.ts". An earlier `rm -rf scratchpad/proj` by one reviewer may have deleted another reviewer's files. Prompts that use generic names such as `proj/` make collisions likely.

## Current code facts

- bento-btv (closed) added hygiene rules to the swarm worker prompt but said nothing about scratch dirs.
- The swarm worker prompt template lives in `catalog/skills/swarm/`, and the audit skill in `catalog/skills/audit/`.

## Acceptance criteria

- The swarm worker prompt template and the bento audit skill's subagent prompt tell each subagent to write only inside `<scratchpad>/<agent-or-area-name>/`, created at start.
- They say not to `rm -rf` outside that subdirectory.
- Example paths in both skills use area-specific names, not `proj/`.
- A grep test asserts the rule appears in both templates.

## Out of scope

- Claude Code harness scratchpad behaviour.
