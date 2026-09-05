---
repo: shatter
type: task
priority: 2
labels: agents, cleanup
existing: none
---
# Triage: does anyone run Codex on this repo? Populate .codex/ and .agents/ accordingly, or delete them

Triage: does anyone run Codex on this repo? The evidence below suggests no; the answer decides which branch is the work.

## Problem
Codex sessions would get AGENTS.md written in Claude tool vocabulary (`Grep`, `Read`, `TeamCreate`, `Monitor`) and no require-worktree equivalent ("enforced by policy only" per `~/dotfiles/codex/AGENTS.md`), while the Codex-facing directories are symlinks or empty. Keeping half-wired Codex surfaces suggests support that does not exist.

## Current code facts
- `.codex/`: symlinks `agents -> ../.claude/agents`, `skills -> ../.claude/skills/`, `swarm-config.md -> ../.claude/swarm-config.md` (2026-03-16), plus an empty `worktrees/` (2026-04-18). No `.codex/AGENTS.md` or hooks.
- `.agents/`: empty directory (2026-06-24).
- `~/.codex/hooks.json` has only tmc; `bento:cross-check` (Claude↔Codex review) is a hard trigger but nothing in the project wires it; 252 session transcripts analysed were all Claude Code.
- AGENTS.md :23-25 (tool names) and the swarm section (:376-449) reference Claude-only tools.

## Options
1. **Support Codex**: add `.codex/AGENTS.md` (or a "Codex" section in AGENTS.md) mapping Claude tool names to Codex equivalents and stating the manual worktree rule; remove or populate `.agents/`; keep the symlinks only if Codex reads them (verify against Codex docs).
2. **Claude-only (proposed default)**: delete `.codex/` and `.agents/`; mark AGENTS.md sections that name Claude tools "Claude Code only" (or drop tool names in a2-38a's rewrite). Rationale: no Codex sessions observed, no Codex hooks, no Codex-specific content in five months.

## Acceptance checks
- Decision recorded; either `.codex/AGENTS.md` exists with the mapping, or both directories are gone and `git grep -n '\.codex\|\.agents/'` returns nothing.

## Scope
In: `.codex/`, `.agents/`, AGENTS.md markers. Out: `.claude/agents/*` (a2-41b); dotfiles-level Codex hooks.

## Size
small.

## Provenance
Audit 2026-09-04, section 11, action item 41; evidence audits/2026-09-04/agent-system.md §1 (project-level surface), §3 (Codex parity), rec 11.
