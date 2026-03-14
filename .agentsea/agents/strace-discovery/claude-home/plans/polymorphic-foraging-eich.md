# Plan: Enhance ship-gates skill (flt-it8.32.1)

## Context

The ship-gates skill at `.claude/skills/ship-gates/SKILL.md` exists but is a lightweight skeleton — it lists scripts to run and a general workflow but lacks concrete risk classification tables, per-risk-class gate selection, test existence checks, and structured PASS/FAIL output. The task is to flesh it out into a comprehensive, actionable skill that blocks optimistic closure.

## Approach

Replace the existing `SKILL.md` with an enhanced version that adds:

1. **Risk classification table** — map changed file patterns to risk classes (go-code, web-code, both, schema/graphql, migrations/db, docs-only, config-only) with clear glob patterns
2. **Quality gates per risk class** — specify exact `make` targets to run for each class
3. **Test existence check** — instructions to verify new `.go` files have `_test.go` siblings and new `.tsx` components have `.test.tsx` siblings
4. **Docs alignment check** — verify README/CLAUDE.md/specs reflect actual behavior when behavior changes
5. **Structured PASS/FAIL output format** — template for reporting gate results
6. **Closure blocker rules** — explicit conditions that block issue closure

## Files to modify

- `.claude/skills/ship-gates/SKILL.md` — rewrite with enhanced content

## Verification

- Confirm the file is valid markdown
- Confirm instructions are clear, complete, and reference existing scripts/make targets
- Confirm alignment with `docs/policies/issue-closure.md` and `docs/policies/docs-truthfulness.md`
