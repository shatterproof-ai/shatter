# Plan: Implement tools/kapow/internal/dbutil

## Context

Issue kapow-as6 requires implementing the `dbutil` package for the kapow data import
pipeline tool. The package provides database connectivity, upsert helpers, batch
operations, and materialized view refresh for importing data into the Kapow PostgreSQL
database. An empty scaffold (`dbutil.go`) already exists with `Connect()` and
`RefreshMaterializedViews()` stubs that must be enhanced without changing their signatures.

---

## Files to create / modify

| Path | Action |
|------|--------|
| `tools/kapow/internal/dbutil/dbutil.go` | Enhance: retry in Connect, implement RefreshMaterializedViews |
| `tools/kapow/internal/dbutil/models.go` | Create: Institution, Dataset, AnnualDataset Go structs |
| `tools/kapow/internal/dbutil/upsert.go` | Create: UpsertInstitution, UpsertDataset, UpsertAnnualDataset, MergeJSONB |
| `tools/kapow/internal/dbutil/batch.go` | Create: CopyFrom wrapper, BatchExec using pgx.Batch |
| `tools/kapow/internal/dbutil/dbutil_test.go` | Create: unit + integration tests |

---

## Implementation Details

### 1. `dbutil.go` — enhanced Connect + RefreshMaterializedViews

`Connect()` — mirror pattern from `api/internal/db/db.go`:
- 3 attempts, exponential backoff: `time.Sleep(time.Duration(attempt) * time.Second)` (1s, 2s on retries)
- Use `pgxpool.ParseConfig()` then `pgxpool.NewWithConfig()`
- Log warnings via `slog` on failure, info on success
- Error: `"dbutil: failed to connect after 3 attempts: %w"`

`RefreshMaterializedViews()`:
- Execute two SQL statements in sequence:
  ```sql
  REFRESH MATERIALIZED VIEW search_base.dataset_merged
  REFRESH MATERIALIZED VIEW search_base.dataset_latest
  ```
- Return error wrapping each: `"dbutil: refresh dataset_merged: %w"`

### 2. `models.go` — Go structs

```go
type Institution struct {
    UnitID  int
    OPE6ID  string
    OPE8ID  string
    Data    []byte  // JSONB
}

type Dataset struct {
    UnitID  int
    Theme   string
    Datakey string
    Data    []byte  // JSONB
}

type AnnualDataset struct {
    UnitID  int
    Year    int
    Theme   string
    Datakey string
    Data    []byte  // JSONB
}
```

### 3. `upsert.go` — Upsert functions

**UpsertInstitution**: ON CONFLICT (unit_id) DO UPDATE with JSONB merge:
```sql
INSERT INTO search_base.institution (unit_id, ope6_id, ope8_id, <col>, updated, version)
VALUES ($1, $2, $3, $4, now(), 1)
ON CONFLICT (unit_id) DO UPDATE SET
  <col> = COALESCE(search_base.institution.<col>, '{}'::jsonb) || $4,
  updated = now(),
  version = search_base.institution.version + 1
```

**UpsertDataset**: ON CONFLICT (unit_id, theme, datakey) DO UPDATE:
```sql
INSERT INTO search_base.dataset (unit_id, theme, datakey, data, created, updated, version)
VALUES ($1, $2, $3, $4, now(), now(), 1)
ON CONFLICT (unit_id, theme, datakey) DO UPDATE SET
  data = COALESCE(search_base.dataset.data, '{}'::jsonb) || $4,
  updated = now(),
  version = search_base.dataset.version + 1
```

**UpsertAnnualDataset**: similar conflict on (unit_id, year, theme, datakey):
```sql
INSERT INTO search_base.annual_dataset (unit_id, year, theme, datakey, data, is_latest, created, updated, version)
VALUES ($1, $2, $3, $4, $5, false, now(), now(), 1)
ON CONFLICT (unit_id, year, theme, datakey) DO UPDATE SET
  data = COALESCE(search_base.annual_dataset.data, '{}'::jsonb) || $5,
  updated = now(),
  version = search_base.annual_dataset.version + 1
```

**MergeJSONB(dst, src []byte) ([]byte, error)**: pure function, merges two JSON objects at the Go level using `encoding/json`. Used for in-memory merge before upsert when needed.

### 4. `batch.go` — Batch operations

**BatchExec(ctx, pool, queries []BatchQuery) error** using `pgx.Batch`:
```go
type BatchQuery struct {
    SQL  string
    Args []any
}
```
- Build `pgx.Batch`, `pool.SendBatch(ctx, &batch)`, call `Exec()` for each, close results.

**CopyFrom(ctx, pool, tableName pgx.Identifier, columns []string, rows [][]any) (int64, error)**:
- Wraps `pgxpool.Pool.CopyFrom()` with proper error wrapping.

### 5. `dbutil_test.go` — Tests

**Unit tests** (no DB, always run):
- `TestMergeJSONB` — table-driven: merge two JSON objects, overlapping keys, nil inputs
- `TestBatchQueryStructure` — verify BatchQuery struct fields are correct

**Integration tests** (skip when `testing.Short()` or `DATABASE_URL` unset):
- `TestIntegrationConnect` — connects, pings
- `TestIntegrationUpsertInstitution` — upsert, re-upsert, check version incremented
- `TestIntegrationUpsertDataset` — upsert dataset row
- `TestIntegrationUpsertAnnualDataset` — upsert annual row
- `TestIntegrationRefreshMaterializedViews` — refresh views (no error)
- `TestIntegrationBatchExec` — batch multiple dataset inserts
- Cleanup: each test deletes the rows it inserted (using a test unit_id in a safe range)

---

## Key Constraints

- No new go.mod dependencies — use only `pgx/v5` and stdlib (`encoding/json`, `slog`, `time`)
- All SQL uses `$N` parameterized queries, never `fmt.Sprintf`
- Error wrapping prefix: `"dbutil: <context>: %w"`
- Keep existing `Connect()` and `RefreshMaterializedViews()` function signatures unchanged

---

## Verification

```bash
cd tools/kapow && go test -short ./internal/dbutil/... -v -race -count=1
cd tools/kapow && go vet ./...
```

Both must pass with zero errors before reporting done.

Worktree: `/home/ketan/project/kapow/.claude/worktrees/worktree/dbutil`
Branch: `worktree/dbutil`
