# Plan: Hide 4 facets from frontend sidebar (kapow-hxur, kapow-zktv, kapow-d5d2, kapow-x92u)

## Context

Four search fields are exposed in the frontend facet sidebar but shouldn't be:
- `room_and_board` — UI clutter; remains API-searchable
- `grad_enrollment` — UI clutter; remains API-searchable
- `city` — superseded by state/locale controls; remains API-searchable
- `location` (lat/lng geo proximity) — hidden from UI; API still accepts, but must reject bare lat/long (radius_km ≤ 0)

The codebase already has full infrastructure for this:
- `FieldVisibility` enum: `VISIBLE` / `RESTRICTED` / `HIDDEN` (`api/internal/search/field.go`)
- `IsFieldVisible()` in `api/internal/search/access.go` filters hidden fields from the GraphQL `searchFields` query
- The frontend (`web/src/pages/Search.tsx:457`) already filters `f.visibility !== 'HIDDEN'` before building the sidebar

No GraphQL schema changes are needed — `visibility: FieldVisibility!` is already part of the `SearchField` interface and returned by the resolver.

## Changes

### 1. `api/internal/search/registry.go` — change visibility to `VisibilityHidden`

Four fields, one property change each:

| Field | ~Line | Change |
|---|---|---|
| `city` | 142 | `visibility: VisibilityVisible` → `VisibilityHidden` |
| `location` | 176 | `visibility: VisibilityVisible` → `VisibilityHidden` |
| `grad_enrollment` | 354 | `visibility: VisibilityVisible` → `VisibilityHidden` |
| `room_and_board` | 520 | `visibility: VisibilityVisible` → `VisibilityHidden` |

### 2. `api/internal/search/field.go` — add radius_km > 0 validation in `GeoField.BuildSQL`

In the `GeoFilterKindRadius` case (~line 539), after the nil check on `filter.Radius`, add:

```go
if r.RadiusKm <= 0 {
    return "", nil, fmt.Errorf("radius filter requires radius_km > 0")
}
```

This prevents exact-coordinate matching (lat/lng without a meaningful radius), satisfying kapow-x92u's requirement to "reject bare lat/long queries."

### 3. Tests — `api/internal/search/search_test.go` (or `access_test.go`)

- Update any existing test assertions that expect `city`, `location`, `grad_enrollment`, or `room_and_board` to have `visibility = VISIBLE` — they should now assert `HIDDEN`
- Add a test case in the geo field test table for `radius_km: 0` that expects an error
- Add a test case for `radius_km: -1` that expects an error

### 4. No frontend changes needed

The frontend already filters `f.visibility !== 'HIDDEN'`. Since the 4 fields will now return `HIDDEN` from the API, they will automatically disappear from the sidebar. No React/TS changes required.

## Files to modify

| File | Change |
|---|---|
| `api/internal/search/registry.go` | 4× `visibility: VisibilityHidden` |
| `api/internal/search/field.go` | Add `radius_km > 0` guard in `GeoField.BuildSQL` |
| `api/internal/search/search_test.go` | Update visibility assertions; add geo radius_km validation tests |

## Verification

```bash
# From worktree root:
make api-test-unit && make api-lint
cd web && pnpm build && pnpm lint && pnpm test
```

No DB required — all changes are pure logic, covered by unit tests.
