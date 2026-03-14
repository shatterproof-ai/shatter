# Plan: Convert Region Field to Multi-Select (kapow-gna9)

## Context

The "Region" facet currently uses `SingleEnumField`, restricting users to one region at a time. The task converts it to `MultiEnumField` so users can select multiple regions simultaneously. The region field stores a plain text value at JSONB path `{nces, region_id}`, which is not supported by the current `MultiEnumField` (which only handles JSONB arrays and object keys). A new `JSONBStorageFormatText` variant must be added.

## Files to Modify

| File | Change |
|---|---|
| `api/internal/search/field.go` | Add `JSONBStorageFormatText` constant; add new case in `MultiEnumField.BuildSQL` |
| `api/internal/search/registry.go` | Convert region from `SingleEnumField` → `MultiEnumField` |
| `api/internal/search/search_test.go` | Add test for new `JSONBStorageFormatText` format |

**No frontend changes needed** — `web/src/components/search/FilterControl.tsx` already routes `KindMultiEnum` fields to `MultiSelectCheckboxes`, which handles checkboxes with search. The switch is automatic once the backend returns `KindMultiEnum`.

## Implementation Steps

### Step 1 — Add `JSONBStorageFormatText` constant (`field.go`)

In the storage format constants block, add:

```go
JSONBStorageFormatText JSONBStorageFormat = "text"
```

### Step 2 — Handle text format in `MultiEnumField.BuildSQL` (`field.go`)

Inside the `switch` in `BuildSQL`, before the `default` case, add:

```go
case f.StorageFormat == JSONBStorageFormatText:
    // Plain JSONB text value: data #>> '{path}' = ANY($N::text[])
    // Supports multi-level paths (e.g. {nces, region_id}).
    textRef := f.source.SQLRef() // e.g. data #>> '{nces,region_id}'
    expr = fmt.Sprintf("%s = ANY($%d::text[])", textRef, argStart)
    args = []any{filter.Values}
```

This reuses the existing `SQLRef()` helper from `source.go`, which already generates `data #>> '{nces,region_id}'` for nested JSONB paths.

### Step 3 — Convert region field (`registry.go`)

Replace the `SingleEnumField` for region with:

```go
&MultiEnumField{
    baseField: baseField{
        name: "region", label: "Region",
        description:   "NCES geographic region",
        kind:          KindMultiEnum,
        theme:         ThemeLocation,
        source:        FieldSource{Kind: SourceJSONBPath, JSONBPath: []string{"nces", "region_id"}, ValueType: JSONBText},
        visibility:    VisibilityVisible,
        searchability: SearchabilitySearchable,
        exportable:    boolPtr(true),
        outputDefault: boolPtr(false),
        sortable:      false,
        outputKey:     "locationDetails.region",
        dataSourceID:  SourceNCESIPEDS.ID,
    },
    Options: []EnumOption{
        {Value: "1", Label: "New England"},
        {Value: "2", Label: "Mid East"},
        {Value: "3", Label: "Great Lakes"},
        {Value: "4", Label: "Plains"},
        {Value: "5", Label: "Southeast"},
        {Value: "6", Label: "Southwest"},
        {Value: "7", Label: "Rocky Mountains"},
        {Value: "8", Label: "Far West"},
        {Value: "9", Label: "Outlying Areas"},
    },
    SelectionMode: SelectionModeAny,
    StorageFormat: JSONBStorageFormatText,
},
```

Key changes: `kind: KindMultiEnum`, type is `MultiEnumField`, adds `SelectionMode: SelectionModeAny`, `StorageFormat: JSONBStorageFormatText`.

### Step 4 — Add test (`search_test.go`)

Add a test parallel to `TestMultiEnumField_ArrayAny` and `TestMultiEnumField_ObjectKeyAny`:

```go
func TestMultiEnumField_TextAny(t *testing.T) {
    t.Parallel()
    reg := search.NewRegistry()
    f, err := reg.Lookup("region")
    if err != nil {
        t.Fatal(err)
    }
    expr, args, err := f.BuildSQL(mustJSON(search.MultiEnumFilter{Values: []string{"1", "5"}}), 1)
    if err != nil {
        t.Fatal(err)
    }
    if !strings.Contains(expr, "= ANY(") {
        t.Errorf("expected = ANY() operator for text/any: %s", expr)
    }
    if len(args) != 1 {
        t.Errorf("expected 1 arg, got %d", len(args))
    }
}
```

The existing `TestBuildSQLAllFields` table-driven test automatically covers region after the conversion (it calls `minimalFilter` which uses the first option value — but for `MultiEnumField` it will need to pass `MultiEnumFilter{Values: []string{...}}`; verify `minimalFilter` handles `KindMultiEnum`).

## Verification

```bash
cd /home/ketan/project/kapow/.claude/worktrees/worktree/region-multiselect
make api-test-unit && make api-lint
cd web && pnpm build && pnpm lint && pnpm test
```

Then commit and push on `worktree/region-multiselect`.
