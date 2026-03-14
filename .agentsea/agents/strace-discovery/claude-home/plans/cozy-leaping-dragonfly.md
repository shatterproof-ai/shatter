# Application Platform Scraper — Implementation Plan

## Context

The admissions importer was originally designed around hardcoded institution
lists verified against NCES data (`kapow-jsuj`). NCES data lags significantly,
so we're pivoting to **web scraping** — pulling membership lists directly from
each application platform's website. This also expands coverage from 3 platforms
to 14.

The existing importer infrastructure (registry, namematch, dbutil, fetch) is
mature and ready. namematch Tier 1 + Tier 2 are already implemented. The main
new work is the scraping layer and per-platform extractors.

---

## Architecture

```
tools/kapow/
  internal/
    scraper/           # NEW — chromedp browser + result caching
      browser.go       # Browser lifecycle, Navigate, EvalStrings, retry
      cache.go         # JSON file cache with date stamps + staleness
    platform/          # NEW — per-platform extractors
      platform.go      # Extractor interface, All() registry, enum constants
      commonapp.go     # Common App (~1100 schools, JS SPA)
      coalition.go     # Coalition/Scoir (~150 schools)
      cbca.go          # Common Black College App (~50-70 HBCUs)
      questbridge.go   # QuestBridge (~55 schools, server HTML)
      applytexas.go    # ApplyTexas (~90 schools)
      applysuny.go     # Apply SUNY (~60 schools)
      applyidaho.go    # Apply Idaho (~10 schools, React SPA)
      gafutures.go     # GAfutures (~48 GA schools, server HTML)
      unc.go           # NC College Connect (~17 UNC schools)
      wisconsin.go     # Wisconsin UW system (~13 campuses)
      static.go        # UC (9), CSU (23), Florida SUS (12) — hardcoded
  admissions.go        # NEW — Importer implementation
```

**Data flow:**
```
Platform extractors  →  map[platformKey][]string{names}
        ↓
  namematch.Match()  →  map[unitID][]string{platformKeys}
        ↓
  JSON marshal       →  {"applications": ["Common", "Coalition", ...]}
        ↓
  dbutil.UpsertAdmissions (replace semantics, not merge)
```

**Key decisions:**
- **chromedp** (pure Go, no external deps beyond Chrome/Chromium) for all scrapers uniformly
- **Single `admissions` importer** orchestrating all platform extractors
- **Replace semantics** for admissions JSONB (not merge — dropped platforms must disappear)
- **Cache scraped results** as JSON in `{cacheDir}/admissions/{platform}-{date}.json`
- **Individual scraper failures don't block others** — log warning, continue

---

## Platforms (14 total)

### National (5)

| Key | Label | ~Count | Source | Method |
|---|---|---|---|---|
| `Common` | Common App | 1,100 | commonapp.org/explore | JS SPA scrape |
| `Coalition` | Coalition App | 150 | coalitionforcollegeaccess.org | AJAX/HTML scrape |
| `CBCA` | Common Black College App | 50-70 | commonblackcollegeapp.com/schools | JS scrape |
| `QuestBridge` | QuestBridge | 55 | questbridge.org/partners/college-partners | Server HTML |
| ~~Universal~~ | ~~Universal College App~~ | ~~1~~ | — | **Dropped (defunct)** |

### State-Specific (9)

| Key | Label | ~Count | State | Method |
|---|---|---|---|---|
| `ApplyTexas` | ApplyTexas | 90 | TX | Scrape (403 risk) |
| `ApplySUNY` | Apply SUNY | 60 | NY | Scrape (403 risk) |
| `UCApplication` | UC Application | 9 | CA | Static list |
| `CalStateApply` | Cal State Apply | 23 | CA | Static list |
| `UWSystem` | Universities of Wisconsin | 13 | WI | Server HTML |
| `ApplyIdaho` | Apply Idaho | 10 | ID | React SPA scrape |
| `GAFutures` | GAfutures | 48 | GA | Server HTML |
| `CFNC` | NC College Connect | 17 | NC | Server HTML |
| `FloridaSUS` | Florida SUS | 12 | FL | Static list |

---

## Issue Breakdown (1 epic + 13 issues)

### Epic: Application platform scrapers
> Scrape membership lists from 14 college application platforms, match to DB
> institutions via namematch, and write admissions JSONB. Replaces the
> hardcoded-list approach in kapow-jsuj.

### Phase 0 — Foundation (parallel, no deps)

**1. Scraper infrastructure (chromedp + cache)**
- Type: feature, P0
- Deps: none
- Create `internal/scraper/` with `Browser` (chromedp allocator, Navigate,
  EvalStrings, Close, retry/timeout) and `Cache` (JSON save/load with date
  stamps, staleness check via `--max-cache-age`). Add `chromedp` to go.mod.
- Tests: Cache unit tests (round-trip, staleness, missing file). Browser
  guarded by build tag `integration`.

**2. Platform extractor interface**
- Type: task, P0
- Deps: none
- Create `internal/platform/platform.go`: `Extractor` interface
  (`Key() string`, `Label() string`, `Extract(ctx, browser) ([]string, error)`),
  enum key constants, `All()` registry function.
- Tests: `All()` returns 14 extractors with unique keys.

**3. Admissions upsert (replace semantics)**
- Type: task, P0
- Deps: none
- Add `SetAdmissions(ctx, pool, unitID, admissionsJSON)` to `internal/dbutil`.
  Uses `UPDATE ... SET admissions = $2` (replace, not COALESCE merge).
- Tests: Unit test for SQL structure. Integration test confirms replace.

**4. API enum update**
- Type: feature, P0
- Deps: none
- In `api/internal/search/registry.go`: remove "Universal College Application",
  add 11 new `EnumOption` entries to `application_platforms` field. Rename
  `SourceCommonApp` → `SourceApplicationPlatforms` in `datasource.go`, update
  ID/name/description. Run `make web-schema-sync`.
- Tests: `TestBuildSQLAllFields` passes. Build + lint clean.

### Phase 1 — Core extractors (deps: issues 1, 2)

**5. Static platform lists**
- Type: task, P1
- Deps: 2
- Implement `static.go` with hardcoded slices for UC Application (9), Cal
  State Apply (23), Florida SUS (12). Names must match NCES canonical names
  or existing namematch aliases.
- Tests: Count assertions, spot-check names.

**6. Common App scraper**
- Type: task, P1
- Deps: 1, 2
- Navigate commonapp.org/explore, handle JS SPA render, extract institution
  names. Handle pagination/infinite scroll. Expected: 1000+ names.
- Tests: HTML fixture extraction test. Timeout handling.

**7. Coalition/Scoir scraper**
- Type: task, P1
- Deps: 1, 2
- Scrape Coalition member list. Try jQuery AJAX endpoint directly; fall back
  to DOM scrape. Expected: 100+ names.
- Tests: HTML fixture test.

**8. QuestBridge + CBCA scrapers**
- Type: task, P1
- Deps: 1, 2
- QuestBridge: server-rendered HTML at questbridge.org/partners/college-partners (~55).
  CBCA: commonblackcollegeapp.com/schools, likely JS (~50-70).
- Tests: HTML fixture tests for both.

### Phase 2 — State platform scrapers + orchestrator

**9. State platform scrapers (6 extractors)**
- Type: task, P2
- Deps: 1, 2
- ApplyTexas, Apply SUNY, Apply Idaho, GAfutures, NC/UNC, Wisconsin.
  Each in its own file. Mix of server HTML and JS SPA.
- Tests: HTML fixture tests for server-rendered ones (GAfutures, UNC,
  Wisconsin). Smoke tests for JS ones.

**10. Admissions importer orchestrator**
- Type: feature, P1
- Deps: 1, 2, 3, 5 (static lists at minimum)
- Create `admissions.go` implementing `Importer` interface. Name: `"admissions"`.
  Flow: load namematch → run all extractors (parallel where possible, respect
  cache) → match names → aggregate per-institution → build JSONB → upsert.
  Flags: `--max-cache-age` (default 7d), `--require-all`, `--platforms` (CSV subset).
- Tests: Unit test with mock extractors. Integration test with fixture DB.

### Phase 3 — Quality + polish

**11. Match reporting + fuzzy suggestions**
- Type: task, P2
- Deps: 10
- After scraping, produce `match-report-{date}.json`: per-platform match
  rate, unmatched names with top 3 fuzzy candidates + scores. Log summary
  stats. This informs alias additions.
- Tests: Report generation with mock data.

**12. Integration test (end-to-end)**
- Type: task, P2
- Deps: 10, 3
- Load fixture data → run admissions importer with mock browser (HTML
  fixtures) → verify JSONB in DB. Test partial failure (one extractor fails,
  others succeed). Guarded by `testing.Short()` + `DATABASE_URL`.

**13. Namematch alias additions**
- Type: task, P3
- Deps: 11 (informed by live match report)
- Run admissions importer against live DB, analyze report, add builtin
  aliases to `namematch/aliases.go` for unmatched institutions. Target:
  Common App > 95% match rate, static lists 100%.

---

## Critical Path

```
Issue 1 (scraper infra) ──→ Issue 6 (Common App) ──→ Issue 10 (orchestrator) ──→ Issue 12 (integration test)
Issue 2 (interface)     ──↗                        ↗
Issue 3 (upsert)       ────────────────────────────
```

Phase 0 issues (1-4) are fully parallel. Phase 1 extractors (5-8) are
parallel with each other. The orchestrator (10) can start once foundation +
at least static lists are done, then grow as more extractors land.

---

## Files Modified

| File | Change |
|---|---|
| `api/internal/search/registry.go` (L825-847) | Remove UCA enum, add 11 new options |
| `api/internal/search/datasource.go` (L113-120) | Rename SourceCommonApp → SourceApplicationPlatforms |
| `tools/kapow/go.mod` | Add `chromedp` dependency |
| `tools/kapow/internal/dbutil/upsert.go` | Add `SetAdmissions()` |
| `tools/kapow/DATA_SOURCES.md` | Update admissions section |
| `tools/CLAUDE.md` | Add Chrome/Chromium prerequisite |
| `web/schema.graphql` | Regenerated via `make web-schema-sync` |

All other files are **new** (see package structure above).

---

## Verification

1. `make api-test-unit` — search field enum tests pass
2. `make web-build && make web-lint` — frontend builds with updated schema
3. `cd tools/kapow && go test ./...` — all scraper/platform/importer tests pass
4. `kapow admissions --dry-run --platforms Common,QuestBridge` — scrapes + reports match stats
5. `kapow admissions --dry-run` — full run, match report generated
6. Against live DB: verify `admissions` JSONB populated, search filter works in UI

---

## Relation to Existing Issues

- **kapow-jsuj** (admissions importer): Superseded by this plan. Should be
  closed and replaced with the new epic.
- **kapow-7xk** (namematch): Already implemented (Tier 1 + 2). No longer a
  blocker.
- **kapow-89va** (NCES Scorecard): No longer a dependency for admissions.
  Admissions importer is independent of NCES data.
