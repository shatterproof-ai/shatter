# Filter Non-Degree-Granting Institutions

## Context
Search results currently include satellite campuses, certificate-only programs, and community colleges. We need to unconditionally exclude institutions where `nces->>'degrees_awarded_highest'` is 0 (non-degree-granting), 1 (certificate only), or 2 (associate's only), keeping only Bachelor's+ (≥3).

## Approach
Add a permanent WHERE clause in `buildWhere()` that is prepended before any user-supplied filters. This is the single function all three query builders (`BuildQuery`, `BuildCountQuery`, `BuildExportQuery`) funnel through, so one change covers all search paths.

## Changes

### 1. `api/internal/search/sql.go` — `buildWhere()`
- Add a hardcoded clause as the first element of `clauses`:
  ```
  COALESCE((data #>> '{nces,degrees_awarded_highest}')::int, 0) >= 3
  ```
- No parameterized args needed — the threshold is a constant, not user input
- `COALESCE(..., 0)` ensures institutions with missing data are also excluded

### 2. `api/internal/search/search_test.go` — unit test
- Add `TestBuildWhere_PermanentDegreeFilter` verifying:
  - The degree filter clause appears in output even with zero user filters
  - The degree filter clause appears alongside user filter clauses
  - Placeholder numbering remains correct (starts at $1 for user filters since the permanent clause uses no placeholders)

## Verification
```bash
make api-test-unit && make api-lint
```
