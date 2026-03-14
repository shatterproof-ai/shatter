# Plan: Contract Gate Documentation (kapow-8ao.3)

## Context

Kapow has a contract test infrastructure (`api/internal/search/contract.go`) that validates
search request handling is consistent across 3 entry points (GraphQL, CSV export, MCP).
There is no documentation describing how these contracts (and other quality gates) should
behave in CI merge/release workflows — when they block, when waivers are allowed, and what
evidence is needed.

## Approach

Create `docs/specs/contract-gates.md` following the existing spec style (H1 title, related
specs, H2/H3 sections, tables for quick reference). The document will cover:

1. **Overview** — what contract gates are and why they exist
2. **Contract types** — product contracts vs technical contracts, with examples
3. **Test tier mapping** — which contracts run at which tier (quick/standard/full)
4. **Merge workflow gates** — what blocks merge, what warns
5. **Release workflow gates** — additional checks before release
6. **Waiver paths** — when waivers are allowed, approval process, evidence requirements
7. **Evidence requirements** — what proof is needed for contract compliance
8. **Adding new contracts** — how to add a new gate

## Files

- **Create**: `docs/specs/contract-gates.md`

## Verification

```bash
make test-quick   # docs-only change, just confirm nothing broke
```
