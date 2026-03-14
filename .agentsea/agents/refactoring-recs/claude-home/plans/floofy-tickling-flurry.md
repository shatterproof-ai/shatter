# Fix: Demo Step 14 Mobile View — "Site header" Assertion Timing

## Context

Issue `kapow-1kfj`: Demo step 14/16 (Mobile View) fails the "Site header" assertion
(`selector: 'header'`). The `<header>` element is visually present on mobile, confirmed
by screenshot. The failure is a timing issue: `page.goto()` with `waitUntil: 'networkidle'`
returns before the browser finishes re-laying out the page for the newly set 375×812
viewport, so the `header` element is not visible when `verify()` polls it.

## Root Cause

Step 14 action sequence:
1. `page.setViewportSize({ width: 375, height: 812 })`
2. `page.goto('/', { waitUntil: 'networkidle' })`
3. `page.waitForTimeout(500)` — 500ms is not reliably enough after a viewport resize + navigation

`verify()` then calls `page.locator('header').isVisible({ timeout: 3000 })`.
The 3s timeout in `verify()` _should_ be enough, but if `networkidle` resolves before
mobile layout is computed (especially in headless CI), the `header` can remain non-visible
or outside the visual viewport.

## Fix

**File:** `web/demo/steps.ts` — step 14 action function (lines ~339-363)

Add `await page.waitForSelector('header', { state: 'visible' })` after the `goto`/`waitForTimeout`
block, before the burger menu interaction. This is an explicit, deterministic wait that blocks
until the `<header>` element is in the visible state, replacing the implicit guess of 500ms.

```typescript
// After:
await page.goto('/', { waitUntil: 'networkidle' })
await page.waitForTimeout(500)

// Add:
await page.waitForSelector('header', { state: 'visible' })
```

This pattern is minimal and targeted — it doesn't remove the existing `waitForTimeout(500)` (preserves visual demo pacing) but adds a reliable gate before the burger-menu interaction and the subsequent `verify()` call.

## Critical Files

- `web/demo/steps.ts` — step 14, lines ~339-363 (action function)
- `web/demo/verify.ts` — verify logic (read-only reference, no changes needed)
- `web/demo/run.ts` — runner (read-only reference, no changes needed)

## Implementation Steps

1. Open `web/demo/steps.ts`
2. In step 14's action function, after `await page.waitForTimeout(500)`, add:
   ```typescript
   await page.waitForSelector('header', { state: 'visible' })
   ```
3. Commit: `fix: wait for header visibility before mobile view assertions in demo step 14`

## Verification

```bash
cd web
pnpm build   # must pass with zero errors
pnpm lint    # must pass with zero warnings
pnpm test    # unit tests must pass
```

Note: `pnpm demo:test` requires a browser and running dev server — cannot run in worktree CI.
The build + lint + test gates are sufficient for this change.
