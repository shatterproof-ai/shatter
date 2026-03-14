# BigFuture __NEXT_DATA__ Scraper (kapow-t77.3)

## Context

Kapow needs admissions deadline data. BigFuture (Tier 2) is the simplest source — it's a Next.js SSR app with structured JSON embedded in the HTML as `__NEXT_DATA__`. This scraper fetches pages via plain HTTP GET, parses the JSON, and converts it to the `DeadlineData` format defined in kapow-t77.2.

This issue covers only the scraper logic and tests — NOT the importer registration (kapow-t77.4).

## Files to Create

1. `tools/kapow/internal/deadlines/bigfuture.go` — scraper logic
2. `tools/kapow/internal/deadlines/bigfuture_test.go` — unit tests
3. `tools/kapow/internal/deadlines/testdata/bigfuture_harvard.html` — fixture

## Implementation

### bigfuture.go

**BigFutureData struct** — maps to `props.pageProps.collegeData` in __NEXT_DATA__:
```go
type BigFutureData struct {
    OrgID                                    string   `json:"orgId"`
    DICode                                   string   `json:"diCode"`
    IPEDSId                                  string   `json:"ipedsId"`
    EarlyDecision                            bool     `json:"earlyDecision"`
    EarlyDecisionDate                        string   `json:"earlyDecisionDate"`
    EarlyActionDate                          string   `json:"earlyActionDate"`
    RegularDecisionDate                      string   `json:"regularDecisionDate"`
    NotificationDate                         string   `json:"notificationDate"`
    ResponseDeadline                         string   `json:"responseDeadline"`
    FinancialAidRegularDeadline              string   `json:"financialAidApplicationRegularDeadline"`
    FinancialAidPriorityDeadline             string   `json:"financialAidApplicationPriorityDeadline"`
    ApplicationSiteURL                       string   `json:"applicationSiteUrl"`
    ApplicationsAccepted                     []string `json:"applicationsAccepted"`
}
```

Wrapper struct for the __NEXT_DATA__ nesting:
```go
type nextDataEnvelope struct {
    Props struct {
        PageProps struct {
            CollegeData BigFutureData `json:"collegeData"`
        } `json:"pageProps"`
    } `json:"props"`
}
```

**FetchPage(ctx, client, slug) ([]byte, error)**:
- HTTP GET to `https://bigfuture.collegeboard.org/colleges/{slug}`
- Set reasonable User-Agent header
- Return raw HTML bytes
- Error on non-200 status

**ParseNextData(html []byte) (*BigFutureData, error)**:
- Find `<script id="__NEXT_DATA__" type="application/json">` via string search (bytes.Index)
- Find closing `</script>` tag
- Unmarshal JSON into `nextDataEnvelope`
- Return `&envelope.Props.PageProps.CollegeData`
- Error if tag not found or JSON invalid

**ToDeadlineData(bf *BigFutureData, year int) DeadlineData**:
- Convert date strings from `MM/DD/9999` format to `YYYY-MM-DD` using the target cycle year
- Dates with year `9999` get the target year substituted (or year+1 for spring dates like notification/regular)
- Null/empty dates are skipped
- Map fields to DeadlinePlan entries:
  - `earlyDecision=true` + `earlyDecisionDate` → plan `early_decision_1` with binding=true
  - `earlyActionDate` → plan `early_action`
  - `regularDecisionDate` → plan `regular`
  - `notificationDate` → notification field on `regular` plan
- Set `confidence = "medium"` on all plans (Tier 2 data)
- Set `source_url` to BigFuture college URL
- Set top-level `has_early_decision`, `has_early_action` booleans
- Set `bigfuture_id` to `OrgID`

**SlugFromName(name string) string**:
- Convert institution name to kebab-case slug
- Lowercase, replace spaces/special chars with hyphens, collapse multiple hyphens, trim

### bigfuture_test.go

Table-driven tests using fixture HTML in `testdata/bigfuture_harvard.html`:

1. **TestParseNextData** — parse fixture, verify all fields populated correctly
2. **TestParseNextData_NoScript** — error on HTML without __NEXT_DATA__
3. **TestParseNextData_InvalidJSON** — error on malformed JSON
4. **TestToDeadlineData** — verify date conversion (MM/DD/9999 → YYYY-MM-DD), plan mapping, confidence=medium
5. **TestToDeadlineData_NullDates** — verify empty/null dates are omitted
6. **TestSlugFromName** — table-driven: "Harvard University" → "harvard-university", edge cases

### testdata/bigfuture_harvard.html

Minimal fixture HTML containing a realistic `<script id="__NEXT_DATA__">` block with Harvard's data structure. Only needs enough HTML to test parsing — not a full page.

## Key Design Decisions

- **String search, not regex or HTML parser** for __NEXT_DATA__ extraction — it's always a well-formed `<script>` tag with a known id, and string search is simpler and faster
- **No network calls in tests** — fixture HTML only
- **Date year mapping**: BigFuture uses `9999` as a placeholder year. For fall deadlines (ED/EA, typically Oct-Jan), use `year`. For spring dates (RD notification, typically Feb-Apr), use `year+1`. For regular decision deadline (typically Jan), use `year+1`.
- **unit_id IS the IPEDS ID** — so when BigFuture's `ipedsId` is non-null, it can match directly against the institution table's PK

## Existing Code to Reuse

- `tools/kapow/internal/deadlines/types.go` — `DeadlineData`, `DeadlinePlan`, plan key constants
- `tools/kapow/internal/deadlines/store.go` — `StoreScrapedPage`, `UpsertDeadlines` (used by importer in t77.4, not this issue)
- `tools/kapow/internal/namematch/matcher.go` — `Matcher.Match()` (used by importer in t77.4)

## Verification

```bash
cd /home/ketan/project/kapow/.claude/worktrees/worktree/bigfuture
make kapow-test-unit
```
