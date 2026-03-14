# Plan: AGENTS.md Governance Refresh

## Context
AGENTS.md has accumulated stale content: references to `bd sync` (doesn't exist), a large auto-injected beads integration section (lines 132-240) that duplicates what `bd prime` provides at session start, and outdated operational guidance. The goal is to reduce it to a concise quick-reference with stable rules only.

## Changes

### File: `AGENTS.md`

Rewrite to contain only these sections:

1. **Header** — one-liner about bd + `bd onboard`/`bd prime`
2. **Quick Reference** — commands: `bd ready`, `bd show`, `bd update --claim`, `bd close`, `bd prime` (fix `bd sync` → `bd dolt pull`/`bd dolt push`)
3. **Non-Interactive Shell Commands** — keep as-is (stable rule)
4. **Issue Title Guidelines** — keep as-is (stable rule)
5. **Issue Types** — keep as-is (stable rule), consolidate with Creating Issues section
6. **Governance Gates** — NEW section pointing to `ship-gates` and `security-review` skills (`.claude/skills/ship-gates/SKILL.md`, `.claude/skills/security-review/SKILL.md`)
7. **Implementation Plans** — keep as-is (stable rule)
8. **Completing an Issue** — keep but fix `bd sync` reference
9. **Git Workflow** — keep commit conventions, worktree rules
10. **Session Completion Protocol** — keep but fix `bd sync` → `bd dolt pull`/`bd dolt push`

### Remove
- Lines 132-240: entire `<!-- BEGIN BEADS INTEGRATION -->` block (auto-injected by beads hooks, duplicates `bd prime` output)
- `bd sync` references throughout (replace with correct commands)
- Redundant issue type/priority/workflow content that duplicates `bd prime`

## Verification
- Result is valid markdown
- No references to `bd sync`
- All referenced skills exist (ship-gates, security-review)
- No references to non-existent scripts/commands
- File is concise (~100-120 lines vs current ~240)
