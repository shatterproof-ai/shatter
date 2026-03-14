# Plan: Product Contract Seed Data (kapow-pr9.4)

## Context
The contract registry framework has been created (schema, examples, lifecycle docs). Now we need to populate `contracts/product-contracts.json` with entries for all user-visible features — both shipped and planned — so the registry reflects reality.

## Approach
Create a single file `contracts/product-contracts.json` containing an array of 8 contract entries. Each entry follows the schema in `contracts/schema/product-contract.schema.json`. Source files and tests are linked to actual paths confirmed via codebase exploration.

## Contract Entries

### Shipped (5)
1. **institution-search** — Core search with 48 fields, 11 themes, 4 views. Category: `feature`.
2. **csv-export** — Server-side CSV export via `/api/export/csv`. Category: `feature`.
3. **user-authentication** — Supabase OAuth (Google, GitHub, email). Category: `integration`.
4. **saved-home-location** — Authenticated users save home lat/lng/state for distance + My Price. Category: `feature`.
5. **similar-institutions** — Find similar schools by profile metrics. Category: `feature`.

### Shipped (1, could be argued experimental but has full impl + tests)
6. **map-view** — Leaflet map view of search results. Category: `feature`.

### Planned (2)
7. **saved-searches** — Save/restore search configurations. Status: `planned`. Issue: `kapow-5oh`.
8. **college-bookmarks** — Bookmark institutions for later. Status: `planned`. Issue: `kapow-u9s`.

## Key Files
- **Create**: `contracts/product-contracts.json`
- **Reference**: `contracts/schema/product-contract.schema.json`, `contracts/examples/product-contract.example.json`
- **Do NOT touch**: `contracts/technical-contracts.json` (another teammate)

## Source File Mappings (confirmed existing)

| Contract | Source Files | Tests |
|---|---|---|
| institution-search | `api/internal/search/registry.go`, `api/internal/search/field.go`, `web/src/pages/Search.tsx`, `api/graph/schema/search.graphql` | `api/internal/search/search_test.go`, `web/src/pages/Search.test.tsx`, `web/e2e/search-filters.spec.ts` |
| csv-export | `api/internal/handler/export.go`, `web/src/pages/Search.tsx` | `api/internal/handler/export_test.go`, `web/e2e/blend-export.spec.ts` |
| user-authentication | `web/src/stores/authStore.ts`, `web/src/components/auth/SignInMenu.tsx`, `api/internal/auth/jwt.go`, `api/internal/middleware/auth.go` | `api/internal/auth/jwt_test.go`, `web/src/stores/authStore.test.ts`, `web/e2e/auth-prefs.spec.ts` |
| saved-home-location | `web/src/stores/preferencesStore.ts`, `web/src/components/auth/PreferencesModal.tsx`, `api/internal/preference/service.go`, `api/graph/schema/preferences.graphql` | `web/src/stores/preferencesStore.test.ts`, `api/internal/preference/service_test.go`, `web/e2e/auth-prefs.spec.ts` |
| similar-institutions | `web/src/components/search/SimilarInstitutions.tsx`, `api/internal/search/similarity.go`, `api/graph/schema/similarity.graphql` | `api/internal/search/similarity_test.go`, `web/src/components/search/SimilarInstitutions.test.tsx` |
| map-view | `web/src/components/search/ResultsMap.tsx`, `web/src/components/search/ViewToggle.tsx` | `web/src/components/search/ResultsMap.test.tsx`, `web/e2e/views-detail.spec.ts`, `web/e2e/geo-features.spec.ts` |
| saved-searches | (none — planned) | (none) |
| college-bookmarks | (none — planned) | (none) |

## Verification
- Validate JSON against schema: `npx ajv validate -s contracts/schema/product-contract.schema.json -d contracts/product-contracts.json` (or manual review)
- `make test-quick` — config-only change, no code affected
