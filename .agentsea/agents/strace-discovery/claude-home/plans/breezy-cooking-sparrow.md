# Fix Demo + E2E Guarantee

## Context

`make demo` fails at step 5 ("Browse Results") because the step expects a
`<table>` but the default view mode is `card`. A secondary bug: step 10 uses
a `data-testid` that doesn't exist on `SampleSearchCard`.

Beyond fixing these bugs, the user wants a structural guarantee: **if e2e tests
pass, the demo will pass.** The strongest form: an e2e spec that imports and
runs the exact same step `action` functions the demo uses.

## Changes

### 1. Fix `web/demo/steps.ts` — two bugs + relative URLs

**Step 5 (Browse Results):** Click the table view toggle before interacting
with the table. Mantine's `SegmentedControl` renders hidden radio inputs with
`value` attributes. Selector: click `input[value="table"]` inside the
`[aria-label="View mode"]` container with `{ force: true }` (input is hidden).
Then wait for `table` to appear.

**Step 10 (Sample Searches):** Uses `[data-testid="sample-search-card"]` which
doesn't exist. Add the attribute to the component (see change 2).

**Relative URLs:** Replace `${BASE_URL}/path` with `/path` and `BASE_URL` with
`/` in all `page.goto()` calls. Export `BASE_URL` for the demo runner.
This makes steps work with Playwright's `baseURL` config.

### 2. Add `data-testid` to `web/src/components/search/SampleSearchCard.tsx`

Add `data-testid="sample-search-card"` to the `<Card>` element (line 47).

### 3. Update `web/demo/run.ts` — use baseURL on context

Import `BASE_URL` from `./steps`. Set `baseURL` on the browser context so
relative `page.goto('/')` calls resolve correctly:

```ts
const context = await browser.newContext({
  viewport: { width: 1280, height: 800 },
  baseURL: BASE_URL,
})
```

### 4. Create `web/e2e/demo.spec.ts` — the guarantee

Single test using `test.step` for sub-step reporting:

```ts
import { test } from '@playwright/test'
import { steps } from '../demo/steps'

test.use({ viewport: { width: 1280, height: 800 } })

test('demo walkthrough', async ({ page }) => {
  test.setTimeout(120_000)
  for (const [i, step] of steps.entries()) {
    await test.step(`${i + 1}. ${step.title}`, async () => {
      await step.action(page)
    })
  }
})
```

This runs the **exact same action functions** the demo uses. If this test
passes, the demo actions are proven to work.

## Files to modify

| File | Action |
|------|--------|
| `web/demo/steps.ts` | Fix step 5 (table toggle), relative URLs |
| `web/demo/run.ts` | Import BASE_URL, set baseURL on context |
| `web/src/components/search/SampleSearchCard.tsx` | Add data-testid |
| `web/e2e/demo.spec.ts` | **New** — e2e spec running demo steps |

## Verification

1. `pnpm build && pnpm lint` — zero errors/warnings
2. `make demo` — all 16 steps complete, browser launches
3. `pnpm test:e2e` (with dev server running) — demo spec passes
