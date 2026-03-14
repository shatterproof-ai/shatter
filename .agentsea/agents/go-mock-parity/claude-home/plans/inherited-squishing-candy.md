# Preferences UI for Location (kapow-0pb)

## Context

The API and Zustand store fully support user preferences (home location, home state), but there's no UI for managing them. Users need a visible way to view/edit/clear their saved preferences.

## Approach

Add a **PreferencesModal** component opened from the UserMenu dropdown. Reuse the existing `LocationInput` component for location entry and `US_STATES` constant for state selection. Wire it to the existing `usePreferencesStore` for persistence.

## Files to Create

### 1. `web/src/components/auth/PreferencesModal.tsx`

Modal component with:
- Location input using existing `LocationInput` component from `@/components/search/LocationInput`
- State selector using Mantine `Select` with `US_STATES` from `@/components/search/constants`
- Save button → calls `preferencesStore.update()` with merged current + new values
- Clear button → calls `preferencesStore.update()` with all nulls to clear server-side, then `preferencesStore.clear()` to reset local state
- Success/error feedback via Mantine `notifications` (check if already configured) or inline `Alert`
- Loading states during save

**Key design decisions:**
- The `update()` store method replaces the full object — so on save, send all 3 fields (lat, lng, state)
- On clear, send `{ homeLatitude: null, homeLongitude: null, homeState: null }` to the API to persist the clear
- Load preferences on modal open (store already has `load()` with dedup)

### 2. `web/src/components/auth/PreferencesModal.test.tsx`

Tests covering:
- Renders with loaded preferences (pre-populated fields)
- Save calls store `update()` with correct values
- Clear calls store `update()` with nulls
- Not rendered / returns null when unauthenticated
- Success feedback shown after save

Use `renderWithMantine` from `@/test/render`, mock `usePreferencesStore` and `useAuthStore`.

## Files to Modify

### 3. `web/src/components/auth/UserMenu.tsx`

Add a "Preferences" menu item between the label section and sign-out:
- Import `useDisclosure` from `@mantine/hooks`
- Add `PreferencesModal` with `opened`/`onClose` props
- Menu.Item with settings icon (lucide `Settings`) that opens the modal

## Reusable Existing Code

| What | Where |
|---|---|
| `LocationInput` component | `web/src/components/search/LocationInput.tsx` |
| `US_STATES` constant | `web/src/components/search/constants.ts` |
| `usePreferencesStore` | `web/src/stores/preferencesStore.ts` |
| `useAuthStore` | `web/src/stores/authStore.ts` |
| `renderWithMantine` | `web/src/test/render.tsx` |
| `Button` wrapper | `web/src/components/ui/button.tsx` |

## Verification

```bash
cd web && pnpm build && pnpm lint && pnpm test
```
