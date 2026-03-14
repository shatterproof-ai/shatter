# Plan: flt-bt5 — Capture Round-Trip Demo

## Context
The demo scaffold has boot-verification tests but doesn't demonstrate the core product loop: capture → persist → retrieve. This spec proves that text notes and bookmarks can be created through the web UI and found again via browse/search.

## Implementation

Create **one file**: `demo/tests/capture-round-trip.spec.ts`

### Structure
```
test.describe('Capture Round-Trip')
  - helper: login(page) — reusable login sequence (same as boot-verification)
  - const uniqueId = `demo-${Date.now()}` for unique titles
  - const isManual = process.env.DEMO_MODE === 'manual'

  test 1: Capture a text note and retrieve it
    1. Login
    2. Navigate to /capture
    3. Click "Note" tab
    4. Fill title: `Demo Note ${uniqueId}`, content: `This is a demo note...`
    5. Click "Save Note" — page redirects to /
    6. Navigate to /browse, wait for networkidle
    7. Verify the note title appears (getByText with the unique title)
    8. Click the note card → verify item detail page shows title and content
    9. Manual pause at key points

  test 2: Capture a bookmark and retrieve it
    1. Login
    2. Navigate to /capture
    3. Bookmark tab is default — fill URL: `https://example.com/${uniqueId}`, title: `Demo Bookmark ${uniqueId}`
    4. Click "Save Bookmark" — redirects to /
    5. Navigate to /browse, wait for networkidle
    6. Verify the bookmark title appears
    7. Click the bookmark card → verify detail page shows title and source URL
    8. Manual pause at key points

  test 3: Search retrieves captured items
    1. Login
    2. Capture a note (reuse steps from test 1 with different uniqueId)
    3. Navigate to /search
    4. Type the unique title into search input
    5. Verify the note appears in results
    6. Manual pause
```

### Key selectors (from Capture.tsx and existing spec)
- Login: `getByLabel('Email')`, `getByLabel('Password')`, `getByRole('button', { name: 'Login' })`
- Note tab: `getByRole('tab', { name: /Note/i })`
- Title input: `getByLabel('Title')`
- Content textarea: `getByLabel('Content')`
- URL input: `getByLabel('URL')`
- Save buttons: `getByRole('button', { name: 'Save Note' })`, `getByRole('button', { name: 'Save Bookmark' })`
- Search input: `getByPlaceholder('Search your knowledge base...')`

### Success feedback
Capture success = silent redirect to `/`. So verification is: wait for URL to be `/`, then check item appears on browse page.

## Files
- **Create**: `demo/tests/capture-round-trip.spec.ts`
- **Do NOT modify**: `demo/playwright.config.ts` (per instructions)

## Verification
```bash
cd demo && npx tsc --noEmit
```
Must pass with zero errors.
