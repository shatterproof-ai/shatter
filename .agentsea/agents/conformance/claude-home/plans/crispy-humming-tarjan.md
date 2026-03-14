# Plan: flt-1vl — Voice Demo Integration

## Context
The demo walkthrough currently covers boot verification (health, login, browse) but has no voice-note segment. Two seed voice notes already exist ("Q2 Planning Meeting Recap", "Book Recommendation: Designing Data-Intensive Applications"). This task adds a Playwright spec that demonstrates voice capture and verifies voice notes appear in the UI.

## Approach: Hybrid manual/auto

**Manual mode** (`DEMO_MODE=manual`): Navigate to `/capture`, select Voice tab, show the recording UI, pause for human interaction (real recording).

**Auto/headless mode**: Skip actual recording (no mic in CI). Instead, verify the voice capture UI loads correctly, then navigate to `/browse` to find the seeded voice notes and view their detail pages. This proves the voice-note flow end-to-end using seed data.

Real MediaRecorder mocking is fragile and over-engineered for a demo spec. The seed data gives us deterministic voice notes to verify against.

## File to create
`demo/tests/voice-demo.spec.ts`

## Implementation

```typescript
import { test, expect } from '@playwright/test'

const isManual = process.env.DEMO_MODE === 'manual'

// Helper: login with seed credentials
async function login(page) {
  await page.goto('/login')
  await page.getByLabel('Email').fill('ketan@example.com')
  await page.getByLabel('Password').fill('testpassword123')
  await page.getByRole('button', { name: 'Login' }).click()
  await expect(page.getByRole('button', { name: 'Capture' })).toBeVisible()
}

test.describe('Voice Demo', () => {
  test('Navigate to voice capture tab', async ({ page }) => {
    // Login, go to /capture, click Voice tab, verify recording UI visible
    if (isManual) await page.pause()  // human can record
  })

  test('Voice notes visible in browse', async ({ page }) => {
    // Login, go to /browse, verify seeded voice notes appear
    // Look for "Q2 Planning Meeting Recap" or "Book Recommendation"
  })

  test('View voice note detail', async ({ page }) => {
    // Login, browse, click a voice note, verify detail page shows title + type badge
  })
})
```

Key patterns (matching boot-verification.spec.ts):
- `const isManual = process.env.DEMO_MODE === 'manual'`
- `if (isManual) await page.pause()` at showcase points
- Each test self-contained with its own login
- Use `page.getByRole()`, `page.getByText()`, `page.getByLabel()` selectors
- `expect(...).toBeVisible()` for assertions

## Quality gate
```bash
cd demo && npx tsc --noEmit
```
Must pass with zero errors.

## Commit
`Add voice demo integration (flt-1vl)`
