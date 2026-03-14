# Plan: QuestBridge + CBCA Platform Scrapers (kapow-j2l.8)

## Context

The platform scraper framework defines `Extractor` and `Browser` interfaces in `tools/kapow/internal/platform/platform.go`. Key constants `KeyQuestBridge` and `KeyCBCA` already exist. No extractors are implemented yet — these are the first two. Other teammates are implementing other extractors in parallel, so we must NOT modify `All()`.

## Files to create

### 1. `tools/kapow/internal/platform/questbridge.go`

- Struct `questBridge` implementing `Extractor`
- Constructor `NewQuestBridge() Extractor`
- `Key()` → `KeyQuestBridge`, `Label()` → `"QuestBridge"`
- `Extract()`: Navigate to `https://www.questbridge.org/partners/college-partners`, then `EvalStrings` with JS that selects partner college name elements from the server-rendered HTML
- JS selector strategy: inspect likely DOM structure (heading/list elements containing college names). Use a broad selector like `document.querySelectorAll()` targeting the partner list, extracting `textContent` and trimming whitespace
- Return deduplicated, sorted list; error-wrap with `"platform: questbridge:"` prefix

### 2. `tools/kapow/internal/platform/questbridge_test.go`

- Mock `Browser` struct that records Navigate calls and returns fixture HTML via `EvalStrings`
- `TestQuestBridgeKey` — verify Key() and Label()
- `TestQuestBridgeExtract` — provide representative fixture HTML (mock EvalStrings returns ~5 sample college names), verify extraction count and content
- `TestQuestBridgeExtractError` — mock Browser returning error, verify error propagation

### 3. `tools/kapow/internal/platform/cbca.go`

- Struct `cbca` implementing `Extractor`
- Constructor `NewCBCA() Extractor`
- `Key()` → `KeyCBCA`, `Label()` → `"Common Black College App"`
- `Extract()`: Navigate to `https://www.commonblackcollegeapp.com/schools`, then `EvalStrings` with JS to extract school names after JS rendering completes
- The page is JS-rendered, so the Browser's Navigate (which waits for readiness) handles that; then EvalStrings extracts names
- Return deduplicated, sorted list; error-wrap with `"platform: cbca:"` prefix

### 4. `tools/kapow/internal/platform/cbca_test.go`

- Same mock Browser pattern as QuestBridge tests
- `TestCBCAKey` — verify Key() and Label()
- `TestCBCAExtract` — mock EvalStrings returns ~5 sample HBCU names, verify extraction
- `TestCBCAExtractError` — verify error propagation

## Mock Browser pattern (shared across both test files)

```go
type mockBrowser struct {
    navigateURL string
    names       []string
    err         error
}
func (m *mockBrowser) Navigate(ctx context.Context, url string) error {
    m.navigateURL = url
    return m.err
}
func (m *mockBrowser) EvalStrings(ctx context.Context, js string) ([]string, error) {
    if m.err != nil { return nil, m.err }
    return m.names, nil
}
```

Each test file gets its own copy (unexported, test-only — no shared test helpers needed for 2 files).

## Key design decisions

- Extractors use `Browser.Navigate` + `Browser.EvalStrings` — no direct HTTP fetching
- JS expressions embedded as string constants in each extractor file
- Deduplication via map; sort for deterministic output
- No modification to `All()` — export constructors only per team instructions

## Verification

```bash
cd /home/ketan/project/kapow/.claude/worktrees/worktree/questbridge-cbca
cd tools/kapow && go test ./internal/platform/... -short -count=1
cd tools/kapow && go vet ./internal/platform/...
```
