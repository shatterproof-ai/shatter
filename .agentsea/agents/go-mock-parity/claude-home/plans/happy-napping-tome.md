# Plan: Web App-Shell Parity Tests (kapow-tpt)

## Context

Tests use `renderWithMantine()` which passes `theme={theme}`, but production `main.tsx` uses `<MantineProvider>` **without the theme prop** — so brand colors, fonts, etc. are silently missing in production. This divergence exists because providers are composed separately in `main.tsx` and `test/render.tsx`. The fix: extract a shared provider stack used by both, then add tests that verify the production composition.

## Changes

### 1. Extract shared `AppProviders` component

**New file**: `web/src/providers/AppProviders.tsx`

```tsx
export function AppProviders({ children, router?: 'browser' | 'memory' }) {
  // Wraps children in: MantineProvider(theme) + router
  // UrqlProvider stays in main.tsx (tests mock urql separately)
}
```

Actually, simpler approach — just extract the provider stack into a wrapper:

```tsx
// web/src/providers/AppProviders.tsx
import { MantineProvider } from '@mantine/core'
import { theme } from '../theme'

export function AppProviders({ children }: { children: React.ReactNode }) {
  return <MantineProvider theme={theme}>{children}</MantineProvider>
}
```

This is the minimal shared piece. Router and urql stay where they are since tests need `MemoryRouter` (not `BrowserRouter`) and mock urql.

### 2. Fix `main.tsx` — use `AppProviders`

**File**: `web/src/main.tsx`

Replace `<MantineProvider>` with `<AppProviders>`, which passes the theme. This fixes the production bug.

### 3. Update `test/render.tsx` — use `AppProviders`

**File**: `web/src/test/render.tsx`

Replace the inline `MantineProvider` wrapper with `AppProviders`, ensuring tests use the exact same provider configuration as production.

### 4. Write app-shell parity tests

**New file**: `web/src/providers/AppProviders.test.tsx`

Tests that verify:
- `AppProviders` renders children
- The Mantine theme is wired (brand colors are accessible via `useMantineTheme()`)
- `primaryColor` is `'brand'`
- `theme.colors.brand` and `theme.colors.accent` exist with correct values

These tests catch the class of bug where a provider or its config is missing.

## Files to modify

| File | Action |
|---|---|
| `web/src/providers/AppProviders.tsx` | **Create** — shared provider wrapper |
| `web/src/providers/AppProviders.test.tsx` | **Create** — parity tests |
| `web/src/main.tsx` | **Edit** — use `AppProviders` instead of bare `MantineProvider` |
| `web/src/test/render.tsx` | **Edit** — use `AppProviders` instead of inline wrapper |

## Verification

```bash
cd web && pnpm build && pnpm lint && pnpm test
```

- `pnpm build` — confirms no import/type errors
- `pnpm lint` — zero warnings
- `pnpm test` — new parity tests pass + existing tests unbroken
