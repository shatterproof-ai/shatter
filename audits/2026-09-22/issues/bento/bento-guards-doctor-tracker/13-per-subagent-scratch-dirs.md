---
slug: per-subagent-scratch-dirs
kind: new
title: "code-bloat-sniffer and swarm prompts: give each parallel subagent or teammate its own scratch subdirectory and forbid rm -rf outside it"
priority: P3
type: task
labels: [audit, swarm, skills]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# code-bloat-sniffer and swarm prompts: give each parallel subagent or teammate its own scratch subdirectory and forbid rm -rf outside it

Related: bento-btv (closed; added hygiene rules to the swarm worker prompt, nothing about scratch dirs). Source finding: cli-ux-20 (shatter audit 2026-09-22).

**Scope correction from review.** The audit draft targeted "the audit skill's subagent prompt" and "`proj/` example paths". Neither exists in bento: `catalog/skills/audit/` (SKILL.md, references/, scripts/audit-discover.py) has no subagent prompt and no parallel dispatch, and `git grep 'proj/' -- catalog/skills` finds nothing. The prompt that caused the incident came from the shatter audit workflow script, which is not a bento template. The bento surfaces that do dispatch parallel agents from a prompt template are listed below; this issue covers only those.

## Problem

Parallel subagents inherit the parent session's scratchpad path, so they share one directory. During the shatter audit on 2026-09-22, one reviewer replaced `scratchpad/proj` while another was using it: a later `cp` failed with "cannot stat .../proj/01-arithmetic.ts". An earlier `rm -rf scratchpad/proj` by one reviewer may have deleted another reviewer's files.

Bento templates that fan out work in parallel carry no rule about scratch space:

- `catalog/skills/code-bloat-sniffer/SKILL.md` step 4 (lines 40-54) dispatches one subagent per chunk and lists what each receives; nothing about where it may write. Its tool guide (`references/tools-by-language.md` line 54) tells subagents to "copy `go.sum`/`go.mod` to a scratch dir", so parallel Go chunks can collide on the same scratch path.
- `catalog/skills/swarm/SKILL.md` lines 236-245: the working-hygiene rules every teammate prompt must carry (also required by `CODEX.md` line 42). Teammates get their own worktrees, but nothing covers temp or scratch files outside the worktree.

## Acceptance criteria

- code-bloat-sniffer step 4 adds to what each subagent receives: "write temporary files only under `<scratch>/<chunk-slug>/`, created at start; never `rm -rf` outside it". The Go `go mod tidy` instruction in `tools-by-language.md` uses that per-chunk directory.
- swarm's teammate working-hygiene rules add the same rule, keyed by the teammate's issue id (`<scratch>/<issue-id>/`), so it lands in every teammate prompt.
- A test (in the existing skill-lint or catalog tests) asserts both skills contain the rule text, and fails if either is removed.
- Proof at close: the close note names the test with a failing-then-passing run.

## Out of scope

- Claude Code harness scratchpad behaviour.
- The shatter audit workflow script itself (a shatter or dotfiles concern).
- The audit skill, which dispatches no subagents today.

## Priority / Type / Labels

P3 / task / audit, swarm, skills

## Parent epic

Epic: Audit 2026-09-22 findings (bento)

## Dependencies

None.
