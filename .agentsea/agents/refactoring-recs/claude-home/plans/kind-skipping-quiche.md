# Fix Detail Card: No Data Bug + Enhanced Field Display

## Context

The institution detail card (expanded result card) shows no data when clicked. The root cause is `BuildInstitutionQuery()` in `api/internal/search/sql.go:159` which uses incorrect JSONB paths (e.g., `data->>'control'` instead of `data #>> '{nces,ownership}'`). Meanwhile, the main search query already fetches the full `data` JSONB and correctly extracts all fields via `extract*` functions in `search_data_extract.go`.

**Fix**: Eliminate the broken separate query. Pass the already-fetched search result data directly to the detail modal. Then enhance the display with themed field groups.

---

## Step 1: Fix the Bug — Pass Search Data to Detail Modal

### 1a. Update InstitutionDetail props

**File**: `web/src/components/search/InstitutionDetail.tsx`

- Change props from `{ unitID: string | null, opened, onClose }` to `{ institution: SearchInstitution | null, opened, onClose }`
- Import `SearchInstitution` from `./types`
- Remove `InstitutionDetailQuery` (graphql query), `InstitutionDetailData` interface, and `useQuery` call
- Remove loading/error states (data is already available from search results)
- Keep the "Institution not found" state for null institution

### 1b. Update Search.tsx state management

**File**: `web/src/pages/Search.tsx`

- Replace `selectedUnitID: string | null` state with `selectedInstitution: SearchInstitution | null`
- In click handlers, find institution from `institutions` array by unitID and set it
- Pass `institution={selectedInstitution}` to `<InstitutionDetail>` instead of `unitID`

---

## Step 2: Add Missing Fields to GraphQL Schema + Backend

Several fields in the issue spec exist in the NCES JSONB data but aren't in current GraphQL types. Add to the appropriate sub-types:

### 2a. Schema additions (`api/graph/schema/search.graphql`)

**InstitutionAcademics** — add:
- `carnegieBasic: Int` (nces.carnegie_basic)
- `carnegieSize: String` (nces.carnegie_size_setting — the locale/setting code)

**InstitutionCost** — add:
- `totalPrice: Int` (nces.total_price_for_in_state or out_of_state)

**InstitutionAdmissions** — add:
- `satMath25: Int`, `satMath75: Int` (nces.sat_scores_midpoint_math_25th/75th)
- `satVerbal25: Int`, `satVerbal75: Int` (nces.sat_scores_midpoint_critical_reading_25th/75th)
- `actComposite25: Int`, `actComposite75: Int` (nces.act_scores_25th/75th percentile)
- `applicationFee: Int` (nces.application_fee)

**InstitutionOutcomes** — add:
- `medianEarnings: Int` (nces.earnings_median_after_10_years or similar)

**InstitutionDemographics** (NEW type):
- `totalEnrollment: Int` (same as academics.enrollment, but semantically fits here)
- `undergradEnrollment: Int` (nces.undergrad_enrollment)
- `percentMale: Float`, `percentFemale: Float`
- `hbcu: Boolean`, `hsi: Boolean`, `aanapisi: Boolean`, `pbi: Boolean`

**InstitutionMetadata** (NEW type):
- `schoolUrl: String` (nces.insturl)
- `accreditor: String` (nces.accreditor)
- `highestDegree: String` (same as academics.highestDegree)
- `religiousAffiliation: String` (nces.religious_affiliation)

**SearchInstitution** — add:
- `demographics: InstitutionDemographics`
- `metadata: InstitutionMetadata`

### 2b. Backend extract functions (`api/graph/resolver/search_data_extract.go`)

- Add `extractDemographics(data)` and `extractMetadata(data)` functions
- Extend `extractAdmissions`, `extractCost`, `extractOutcomes` with new fields
- Add carnegie fields to `extractAcademics`

### 2c. Wire new extractors (`api/graph/resolver/search.resolvers.go`)

Add `inst.Demographics = extractDemographics(rawData)` and `inst.Metadata = extractMetadata(rawData)` in the search results scan loop (line ~142-152).

### 2d. Regenerate

```bash
make api-generate
make web-schema-sync
```

**NOTE**: Only add fields that actually exist in the NCES fixture data. Check `data` JSONB keys before adding. If a key doesn't exist, skip it — the field will just return null.

---

## Step 3: Enhance the Detail Card Display

**File**: `web/src/components/search/InstitutionDetail.tsx`

Rewrite the card body to show themed field groups. Use existing `DetailRow` component and format helpers. Only render a section if data exists.

### Themed sections (order as specified in issue):

1. **Core**: Name, type (with INSTITUTION_TYPES mapping), Carnegie Basic (code→label mapping in frontend)
2. **Location**: City/state (from top-level), locale, region, setting (carnegie_size)
3. **Academics**: Student-faculty ratio, full-time faculty rate
4. **Cost**: In-state tuition, out-of-state tuition, total price/myPrice, net price, room & board
5. **Admissions**: Acceptance rate, SAT range, ACT range, app fee, app platforms
6. **Outcomes**: Graduation rate, retention rate, median earnings, transfer rate
7. **Ranking**: All non-null rankings + blend score
8. **Athletics**: NCAA division, conference
9. **Demographics**: Total enrollment, undergrad enrollment, gender, minority-serving badges (HBCU, HSI, AANAPISI, PBI — show as Mantine `Badge` components only when true)
10. **Cultural Environment**: Partisan score/direction, religious affiliation, coeducational (from gender)
11. **Weather**: Climate group, temperature, precipitation
12. **Student Life**: Religious affiliation, coeducational, room & board
13. **Metadata**: School URL (as anchor link), accreditor, highest degree

### Code-to-label mappings (frontend constants):

- `INSTITUTION_TYPES`: Already exists — `{1: "Public", 2: "Private nonprofit", 3: "Private for-profit"}`
- `HIGHEST_DEGREES`: `{1: "Certificate", 2: "Associate's", 3: "Bachelor's", 4: "Master's", 5: "Doctorate"}`
- `LOCALE_LABELS`: `{11: "City: Large", 12: "City: Midsize", ...}` (from registry)
- `REGION_LABELS`: `{0: "U.S. Service Schools", 1: "New England", ...}`
- `NCAA_DIVISIONS`: `{1: "Division I", 2: "Division II", 3: "Division III"}`
- `KOPPEN_CLIMATE`: `{A: "Tropical", B: "Dry", C: "Temperate", D: "Continental", E: "Polar"}`

### Layout:
- `Modal` with `size="xl"` (wider than current "lg")
- Header: Name (brand.9) + City, State subtitle
- `SimpleGrid cols={{ base: 1, sm: 2 }}` for section pairs
- Each section: `Stack` with uppercase dimmed header + `DetailRow` entries
- Minority-serving badges: `Group` of `Badge variant="light"` components
- School URL: `<Anchor>` component

---

## Step 4: Update Tests

**File**: `web/src/components/search/InstitutionDetail.test.tsx`

- Remove GraphQL query mocking (no more `mockUseQuery` for InstitutionDetail itself)
- Still mock `urql` for `SimilarInstitutions` child component
- Change mock data shape from flat `InstitutionDetailData` to nested `SearchInstitution`
- Remove tests: loading state, error state, paused query
- Update tests: renders data, null institution, null optional fields
- Add tests: renders themed sections, minority-serving badges, school URL as link, code-to-label mappings, sections hidden when data is null

---

## Step 5: Backend Cleanup

- Remove `BuildInstitutionQuery()` from `api/internal/search/sql.go`
- Remove `Institution` resolver from `api/graph/resolver/search.resolvers.go`
- Remove `InstitutionDetail` type and `institution` query from `api/graph/schema/search.graphql`
- Remove `TestBuildInstitutionQuery` from `api/internal/search/search_test.go`
- Run `make api-generate && make web-schema-sync`

---

## Step 6: Verify

**Note**: Step 2 (adding missing fields) depends on checking which JSONB keys actually exist in fixture data. If keys are missing from the data, skip those fields — they can be added later when data is sourced.

### Scoping decision

For fields that don't exist in the current data model (SAT, ACT, app fee, median earnings, minority-serving, school URL, accreditor, religious affiliation, Carnegie size/setting), I'll check the fixture data JSONB to see which keys are actually present before adding them to the schema. This avoids adding dead fields.

---

## Files to Modify

| File | Change |
|------|--------|
| `web/src/components/search/InstitutionDetail.tsx` | Rewrite: remove query, accept data prop, themed display |
| `web/src/components/search/InstitutionDetail.test.tsx` | Rewrite tests for new interface |
| `web/src/pages/Search.tsx` | Pass full institution to detail modal |
| `api/graph/schema/search.graphql` | Add new fields to types, remove InstitutionDetail |
| `api/graph/resolver/search_data_extract.go` | Add new extract functions |
| `api/graph/resolver/search.resolvers.go` | Wire new extractors, remove Institution resolver |
| `api/internal/search/sql.go` | Remove BuildInstitutionQuery |
| `api/internal/search/search_test.go` | Remove TestBuildInstitutionQuery |

## Verification

```bash
cd web && pnpm build && pnpm lint && pnpm test
cd api && make test-unit
```

## Reusable Code

- `DetailRow` component — keep and reuse (InstitutionDetail.tsx:77-88)
- `formatPercent`, `formatCurrency`, `formatNumber` — keep and reuse
- `jsonbObject`, `jsonbString`, `jsonbInt`, `jsonbFloat`, `jsonbBool01`, `jsonbStringSlice` — reuse for new extract functions (search_data_extract.go)
- `INSTITUTION_TYPES` mapping — keep and reuse
- `renderWithMantine` from `web/src/test/render.tsx` — use in tests
