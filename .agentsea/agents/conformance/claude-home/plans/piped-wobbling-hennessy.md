# Plan: Web Shared Bootstrap (kapow-r4q.1)

## Context

The `AppProviders` component already exists at `web/src/providers/AppProviders.tsx` wrapping `MantineProvider` with the production theme. Both `main.tsx` and `test/render.tsx` use it. However, **9 test files** bypass this by using bare `<MantineProvider>` without the theme, creating divergence between test and production provider composition.

## Changes

### 1. Update 9 test files to use `renderWithMantine` or `AppProviders`

Replace bare `<MantineProvider>` usage with `renderWithMantine()` from `@/test/render` (preferred) or wrap with `<AppProviders>` where `renderWithMantine` doesn't fit (e.g., custom render wrappers).

Files to update:
- `src/components/ui/tooltip.test.tsx`
- `src/pages/AuthCallback.test.tsx`
- `src/pages/Search.test.tsx`
- `src/components/search/CipCodeSelect.test.tsx`
- `src/components/search/ColumnPicker.test.tsx`
- `src/components/search/FilterControl.test.tsx`
- `src/components/search/InstitutionDetail.test.tsx`
- `src/components/search/LocationInput.test.tsx`
- `src/components/search/ResultsTable.test.tsx`

Each file: replace `import { MantineProvider } from '@mantine/core'` with `import { AppProviders } from '@/providers/AppProviders'` (or use `renderWithMantine`), and swap the JSX wrapper.

### 2. No structural changes needed

- `AppProviders` already exists and is the shared bootstrap
- `main.tsx` already uses it
- `test/render.tsx` already uses it
- The theme is already wired through `AppProviders`

## Verification

```bash
cd web && pnpm build && pnpm lint && pnpm test
```
