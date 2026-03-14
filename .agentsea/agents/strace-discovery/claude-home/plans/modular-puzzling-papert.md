# E2E Test Coverage — Issue Plan

## Context

Kapow has Playwright infrastructure but only 2 smoke tests. This plan defines a
series of beads issues to build out E2E coverage across two orthogonal dimensions:

1. **Thoroughness tiers** — how deep the tests go
2. **Feature areas** — what functionality is being tested

This lets you run "just the critical smoke tests" across everything, "all tests
for search filtering", or "comprehensive tests for the detail modal" — any
combination.

---

## Dimensions

### Thoroughness Tiers

| Tier | Tag | Purpose | Count |
|------|-----|---------|-------|
| **Critical** | `@critical` | Bare minimum — if these fail, the product is broken. ~1-2 tests per feature. | ~12 tests |
| **Standard** | `@standard` | Solid coverage of main paths and common interactions. | ~20 tests |
| **Comprehensive** | `@comprehensive` | Edge cases, persistence, state recovery, combinations. | ~15 tests |

### Feature Areas

| Feature | Tag | What it covers |
|---------|-----|----------------|
| Search | `@search` | Home quicksearch, name/filter search, result count |
| Filters | `@filters` | Enum, range, text filters; clear; combine; no-results |
| Pagination | `@pagination` | Page nav, page size, count display |
| Sorting | `@sorting` | Column sort, direction toggle, sort + blend interaction |
| Views | `@views` | Card/list/table/map switching, persistence |
| Detail | `@detail` | Institution modal, metrics display, similar institutions |
| Columns | `@columns` | Table column picker, visibility, order, persistence |
| Geo | `@geo` | ZIP resolution, distance display, sort-by-distance |
| Blend | `@blend` | Ranking blend sliders, score sorting, reset |
| Export | `@export` | CSV download, PDF generation |
| Auth | `@auth` | Sign-in UI, user menu, preferences |
| Navigation | `@nav` | Header links, deep links, URL state round-trip |
| Responsive | `@responsive` | Mobile hamburger, filter collapse, viewport layouts |

### Running tests by dimension

```bash
# By tier
pnpm test:e2e --grep @critical        # ~12 tests, <30s
pnpm test:e2e --grep @standard         # ~20 more tests
pnpm test:e2e                          # everything

# By feature
pnpm test:e2e --grep @search
pnpm test:e2e --grep @filters
pnpm test:e2e --grep @detail

# Combined
pnpm test:e2e --grep "(?=.*@critical)(?=.*@filters)"
```

### Test annotation pattern

Each test gets both a tier tag and a feature tag in its title:

```ts
test('@critical @search home search navigates to results', ...)
test('@standard @filters numeric range narrows results', ...)
test('@comprehensive @views view preference persists across reload', ...)
```

---

## Issues

### Issue 1: E2E infrastructure & tagging convention

**Priority**: Prerequisite for all other issues

- Add `webServer` block to `playwright.config.ts` (auto-start dev server, reuse existing)
- Create `e2e/helpers.ts` with shared utilities:
  - `searchWith(page, params)` — navigate to `/search` with URL params
  - `waitForResults(page)` — wait for skeleton to clear, results visible
  - `getResultCount(page)` — parse displayed count
- Document tagging convention (`@tier @feature` in test titles)
- Add `web-test-e2e` and `web-test-e2e-critical` targets to root Makefile
- Wire `web-test-e2e` into `test-full`

### Issue 2: E2E — Search & Filters

**Feature tags**: `@search`, `@filters`

| Tier | Tests |
|------|-------|
| **Critical** | Home quicksearch → results page with results visible; Apply one filter → results change |
| **Standard** | Single-enum filter (state) narrows results; Numeric range filter (enrollment) narrows results; Full-text filter (major/field of study) returns matches; Clear one filter → results widen; Combine two filters from different themes → AND behavior |
| **Comprehensive** | Apply filter that yields zero results → "No colleges match" message + clear button; Clear all filters → full results return; Filter state reflected in URL params; Rapid filter changes don't cause stale results |

### Issue 3: E2E — Pagination & Sorting

**Feature tags**: `@pagination`, `@sorting`

| Tier | Tests |
|------|-------|
| **Critical** | Result count displayed; Click page 2 → different results shown |
| **Standard** | Page size change → correct number of results, resets to page 1; Click sortable column header → results reorder; Sort direction toggles on repeated click |
| **Comprehensive** | Pagination controls hidden when results fit one page; URL reflects page and sort state; Sort disabled when ranking blend is active |

### Issue 4: E2E — Result Views & Detail Modal

**Feature tags**: `@views`, `@detail`

| Tier | Tests |
|------|-------|
| **Critical** | Card view renders institution cards with name/city/state; Click institution → detail modal opens with name and key metrics |
| **Standard** | Switch to list view → list items rendered; Switch to table view → table with headers rendered; Switch to map view → map container rendered; Close detail modal → results still visible |
| **Comprehensive** | View preference persists across page reload (localStorage); Detail modal shows enrollment, tuition, graduation rate; Similar institutions section loads in detail modal |

### Issue 5: E2E — Table Column Customization

**Feature tag**: `@columns`

| Tier | Tests |
|------|-------|
| **Critical** | Table view shows default columns |
| **Standard** | Open column picker → uncheck column → column disappears from table; Check column back → column reappears |
| **Comprehensive** | Column visibility persists across reload (localStorage); Column order changes via picker are reflected in table |

### Issue 6: E2E — Geographic Features

**Feature tag**: `@geo`

| Tier | Tests |
|------|-------|
| **Critical** | Enter valid ZIP → location resolves, distance appears on results |
| **Standard** | "Sort by distance" button appears when location set; Clear location → distance column disappears; Sample search card on home → navigates with correct filters |
| **Comprehensive** | Invalid ZIP handled gracefully; Distance values are reasonable (not NaN/null) |

### Issue 7: E2E — Ranking Blend & Export

**Feature tags**: `@blend`, `@export`

| Tier | Tests |
|------|-------|
| **Critical** | Adjust one blend slider → results reorder |
| **Standard** | Reset blend → returns to default sorting; CSV export downloads a file; PDF export downloads a file |
| **Comprehensive** | Blend weights reflected in URL; Export respects active filters (not all institutions); Multiple blend sliders combine correctly |

### Issue 8: E2E — Auth & Preferences

**Feature tag**: `@auth`

| Tier | Tests |
|------|-------|
| **Critical** | Unauthenticated: "Sign In" button visible in header |
| **Standard** | Sign-in dropdown shows Google option; (If mock auth available) signed-in state shows user menu |
| **Comprehensive** | Preferences round-trip: save home location → reload → location persists |

*Note: Full OAuth flow requires either a test account or mock Supabase setup.*

### Issue 9: E2E — Navigation, Deep Links & Responsive

**Feature tags**: `@nav`, `@responsive`

| Tier | Tests |
|------|-------|
| **Critical** | Navigate to `/search?state=CA` → state filter pre-populated, matching results shown |
| **Standard** | Header nav: Home → About → Home works; Logo click returns to home from any page; Mobile viewport: hamburger menu appears |
| **Comprehensive** | Mobile: filter collapse toggle works; Tablet: layout adapts correctly; All filters round-trip through URL (set filter → copy URL → navigate → same state) |

---

## Issue dependency graph

```
Issue 1 (Infrastructure)
  ├── Issue 2 (Search & Filters)
  ├── Issue 3 (Pagination & Sorting)
  ├── Issue 4 (Views & Detail)
  ├── Issue 5 (Columns)
  ├── Issue 6 (Geo)
  ├── Issue 7 (Blend & Export)
  ├── Issue 8 (Auth)
  └── Issue 9 (Nav & Responsive)
```

All feature issues depend on Issue 1 (infrastructure). Feature issues are
independent of each other — can be worked in any order or in parallel.

---

## Suggested implementation order

1. **Issue 1** — Infrastructure (prerequisite)
2. **Issue 2** — Search & Filters (highest value, exercises the full stack)
3. **Issue 3** — Pagination & Sorting (extends search tests naturally)
4. **Issue 4** — Views & Detail (second most-used feature)
5. **Issue 5** — Columns (small, focused)
6. **Issues 6–9** — Remaining features (any order)
