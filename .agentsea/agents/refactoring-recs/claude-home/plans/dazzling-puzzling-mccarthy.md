# Plan: Product Contract Acceptance Suite (kapow-4jz)

## Context

Product contracts exist in `contracts/product-contracts.json` with acceptance criteria, and E2E tests exist in `web/e2e/`, but there's no dedicated acceptance test suite that maps 1:1 to product contract promises. The existing E2E tests are organized by feature area (search-filters, pagination-sorting, etc.), not by contract acceptance criteria. This makes it hard to answer: "Is each product promise verified by an executable test?"

The goal is a maintained contract-to-test mapping and a Playwright acceptance suite where each test traces back to a specific contract acceptance criterion.

## Deliverables

### 1. Product contract mapping document: `docs/contracts/product.md`

A human-readable document listing every product contract, its acceptance criteria, and for each criterion: the test file + test name that verifies it, or an explicit "not yet implemented" / "out of scope" marker.

Structure:
```markdown
# Product Contract Acceptance Map

## institution-search (shipped)
| # | Acceptance Criterion | Test | Status |
|---|---|---|---|
| 1 | Search returns results for text queries | acceptance.spec.ts: "text search returns results" | covered |
| 2 | Filters available for all 11 themes | acceptance.spec.ts: "all 11 themes have filters" | covered |
...

## saved-searches (planned)
All criteria: **not yet implemented** (tracked in kapow-5oh)
```

### 2. Playwright acceptance test file: `web/e2e/acceptance.spec.ts`

A single spec file organized by contract ID with tags `@acceptance @critical`. Each `test()` maps to one or more acceptance criteria from `contracts/product-contracts.json`.

**Contract: institution-search** (6 criteria → ~6 tests)
- Text search returns results
- All 11 themes have at least one filter in the sidebar
- Results can be sorted (verify sort control works)
- Geographic radius filter returns results
- Pagination with configurable page size
- URL encodes search state (navigate, verify filters persist from URL)

**Contract: csv-export** (5 criteria → ~3 tests)
- CSV download triggers (verify download attribute on link)
- Filters applied to export (apply filter, verify export URL has params)
- Sort order in export URL

**Contract: user-authentication** (5 criteria → ~3 tests)
- Sign-in buttons visible (Google, GitHub, email)
- User menu visible when authenticated (limited — can't fully test OAuth in E2E)
- Sign-out flow (if testable without real auth)
- Mark OAuth provider tests as "requires manual verification"

**Contract: saved-home-location** (5 criteria → ~3 tests)
- ZIP code entry in preferences modal
- Distance column appears when location set
- Sort by distance available

**Contract: similar-institutions** (4 criteria → ~2 tests)
- Similar institutions section in detail view
- At least 5 similar results shown

**Contract: map-view** (5 criteria → ~3 tests)
- Map view toggle is available
- Map shows markers for results
- View mode persists (set map, reload, verify)

**Contract: saved-searches, college-bookmarks** (planned)
- Explicitly marked as "not yet implemented" with skip annotations

### 3. Makefile target: `make web-test-e2e-acceptance`

Add a target that runs only `@acceptance` tagged tests:
```makefile
web-test-e2e-acceptance:
	cd web && pnpm exec playwright test --grep @acceptance
```

### 4. Update `contracts/product-contracts.json`

Add `web/e2e/acceptance.spec.ts` to the `tests` array of each shipped contract.

### 5. Update `docs/specs/contract-gates.md`

Add the acceptance suite as a product contract gate in the Full tier.

## Key files to modify

- **Create**: `web/e2e/acceptance.spec.ts` — main acceptance test file
- **Create**: `docs/contracts/product.md` — contract-to-test mapping
- **Modify**: `Makefile` — add `web-test-e2e-acceptance` target
- **Modify**: `contracts/product-contracts.json` — add acceptance test references
- **Modify**: `docs/specs/contract-gates.md` — add acceptance gate row

## Reuse

- `web/e2e/helpers.ts` — `searchWith()`, `waitForResults()`, `getResultCount()` utilities
- Existing E2E test patterns for selectors and wait strategies
- Existing Playwright config (no changes needed)

## Verification

1. `make test-quick` — must pass (no Go/TS build changes)
2. The acceptance tests themselves won't run without a dev server, but the test file must compile: `cd web && pnpm exec playwright test --list --grep @acceptance` should list all tests without error
3. Review the mapping doc to confirm every shipped contract criterion has a test or explicit marker
