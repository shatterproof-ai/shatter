# Plan: Bugfix Workflow Skill + Pre-Completion Workflow Skill

## Context

Two related issues need project-local Claude agent skills:
- **str-28vd.8**: A `/bugfix` skill that enforces the test-first discipline for bug fixes
- **str-28vd.9**: A `/pre-completion` workflow consolidation — AGENTS.md's completion checklist (lines 191-199) should reference the existing `/pre-completion` skill instead of duplicating guidance

The bug fix policy exists in prose (AGENTS.md lines 178-189) and CLAUDE.md ("Bug fixes require a reproduction test first"), but there's no invocable skill that guides agents through the workflow step by step. The pre-completion skill already exists and is excellent — the issue is about making it the single source of truth.

## Task 1: str-28vd.8 — Bugfix Workflow Skill

### Create `.claude/skills/bugfix/SKILL.md`

A user-invocable skill (`/bugfix`) that enforces:
1. **Phase 1 — Reproduce**: Write a failing test that demonstrates the bug. Run it. Verify it fails for the right reason.
2. **Phase 2 — Fix**: Implement the fix (no test changes allowed in this phase).
3. **Phase 3 — Verify**: Run the test again, verify it passes. Run affected quality gates.
4. **Phase 4 — Pre-completion**: Invoke `/pre-completion` to run full checks.

Frontmatter: `name: bugfix`, `description: ...`, `user-invocable: true`. No tool restrictions (agent needs full access).

The skill should accept an optional argument: the issue key (e.g., `/bugfix str-abc.1`) for commit message formatting.

### Files to create
- `.claude/skills/bugfix/SKILL.md`

## Task 2: str-28vd.9 — Pre-Completion Workflow Consolidation

### Update `AGENTS.md` completion section

The "Completing an Issue" section (lines 191-201) duplicates checklist items that `/pre-completion` already covers. Replace the inline checklist with a reference to the skill.

Keep the high-level policy points (dedicated branch, merge to main, close issue) but remove duplicated quality gate checks and point to `/pre-completion` for verification.

### Files to modify
- `AGENTS.md` (lines ~191-201)

## Verification

1. Confirm skill appears in the catalog: check that `/bugfix` is listed
2. Read through both skills to verify cross-references are consistent
3. Run `cargo test` (sanity — no code changes, but good practice)
4. Commit each task on its own branch per the workflow
