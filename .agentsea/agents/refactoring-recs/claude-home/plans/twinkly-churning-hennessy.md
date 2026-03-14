# Plan: Contract Traceability Model (kapow-pr9.5)

## Context

The contract registry (schemas, examples, README) and gate behavior doc exist, but there's no specification for how automation traces changes back to contracts. The traceability model defines the rules that a future contract traceability checker (kapow-n92.6) will use to determine: "this file changed → these contracts need review."

## Deliverable

Create `docs/specs/contract-traceability.md` — a single spec document.

## Document Outline

1. **Overview** — purpose, cross-references to `contract-gates.md` and `contracts/README.md`
2. **Traceability fields** — how `source_files`, `tests`, `issues`, and `evidence.references` link contracts to artifacts
3. **Change-to-contract mapping** — rules for determining which contracts are affected by a file change:
   - Direct match: changed file appears in a contract's `source_files`
   - Test match: changed file appears in a contract's `tests`
   - Evidence match: changed file in `evidence.references`
   - Pattern matching: glob support for `source_files` (e.g., `api/internal/search/*.go`)
4. **Enforcement model** — how automation uses these rules:
   - On PR/merge: scan changed files → resolve to affected contracts → require contract review/update
   - Contract staleness: `updated` date vs last source file change
   - Status-based rules: `shipped` contracts are enforced; `planned`/`experimental` are advisory
5. **Protected file → contract mapping** — how to determine which files are "protected" (any file referenced by a `shipped` contract)
6. **Traceability chains** — concrete examples:
   - Source change → contract lookup → test verification → evidence
   - New field added → registry.go change → search contract needs update
   - Schema change → migration file → data contract + fixture contract
7. **Automation interface** — what the checker script receives and returns (inputs: list of changed files; outputs: list of affected contracts + required actions)

## Key files to reference

- `contracts/README.md` — field definitions
- `contracts/schema/product-contract.schema.json` — schema with `source_files`, `tests`, `issues` fields
- `contracts/schema/technical-contract.schema.json` — same fields + `dependencies`
- `contracts/examples/product-contract.example.json` — institution-search example
- `contracts/examples/technical-contract.example.json` — search-validation-parity example
- `docs/specs/contract-gates.md` — gate behavior

## Style

Follow existing `docs/specs/` conventions: H1 title, cross-references, tables for structured data, code blocks for examples, numbered lists for procedures.

## Verification

```bash
make test-quick
```
Docs-only change — no code affected.
