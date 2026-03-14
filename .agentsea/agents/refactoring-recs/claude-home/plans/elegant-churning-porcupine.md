# Plan: Contract Registry Schemas (kapow-pr9.1)

## Context

Kapow needs machine-readable contract registries to formalize what the product promises (product contracts) and what the system guarantees (technical contracts). These schemas will be consumed by validation scripts (kapow-n92.3), seeded with data (kapow-pr9.2, pr9.4), and documented (kapow-pr9.3). The existing search contract test matrix in `api/internal/search/contract.go` is a code-level precedent; this task creates the declarative schema layer.

## Approach

Create `contracts/` at repo root with JSON Schema (draft-07) definitions and supporting docs.

### Files to create

1. **`contracts/schema/product-contract.schema.json`** — JSON Schema for product-facing contracts (features, pages, user-visible promises)
2. **`contracts/schema/technical-contract.schema.json`** — JSON Schema for technical/system contracts (API guarantees, search behavior, auth rules)
3. **`contracts/README.md`** — explains the registry, field definitions, statuses, lifecycle
4. **`contracts/examples/product-contract.example.json`** — example product contract entry
5. **`contracts/examples/technical-contract.example.json`** — example technical contract entry

### Schema design

**Shared fields** (both schemas):
- `id` (string, required) — unique identifier, kebab-case
- `title` (string, required) — human-readable name
- `description` (string, required) — what this contract promises
- `status` (enum: `shipped`, `experimental`, `planned`, `deprecated`, required)
- `owner` (string, required) — team or person responsible
- `source_files` (array of strings) — file paths that implement this contract
- `tests` (array of strings) — test file paths or test name patterns
- `issues` (array of strings) — beads issue IDs (e.g., `kapow-4vy`)
- `evidence` (object) — how the claim is verified
  - `type` (enum: `test`, `manual`, `monitoring`, `assertion`)
  - `description` (string)
  - `references` (array of strings) — links to test runs, dashboards, etc.
- `created` (date string)
- `updated` (date string)

**Product contract additions**:
- `category` (enum: `feature`, `page`, `integration`, `data-quality`)
- `user_story` (string) — "As a ... I can ..."
- `acceptance_criteria` (array of strings)

**Technical contract additions**:
- `category` (enum: `api`, `search`, `auth`, `data`, `performance`, `infrastructure`)
- `guarantees` (array of objects) — specific technical guarantees
  - `description` (string)
  - `metric` (string, optional)
  - `threshold` (string, optional)
- `dependencies` (array of strings) — other contract IDs this depends on

### Examples

Product example: "Institution search" — the core search feature from product-overview.md
Technical example: "Search validation parity" — the existing contract test matrix ensuring all entry points validate identically

## Verification

- `make test-quick` passes (no code changes, just docs/config)
- JSON Schema files are valid JSON
- Example files validate against their schemas (can test with `python3 -c` or `node -e` one-liner)

## Files referenced
- `api/internal/search/contract.go` — existing contract test pattern
- `docs/specs/product-overview.md` — product features to reference in examples
