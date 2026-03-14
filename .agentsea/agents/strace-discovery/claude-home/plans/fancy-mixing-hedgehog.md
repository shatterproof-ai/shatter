# Plan: Sort Options for List and Card View (kapow-4eyw)

## Context

Currently, sorting only works in **table view** (via clickable column headers). Card and list views have no sort controls — only a limited "Sort by distance" button. Users need to sort search results in all views by meaningful fields like enrollment, price, acceptance rate, etc.

The backend already fully supports sorting via `SortInput` in the GraphQL API. The URL params `sort` and `sortDir` already drive sort state. This is primarily a **frontend task** with a small backend tweak for blend score sorting.

## Approach

### 1. New `SortControl` component (`web/src/components/SortControl.tsx`)

A dropdown (`Select`) for sort field + an `ActionIcon` for direction toggle. Placed in the results header bar alongside `ViewToggle`, replacing the existing "Sort by distance" button.

**Sort options** (curated list, not all 32 sortable fields):

| Label | Field value | Default dir | Condition |
|---|---|---|---|
| Alphabetical | `name` | ASC | Always |
| Distance | `distance` | ASC | Only when `hasLocation` |
| Total Enrollment | `enrollment` | DESC | Always |
| Acceptance Rate | `admission_rate` | ASC | Always |
| Total Price | `avg_net_price` | ASC | Always |
| Blend Score | `blend_score` | ASC | Only when `hasBlend` |

**Deferred** (needs more backend work):
- **Relevance** — requires passing full-text search terms into the ORDER BY builder as `ts_rank()`. Non-trivial; will be a separate issue.

**Component API:**
```tsx
interface SortControlProps {
  sort: SortState | undefined
  onSortChange: (field: string, direction: 'ASC' | 'DESC') => void
  onSortClear: () => void
  hasLocation: boolean
  hasBlend: boolean
}
```

**Behavior:**
- Selecting a field applies the sort with its default direction
- Clicking the direction toggle flips ASC↔DESC
- Clearing the select removes the sort (back to default name ASC)
- When blend is active and no explicit sort is chosen, show "Blend Score" as the effective sort

### 2. Backend: Allow sort override when blend is active (`api/internal/search/sql.go`)

Currently `sql.go:60-62` forces `ORDER BY 10 ASC NULLS LAST` when blend is set, ignoring user sort. Change to:
- If blend is active AND sort is nil → default to blend score sort (`ORDER BY 10 ASC NULLS LAST`)
- If blend is active AND sort is set → use the user's sort (blend score is still computed as column 10)
- Add `"blend_score"` as a special case in `buildOrderBy` → `ORDER BY 10 <dir> NULLS LAST/FIRST`

Also update `validate.go` to accept `"blend_score"` as a valid sort field when blend is present.

### 3. Frontend: Update `Search.tsx` sort handling

- Remove the "Sort by distance" button
- Add `SortControl` in the results header `<Group>` (between `ViewToggle` and `ColumnPicker`)
- Modify `handleSort` to accept both field and direction (for dropdown use), keeping the existing cycling behavior for table column headers
- Add a new `handleSortSelect` for the dropdown: sets field + default direction
- Stop nullifying sort when blend is active (currently line ~571 clears sort on blend change) — instead, only clear sort if blend is being activated and no explicit sort was set
- Pass sort to the GraphQL query even when blend is active (remove the `hasBlend ? null : sort` guard)

### 4. Frontend: Update GraphQL query variables (`Search.tsx`)

Change line ~449 from:
```tsx
sort: hasBlend ? null : (sort ?? null)
```
to:
```tsx
sort: sort ?? null
```

The backend now handles blend+sort coexistence.

## Files to modify

| File | Change |
|---|---|
| `web/src/components/SortControl.tsx` | **New** — sort dropdown + direction toggle |
| `web/src/components/SortControl.test.tsx` | **New** — unit tests |
| `web/src/pages/Search.tsx` | Add SortControl, update sort handlers, remove distance button |
| `api/internal/search/sql.go` | Allow sort when blend active; add blend_score special case |
| `api/internal/search/validate.go` | Accept blend_score sort field when blend present |
| `api/internal/search/sql_test.go` | Test blend_score sort, blend+sort coexistence |

## Verification

1. `make test-standard` — full lint + unit tests for both Go and web
2. Manual: card/list views show sort dropdown; selecting a sort field changes results order
3. Manual: direction toggle flips order; URL params update correctly
4. Manual: blend active → blend score sort available; can still sort by other fields
