# Saved Search MVP — Implementation Plan

## Context

Users want to save their current search configuration (filters, sort, view mode, location, ranking blend) and restore it later. The `ugc.saved_search` table already exists in the initial migration with `id`, `team_id`, `name`, `hash` (SHA-256 of query for dedup index), and `query` (JSONB). FK to `subscription.team` and index on `(team_id, hash)` are in place. **No migration needed.**

## Implementation

### 1. Backend Service — `api/internal/savedsearch/service.go` (new)

Follow `preference/service.go` pattern exactly.

```go
type SavedSearch struct {
    ID, Name, Hash, SearchState string
    Created, Updated            time.Time
}

type Service struct { pool *pgxpool.Pool }
func NewService(pool *pgxpool.Pool) *Service

func (s *Service) TeamIDForUser(ctx, userID) (string, error)   // same SQL as preference
func (s *Service) List(ctx, teamID) ([]SavedSearch, error)     // ORDER BY updated DESC
func (s *Service) Save(ctx, teamID, name, searchState) (*SavedSearch, error)
func (s *Service) Delete(ctx, teamID, id) error
```

- `query` JSONB stores `{"s": "<url-params-string>"}` — minimal wrapper for valid JSON
- `hash` = `sha256(searchState)` hex string for the dedup index
- IDs via `gen_random_uuid()::varchar(62)` in SQL
- Delete uses `WHERE id = $1 AND team_id = $2` for ownership check

**Tests** — `service_test.go`:
- Unit: `TestComputeHash` (deterministic)
- Integration: List empty, Save, Save+List, Delete, Delete wrong team (using `testdb.New(t)`)

### 2. GraphQL Schema — `api/graph/schema/saved_search.graphql` (new)

```graphql
type SavedSearch {
  id: String!
  name: String!
  searchState: String!
  createdAt: String!
  updatedAt: String!
}

input SaveSearchInput {
  name: String!
  searchState: String!
}

extend type Query {
  savedSearches: [SavedSearch!]!
}

extend type Mutation {
  saveSearch(input: SaveSearchInput!): SavedSearch!
  deleteSavedSearch(id: String!): Boolean!
}
```

Then `make api-generate`.

### 3. Resolvers — `api/graph/resolver/saved_search.resolvers.go` (auto-generated stubs)

Each resolver:
1. `auth.GetClaims(ctx)` → check non-nil + Subject non-empty
2. `r.SavedSearch.TeamIDForUser(ctx, claims.Subject)` → get team_id
3. Delegate to service method
4. Convert domain type → GraphQL model type

### 4. Wiring

- **`api/graph/resolver/resolver.go`**: Add `SavedSearch *savedsearch.Service` to `Resolver` struct
- **`api/internal/router/router.go`**: Instantiate `savedsearch.NewService(deps.DB)` and inject into Resolver

### 5. Frontend Schema Sync

`make web-schema-sync` → regenerates `web/schema.graphql` + `web/src/graphql-env.d.ts`

### 6. Frontend Store — `web/src/stores/savedSearchStore.ts` (new)

Follow `preferencesStore.ts` pattern with `urqlClient` direct calls:

```ts
// gql.tada queries (MANDATORY)
const SavedSearchesQuery = graphql(`query SavedSearches { ... }`)
const SaveSearchMutation = graphql(`mutation SaveSearch($input: SaveSearchInput!) { ... }`)
const DeleteSavedSearchMutation = graphql(`mutation DeleteSavedSearch($id: String!) { ... }`)

interface SavedSearchState {
  searches: SavedSearchItem[]
  loaded: boolean; loading: boolean
  load: () => Promise<void>
  save: (name: string, searchState: string) => Promise<SavedSearchItem | null>
  remove: (id: string) => Promise<boolean>
  clear: () => void
}
```

**Test** — `savedSearchStore.test.ts`: mock `urqlClient`, test load/save/remove/clear.

### 7. Frontend UI Components

#### `web/src/components/search/SaveSearchButton.tsx` (new)
- If not authenticated: button shows tooltip "Sign in to save searches"
- If authenticated: opens Popover with TextInput for name + Save button
- On save: calls `savedSearchStore.save(name, searchParams.toString())`
- Strips `page`/`pageSize` from saved state (not useful on restore)
- Uses Bookmark icon from lucide-react

#### `web/src/components/search/SavedSearchesDrawer.tsx` (new)
- Mantine Drawer listing saved searches (name, date, delete button)
- Click a saved search → `navigate('/search?' + searchState)`
- Delete button with confirmation → calls store `remove(id)`
- Empty state message when no saved searches

**Tests** for both components using `renderWithMantine()` + mocked store.

### 8. Integration into Existing Files

- **`web/src/pages/Search.tsx`** (line ~819): Add `<SaveSearchButton />` and saved-searches toggle button in the toolbar `<Group>` next to Export buttons
- **`web/src/components/auth/UserMenu.tsx`** (line ~48): Add "Saved Searches" `<Menu.Item>` with Bookmark icon that opens the drawer

### Files Modified (existing)
- `api/graph/resolver/resolver.go` — add SavedSearch field
- `api/internal/router/router.go` — wire service
- `web/src/pages/Search.tsx` — add buttons to toolbar
- `web/src/components/auth/UserMenu.tsx` — add menu item

### Files Created (new)
- `api/internal/savedsearch/service.go`
- `api/internal/savedsearch/service_test.go`
- `api/graph/schema/saved_search.graphql`
- `api/graph/resolver/saved_search.resolvers.go` (gqlgen generates stubs)
- `web/src/stores/savedSearchStore.ts`
- `web/src/stores/savedSearchStore.test.ts`
- `web/src/components/search/SaveSearchButton.tsx`
- `web/src/components/search/SaveSearchButton.test.tsx`
- `web/src/components/search/SavedSearchesDrawer.tsx`
- `web/src/components/search/SavedSearchesDrawer.test.tsx`

## Verification

1. `make api-generate` — gqlgen codegen succeeds
2. `make web-schema-sync` — frontend schema updated
3. `make test-standard` — Go unit tests + TS build + ESLint + web unit tests pass
4. Manual: authenticate, save search, see it in list, restore it, delete it
