# Plan: Contract Lifecycle Documentation (kapow-pr9.3)

## Context

The Kapow project has a contract testing system that ensures search validation behaves consistently across all entry points (GraphQL, CSV export, MCP). The system is documented piecemeal across AGENTS.md (contract test rules), docs/specs/contract-gates.md (gate behavior), and the source code itself (api/internal/search/contract.go). There is no single document explaining the full lifecycle of contract entries — how they're created, what each field means, how they progress through statuses, who owns them, and how they relate to tests and issues.

## Approach

Create `docs/specs/contract-lifecycle.md` following the existing docs/specs/ style (see contract-gates.md and search-engine.md as templates). The document will consolidate and extend the scattered contract knowledge into a single lifecycle reference.

## File to create

**`docs/specs/contract-lifecycle.md`** — covering:

1. **Purpose** — why contracts exist (product correctness + technical consistency)
2. **Anatomy of a contract entry** — `ContractCase` struct field definitions with explanations
3. **Status lifecycle** — planned → experimental → shipped → deprecated, with transition criteria
4. **Creating a new contract entry** — step-by-step with code example
5. **Updating an existing entry** — when and how
6. **Review expectations** — when contracts need review
7. **Relationship to source files, tests, and issues** — traceability model
8. **Ownership model** — who owns what
9. **Retirement process** — deprecation and removal

## Key source files

- `api/internal/search/contract.go` — ContractCase struct and ContractCases() function
- `api/internal/search/contract_test.go` — baseline test runner
- `api/internal/handler/contract_test.go` — CSV export test runner
- `api/internal/mcp/contract_test.go` — MCP test runner
- `docs/specs/contract-gates.md` — gate behavior (complementary doc)
- `AGENTS.md` lines 155-179 — contract test rules

## Verification

- `make test-quick` — docs-only change, should pass unchanged
- Manual review that doc cross-references are accurate
