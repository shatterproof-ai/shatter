# Plan: Migrate GraphQL queries to gql.tada (kapow-mfp)

## Context

All 12 GraphQL queries across 8 files use raw template strings, bypassing
gql.tada's compile-time schema validation. This caused a production bug where
`Search.tsx` queried `menOnly`/`womenOnly` fields that don't exist in the schema.

gql.tada is installed and configured (`graphql-env.d.ts`) but unused. A strict
`useQuery` wrapper (`src/lib/graphql.ts`) and ESLint rule already exist.

## Approach

**Keep shared types in `types.ts`.** The `SearchField`, `SearchInstitution`,
`SearchOutputField`, `DataSource` etc. interfaces are imported by 18+ files.
Replacing them with gql.tada `ResultOf` types would cascade everywhere. Instead,
keep manual type annotations on `useQuery<ManualType>` — the goal is getting
`graphql()` to validate **field names** at compile time, not eliminating all
manual types.

For each file the pattern is:
1. Add `import { graphql } from 'gql.tada'`
2. Wrap the raw string with `graphql(\`...\`)`
3. Change `useQuery` import from `'urql'` to `'@/lib/graphql'`
4. Keep existing `useQuery<Type>` annotations (shared types stay stable)

## Steps

### Step 1: Simple useQuery files (4 files, 4 queries)

- `src/components/search/useDataSources.ts` — `DataSourcesQuery`
- `src/components/search/LocationInput.tsx` — `ResolveZipQuery`
- `src/components/search/InstitutionDetail.tsx` — `InstitutionDetailQuery`
- `src/components/search/SimilarInstitutions.tsx` — `SimilarInstitutionsQuery`

### Step 2: SampleSearchCard (1 file, 1 query)

- `src/components/search/SampleSearchCard.tsx` — `SAMPLE_SEARCH_QUERY`

### Step 3: Search.tsx (1 file, 3 queries)

- `src/pages/Search.tsx` — `SearchFieldsQuery`, `SearchOutputFieldsQuery`, `SearchQuery`

### Step 4: preferencesStore (1 file, 2 queries — direct urqlClient calls)

- `src/stores/preferencesStore.ts` — `PreferencesQuery`, `UpdatePreferencesMutation`
- `urqlClient.query()` accepts `DocumentInput` which includes `TypedDocumentNode`,
  so wrapping with `graphql()` works without changing the client.

### Step 5: useWebMCP (1 file, 2 queries — custom fetch)

- `src/hooks/useWebMCP.ts` — `SEARCH_FIELDS_QUERY`, `SEARCH_QUERY`
- Change `gqlFetch(query: string, ...)` to accept `DocumentNode`
- Use `print()` from `graphql` (already a direct dependency) to serialize

### Step 6: Enable ESLint rule + cleanup

- Flip `kapow/no-raw-graphql-strings` from `'off'` to `'error'` in `eslint.config.js`
- Optional: clean up dead string branch in `Search.test.tsx` `getOperationName()`

### Verification

After each step: `pnpm build && pnpm lint && pnpm test` (328 tests).
Final: `make test-quick` from root.

## Files modified

| File | Change |
|---|---|
| `src/components/search/useDataSources.ts` | Wrap query, change import |
| `src/components/search/LocationInput.tsx` | Wrap query, change import |
| `src/components/search/InstitutionDetail.tsx` | Wrap query, change import |
| `src/components/search/SimilarInstitutions.tsx` | Wrap query, change import |
| `src/components/search/SampleSearchCard.tsx` | Wrap query, change import |
| `src/pages/Search.tsx` | Wrap 3 queries, change import |
| `src/stores/preferencesStore.ts` | Wrap query + mutation |
| `src/hooks/useWebMCP.ts` | Wrap queries, change gqlFetch signature, add print() |
| `web/eslint.config.js` | Flip rule to `'error'` |
