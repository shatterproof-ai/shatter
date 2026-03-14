# Plan: Export auth-aware validation (kapow-lgd)

## Context

The CSV export handler (`/api/export/csv`) uses `OptionalAuth` middleware, meaning it accepts both authenticated and anonymous requests. However, line 70 of `export.go` hardcodes `authenticated=true` when calling `search.ValidateRequest`, bypassing field-level access controls. While no RESTRICTED fields exist today, this policy is wrong and will silently grant anonymous users access to restricted filters as soon as any are added.

The GraphQL search resolver (`search.resolvers.go:83-84`) already does this correctly — it reads `auth.GetClaims(ctx) != nil` and passes the real auth state.

## Changes

### 1. Fix `api/internal/handler/export.go` (lines 68-70)

- Add import: `"github.com/ketang/kapow/api/internal/auth"`
- Replace the hardcoded `true`:
  ```go
  // Validate the request using the caller's real auth state.
  authenticated := auth.GetClaims(r.Context()) != nil
  if errs := search.ValidateRequest(reg, req, authenticated); len(errs) > 0 {
  ```

### 2. Add `NewRegistryFromFields` in `api/internal/search/registry.go`

The `baseField` struct is unexported, so external packages can't construct fields. But `SingleEnumField` is exported. The problem is its embedded `baseField` fields are unexported — external test code can't set `searchability`.

**Solution**: Add a minimal exported constructor to `registry.go`:

```go
// NewRegistryFromFields builds a Registry from an explicit list of fields.
// Intended for testing.
func NewRegistryFromFields(fields []Field) *Registry {
    r := &Registry{fields: make(map[string]Field)}
    for _, f := range fields {
        r.fields[f.Name()] = f
    }
    return r
}
```

And add a test-only field builder in a `_test.go` file within the search package (so it can access unexported fields), exported for use by other packages' tests:

Actually, simpler: add a `testfield_test.go` in the **handler** package won't work (can't access unexported `baseField`). Instead, add a small exported helper in `search/` package — a `NewTestSingleEnumField` function in a file like `search/testing.go`:

```go
// NewTestSingleEnumField creates a SingleEnumField for use in tests.
func NewTestSingleEnumField(name string, searchability FieldSearchability, options []EnumOption) *SingleEnumField {
    return &SingleEnumField{
        baseField: baseField{
            name:          name,
            label:         name,
            kind:          FacetKindDropdown,
            theme:         ThemeOther,
            source:        SourceColumn("test_col"),
            visibility:    VisibilityVisible,
            searchability: searchability,
            exportable:    ptr(false),
            outputDefault: ptr(false),
        },
        Options: options,
    }
}
```

### 3. Add tests in `api/internal/handler/export_test.go`

Two new test functions using a custom registry with a RESTRICTED `SingleEnumField`:

- **`TestExportCSV_RestrictedField_Anonymous`**: request with restricted field filter, no auth context → 400 with `"field requires authentication"` error
- **`TestExportCSV_RestrictedField_Authenticated`**: same filter with `auth.SetClaims(ctx, &auth.Claims{Subject: "user1"})` → passes validation (panics or errors on nil DB pool, but never returns 400 validation error — test uses `recover` or checks status != 400)

## Files to modify

| File | Change |
|---|---|
| `api/internal/handler/export.go` | Replace hardcoded `true` with `auth.GetClaims(r.Context()) != nil` |
| `api/internal/handler/export_test.go` | Add restricted-field auth tests |
| `api/internal/search/registry.go` | Add `NewRegistryFromFields([]Field) *Registry` |
| `api/internal/search/testing.go` | Add `NewTestSingleEnumField` helper |

## Verification

```bash
make api-test-unit && make api-lint
```

- New tests pass
- Existing export + search tests still pass
- No lint warnings
