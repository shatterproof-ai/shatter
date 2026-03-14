# Plan: AGENTS.md Quality Gates (kapow-c0s.5)

## Context

AGENTS.md currently has soft guidance ("pass quality gates") but no explicit, mandatory rules for which commands to run per change type, when contract tests must be updated, or how to handle verification failures. This update makes agents explicitly constrained.

## Changes

**File**: `AGENTS.md` — add 4 new sections after the existing "Completing an Issue" section (before "Git Workflow").

### 1. Quality Gate Commands (mandatory per change type)

Table mapping change type → required commands:

| Change type | Required gate | Command |
|---|---|---|
| Go only | Standard | `make test-standard` |
| Web only | Standard | `make test-standard` |
| Both Go + Web | Standard | `make test-standard` |
| GraphQL schema | Standard + regenerate | `make api-generate && make web-schema-sync && make test-standard` |
| Database migration | Full | `make test-full` (requires DB) |
| Docs only | Quick | `make test-quick` |
| Fixture data | Fixture | `make test-fixture` |

Rules: gate must pass with zero errors/warnings before commit. If gate fails, fix and re-run — do not skip or `--no-verify`.

### 2. Contract Test Rules

When to update `api/internal/search/contract.go` (`ContractCases()`):
- Adding/removing/renaming a search field
- Changing validation logic in `ValidateRequest`
- Adding a new entry point that consumes search requests
- Changing auth requirements for search

All three contract test runners (`search/contract_test.go`, `handler/contract_test.go`, `mcp/contract_test.go`) must pass.

### 3. Protected Surfaces

Surfaces that require extra verification beyond standard gates:
- **Generated files** (`graph/generated/`, `graph/model/`, `web/src/graphql-env.d.ts`) — never edit manually; always regenerate
- **Migration files** — require `make test-full` with live DB; update fixtures if schema changes
- **Search field registry** — changes auto-covered by `TestBuildSQLAllFields`; update contract cases
- **Auth middleware** — changes require integration tests against real JWT validation
- **GraphQL schema** (`.graphql` files) — always run `make api-generate && make web-schema-sync`

### 4. Responding to Verification Failures

Prescriptive steps:
1. Read the error output — identify root cause
2. Fix the code, not the test (unless the test is wrong)
3. Re-run the full gate command — partial re-runs can miss cascading failures
4. If a failure is in generated code, regenerate (`make api-generate` / `make web-schema-sync`)
5. Never use `--no-verify`, `--force`, or skip gates

## Verification

`make test-quick` — docs-only change, quick gate is sufficient.
