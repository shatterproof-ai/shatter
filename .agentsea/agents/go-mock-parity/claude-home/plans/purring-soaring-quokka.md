# Plan: Implement tools/kapow/internal/fetch

## Context

The `tools/kapow` data pipeline tool needs an HTTP download + archive infrastructure
to fetch government/third-party data files. The existing `internal/fetch/fetch.go`
is a minimal scaffold with only `New()` and `CacheDir()`. This plan implements the
full fetch package with conditional HTTP requests, archive management, xz compression,
and stale-cache resilience.

## Critical Files

- `tools/kapow/internal/fetch/fetch.go` — extend existing scaffold (keep `New`, `CacheDir`)
- `tools/kapow/internal/fetch/fetch_test.go` — new unit tests with httptest.Server
- `tools/kapow/go.mod` / `go.sum` — may need `github.com/ulikunitz/xz` for pure-Go xz

## Design

### FetchConfig (registered per importer)

```go
type FetchConfig struct {
    Name   string // e.g. "nces" — used in archive filename
    URL    string // download URL
    Format string // "csv", "csv.gz", "zip" — source format before re-compression
}
```

### Archive naming

Files stored as `{name}-{YYYY-MM-DD}.{format}.xz` in the cache dir (e.g. `nces-2026-03-09.csv.xz`).

### ETag/Last-Modified sidecar

A JSON sidecar `{name}.cache.json` stores `{"etag":"...", "last_modified":"...", "file":"..."}` alongside archives to enable conditional HTTP requests on subsequent fetches.

### Fetcher methods

**`Fetch(ctx, cfg FetchConfig) (io.ReadCloser, error)`** — main entry point:
1. Check for manually-dropped files matching `{name}-*.{format}.xz` → if found, return newest (no download)
2. Load sidecar to get prior ETag/Last-Modified
3. HTTP GET with `If-None-Match`/`If-Modified-Since` headers if sidecar exists
4. If 304 → return reader for existing archive file
5. If network error and existing archive file → log warning, return stale archive (resilience)
6. On 200: decompress source format (gzip/zip/plain), re-compress with xz, save archive + sidecar
7. Return `io.ReadCloser` of decompressed content

**`Latest(name, format string) (string, bool)`** — find newest archive file for a source (for manual drop detection).

### Compression strategy

Use `exec.Command("xz", "-9e", ...)` to compress to xz — avoids pure-Go dependency and matches required `-9e` flag. For decompression of stored `.xz` files, use `github.com/ulikunitz/xz` (pure Go, no exec needed for reads). Alternatively, use `exec.Command("xz", "-d", ...)` for decompression too — simpler, consistent.

**Decision**: Use exec for both compress and decompress (system xz). Avoids adding any dependency. In tests, use a small test file to verify roundtrip.

### Source format decompression

| Format | Library |
|--------|---------|
| `csv`  | no-op (pass through) |
| `csv.gz` / `gz` | `compress/gzip` (stdlib) |
| `zip`  | `archive/zip` (stdlib), read first entry |

### Manual file drop

Users can place `{name}-YYYY-MM-DD.{format}.xz` in the cache dir. `Latest()` globs for `{name}-*.{format}.xz`, sorts by filename (date), returns newest path.

## Implementation Steps

1. **Define types** in `fetch.go`:
   - `FetchConfig` struct
   - `cacheEntry` (internal sidecar JSON struct)

2. **Implement `Fetch(ctx, cfg)`** with the 7-step flow above.

3. **Implement helpers**:
   - `latest(dir, name, format)` — glob + sort for manual drop detection
   - `loadSidecar(path)` / `saveSidecar(path, entry)` — JSON read/write
   - `decompressSource(r io.Reader, format string)` — gzip/zip/plain
   - `compressXZ(ctx, src io.Reader, dst string)` — exec xz -9e
   - `openXZ(ctx, path string)` — exec xz -d -c (stream to stdout)

4. **Write tests** in `fetch_test.go`:
   - `TestFetch_Download` — httptest.Server returns 200 + CSV body → file created in tmpdir, content correct
   - `TestFetch_Conditional304` — second fetch sends If-None-Match, server returns 304 → no new file, returns existing
   - `TestFetch_StaleOnError` — server errors out, existing archive present → returns stale content, no error
   - `TestFetch_ManualDrop` — pre-place a `.xz` file in cache dir → fetch returns its content without HTTP call
   - `TestFetch_GzipDecompression` — server returns gzip-compressed CSV → stored as .csv.xz, returned decompressed
   - `TestLatest` — multiple files in dir, returns newest by date

## Quality Gate

```bash
cd tools/kapow && go test ./internal/fetch/... -v -race -count=1
cd tools/kapow && go vet ./...
```

Both must pass with zero errors before reporting done.

## Notes

- Keep existing `New(cacheDir string) *Fetcher` and `CacheDir() string` — main.go depends on them
- Error wrapping: `fmt.Errorf("fetch: context: %w", err)`
- Always pass `context.Context` as first param
- Tests must not require network access (httptest.Server only)
- System `xz` binary required at runtime; tests should skip if not available (`exec.LookPath("xz")`)
