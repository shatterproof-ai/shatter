# Plan: Demo Health Checks — Issue Breakdown

## Context

The demo (`web/demo/`) runs Playwright steps but doesn't verify page elements are present. A broken instance can pass all steps silently. Goal: each step declares required elements and reports pass/fail, exiting non-zero in headless mode on failure.

## Issues

### Epic: Demo element health checks

**Type**: epic
**Description**: Make the demo verify that expected page elements are present after each step, reporting results and failing in headless mode when required elements are missing.

---

### Issue 1: Add `ElementCheck` type and `expect` field to `DemoStep`

**Type**: task
**File**: `web/demo/types.ts`
**Dependencies**: none

Add `ElementCheck` interface (`selector`, `label`) and an optional `expect` array to `DemoStep`.

---

### Issue 2: Create `verify()` function

**Type**: task
**File**: `web/demo/verify.ts` (new)
**Dependencies**: blocks-on Issue 1

Implement `verify(page, checks)` that:
- Tests each `ElementCheck` with `page.locator(selector).isVisible({ timeout: 3000 })`
- Logs `[PASS]`/`[FAIL]` per element
- Returns `{ passed, failed, details[] }`

---

### Issue 3: Add `expect` arrays to all demo steps

**Type**: task
**File**: `web/demo/steps.ts`
**Dependencies**: blocks-on Issue 1

Add `expect` to each of the 16 steps — only the elements necessary for that step:

| Step | Expected elements |
|---|---|
| Welcome | `header`, `input[type="search"]`, `button[type="submit"]` |
| Quick Search | `input[type="search"]` |
| Add Filters | state `select`, ranking `select` |
| Submit Search | `table` |
| Browse Results | `table tbody tr` |
| Ranking Blend | `[data-testid="ranking-blend-builder"]` |
| Column Picker | `button:has-text("Columns")` |
| Filter Sidebar | filters heading |
| Pagination | `[data-testid="pagination-controls"]` |
| Sample Searches | `text=Explore Sample Searches` |
| About Page | `header` |
| Tablet View | `header`, `input[type="search"]` |
| Tablet Search | `table` |
| Mobile View | `header` |
| Mobile Search | `table` |
| Demo Complete | `header`, `input[type="search"]` |

---

### Issue 4: Wire verification into run loop and summary

**Type**: task
**File**: `web/demo/run.ts`
**Dependencies**: blocks-on Issue 2, blocks-on Issue 3

After `step.action(page)`, call `verify()`. Log per-step results. Accumulate totals. In headless mode, exit(1) if any check failed. Print summary at end.

---

## Dependency graph

```
Epic
 ├── Issue 1 (types)
 │    ├── Issue 2 (verify fn)  ─┐
 │    └── Issue 3 (step expects) ├── Issue 4 (wire into run loop)
 │                              ─┘
```

## Verification (after all issues complete)

- `pnpm demo:test` against running instance — all PASS
- Stop API, `pnpm demo:test` — FAIL on data-dependent checks, exit(1)
- `pnpm demo` (visual) — check results in console output
