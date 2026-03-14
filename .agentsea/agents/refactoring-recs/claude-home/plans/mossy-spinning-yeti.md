# Coalition/Scoir Scraper — Implementation Plan

## Context

Issue kapow-j2l.7: Implement a Coalition App platform scraper that extracts member institution names. The scraper implements the `Extractor` interface from `tools/kapow/internal/platform/platform.go` and uses the `Browser` interface for web scraping via chromedp.

**Key constraint**: `Navigate` and `EvalStrings` each open separate browser tabs (see `scraper/browser.go:106-132`). To scrape a page's DOM, we must use `EvalStrings` with embedded `fetch()` + `DOMParser` in the JS expression, since we can't navigate and eval in the same tab through the interface.

---

## Files to Create

### 1. `tools/kapow/internal/platform/coalition.go`

**Struct & constructor:**
```go
type coalition struct{}
func NewCoalition() Extractor { return &coalition{} }
func (c *coalition) Key() string   { return KeyCoalition }   // "Coalition"
func (c *coalition) Label() string { return "Coalition App" }
```

**Extract strategy** — two sources, tried in order:

1. **Scoir HTML** (`https://www.scoir.com/apply-colleges`) — server-rendered HTML with ~123 colleges. Use `EvalStrings` with JS that fetches the page HTML, parses with `DOMParser`, and extracts college names from the DOM.

2. **Coalition members page** (`https://www.coalitionforcollegeaccess.org/our-members`) — Squarespace site. Use `EvalStrings` with JS that fetches HTML and extracts member names from the Squarespace layout.

**Sanity threshold**: `minExpectedCoalition = 20`. If a source returns fewer names, treat it as failed and try the next.

**Post-processing** via `cleanNames()` helper:
- `strings.TrimSpace` each name
- Remove empty strings
- Deduplicate via map

**Error handling**: wrap errors with `fmt.Errorf("platform: coalition: %w", err)`. If both sources fail, return combined error.

### 2. `tools/kapow/internal/platform/coalition_test.go`

**Mock browser** — dispatches on URL substrings in the JS expression:
```go
type mockBrowser struct {
    navigateErr error
    calls       map[string]mockResult  // URL substring → {names, err}
}
```

**Test cases:**
- `TestCoalitionKeyAndLabel` — verify Key/Label values
- `TestCoalitionExtractScoir` — mock returns fixture names for Scoir URL → success
- `TestCoalitionExtractCoalitionDOM` — mock fails Scoir, returns names for Coalition URL → fallback works
- `TestCoalitionExtractAllFail` — both sources error → returns error
- `TestCoalitionExtractTooFewResults` — Scoir returns <20 names → falls through to Coalition DOM
- `TestCoalitionCleanNames` — dedup, trim, empty removal

**Fixture data**: ~25 representative institution names (Amherst College, Arizona State University, Brown University, etc.)

---

## Patterns to Follow

- Static extractor pattern from `static.go` (unexported struct, exported constructor)
- Test pattern from `static_test.go` (Key/Label checks, count assertions, spot-check names)
- Error wrapping: `fmt.Errorf("platform: coalition: source: %w", err)` per CLAUDE.md

## Do NOT Modify

- `platform.go` — do not touch `All()` (team lead handles wiring)

---

## Verification

```bash
cd /home/ketan/project/kapow/.claude/worktrees/worktree/coalition
cd tools/kapow && go test ./internal/platform/... -short -count=1 -v
```

All tests must pass. Then commit and push the branch.
