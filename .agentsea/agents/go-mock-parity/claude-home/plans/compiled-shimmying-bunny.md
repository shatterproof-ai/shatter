# Fix: Fields of Study Facet (kapow-n0a9)

## Context

The "fields of study" search facet is non-functional. The backend field exists (`fields_of_study` MultiEnumField in registry.go:882) and SQL generation works (`nces_fos ?| $1::text[]`), but the field is registered with `Options: nil` (open-ended). The API returns `options: []`, so the `MultiSelectCheckboxes` UI component renders nothing to interact with.

The `search_base.cip_data` table (~2000+ rows) has CIP codes with descriptions and credential levels — this is the data source for populating the facet. The Home page also has a free-text "Major" input that sets `fields_of_study` as raw text, which won't work with CIP-code-based filtering.

**Note:** `program_percentage_*` and `program_bachelors_*` fields do NOT exist in the registry — no removal needed.

## Approach

Add a `cipCodes` GraphQL query for search-as-you-type lookup against `cip_data` (Bachelor's only). Build a new `CipCodeSelect` frontend component that queries this endpoint and renders an async multi-select. Wire it into FilterControl for the `fields_of_study` field.

---

## Steps

### 1. Add GraphQL schema for CIP code queries

**File:** `api/graph/schema/search_fields.graphql`

```graphql
type CipCode {
  code: String!
  description: String!
}

extend type Query {
  cipCodes(search: String!, limit: Int): [CipCode!]!
  cipCodesByCode(codes: [String!]!): [CipCode!]!
}
```

- `cipCodes`: search by description or code substring, filtered to Bachelor's level, default limit 20
- `cipCodesByCode`: reverse lookup for restoring selections from URL params

### 2. Regenerate gqlgen code

```bash
cd api && make generate
```

### 3. Implement resolvers

**File:** `api/graph/resolver/search_fields.resolvers.go`

`CipCodes` resolver:
- Return empty if `search` < 2 chars
- Default limit to 20, cap at 50
- SQL: `SELECT DISTINCT code, description FROM search_base.cip_data WHERE level = 'BACHELOR' AND (description ILIKE '%' || $1 || '%' OR code ILIKE '%' || $1 || '%') ORDER BY description LIMIT $2`

`CipCodesByCode` resolver:
- Return empty if `codes` is empty
- SQL: `SELECT DISTINCT code, description FROM search_base.cip_data WHERE level = 'BACHELOR' AND code = ANY($1::text[]) ORDER BY description`

Both use `r.DB.Query(ctx, sql, args...)` with parameterized queries.

### 4. Backend tests

**File:** `api/graph/resolver/search_fields.resolvers_test.go` (new or extend existing)

- Unit test: `CipCodes` returns empty for short search strings
- Unit test: `CipCodes` caps limit at 50
- Unit test: `CipCodesByCode` returns empty for empty codes slice

Note: Integration tests would need a live DB with cip_data loaded — skip for now as the SQL is straightforward and cip_data is a simple lookup table.

### 5. Schema sync

```bash
make web-schema-sync
```

Regenerates `web/schema.graphql` and `web/src/graphql-env.d.ts`.

### 6. Create CipCodeSelect component

**File:** `web/src/components/search/CipCodeSelect.tsx` (new)

Behavior:
- Text input with 400ms debounce (matching existing `DebouncedTextInput` pattern)
- When input >= 2 chars, fires `CipCodesQuery` via `useQuery` with `pause: searchText.length < 2`
- Results shown as scrollable checkbox list (same visual style as `MultiSelectCheckboxes`)
- Selected items displayed as removable tags above the search input
- On mount with existing URL values, fires `CipCodesByCodeQuery` to resolve descriptions
- Emits `{ values: string[] }` via `onChange` (CIP codes only)

Props: `{ field: SearchField, value: unknown, onChange: (value: unknown | undefined) => void, source?: DataSource }`

Uses `graphql()` from `gql.tada` for both queries (mandatory per web/CLAUDE.md).

### 7. Wire into FilterControl

**File:** `web/src/components/search/FilterControl.tsx`

Add dispatch before the generic `ChoiceSearchField` handler (line 375):

```tsx
if (field.__typename === 'ChoiceSearchField' && field.name === 'fields_of_study') {
  return <CipCodeSelect field={field} value={value} onChange={onChange} source={source} />
}
```

### 8. Update Home page major input

**File:** `web/src/pages/Home.tsx`

Remove the free-text "Major" `TextInput` and its state. The fields_of_study filter now requires CIP codes which aren't suitable for a quick-search hero form. The search page sidebar provides the proper CipCodeSelect. Keep the 3-column grid with State and Ranking only (or 2-column).

### 9. Update barrel export

**File:** `web/src/components/search/index.ts`

Add `export { CipCodeSelect } from './CipCodeSelect'`.

### 10. Frontend tests

**File:** `web/src/components/search/CipCodeSelect.test.tsx` (new)

- Renders search input with field label
- Shows results when query returns data (mock useQuery)
- Selecting a result calls onChange with `{ values: [code] }`
- Removing a selected item updates values
- Shows "No results" message when search returns empty

### 11. Quality gate

```bash
make test-standard   # Go unit + TS build + ESLint + web unit tests
```

---

## Files changed

| File | Action |
|---|---|
| `api/graph/schema/search_fields.graphql` | Add CipCode type + 2 queries |
| `api/graph/generated/generated.go` | Regenerated |
| `api/graph/model/models_gen.go` | Regenerated |
| `api/graph/resolver/search_fields.resolvers.go` | Add CipCodes + CipCodesByCode resolvers |
| `api/graph/resolver/search_fields.resolvers_test.go` | Add resolver unit tests |
| `web/schema.graphql` | Regenerated |
| `web/src/graphql-env.d.ts` | Regenerated |
| `web/src/components/search/CipCodeSelect.tsx` | New: async search multi-select |
| `web/src/components/search/CipCodeSelect.test.tsx` | New: component tests |
| `web/src/components/search/FilterControl.tsx` | Add fields_of_study dispatch |
| `web/src/components/search/index.ts` | Export CipCodeSelect |
| `web/src/pages/Home.tsx` | Remove Major text input |

## Verification

1. `make test-standard` passes (Go unit + TS build + lint + web unit tests)
2. Manual: search page shows "Fields of Study" facet with search input, typing "Biology" shows matching CIP codes, selecting one filters results
3. URL contains CIP codes (e.g. `fields_of_study=26.0101`), page reload restores selections with descriptions
