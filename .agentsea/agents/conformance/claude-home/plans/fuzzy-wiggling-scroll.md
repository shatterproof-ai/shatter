# Plan: Registry.go Fixes — kapow-989n, kapow-ptkg, kapow-b1n0

## Context

Three related issues all targeting `api/internal/search/registry.go`:

1. **kapow-989n**: Strip `(NCES: x_y_z)` suffixes from 22 field descriptions — these are internal data-dictionary references, not user-facing text.
2. **kapow-ptkg**: Rename the "gender" facet to "Coeducation Status" with updated label/description and option labels.
3. **kapow-b1n0**: Remove "U.S. Service Schools" (region_id=0) from Region enum options and fix the underlying data for the 5 affected institutions via a database migration.

All work happens in a worktree branch (`worktree/registry-fixes`).

---

## Worktree Setup

```bash
REPO_ROOT="$(git rev-parse --show-toplevel)"
BRANCH_NAME="worktree/registry-fixes"
WORKTREE_DIR="$REPO_ROOT/.claude/worktrees/$BRANCH_NAME"
git worktree add "$WORKTREE_DIR" -b "$BRANCH_NAME"
cd "$WORKTREE_DIR"
```

---

## Step 1 — Strip NCES references from descriptions (kapow-989n)

**File**: `api/internal/search/registry.go`

Strip the ` (NCES: xxx)` suffix (including the leading space) from all 22 description strings. The descriptions should remain meaningful without the NCES field name appended. Example changes:

| Field | Before | After |
|---|---|---|
| `locale` | `"IPEDS locale code describing the campus setting (NCES: locale)"` | `"IPEDS locale code describing the campus setting"` |
| `region` | `"NCES geographic region (NCES: region_id)"` | `"NCES geographic region"` |
| `institution_type` | `"Control / ownership type of the institution (NCES: ownership)"` | `"Control / ownership type of the institution"` |
| `highest_degree` | `"Highest degree awarded by the institution (NCES: degrees_awarded_highest)"` | `"Highest degree awarded by the institution"` |
| `enrollment` | `"Total undergraduate enrollment (NCES: size)"` | `"Total undergraduate enrollment"` |
| `grad_enrollment` | `"Total graduate student enrollment (NCES: grad_students)"` | `"Total graduate student enrollment"` |
| `student_faculty_ratio` | `"Student-to-faculty ratio (NCES: demographics_student_faculty_ratio)"` | `"Student-to-faculty ratio"` |
| `full_time_faculty_rate` | `"Share of faculty employed full-time (NCES: ft_faculty_rate)"` | `"Share of faculty employed full-time"` |
| `online_only` | `"Whether the institution is online-only (NCES: online_only)"` | `"Whether the institution is online-only"` |
| `carnegie_basic` | `"Carnegie Basic Classification code (NCES: carnegie_basic)"` | `"Carnegie Basic Classification code"` |
| `avg_net_price` | `"Average net price paid by students (NCES: avg_net_price)"` | `"Average net price paid by students"` |
| `tuition_in_state` | `"In-state tuition and fees (NCES: tuition_in_state)"` | `"In-state tuition and fees"` |
| `tuition_out_of_state` | `"Out-of-state tuition and fees (NCES: tuition_out_of_state)"` | `"Out-of-state tuition and fees"` |
| `room_and_board` | `"On-campus room and board cost (NCES: roomboard_oncampus)"` | `"On-campus room and board cost"` |
| `avg_net_price_0_30k` | `"Average net price for students with family income... (NCES: net_price_by_income_level_0-30000)"` | (strip suffix) |
| `pell_grant_rate` | `"Share of undergraduates receiving Pell grants (NCES: pell_grant_rate)"` | `"Share of undergraduates receiving Pell grants"` |
| `federal_loan_rate` | `"Share of undergraduates receiving federal loans (NCES: federal_loan_rate)"` | `"Share of undergraduates receiving federal loans"` |
| `endowment` | `"End-of-year endowment value (NCES: endowment_end)"` | `"End-of-year endowment value"` |
| `admission_rate` | `"Overall admission rate (NCES: admission_rate_overall)"` | `"Overall admission rate"` |
| `graduation_rate` | `"4-year 150% graduation rate (NCES: completion_rate_4yr_150nt)"` | `"4-year 150% graduation rate"` |
| `retention_rate` | `"First-year retention rate for full-time students (NCES: retention_rate_four_year_full_time_pooled)"` | `"First-year retention rate for full-time students"` |
| `transfer_rate` | `"4-year transfer-out rate for full-time students (NCES: transfer_rate_4yr_full_time_pooled)"` | `"4-year transfer-out rate for full-time students"` |

---

## Step 2 — Rename gender facet to Coeducation Status (kapow-ptkg)

**File**: `api/internal/search/registry.go` — `GenderField` block (lines 401-421)

Changes:
- `name`: `"gender"` → `"coeducation_status"` (snake_case of new label)
- `label`: `"Gender"` → `"Coeducation Status"`
- `description`: `"Gender composition of the student body (coed, men-only, or women-only)"` → `"Coeducational status of the institution"`
- `outputKey`: `"academics.gender"` → `"academics.coeducationStatus"`
- Options:
  - `{Value: "coed", Label: "Coed"}` → `{Value: "coed", Label: "Coeducational"}`
  - `{Value: "men-only", Label: "Men Only"}` → unchanged
  - `{Value: "women-only", Label: "Women Only"}` → unchanged

Note: Option *values* (`coed`, `men-only`, `women-only`) are NOT changed since they map directly to the GenderField SQL logic in `field.go` which checks `nces.men_only` / `nces.women_only` JSONB booleans.

---

## Step 3 — Remove "U.S. Service Schools" from Region enum (kapow-b1n0)

### 3a. registry.go change

**File**: `api/internal/search/registry.go` — `SingleEnumField` for `region` (lines 217-244)

- Update `description`: `"NCES geographic region (NCES: region_id)"` → `"NCES geographic region"` (also covered by Step 1)
- Remove the option: `{Value: "0", Label: "U.S. Service Schools"}`

### 3b. Migration: `api/migrations/00008_remap_service_schools_region.sql`

Create a new Goose migration to update `nces.region_id` in the JSONB column for the 5 service school institutions:

```sql
-- +goose Up
-- +goose StatementBegin
-- Remap the 5 US service academies from region_id=0 ("U.S. Service Schools")
-- to their actual NCES geographic regions. Region 0 is not a real geographic
-- region and is being removed from the Region facet options.
UPDATE search_base.institution
SET nces = jsonb_set(nces, '{region_id}', '"7"')
WHERE institution_name = 'United States Air Force Academy';  -- CO → Rocky Mountains (7)

UPDATE search_base.institution
SET nces = jsonb_set(nces, '{region_id}', '"1"')
WHERE institution_name = 'United States Coast Guard Academy';  -- CT → New England (1)

UPDATE search_base.institution
SET nces = jsonb_set(nces, '{region_id}', '"2"')
WHERE institution_name IN (
    'United States Merchant Marine Academy',   -- NY → Mid East (2)
    'United States Military Academy',           -- NY → Mid East (2)
    'United States Naval Academy'               -- MD → Mid East (2)
);
-- +goose StatementEnd

-- +goose Down
-- +goose StatementBegin
-- Restore region_id=0 for the 5 US service academies
UPDATE search_base.institution
SET nces = jsonb_set(nces, '{region_id}', '"0"')
WHERE institution_name IN (
    'United States Air Force Academy',
    'United States Coast Guard Academy',
    'United States Merchant Marine Academy',
    'United States Military Academy',
    'United States Naval Academy'
);
-- +goose StatementEnd
```

### 3c. Fixture data

Run `make test-fixture` after changes. If any of the 5 service schools appear in `common/src/main/resources/data/sample-100.sql`, those rows' `nces.region_id` values must be updated manually to match (the migration won't run against the fixture DB used in tests).

---

## Step 4 — Update tests

**File**: `api/internal/search/search_test.go`

- Search for any test cases referencing `"gender"` field name — update to `"coeducation_status"`
- Search for `"coed"` label or region value `"0"` / `"U.S. Service Schools"` — update/remove
- Check `TestBuildSQLAllFields` table-driven test to ensure it still covers GenderField with the new field name

---

## Critical Files

| File | Change |
|---|---|
| `api/internal/search/registry.go` | Strip NCES descriptions, rename gender field, remove region=0 option |
| `api/internal/search/search_test.go` | Update field name references from `gender` → `coeducation_status` |
| `api/migrations/00008_remap_service_schools_region.sql` | New migration to remap 5 institutions |
| `common/src/main/resources/data/sample-100.sql` | Update if service schools appear in fixture |

---

## Verification

```bash
# From worktree root:
make api-test-unit  # all unit tests must pass
make api-lint       # golangci-lint + go vet — zero warnings

# If DB is available:
make api-migrate-up   # apply the new migration
make test-fixture     # validate fixture data loads cleanly
```

---

## Commit

```
feat: strip NCES refs from descriptions, rename gender facet, remove service schools region

- kapow-989n: Strip (NCES: x_y_z) suffix from all 22 field descriptions
- kapow-ptkg: Rename gender facet to "Coeducation Status"; option "Coed" → "Coeducational"
- kapow-b1n0: Remove region_id=0 from Region enum; migrate 5 service academies to geographic regions
```
