# Plan: Common App Scraper (kapow-j2l.6)

## Context

Kapow needs scrapers for 13 college application platforms to track which institutions accept each platform. This task implements the Common App extractor for commonapp.org/explore (~1100 institutions). The platform scraper framework (interfaces, browser, cache) is already built; this is the first concrete extractor.

## Critical Design Constraint: Tab Isolation

The `scraper.Browser` creates a **new tab** for each `Navigate()` and `EvalStrings()` call (browser.go:106, 121). Calling Navigate then EvalStrings runs JS on a blank tab, not the navigated page. Therefore, the extractor must use `EvalStrings` with a **self-contained JS expression** that uses `fetch()` to retrieve data directly — no DOM scraping across calls.

## Implementation

### File 1: `tools/kapow/internal/platform/commonapp.go`

- Unexported `commonApp` struct implementing `Extractor`
- Exported `NewCommonApp() Extractor` constructor
- `Key()` → `KeyCommon` ("Common"), `Label()` → "Common App"
- `Extract()` flow:
  1. `browser.Navigate(ctx, commonAppURL)` — warms session/cookies (even though tab closes)
  2. `browser.EvalStrings(ctx, extractJS)` — self-contained JS that uses `fetch()` to hit the Common App explore page, finds embedded data (`__NEXT_DATA__` or similar JSON payload), and returns institution names as `[]string`
  3. Deduplicate and validate count (minimum ~900 expected)
- `deduplicate()` unexported helper: trims whitespace, removes empty strings, deduplicates
- Error wrapping follows convention: `fmt.Errorf("platform: commonapp: context: %w", err)`

**JS extraction strategy** (in `extractJS` constant):
- Fetch `https://www.commonapp.org/explore` HTML via `fetch()`
- Parse `__NEXT_DATA__` script tag for embedded school data (common Next.js pattern)
- If not found, try known API endpoint patterns
- Return array of institution name strings

### File 2: `tools/kapow/internal/platform/commonapp_test.go`

Mock browser in test file (package `platform`, same package tests):

```go
type mockBrowser struct {
    navigateErr error
    evalResults []string
    evalErr     error
}
```

Test cases:
1. `TestCommonAppKeyAndLabel` — verify Key/Label constants
2. `TestCommonAppExtractSuccess` — mock returns 1100 names, verify deduplication
3. `TestCommonAppExtractNavigateError` — Navigate fails, error propagated
4. `TestCommonAppExtractEvalError` — EvalStrings fails, error propagated
5. `TestCommonAppExtractTooFewResults` — returns partial results + error when below threshold
6. `TestDeduplicate` — table-driven: empty, dupes, whitespace, normal

### NOT Modified
- `platform.go` — `All()` untouched per coordination requirement

## Verification

```bash
cd tools/kapow && go vet ./internal/platform/... && go test ./internal/platform/... -short -count=1
```
