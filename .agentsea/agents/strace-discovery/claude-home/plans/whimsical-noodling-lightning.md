# Plan: Köppen Temperature/Precipitation Label Resolution in Exports

## Context

CSV exports show raw letter codes (e.g., "a", "b", "f") instead of human-readable labels ("Hot summer", "Warm summer", "No dry season") for Köppen climate temperature and precipitation subtypes. The climate group field (`koppen_climate`) also has this issue. All three Köppen fields have `EnumOption` mappings defined in the registry (Value→Label), but the export handler ignores them.

## Approach

Add a `LabelForCode` method to `FieldDescriptor` in the search package, then use it in the export handler's `extractCoreOrJSONB` to resolve enum codes to labels. This is a generic solution — any enum field with Options will automatically get label resolution in exports.

## Changes

### 1. `api/internal/search/field.go` — Add `LabelForCode` method

Add to `FieldDescriptor`:

```go
// LabelForCode returns the human-readable label for an enum code.
// If no matching option is found, it returns the code unchanged.
func (d FieldDescriptor) LabelForCode(code string) string {
    for _, o := range d.Options {
        if o.Value == code {
            return o.Label
        }
    }
    return code
}
```

### 2. `api/internal/handler/export.go` — Use label resolution

Change `extractCoreOrJSONB` to accept `FieldDescriptor` (it already does) and resolve enum labels:

```go
func extractCoreOrJSONB(d search.FieldDescriptor, ...) string {
    // ... existing switch for core fields ...
    default:
        if d.OutputKey == "" {
            return ""
        }
        raw := extractJSONBValue(data, d.OutputKey)
        if raw != "" && len(d.Options) > 0 {
            return d.LabelForCode(raw)
        }
        return raw
}
```

### 3. `api/internal/search/field_test.go` — Unit tests for `LabelForCode`

Table-driven test covering:
- Known code → returns label
- Unknown code → returns code unchanged
- Empty options → returns code unchanged
- Empty code → returns empty string

### 4. `api/internal/handler/export_test.go` — Test label resolution in export extraction

Test `extractCoreOrJSONB` with a descriptor that has Options, verifying enum codes are resolved to labels.

## Files to modify

- `api/internal/search/field.go` — add `LabelForCode`
- `api/internal/handler/export.go` — use `LabelForCode` in `extractCoreOrJSONB`
- `api/internal/search/field_test.go` — unit test for `LabelForCode`
- `api/internal/handler/export_test.go` — test label resolution in export path

## Verification

```bash
make api-test-unit && make api-lint
```
