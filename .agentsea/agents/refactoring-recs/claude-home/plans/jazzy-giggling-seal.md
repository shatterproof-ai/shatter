# Plan: kapow-d4v — Saved home location auto-apply

## Context

Authenticated users can save a home location in preferences (via `PreferencesModal`), but the Search page only derives `userLocation` from URL params (`userLat`, `userLng`, `userState`). This means saved preferences never auto-apply to `/search` — users must manually set location every time. The fix makes the search page fall back to saved preferences when URL params are absent.

## Approach

### 1. Modify `userLocation` memo in `Search.tsx` to fall back to saved preferences

**File**: `web/src/pages/Search.tsx` (lines 394-404)

- Import `usePreferencesStore` from `@/stores/preferencesStore`
- Read `homeLatitude`, `homeLongitude`, `homeState`, `loaded` from the store
- In the `userLocation` useMemo:
  - If URL has `userLat`/`userLng` → use URL params (current behavior, URL wins)
  - If URL params absent AND preferences are loaded with valid lat/lng → use saved home location
  - Otherwise → `undefined` (current fallback)
- Add `homeLatitude`, `homeLongitude`, `homeState`, `loaded` to the memo's dependency array

```tsx
import { usePreferencesStore } from '@/stores/preferencesStore'

// Inside Search():
const homeLatitude = usePreferencesStore((s) => s.homeLatitude)
const homeLongitude = usePreferencesStore((s) => s.homeLongitude)
const homeState = usePreferencesStore((s) => s.homeState)
const prefsLoaded = usePreferencesStore((s) => s.loaded)

const userLocation = useMemo(() => {
  // URL params take precedence
  const lat = searchParams.get('userLat')
  const lng = searchParams.get('userLng')
  if (lat != null && lng != null) {
    const latitude = Number(lat)
    const longitude = Number(lng)
    if (!Number.isNaN(latitude) && !Number.isNaN(longitude)) {
      const state = searchParams.get('userState') ?? undefined
      return { latitude, longitude, state }
    }
  }
  // Fall back to saved home location
  if (prefsLoaded && homeLatitude != null && homeLongitude != null) {
    return {
      latitude: homeLatitude,
      longitude: homeLongitude,
      state: homeState ?? undefined,
    }
  }
  return undefined
}, [searchParams, prefsLoaded, homeLatitude, homeLongitude, homeState])
```

### 2. Clearing behavior — no changes needed

`handleLocationChange(undefined)` only deletes URL params (`userLat`, `userLng`, `userState`). It does NOT touch the preferences store. After clearing:
- URL params removed → next render falls back to saved preferences again
- This is actually the desired behavior per acceptance criteria: "Clearing active location does not silently erase saved preferences"

**However**, there's a UX nuance: if the user clears location via the LocationInput, the saved preference will re-apply on next render. To truly "clear" location for the session, we need a way to suppress the fallback. Options:
- Set a `locationCleared` state flag that suppresses fallback until URL params are explicitly set again
- This flag resets on page navigation (component unmount/remount)

I'll add a `locationCleared` ref that gets set when the user explicitly clears location, preventing the auto-apply loop.

### 3. Updated implementation with clear suppression

```tsx
const [locationCleared, setLocationCleared] = useState(false)

const userLocation = useMemo(() => {
  const lat = searchParams.get('userLat')
  const lng = searchParams.get('userLng')
  if (lat != null && lng != null) {
    const latitude = Number(lat)
    const longitude = Number(lng)
    if (!Number.isNaN(latitude) && !Number.isNaN(longitude)) {
      const state = searchParams.get('userState') ?? undefined
      return { latitude, longitude, state }
    }
  }
  // Fall back to saved home location (unless user explicitly cleared)
  if (!locationCleared && prefsLoaded && homeLatitude != null && homeLongitude != null) {
    return {
      latitude: homeLatitude,
      longitude: homeLongitude,
      state: homeState ?? undefined,
    }
  }
  return undefined
}, [searchParams, locationCleared, prefsLoaded, homeLatitude, homeLongitude, homeState])
```

In `handleLocationChange`:
```tsx
const handleLocationChange = useCallback(
  (loc: UserLocationValue | undefined) => {
    if (!loc) setLocationCleared(true)
    else setLocationCleared(false)
    // ... rest of existing logic unchanged
  },
  [setSearchParams],
)
```

### 4. Tests in `Search.test.tsx`

Update the existing mock for `preferencesStore` to support different states, then add tests:

1. **Auto-apply on load**: Mock preferences with saved lat/lng, no URL params → verify `userLocation` variable is passed to search query
2. **URL override**: Set `userLat`/`userLng` in URL AND mock preferences → verify URL values used (not preferences)
3. **Clear behavior**: Simulate clearing location → verify preferences are NOT erased (check that the store's `update`/`clear` was not called) and search works without location

## Files to modify

| File | Change |
|---|---|
| `web/src/pages/Search.tsx` | Import preferences store, add fallback logic + clear suppression |
| `web/src/pages/Search.test.tsx` | Add 3 test cases for auto-apply, URL override, clear |

## Verification

```bash
cd web && pnpm build && pnpm lint && pnpm test
```
