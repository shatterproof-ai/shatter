# Plan: Agent Workflow Documentation (kapow-c0s.4)

## Context

Coding agents working on Kapow need a single reference document that tells them
the exact workflow from task pickup to merge — including which commands to run,
when to update contracts, and what surfaces require extra care. The information
exists scattered across CLAUDE.md, AGENTS.md, api/CLAUDE.md, web/CLAUDE.md, and
contract-gates.md, but there is no unified step-by-step guide.

## Approach

Create `docs/specs/agent-workflow.md` — a concise, actionable reference that
synthesizes the workflow from existing docs without duplicating their detail
(links back to source docs instead).

## Document outline

1. **Workflow overview** — numbered steps from task pickup to merge
2. **Quality gate commands by change type** — table mapping change scope (Go, Web, Both, GraphQL, DB migration) to required commands
3. **Contract update rules** — when agents must update contract files (specs, CLAUDE.md, tests, fixtures)
4. **Protected surfaces** — list of files/patterns that need extra care (generated files, migrations, fixtures, brand colors, SQL)
5. **Responding to verification failures** — decision tree for handling failures
6. **Commit and merge protocol** — conventions, worktree merging, session completion

## Files to create

- `docs/specs/agent-workflow.md` — the new document

## Files referenced (not modified)

- `CLAUDE.md`, `AGENTS.md` — workflow rules, git conventions
- `api/CLAUDE.md`, `web/CLAUDE.md` — component-specific conventions
- `docs/specs/contract-gates.md` — test tiers and gate behavior
- `Makefile` — verification commands

## Verification

- `make test-quick` — docs-only change, should pass unchanged
