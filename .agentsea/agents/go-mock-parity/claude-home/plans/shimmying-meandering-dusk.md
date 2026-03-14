# Admissions Deadline Importer

## Context

Kapow needs current-year, exact-precision application deadlines, notification
dates, and admission plan classifications (EA, ED1, ED2, REA, rolling) for
every institution. IPEDS lags 12-18 months and CDS lacks a structured API, so
neither is acceptable. The solution scrapes institution admissions pages
(primary) and BigFuture (fallback/validation), uses a local LLM for structured
extraction, and stores both raw pages and extracted data for auditability.

---

## Architecture

```
Tier 1: Institution admissions pages (primary, authoritative)
        Playwright-Go renders page → local LLM (Ollama) extracts structured JSON
        Stores: scraped_page (content_text + content_html) + annual_dataset

Tier 2: BigFuture __NEXT_DATA__ (fallback + validation + ID crosswalk)
        Plain HTTP GET + JSON parse (SSR'd Next.js, no browser needed)
        Stores: scraped_page (api_response) + annual_dataset
        Maps: diCode (CB code) / ipedsId → unit_id via namematch

Tier 3: Manual overrides JSON file (REA classification, corrections)
        Highest priority, overwrites Tier 1/2 for specified fields
```

### Data storage

- **Deadline data** → `search_base.annual_dataset`
  - theme=`ADMISSIONS`, datakey=`deadlines`, year=admission cycle year
  - ID format: `{unit_id}:{year}:ADMISSIONS:deadlines`
- **Scraped pages** → new `search_base.scraped_page` table (migration 00009)
  - Append-only with content_hash for change detection
  - Stores rendered text, HTML, and/or raw API response JSON

### JSONB schema (annual_dataset.data)

```json
{
  "plans": {
    "early_decision_1": {
      "deadline": "2026-11-01",
      "notification": "2026-12-15",
      "deposit_due": "2027-01-15",
      "binding": true,
      "source_url": "https://www.example.edu/admissions",
      "confidence": "high"
    },
    "early_decision_2": { ... },
    "early_action": { ... },
    "restrictive_early_action": { ... },
    "regular": { ... },
    "rolling": {
      "opens": "2026-08-01",
      "closes": "2027-06-01",
      "notification_weeks": 4,
      "binding": false,
      "source_url": "...",
      "confidence": "high"
    }
  },
  "fee_usd": 75,
  "fee_waiver": true,
  "has_early_decision": true,
  "has_early_action": false,
  "has_rolling_admissions": false,
  "source_urls": ["https://www.example.edu/admissions"],
  "extracted_at": "2026-08-15T14:30:00Z",
  "bigfuture_id": "1251",
  "bigfuture_agrees": true
}
```

Valid plan keys: `early_decision_1`, `early_decision_2`, `early_action`,
`restrictive_early_action`, `regular`, `rolling`, `priority`.

---

## Phases

### Phase 1: Migration + Data Types

**Migration 00009** — `api/migrations/00009_scraped_page.sql`:

```sql
-- +goose Up
CREATE TABLE search_base.scraped_page (
    id            BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    unit_id       VARCHAR(20) NOT NULL REFERENCES search_base.institution(unit_id),
    page_type     VARCHAR(30) NOT NULL,
    url           TEXT NOT NULL,
    content_text  TEXT NOT NULL,
    content_html  TEXT,
    api_response  JSONB,
    fetched_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    http_status   SMALLINT,
    content_hash  VARCHAR(64),
    UNIQUE (unit_id, page_type, fetched_at)
);
CREATE INDEX scraped_page_unit_type_idx
    ON search_base.scraped_page (unit_id, page_type, fetched_at DESC);

-- +goose Down
DROP INDEX IF EXISTS search_base.scraped_page_unit_type_idx;
DROP TABLE IF EXISTS search_base.scraped_page;
```

**Go types** — `tools/kapow/internal/deadlines/types.go`:

- `DeadlinePlan` struct (deadline, notification, deposit_due, binding, opens,
  closes, notification_weeks, source_url, confidence)
- `DeadlineData` struct (plans map, fee_usd, fee_waiver, has_* booleans,
  source_urls, extracted_at, bigfuture_id, bigfuture_agrees)
- Plan key constants

**DB helpers** — `tools/kapow/internal/deadlines/store.go`:

- `StoreScrapedPage(ctx, pool, input)` — INSERT into scraped_page
- `UpsertDeadlines(ctx, pool, unitID, year, data)` — wraps
  `dbutil.UpsertAnnualDataset` with theme=ADMISSIONS, datakey=deadlines
- `LoadExistingDeadlines(ctx, pool, unitID, year)` — read for merge/compare

**Files to create:**
- `api/migrations/00009_scraped_page.sql`
- `tools/kapow/internal/deadlines/types.go`
- `tools/kapow/internal/deadlines/store.go`
- `tools/kapow/internal/deadlines/store_test.go`

**Verify:** `make api-migrate-up && make test-fixture && make kapow-test-unit`

---

### Phase 2: BigFuture Importer (Tier 2)

Build first because it's simpler (plain HTTP, structured JSON) and provides
validation data for Tier 1.

**BigFuture scraper** — `tools/kapow/internal/deadlines/bigfuture.go`:

- `FetchPage(ctx, client, slug) (htmlBytes, error)` — HTTP GET to
  `https://bigfuture.collegeboard.org/colleges/{slug}`
- `ParseNextData(html []byte) (*BigFutureData, error)` — extract
  `__NEXT_DATA__` JSON from HTML via string search, parse into struct
- `BigFutureData` struct with fields: `OrgID`, `DICode`, `IPEDSId`,
  `EarlyDecision` (bool), `EarlyDecisionDate`, `EarlyActionDate`,
  `RegularDecisionDate`, `NotificationDate`, `ResponseDeadline`,
  `FinancialAidRegularDeadline`, `FinancialAidPriorityDeadline`
- `ToDeadlineData(bf, year) *DeadlineData` — convert MM/DD/9999 dates to
  YYYY-MM-DD using the target cycle year; set confidence=medium

**Slug discovery:**
- Build slugs from institution names in DB: kebab-case normalization
- Also try `ipedsId` field from BigFuture for direct matching
- Match by `diCode` → CB code crosswalk, or by name via `namematch.Matcher`

**Importer registration** — `tools/kapow/importer_deadlines_bigfuture.go`:

- Implements `Importer` interface, name=`deadlines-bigfuture`
- Dependencies: `["nces"]`
- Flags: `--year`, `--concurrency` (default 3), `--rate-limit` (default 500ms),
  `--slug-file` (optional)
- Run: load matcher → build slug list → fetch+parse+match+upsert each
  (with rate limiting) → store scraped page (api_response column) → log summary

**Files to create:**
- `tools/kapow/internal/deadlines/bigfuture.go`
- `tools/kapow/internal/deadlines/bigfuture_test.go` (with fixture HTML in testdata/)
- `tools/kapow/importer_deadlines_bigfuture.go`
- `tools/kapow/internal/deadlines/testdata/bigfuture_harvard.html` (fixture)

**No new Go dependencies** — standard library `net/http`, `encoding/json`,
`strings` suffice.

**Verify:** `make kapow-test-unit`, then manual: `go run . deadlines-bigfuture --dry-run`

---

### Phase 3: Playwright + URL Discovery (Tier 1 Foundation)

**Add dependency:** `playwright-community/playwright-go` to `tools/kapow/go.mod`

**Scraper** — `tools/kapow/internal/deadlines/scraper.go`:

- `type Scraper struct` — manages Playwright browser instance
- `NewScraper(ctx, opts)` — launch headless Chromium
- `RenderPage(ctx, url) (*RenderedPage, error)` — navigate, wait for network
  idle, return `{HTML, Text, URL, StatusCode, FetchedAt}`
- `Close()` — shutdown browser
- Semaphore-based concurrency control (default 2 tabs)

**URL resolver** — `tools/kapow/internal/deadlines/urlresolver.go`:

- `ResolveAdmissionsURL(ctx, client, baseURL) (string, error)` — try common
  paths (`/admissions`, `/apply`, `/admission`, `/admissions/deadlines`,
  `/apply/dates-deadlines`), return first 200 response
- `LoadInstitutionURLs(ctx, pool) (map[string]string, error)` — query
  `nces.school_url` from dataset_merged
- Manual overrides: `tools/kapow/data/admissions_urls.json`

**Files to create:**
- `tools/kapow/internal/deadlines/scraper.go`
- `tools/kapow/internal/deadlines/scraper_test.go`
- `tools/kapow/internal/deadlines/urlresolver.go`
- `tools/kapow/internal/deadlines/urlresolver_test.go`
- `tools/kapow/data/admissions_urls.json` (initially empty `{}`)

**Verify:** Manual test rendering a few institution pages.

---

### Phase 4: LLM Extraction (Tier 1 Completion)

**Ollama client** — `tools/kapow/internal/deadlines/llm.go`:

- `type LLMExtractor struct` — Ollama endpoint + model name
- `NewLLMExtractor(endpoint, model)` — defaults:
  `http://localhost:11434`, `qwen2.5:7b`
- `ExtractDeadlines(ctx, pageText) (*DeadlineData, error)` — POST to
  `/api/generate`, parse JSON from response, validate dates
- `IsAvailable(ctx) bool` — health check

**Prompt** — `tools/kapow/internal/deadlines/prompt.go`:

- System prompt with JSON schema, valid plan keys, date format rules
- Instructs model: extract only explicitly stated dates, use null for missing,
  never invent dates, classify REA vs EA
- Go `text/template` for injecting page text

**Validation** — `tools/kapow/internal/deadlines/validator.go`:

- Date plausibility: EA/ED deadlines Oct-Jan, RD Jan-Mar, notifications Dec-Apr
- Plan consistency: ED must be binding, EA must not be
- Cross-reference with BigFuture data when available

**Main Tier 1 importer** — `tools/kapow/importer_deadlines.go`:

- Name: `deadlines`, Dependencies: `["nces"]`
- Flags: `--year`, `--concurrency` (default 2), `--ollama-endpoint`,
  `--ollama-model`, `--skip-bigfuture-check`
- Run: load URLs → init Scraper + LLMExtractor → for each institution:
  resolve URL → render page → store scraped_page → extract via LLM →
  compare with BigFuture data (if exists) → set bigfuture_agrees → upsert
- Graceful degradation: if Ollama unavailable, log warning and skip

**Files to create:**
- `tools/kapow/internal/deadlines/llm.go`
- `tools/kapow/internal/deadlines/llm_test.go`
- `tools/kapow/internal/deadlines/prompt.go`
- `tools/kapow/internal/deadlines/validator.go`
- `tools/kapow/internal/deadlines/validator_test.go`
- `tools/kapow/importer_deadlines.go`
- `tools/kapow/internal/deadlines/testdata/` (fixture admissions page text)

**Verify:** Run against 10-20 known institutions, compare to published deadlines.

---

### Phase 5: CLI Reports + Manual Overrides

**Report** — `tools/kapow/importer_deadlines_report.go`:

- Name: `deadlines-report`, no DB writes
- Queries annual_dataset for theme=ADMISSIONS coverage stats
- Outputs: total institutions, with data, missing, by confidence, by plan type
- `--gaps-out gaps.json` flag for machine-readable gap list

**Spotcheck** — `tools/kapow/importer_deadlines_spotcheck.go`:

- Name: `deadlines-spotcheck`, no DB writes
- Selects N random institutions with deadline data (--count, default 10)
- Displays extracted values side-by-side with relevant excerpt from
  scraped_page.content_text + source URL
- `--open-browser` flag to open source URLs

**Manual overrides** — `tools/kapow/data/deadline_overrides.json`:

- Keyed by unit_id, contains partial DeadlineData to merge
- Loaded and applied with highest priority in both deadlines and
  deadlines-bigfuture importers
- Initially populated with ~20 REA schools (Harvard, Yale, Stanford,
  Princeton, Notre Dame, Georgetown, etc.)

**Files to create:**
- `tools/kapow/importer_deadlines_report.go`
- `tools/kapow/importer_deadlines_spotcheck.go`
- `tools/kapow/data/deadline_overrides.json`

---

### Phase 6: API + Search Integration

**Data source** — add to `api/internal/search/datasource.go`:

- `SourceBigFuture` data source entry (id=`bigfuture`, dataset=`deadlines`)

**Search fields** — add to `api/internal/search/registry.go`:

| Field | Type | Searchable | Notes |
|---|---|---|---|
| `has_early_decision` | SingleEnumField | Yes | Yes/No |
| `has_early_action` | SingleEnumField | Yes | Yes/No |
| `has_rolling_admissions` | SingleEnumField | Yes | Yes/No |
| `application_fee` | NumericRangeField | Yes | USD |

Date-based fields (deadlines, notifications) start as output-only/exportable,
not searchable, to avoid UX complexity of date range filters initially.

**GraphQL** — extend `InstitutionAdmissions` in
`api/graph/schema/search.graphql`:

```graphql
type InstitutionAdmissions {
  # existing fields...
  regularDecisionDeadline: String
  earlyDecisionDeadline: String
  earlyActionDeadline: String
  regularNotificationDate: String
  hasEarlyDecision: Boolean
  hasEarlyAction: Boolean
  hasRollingAdmissions: Boolean
  applicationFee: Int
  feeWaiver: Boolean
}
```

Then: `make api-generate && make web-schema-sync`

**Resolver wiring** — update extraction in
`api/graph/resolver/search_data_extract.go` to read deadline fields from
`data->'deadlines'->'plans'->...` JSONB paths.

**Files to modify:**
- `api/internal/search/datasource.go`
- `api/internal/search/datasource_test.go`
- `api/internal/search/registry.go`
- `api/graph/schema/search.graphql`
- `api/graph/resolver/search_data_extract.go`

**Verify:** `make api-generate && make web-schema-sync && make test-standard`

---

## Phase Dependencies

```
Phase 1 (migration + types) ─────────────────────────────────┐
    │                                                         │
    ▼                                                         ▼
Phase 2 (BigFuture) ──────────► Phase 5 (reports + overrides) │
    │                                                         │
    ▼                                                         ▼
Phase 3 (Playwright) ─────────────────────────────► Phase 6 (API)
    │
    ▼
Phase 4 (LLM extraction)
```

Phases 1-2 are the MVP — BigFuture alone covers ~80% of institutions for
RD/EA/ED1 deadlines and RD notification dates.

## Technology Choices

| Component | Technology | Rationale |
|---|---|---|
| BigFuture scraping | Go `net/http` | SSR'd Next.js, no JS needed |
| Institution scraping | `playwright-go` | Full JS rendering for SPAs |
| HTML text extraction | `page.InnerText("body")` | Simple, no extra dep |
| LLM extraction | Ollama + Qwen 2.5 7B | Local, ~$0/run, good JSON output |
| Institution matching | Existing `namematch` pkg | 4-tier matching already built |

## Verification

After each phase, run the appropriate test tier:
- Phase 1: `make api-migrate-up && make test-fixture && make kapow-test-unit`
- Phase 2: `make kapow-test-unit` + manual dry-run
- Phase 3-4: Manual test against 10-20 institutions
- Phase 5: `make kapow-test-unit` + `deadlines-report` output review
- Phase 6: `make test-standard` (includes build + lint + unit tests)

End-to-end: run full pipeline against dev DB, then `deadlines-report` +
`deadlines-spotcheck --count 20` to verify data quality.
