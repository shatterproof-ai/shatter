# Plan: Ranking Builder Package (`tools/kapow/internal/ranking/`)

## Context

The kapow tool has 9+ ranking importers planned (US News, Forbes, Niche, etc.). Each will parse ranking data from different sources but all need to write to the same three database targets. This package provides the shared infrastructure so individual importers only need to produce `Opinion` structs and call `Flush`.

## Files to Create

### 1. `tools/kapow/internal/ranking/builder.go`

**`Opinion` struct** — one ranking observation:
```go
type Opinion struct {
    PublicationSlug string  // e.g. "us_news"
    CategorySlug   string  // e.g. "national_universities"
    Method         string  // "rank", "score", "tier", "range"
    Year           int
    UnitID         string
    Rank           int     // 0 if not applicable
    Score          float64 // 0 if not applicable
    Tier           string  // "" if not applicable
}
```

**`deriveOpinion(raw string) (method string, rank int, score float64, tier string)`** — parse rank formats:
- Numeric: `"1"`, `"42"` → method="rank", rank=42
- Tier: `"Tier 1"`, `"Tier 2"` → method="tier", tier="Tier 1"
- Range: `"201-300"` → method="range", rank=250 (midpoint), tier="201-300"
- Score-like floats: `"78.5"` → method="score", score=78.5
- Empty/unparseable → error

**`RankingBuilder` struct**:
```go
type RankingBuilder struct {
    opinions []Opinion
    // Publication metadata for upsert into search_base.ranking
    publications map[string]publicationMeta // key = pub_slug + "____" + cat_slug
}

type publicationMeta struct {
    PublicationSlug string
    Publication     string // display name
    CategorySlug    string
    Category        string // display name
    URL             string
}
```

Methods:
- `NewBuilder() *RankingBuilder`
- `SetPublication(slug, name, catSlug, catName, url string)` — register publication metadata
- `Add(op Opinion)` — accumulate an opinion
- `Flush(ctx context.Context, pool *pgxpool.Pool) error` — batch write:
  1. Upsert each unique publication into `search_base.ranking` using `dbutil.BatchExec`
  2. For each opinion, upsert into `search_base.annual_dataset` (theme=`RANKING`, datakey=`pub____cat`)
  3. Group opinions by unit_id, build rankings JSONB map, upsert into `institution.rankings` via `dbutil.UpsertInstitution` (only Rankings field populated)

**Key design decisions:**
- Ranking ID format: `pub_slug + "____" + cat_slug` (matches existing data in DB, e.g. `us_news____national_universities`)
- Theme constant: `"RANKING"` (matches `search_base.theme` enum)
- Use existing `dbutil.UpsertInstitution`, `dbutil.UpsertAnnualDataset`, `dbutil.BatchExec` — no new SQL abstractions needed
- Batch in chunks (e.g. 500 opinions per batch) to avoid memory issues

### 2. `tools/kapow/internal/ranking/builder_test.go`

**Unit tests** (run with `-short`):
- `TestDeriveOpinion` — table-driven:
  - `"1"` → rank method, rank=1
  - `"42"` → rank method, rank=42
  - `"Tier 1"` → tier method, tier="Tier 1"
  - `"Tier 2"` → tier method, tier="Tier 2"
  - `"201-300"` → range method, rank=250, tier="201-300"
  - `"78.5"` → score method, score=78.5
  - `""` → error
  - `"N/A"` → error
- `TestRankingBuilderAdd` — verify accumulation, publication registration
- `TestBuildRankingsJSONB` — verify JSONB structure matches expected format

**Integration test** (skip when `-short` or no `DATABASE_URL`):
- `TestIntegrationFlush` — create test institution, flush opinions, verify DB state

## Key Files Referenced

- `tools/kapow/internal/dbutil/upsert.go` — `UpsertInstitution`, `UpsertAnnualDataset`, `BatchExec`
- `tools/kapow/internal/dbutil/models.go` — `Institution`, `AnnualDataset` structs
- `tools/kapow/internal/dbutil/batch.go` — `BatchExec`, `CopyFrom`
- `api/migrations/00001_initial_schema.sql` — table definitions
- `api/internal/search/datasource.go` — ranking dataset key format reference

## Verification

```bash
cd tools/kapow && go build ./internal/ranking/ && go test ./internal/ranking/ -short -race -v && go vet ./internal/ranking/
```
