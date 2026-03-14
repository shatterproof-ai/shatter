# Plan: Web Session Restoration (flt-it8.29)

## Context
The web app restores only a raw JWT token from localStorage on startup but never rehydrates the current user via the `me` query. This means after a page refresh, `user` is null despite having a valid token, and auth-required routes (capture, search, browse) are accessible without authentication — they just fail silently at the GraphQL layer.

## Changes

### 1. Update `authStore.ts` — add `me` query rehydration

- Change `initialize()` to be async: if a token exists in localStorage, fire the `me` GraphQL query via the urql client (not a hook — this runs outside React)
- On success: set `{ token, user, loading: false }`
- On failure (expired/invalid token): call `logout()` then set `{ loading: false }`
- Export the `User` type for reuse

**File:** `web/src/stores/authStore.ts`

### 2. Create `MeQuery` in a shared queries file

- Define `const MeQuery = graphql('query Me { me { id email displayName } }')` using gql.tada
- Used by the auth store's initialize function

**File:** `web/src/lib/queries.ts` (new)

### 3. Create `ProtectedRoute` component

- Reads `token`, `loading` from `useAuthStore`
- If `loading` → show Mantine `LoadingOverlay` or `Center` + `Loader`
- If no token → `<Navigate to="/login" replace />`
- If authenticated → render `<Outlet />` (layout route pattern)

**File:** `web/src/components/ProtectedRoute.tsx` (new)

### 4. Update `App.tsx` — wrap auth-required routes

- Import `ProtectedRoute`
- Show a full-page loader while `loading` is true (before routes render)
- Wrap `/search`, `/browse`, `/capture`, `/items/:id` in a `<Route element={<ProtectedRoute />}>` layout route
- Keep `/`, `/login`, `/register` as public routes

**File:** `web/src/App.tsx`

### 5. Tests

- **`authStore.test.ts`**: Test initialize with/without token, mock fetch for me query success/failure
- **`ProtectedRoute.test.tsx`**: Test redirect when not auth'd, render children when auth'd, loading state

**Files:** `web/src/stores/authStore.test.ts`, `web/src/components/ProtectedRoute.test.tsx`

## Key Decisions

- Use urql client directly (not hooks) for the `me` query in the store, since it runs outside React component lifecycle
- `ProtectedRoute` uses React Router's `<Outlet />` pattern for clean nesting
- Loading state blocks the entire app (not per-route) — session check happens once at startup, fast path

## Verification

```bash
cd web && pnpm build && pnpm lint && pnpm test
```
