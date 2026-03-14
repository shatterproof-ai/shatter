# Plan: Alpha-sort facet options (kapow-53vy)

## Context

Facet sidebar checkboxes and dropdowns render options in registration order (the order
they appear in `registry.go`). This is inconsistent and arbitrary for most fields.
The fix: sort alphabetically by default on the frontend, but let the API signal
"preserve my defined order" for fields with a meaningful sequence (urban→rural,
educational progression, climate zones, etc.).

---

## Enum facets inventory & ordering decision

| Field | Type | Options | Decision |
|---|---|---|---|
| `locale` | SingleEnum | 12 | **Ordered** — City:Large→Rural:Remote gradient |
| `region` | SingleEnum | 9 | **Ordered** — geographic grouping (NE→Outlying) |
| `highest_degree` | SingleEnum | 5 | **Ordered** — Certificate→Doctorate progression |
| `ncaa_division` | SingleEnum | 3 | **Ordered** — Division I→II→III |
| `koppen_climate` | SingleEnum | 5 | **Ordered** — Tropical→Dry→Temperate→Continental→Polar |
| `koppen_temperature` | SingleEnum | 9 | **Ordered** — Köppen sub-type scientific ordering |
| `koppen_precipitation` | SingleEnum | 3 | **Ordered** — scientific ordering |
| `state` | MultiColumnEnum | 50 | Alpha-sort (already alphabetical via `allStateOptions()`) |
| `institution_type` | SingleEnum | 3 | Alpha-sort |
| `coeducation_status` | GenderField | 3 | Alpha-sort |
| `application_platforms` | MultiEnum | 3 | Alpha-sort |

---

## Implementation

### Step 1 — API: add `OptionsOrdered` to `FieldDescriptor`

**File**: `api/internal/search/field.go`

Add field to `FieldDescriptor` struct (after `SelectionMode`):
```go
OptionsOrdered bool `json:"options_ordered,omitempty"`
```

### Step 2 — API: add `Ordered bool` to enum field structs

**File**: `api/internal/search/field.go`

Add `Ordered bool` to all four enum field structs:
- `SingleEnumField` (line ~570)
- `GenderField` (line ~601)
- `MultiColumnEnumField` (line ~650)
- `MultiEnumField` (line ~682)

Update each struct's `Descriptor()` method to propagate the flag:
```go
d.OptionsOrdered = f.Ordered
```

### Step 3 — API: GraphQL schema

**File**: `api/graph/schema/search_fields.graphql`

Add `optionsOrdered` to `ChoiceSearchField`:
```graphql
# optionsOrdered: when true, options are in a meaningful defined order (e.g. urban→rural
# gradient, educational progression) and should be displayed as-is. When false,
# the client should sort options alphabetically.
optionsOrdered: Boolean!
```

### Step 4 — Regenerate API code

```bash
cd api && make api-generate
```

This updates `graph/generated/generated.go` and `graph/model/models_gen.go`.

### Step 5 — API: resolver helper

**File**: `api/graph/resolver/search_fields_helpers.go`

Update both `case search.KindSingleEnum` and `case search.KindMultiEnum` blocks to
populate `OptionsOrdered` from `d.OptionsOrdered`:
```go
return &model.ChoiceSearchField{
    ...
    Options:        enumOptionsToModel(d.Options),
    OptionsOrdered: d.OptionsOrdered,
    ...
}
```

### Step 6 — API: registry — mark ordered fields

**File**: `api/internal/search/registry.go`

Add `Ordered: true` to the 7 fields with meaningful ordering:
`locale`, `region`, `highest_degree`, `ncaa_division`, `koppen_climate`,
`koppen_temperature`, `koppen_precipitation`.

### Step 7 — Sync web schema

```bash
make web-schema-sync
```

Updates `web/schema.graphql` and `web/src/graphql-env.d.ts` with the new
`optionsOrdered` field.

### Step 8 — Frontend: update GraphQL query

**File**: `web/src/pages/Search.tsx`

Add `optionsOrdered` to the `ChoiceSearchField` fragment in `SearchFieldsQuery`:
```graphql
... on ChoiceSearchField {
  options {
    value
    displayName
  }
  optionsOrdered
  minSelections
  maxSelections
}
```

### Step 9 — Frontend: sort options in rendering

**File**: `web/src/components/search/FilterControl.tsx`

The `MultiSelectCheckboxes` component (line ~52) receives `field` as a prop.
Add a sort step before rendering:

```tsx
const displayOptions = field.optionsOrdered
  ? field.options
  : [...field.options].sort((a, b) => a.displayName.localeCompare(b.displayName))
```

Apply the same sort for the single-select dropdown case (the `<select>` at line ~384).

Also pass `optionsOrdered` through `FilterControl` props to `MultiSelectCheckboxes` as needed.

### Step 10 — Tests

**API test** (`api/internal/search/search_test.go` or a new `field_test.go`):
- Verify that `SingleEnumField{Ordered: true}.Descriptor().OptionsOrdered == true`
- Verify that `SingleEnumField{Ordered: false}.Descriptor().OptionsOrdered == false`

**Frontend test** (`web/src/components/search/FilterControl.test.tsx`):
- Test that options render in alphabetical order when `optionsOrdered: false`
- Test that options preserve defined order when `optionsOrdered: true`

---

## Critical files

- `api/internal/search/field.go` — `FieldDescriptor`, `SingleEnumField`, `GenderField`, `MultiColumnEnumField`, `MultiEnumField`
- `api/internal/search/registry.go` — all field definitions
- `api/graph/schema/search_fields.graphql` — `ChoiceSearchField` type
- `api/graph/resolver/search_fields_helpers.go` — `descriptorToSearchField()`
- `web/src/pages/Search.tsx` — `SearchFieldsQuery`
- `web/src/components/search/FilterControl.tsx` — `MultiSelectCheckboxes` + dropdown

---

## Verification

```bash
# API quality gate (from worktree root)
make api-test-unit && make api-lint

# Frontend quality gate
cd web && pnpm build && pnpm lint && pnpm test
```
