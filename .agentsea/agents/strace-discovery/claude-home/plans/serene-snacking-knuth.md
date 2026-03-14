# Fix: Export validation rejects maxRows > MaxLimit

## Context

The CSV export handler (`api/internal/handler/export.go:37`) sets `Limit: maxRows` on the `SearchRequest` before calling `ValidateRequest`. The default `maxRows` is 500 (from `MAX_EXPORT_ROWS` config), but `ValidateRequest` (`api/internal/search/validate.go:185`) rejects any `Limit > MaxLimit` (100). This means the export handler always fails validation with the default config. `BuildExportQuery` already correctly caps the limit via its own `maxRows` parameter, so the handler just needs to avoid triggering the search-API limit check.

## Fix

**In `api/internal/handler/export.go`**: Don't set `Limit: maxRows` on the SearchRequest before validation. The `BuildExportQuery` function already receives `maxRows` as a separate parameter and caps the limit internally. The SearchRequest's Limit field is irrelevant for exports.

Change line 35-38 from:
```go
req := search.SearchRequest{
    Filters: filters,
    Limit:   maxRows,
}
```
to:
```go
req := search.SearchRequest{
    Filters: filters,
}
```

That's it. `BuildExportQuery` (called at line 80) already receives `maxRows` and applies it.

## Files to modify

1. `api/internal/handler/export.go` — remove `Limit: maxRows` from SearchRequest construction
2. `api/internal/handler/export_test.go` — add a test that reproduces the bug (maxRows=500 should not fail validation), update existing tests as needed
3. `api/internal/search/contract.go` — remove `SkipExport: true` from the "limit exceeds max" contract test case (if applicable, since the export path no longer sets a high limit)

## Test plan

1. Write a test `TestExportCSV_MaxRowsAboveSearchLimit` that creates an ExportCSV handler with `maxRows=500` and sends a valid request — assert it doesn't return 400
2. Run `make api-test-unit && make api-lint` to verify
