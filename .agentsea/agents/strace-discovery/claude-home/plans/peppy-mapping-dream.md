# Active Filter Pills at Top of Results

## Context

Users applying multiple filters have no quick visual summary of what's active. They must scroll through the sidebar to see/modify filters. Adding dismissible pills above results provides at-a-glance awareness and one-click removal.

## Implementation

### 1. Create `ActiveFilterPills` component

**File**: `web/src/components/search/ActiveFilterPills.tsx`

A pure presentational component that renders active filters as removable pills.

**Props**:
```tsx
interface ActiveFilterPillsProps {
  filters: Record<string, unknown>      // parsed filter state from URL
  fields: SearchField[]                   // field metadata (for labels, options)
  onRemove: (fieldName: string) => void   // calls handleFilterChange(name, undefined)
  onClearAll: () => void                  // clears all filters
}
```

**Rendering logic**:
- Iterate `Object.keys(filters)`, look up `fields` by name for the label
- Format value as human-readable text using a `formatFilterValue()` helper:
  - `{ query: "..." }` → the query string
  - `{ value: "..." }` → look up `displayName` from field `options`, fall back to `lookupEnumLabel` or raw value
  - `{ values: [...] }` → count, e.g. "3 selected"
  - `{ min, max }` → range string using `formatCellValue` for unit formatting
  - `{ latitude, longitude, radiusMiles }` → "within {r} mi"
- Each pill: Mantine `Badge` (variant `"light"`, color `"brand"`) with a `CloseButton` (from `@mantine/core`) as `rightSection`
- Wrap pills in `<Group gap="xs" wrap="wrap">`
- Show "Clear all" as a small `Button` variant `"subtle"` when 2+ filters are active
- Render nothing if no filters are active

### 2. Integrate into Search page

**File**: `web/src/pages/Search.tsx`

- Import `ActiveFilterPills`
- Place it between the "Results" title row (line ~762) and the results count text (line ~823), inside the results `<Box>`
- Wire props:
  - `filters` — already available
  - `fields` — already available
  - `onRemove` — `handleFilterChange(name, undefined)`
  - `onClearAll` — `() => setSearchParams({}, { replace: true })`

### 3. Add unit test

**File**: `web/src/components/search/ActiveFilterPills.test.tsx`

Test cases:
- Renders nothing when filters empty
- Renders pill with correct "Label: Value" for a text filter
- Renders pill with displayName for choice filter
- Shows "Clear all" when 2+ filters active
- Hides "Clear all" when only 1 filter
- Calls `onRemove` with field name when X clicked
- Calls `onClearAll` when "Clear all" clicked

### 4. Export from barrel

**File**: `web/src/components/search/index.ts`

Add `export { ActiveFilterPills } from './ActiveFilterPills'`

## Key files to modify

| File | Change |
|------|--------|
| `web/src/components/search/ActiveFilterPills.tsx` | New component |
| `web/src/components/search/ActiveFilterPills.test.tsx` | New test |
| `web/src/components/search/index.ts` | Add export |
| `web/src/pages/Search.tsx` | Import + render pills above results |

## Reuse

- `SearchField.options` for enum label lookups
- `lookupEnumLabel` from `@/lib/enumLabels` for code→label mapping
- `formatCellValue` from helpers for unit-aware number formatting
- `renderWithMantine` from `src/test/render.tsx` for tests
- Existing `handleFilterChange` and `setSearchParams` for removal actions

## Verification

```bash
cd web && pnpm build && pnpm lint && pnpm test
```
