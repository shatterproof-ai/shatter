# Plan: Web Walkthrough Demo (flt-8wi)

## Context
Someone evaluating Flotsam needs to watch a product tour showing the core read flows end to end. The existing `demo/` scaffold has boot-verification but no walkthrough of the actual product experience.

## What to create
**Single file**: `demo/tests/web-walkthrough.spec.ts`

A sequential Playwright spec that walks through the UI as a logged-in user, verifying seeded data is visible at each step. Follows the patterns in `boot-verification.spec.ts` (manual pause, base URL, selectors).

## Test steps

1. **Home page** — Go to `/`, verify "Your second brain" hero text, feature cards visible
2. **Login** — Fill email/password with seed creds, click Login, verify authenticated state (Capture button)
3. **Browse** — Navigate to `/browse`, wait for items to load, verify known seed titles appear (e.g. "Sourdough bread recipe", "Go Performance Optimization"), verify type badges and tag chips render
4. **Search** — Navigate to `/search`, enter "sourdough" in search input, submit, verify result appears with matching title
5. **Item detail** — Click into an item from search results, verify detail page shows: title, content text, type badge, tags section, metadata (source, captured date)
6. **Tag editing** — On the detail page, click the edit button for tags, verify TagsInput appears, cancel to restore original state

Each step includes `if (isManual) await page.pause()` for human observation.

## Key selectors (from exploration)
- Login: `getByLabel('Email')`, `getByLabel('Password')`, `getByRole('button', { name: 'Login' })`
- Auth confirmation: `getByRole('button', { name: 'Capture' })`
- Browse items: `getByText('Sourdough bread recipe')` etc.
- Search: text input with search icon, submit via Enter key
- Item detail: title as h2, type/status badges, tags section with edit button
- Tag edit: edit button toggles TagsInput, save/cancel buttons

## Quality gate
```bash
cd demo && npx tsc --noEmit
```
Must pass with zero errors.

## Files modified
- **Create**: `demo/tests/web-walkthrough.spec.ts`
- **No changes** to `demo/playwright.config.ts` (per instructions)
