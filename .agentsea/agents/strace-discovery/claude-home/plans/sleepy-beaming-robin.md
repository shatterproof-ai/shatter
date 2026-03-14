# College Bookmarks MVP (kapow-u9s)

## Context

The homepage promises save/compare behavior but no bookmark UX or API exists. The `ugc.saved_institution` table already exists with a unique constraint on `(unit_id, team_id)`, so no migration is needed. This plan adds full-stack bookmark CRUD following the saved search MVP pattern exactly.

## Implementation

### 1. Backend Service — `api/internal/bookmark/service.go`

New package following `savedsearch` pattern. Struct:

```go
type SavedInstitution struct {
    ID, UnitID, Name, City, State string
    Created, Updated              time.Time
}
type Service struct { pool *pgxpool.Pool }
```

Methods:
- `NewService(pool)` — constructor
- `TeamIDForUser(ctx, userID) (string, error)` — same query as savedsearch (`subscription.team_member`)
- `List(ctx, teamID) ([]SavedInstitution, error)` — JOIN with `search_base.institution` for name/city/state, ORDER BY created DESC
- `Add(ctx, teamID, unitID) (*SavedInstitution, error)` — INSERT ON CONFLICT DO UPDATE SET updated=now(), RETURNING with JOIN
- `Remove(ctx, teamID, unitID) error` — DELETE WHERE team_id=$1 AND unit_id=$2
- `IsBookmarkedBatch(ctx, teamID, unitIDs) (map[string]bool, error)` — SELECT unit_id WHERE unit_id = ANY($2)

**File: `api/internal/bookmark/service_test.go`** — integration tests (skip on `-short`/no DB): Add, Remove, List, idempotent Add, team isolation, batch check.

### 2. GraphQL Schema — `api/graph/schema/bookmark.graphql`

```graphql
type BookmarkedInstitution {
  id: String!
  unitID: String!
  name: String!
  city: String!
  state: String!
  createdAt: String!
}

extend type Query {
  bookmarkedInstitutions: [BookmarkedInstitution!]!
  bookmarkedUnitIDs(unitIDs: [String!]!): [String!]!
}

extend type Mutation {
  addBookmark(unitID: String!): BookmarkedInstitution!
  removeBookmark(unitID: String!): Boolean!
}
```

Then `make api-generate`.

### 3. Resolvers — `api/graph/resolver/bookmark.resolvers.go`

- Queries: optional auth (return empty list/slice if anonymous)
- Mutations: required auth (return error if no claims)
- Pattern: `GetClaims(ctx)` → `TeamIDForUser` → service call → map to model

### 4. Wiring

- `api/graph/resolver/resolver.go`: Add `Bookmark *bookmark.Service` to Resolver struct
- `api/internal/router/router.go`: Create `bookmark.NewService(deps.DB)`, wire into resolver

### 5. Schema Sync

`make web-schema-sync` to regenerate `web/schema.graphql` + `web/src/graphql-env.d.ts`.

### 6. Frontend Store — `web/src/stores/bookmarkStore.ts`

```ts
interface BookmarkState {
  bookmarkedIDs: Set<string>
  loaded: boolean
  loading: boolean
  loadAll: () => Promise<void>         // full list for drawer
  checkBatch: (unitIDs: string[]) => Promise<void>  // after search results
  add: (unitID: string) => Promise<boolean>
  remove: (unitID: string) => Promise<boolean>
  isBookmarked: (unitID: string) => boolean
  clear: () => void
}
```

All GraphQL ops via `graphql()` from `gql.tada`. Uses `urqlClient` directly (same as savedSearchStore).

**File: `web/src/stores/bookmarkStore.test.ts`** — unit tests with mocked urqlClient.

### 7. BookmarkButton — `web/src/components/search/BookmarkButton.tsx`

- `ActionIcon` with `Heart` from lucide-react (filled when bookmarked, outline when not)
- Reads from `useBookmarkStore` and `useAuthStore`
- Anonymous: disabled with `Tooltip` "Sign in to bookmark"
- `e.stopPropagation()` on click (cards are clickable)
- Props: `{ unitID: string; size?: 'sm' | 'md' }`

**File: `web/src/components/search/BookmarkButton.test.tsx`**

### 8. BookmarksDrawer — `web/src/components/search/BookmarksDrawer.tsx`

Mantine `Drawer` (right side) following `SavedSearchesDrawer` pattern:
- On open → `loadAll()` which returns full `SavedInstitution` objects (name/city/state)
- Each item: name, city/state, remove button
- Click item navigates to search with detail open
- Empty state message

**File: `web/src/components/search/BookmarksDrawer.test.tsx`**

### 9. Integration into Existing Components

| File | Change |
|---|---|
| `web/src/components/search/InstitutionCard.tsx` | Add `BookmarkButton` top-right corner (absolute positioned) |
| `web/src/components/search/InstitutionDetail.tsx` | Add `BookmarkButton` next to title in modal header |
| `web/src/components/auth/UserMenu.tsx` | Add "Bookmarks" menu item with `Heart` icon |
| `web/src/pages/Search.tsx` | Render `BookmarksDrawer`; call `checkBatch` after results load; handle `?showBookmarks=1` URL param |
| `web/src/components/search/index.ts` | Export new components |

### 10. Key Files to Modify/Create

**Create:**
- `api/internal/bookmark/service.go`
- `api/internal/bookmark/service_test.go`
- `api/graph/schema/bookmark.graphql`
- `api/graph/resolver/bookmark.resolvers.go`
- `web/src/stores/bookmarkStore.ts`
- `web/src/stores/bookmarkStore.test.ts`
- `web/src/components/search/BookmarkButton.tsx`
- `web/src/components/search/BookmarkButton.test.tsx`
- `web/src/components/search/BookmarksDrawer.tsx`
- `web/src/components/search/BookmarksDrawer.test.tsx`

**Modify:**
- `api/graph/resolver/resolver.go` — add Bookmark field
- `api/internal/router/router.go` — wire bookmark service
- `web/src/components/search/InstitutionCard.tsx` — add bookmark button
- `web/src/components/search/InstitutionDetail.tsx` — add bookmark button
- `web/src/components/auth/UserMenu.tsx` — add Bookmarks menu item
- `web/src/pages/Search.tsx` — drawer + batch check
- `web/src/components/search/index.ts` — exports

**Auto-generated (do not edit manually):**
- `api/graph/generated/generated.go`
- `api/graph/model/models_gen.go`
- `web/schema.graphql`
- `web/src/graphql-env.d.ts`

## Verification

1. `make api-generate` — must succeed
2. `make web-schema-sync` — must succeed
3. `make test-standard` — Go unit tests + TS build + ESLint + web unit tests (quality gate)
4. Manual: `make dev` → sign in → bookmark from card → reload → bookmark persists → open drawer → see list → unbookmark
