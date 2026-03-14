# kapow-t77.2 — Deadline data types and DB helpers

## Context

The admissions deadlines feature needs core data types and database operations before scrapers/extractors can store deadline data. This package (`tools/kapow/internal/deadlines/`) provides the type definitions, plan key constants, and DB store functions that downstream tasks will consume.

## Files to create

### 1. `tools/kapow/internal/deadlines/types.go`

Define exported types following existing patterns (exported types first, constants grouped):

- **Plan key constants** (string consts):
  - `PlanEarlyDecision1`, `PlanEarlyDecision2`, `PlanEarlyAction`, `PlanRestrictiveEarlyAction`, `PlanRegular`, `PlanRolling`, `PlanPriority`
  - Values: `early_decision_1`, `early_decision_2`, `early_action`, `restrictive_early_action`, `regular`, `rolling`, `priority`

- **`DeadlinePlan` struct**: `Deadline`, `Notification`, `DepositDue` (string), `Binding` (bool), `Opens`, `Closes` (string), `NotificationWeeks` (int), `SourceURL` (string), `Confidence` (string) — all with `json:"snake_case,omitempty"` tags

- **`DeadlineData` struct**: `Plans` (map[string]DeadlinePlan), `FeeUSD` (int), `FeeWaiver`, `HasEarlyDecision`, `HasEarlyAction`, `HasRollingAdmissions` (bool), `SourceURLs` ([]string), `ExtractedAt` (string), `BigfutureID` (string), `BigfutureAgrees` (bool) — with matching JSON tags

### 2. `tools/kapow/internal/deadlines/store.go`

DB operations using `pgx/v5/pgxpool` and existing `dbutil` helpers:

- **`StoreScrapedPage(ctx, pool, params)`** — INSERT into `search_base.scraped_page` with parameterized SQL (`$1`–`$N`). Params struct: `UnitID`, `PageType`, `URL`, `ContentText`, `ContentHTML` (*string), `APIResponse` ([]byte), `HTTPStatus` (*int16), `ContentHash` (*string). Returns inserted `id` and error.

- **`UpsertDeadlines(ctx, pool, unitID string, year int, data DeadlineData)`** — Marshal `data` to JSON, call `dbutil.UpsertAnnualDataset` with theme=`"ADMISSIONS"`, datakey=`"deadlines"`.

- **`LoadExistingDeadlines(ctx, pool, unitID string, year int)`** — Query `search_base.annual_dataset` WHERE unit_id/year/theme/datakey match, unmarshal JSON into `DeadlineData`. Return zero-value + nil if not found.

### 3. `tools/kapow/internal/deadlines/store_test.go`

Unit tests (no DB required, `-short` safe):

- `TestDeadlineDataMarshalRoundTrip` — marshal DeadlineData → JSON → unmarshal, verify equality
- `TestDeadlinePlanOmitEmpty` — verify omitempty works (empty fields not in JSON output)
- `TestDeadlineDataWithAllPlans` — construct with all 7 plan types, verify map keys survive round-trip
- `TestScrapedPageParamsValidation` — verify params struct fields set correctly

## Patterns to reuse

- **`dbutil.UpsertAnnualDataset`** (`tools/kapow/internal/dbutil/upsert.go:86`) — for deadline upsert
- **`dbutil.AnnualDataset`** model (`tools/kapow/internal/dbutil/models.go`) — struct for annual data
- **Error wrapping**: `fmt.Errorf("deadlines: context: %w", err)`
- **pgxpool usage**: `pool.QueryRow(ctx, sql, args...)` pattern from dbutil

## Verification

```bash
cd tools/kapow && go vet ./internal/deadlines/...
cd tools/kapow && go test ./internal/deadlines/... -short -count=1
```
