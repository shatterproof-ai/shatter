# Plan: Search Page Analytics Events (kapow-d1i.5)

## Context
Add custom PostHog analytics events to the Search page to track user behavior: searches, result clicks, and exports. This is part of the PostHog integration epic (kapow-d1i).

## Approach

Follow the existing codebase pattern: import `posthog` and `posthogEnabled` from `@/lib/posthog` (the codebase doesn't use `PostHogProvider`/`usePostHog()` from `posthog-js/react` — all existing PostHog usage goes through the direct `posthog` singleton).

### Files to modify

1. **`web/src/pages/Search.tsx`** — Add 4 analytics events:

   **a) `search_executed`** — Fire in a `useEffect` when search results arrive (when `searchResult.data` changes and is not fetching). Properties:
   - `filter_count`: `Object.keys(filters).length`
   - `result_count`: `total` (from search response)
   - `has_location`: `hasLocation` (boolean)

   **b) `result_clicked`** — Fire in the institution click handlers (card onClick, list/table/map onClickInstitution). Properties:
   - `unit_id`: institution's `unitID`
   - `name`: institution's `name`

   **c) `export_csv`** — Fire in `handleExportCsv`. Properties:
   - `result_count`: `total`

   **d) `export_pdf`** — Fire in `handleExportPdf`. Properties:
   - `result_count`: `institutions.length`

2. **`web/src/pages/Search.test.tsx`** — Add tests verifying:
   - `posthog.capture` is called with correct event name and properties for each event type
   - Events are not fired when `posthogEnabled` is false

### Implementation details

- Add import: `import { posthog, posthogEnabled } from '@/lib/posthog'`
- Guard all captures: `if (posthogEnabled) posthog.capture(...)`
- For `search_executed`: add a `useEffect` that watches `searchResult.data`, `filters`, `hasLocation` — only fires when data is present and not fetching
- For `result_clicked`: extract a helper function `handleInstitutionClick(inst)` that sets state AND captures the event, then use it in all 4 view modes (card, list, map, table)
- For exports: add capture calls at the top of `handleExportCsv` and `handleExportPdf`

### Testing

PostHog is already mocked globally in `web/src/test/setup.ts`. Tests will verify `posthog.capture` calls using `vi.mocked(posthog.capture)`.

## Verification

```bash
cd web && pnpm install && pnpm build && pnpm lint && pnpm test
```
