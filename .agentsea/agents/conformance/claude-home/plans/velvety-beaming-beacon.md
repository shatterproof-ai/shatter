# Plan: Admissions Importer Orchestrator (kapow-j2l.10)

## Context

The kapow data pipeline needs an orchestrator that ties together platform scrapers, name matching, and DB upserts. The infrastructure is ready: `platform.All()` returns extractors, `scraper.Cache` handles caching, `namematch.Matcher` resolves names to unit IDs, and `dbutil.BulkSetAdmissions()` batch-updates the DB. This importer glues them together.

## Files to create

### 1. `tools/kapow/admissions.go` — Importer implementation

Struct `admissionsImporter` implementing the `Importer` interface (defined in `registry.go`).

**Fields:**
- `maxCacheAge time.Duration` (flag: `--max-cache-age`, default `168h` = 7d)
- `requireAll bool` (flag: `--require-all`, default `false`)
- `platforms string` (flag: `--platforms`, default `""` = all)

**`Run()` flow:**
1. Load `namematch.Matcher` from DB via `namematch.Load(ctx, pool)`
2. Create `scraper.Browser` via `scraper.New()`, defer `Close()`
3. Create `scraper.Cache` via `scraper.NewCache(fetcher.CacheDir(), maxCacheAge)`
4. Parse `--platforms` flag into a `map[string]bool` filter (empty = run all)
5. For each extractor from `platform.All()`:
   - Skip if not in platforms filter
   - Try `cache.Load(ext.Key())`
   - On `ErrNotFound`/`ErrStale`: run `ext.Extract(ctx, browser)`, then `cache.Save(ext.Key(), names)`
   - On other error from Extract: if `requireAll`, return error; else log warning and continue
   - Collect `platformKey → []string{names}` map
6. Build `map[unitID][]string{platformKeys}` by matching each name:
   - For each platform's names, call `matcher.Match(name)` → unitID
   - Append platform key to that unitID's list
7. Build `map[string][]byte` for `BulkSetAdmissions`:
   - For each unitID, marshal `{"applications": ["Common", "CBCA", ...]}` as JSON
8. Call `dbutil.BulkSetAdmissions(ctx, pool, admissions)`
9. Log per-platform stats: scraped count, matched count, unmatched count

**Registration:** `init()` calls `Register(&admissionsImporter{})`

**Key reuse:**
- `scraper.NewCache()` from `internal/scraper/cache.go`
- `scraper.New()` from `internal/scraper/browser.go`
- `namematch.Load()` from `internal/namematch/matcher.go`
- `platform.All()` from `internal/platform/platform.go`
- `dbutil.BulkSetAdmissions()` from `internal/dbutil/upsert.go`
- `fetch.Fetcher.CacheDir()` from `internal/fetch/fetch.go`

### 2. `tools/kapow/admissions_test.go` — Unit tests

Tests using mock extractors (no DB, no browser). Test cases:

1. **TestAdmissionsAggregation** — Two mock extractors returning overlapping names. Verify aggregation produces correct `map[unitID][]string`.
2. **TestAdmissionsJSONB** — Verify output JSON structure is `{"applications": ["Key1", "Key2"]}` with sorted keys.
3. **TestAdmissionsPartialFailure** — One extractor returns error, `requireAll=false` → other extractors still processed.
4. **TestAdmissionsRequireAll** — One extractor returns error, `requireAll=true` → returns error.
5. **TestAdmissionsPlatformFilter** — `--platforms=uc,cbca` filters to only those extractors.

To make the core logic testable, extract the aggregation/matching logic into a helper function with injected dependencies (matcher interface or function, extractor list).

## Verification

```bash
cd tools/kapow && go build ./...
cd tools/kapow && go test ./... -short -count=1 -run TestAdmissions
cd tools/kapow && go vet ./...
```
