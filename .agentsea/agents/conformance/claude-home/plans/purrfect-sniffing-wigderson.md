# flt-it8.15 — Web Search + Browse UI

## Context
The web app has a basic Search page with text input and card results. We need to add type/tag filtering, pagination, an item detail page, and a browse/list page to complete the search + browse experience.

## Files to Modify
- `web/src/pages/Search.tsx` — enhance with filters, sort, pagination
- `web/src/pages/App.tsx` — add new routes
- `web/src/components/layout/Header.tsx` — add Browse nav link

## Files to Create
- `web/src/pages/ItemDetail.tsx` — single item view at `/items/:id`
- `web/src/pages/ItemDetail.test.tsx`
- `web/src/pages/Browse.tsx` — list/grid view at `/browse`
- `web/src/pages/Browse.test.tsx`
- `web/src/pages/Search.test.tsx` — update existing tests
- `web/src/components/ItemCard.tsx` — shared card component (used by Search + Browse)
- `web/src/components/ItemCard.test.tsx`

## Implementation

### 1. Shared ItemCard component (`src/components/ItemCard.tsx`)
Extract the result card from Search.tsx into a reusable component used by both Search and Browse pages.

```tsx
// Props: item data, onClick handler (navigate to detail)
// Shows: title, type badge (color-coded), content preview, tags, date, source
// Clickable → navigates to /items/:id
```

### 2. Enhanced Search page (`src/pages/Search.tsx`)
- Add `ItemFilter` variables to the existing `SearchQuery` (add `filter` param)
- **Type filter**: `Chip.Group` with chips for BOOKMARK, TEXT_NOTE, VOICE_NOTE
- **Tag filter**: collect unique tags from results, show as `Chip.Group` below type filter
- **Sort**: `Select` with options: Relevance (default for search), Date (newest), Date (oldest)
  - Note: `search()` query doesn't have a sort param — sort is only for `items()`. For search, sort client-side or just offer relevance.
- **Load more**: Button at bottom when `hasMore` is true. Increment limit or use offset.
  - Simplest: increase `limit` variable on click (accumulate results)
- Use the shared `ItemCard` component
- Cards link to `/items/:id`

### 3. Item Detail page (`src/pages/ItemDetail.tsx`)
- Route: `/items/:id` — use `useParams()` to get ID
- New GraphQL query: `ItemQuery` fetching full item fields
- Layout:
  - Back button (navigate -1 or to /search)
  - Title + type badge + status badge
  - Content section (contentText or contentUrl as link for bookmarks)
  - Tags section with inline editing via `tagItem` mutation + `TagsInput`
  - Metadata section (dates, source, sensitivity)
  - For bookmarks: clickable sourceUrl
  - For voice notes: transcription text display

### 4. Browse page (`src/pages/Browse.tsx`)
- Route: `/browse`
- GraphQL query: `ItemsQuery` using `items(filter, limit, offset, sort)`
- **View toggle**: `SegmentedControl` with Grid/List options
  - Grid: `SimpleGrid` with `ItemCard` components
  - List: `Stack` with compact `ItemCard` variant
- **Filters** (top bar):
  - Type: `Chip.Group` (same as Search)
  - Sort: `Select` — Created date, Updated date, Title (ASC/DESC)
- **Pagination**: offset-based with Previous/Next buttons
  - Show "Showing X-Y of Z" text
  - Page size: 20

### 5. Routing updates (`src/App.tsx`)
- Add `<Route path="/items/:id" element={<ItemDetail />} />`
- Add `<Route path="/browse" element={<Browse />} />`

### 6. Header update (`src/components/layout/Header.tsx`)
- Add `{ label: 'Browse', to: '/browse' }` to `NAV_LINKS` array

### 7. Tests
Each new component gets a test file following existing pattern:
- Wrap in `UrqlProvider` + `MantineProvider` + `MemoryRouter`
- Test: renders without crashing, key elements present, filter interactions
- Update Search tests for new filter elements

## Key Patterns to Follow
- All GraphQL queries via `graphql()` from `gql.tada`
- `useQuery`/`useMutation` from `@/lib/graphql`
- Mantine style props only (no Tailwind, no hardcoded colors)
- Named exports for all components
- `@/` path alias

## Verification
```bash
cd web && pnpm build && pnpm lint && pnpm test
```
