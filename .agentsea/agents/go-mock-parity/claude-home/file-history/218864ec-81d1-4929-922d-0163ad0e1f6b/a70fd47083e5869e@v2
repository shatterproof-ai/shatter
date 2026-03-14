# Favicon Scraper Tool

## Context

Kapow has no institution logos/favicons. The NCES JSONB data includes
`school_url` for 6,038 of 6,040 institutions (e.g., `"www.cacc.edu/"`).
Clearbit Logo API was discontinued Dec 2025. Scraping favicons from
institution websites is the best current option.

## Tool: `tools/favicon/`

Standalone Go module (like `tools/samplesql/`). Reads institution URLs from
PostgreSQL, scrapes favicons, normalizes to 64x64 PNG, writes to output dir.

### CLI

```
go run . [flags]

  --database-url string   PostgreSQL DSN (default: $DATABASE_URL)
  --output-dir string     Output directory (default: "favicons")
  --concurrency int       Concurrent workers (default: 10)
  --timeout duration      Per-request timeout (default: 15s)
  --size int              Output image size, square (default: 64)
  --force                 Re-fetch existing files (default: false)
  --limit int             Process only N institutions (0 = all)
  --unit-ids string       Comma-separated specific unit_ids
  --google-fallback       Use Google favicon service as last resort (default: true)
  --log-level string      debug|info|warn|error (default: "info")
```

### Resolution Algorithm

For each institution:

1. **Normalize URL** — trim, prepend `https://` if no scheme, parse/validate
2. **Skip if exists** — check `{output_dir}/{unit_id}.png` unless `--force`
3. **Fetch homepage** — GET with 15s timeout, follow redirects; if HTTPS
   fails with TLS error, retry HTTP; if www fails, try without (and vice versa).
   Read up to 1MB.
4. **Extract icon candidates from HTML** — parse `<link>` tags, score by type:
   - `apple-touch-icon`: priority 100 (usually 180x180 PNG, best quality)
   - `icon type="image/png"`: priority 80
   - `icon` (no type): priority 60
   - `shortcut icon`: priority 50
   - `icon type="image/svg+xml"`: priority 10 (can't rasterize without CGO)
   - Within same priority, prefer larger `sizes` attribute
   - Resolve relative URLs against final (post-redirect) page URL
5. **Try candidates in priority order** — GET each, decode image
   (PNG/JPEG/GIF/ICO/BMP), accept if >= 16x16, resize to 64x64 Lanczos, save
   as PNG
6. **Fallback: `/favicon.ico`** — try `{scheme}://{host}/favicon.ico`, decode
   ICO (pick largest embedded size)
7. **Fallback: Google** — `https://www.google.com/s2/favicons?domain={domain}&sz=128`,
   decode PNG, skip if it's the generic globe icon (compare hash)
8. **Report failure** — append to `errors.jsonl`

### File Structure

```
tools/favicon/
  go.mod               # standalone module
  main.go              # CLI, DB query, worker pool orchestration, summary
  scraper.go           # URL normalization, resolution pipeline, candidate scoring
  scraper_test.go      # URL normalization edge cases, httptest integration
  downloader.go        # HTTP client, TLS/www fallback, retry
  downloader_test.go
  html.go              # HTML tokenizer, <link> extraction, scoring/sorting
  html_test.go         # fixture HTML snippets
  image.go             # multi-format decode (register ICO), resize, PNG encode
  image_test.go
  report.go            # errors.jsonl writer, summary stats
  testdata/            # fixture HTML, test ICO/PNG files
```

~1,300 lines estimated.

### Dependencies

```
github.com/jackc/pgx/v5          # DB access
github.com/disintegration/imaging # resize + format conversion (no CGO)
github.com/biessek/golang-ico     # ICO decoder (registers with image pkg)
golang.org/x/net                  # HTML tokenizer
```

### Concurrency & Error Handling

- 10 concurrent workers (configurable via `--concurrency`) via buffered channel + WaitGroup
- No per-domain rate limiting (each institution is a unique domain)
- 15s timeout per request; 5 redirect limit (http.Client default)
- TLS errors: retry with HTTP
- HTTP 5xx: retry once with 2s backoff
- DNS/connection failures: log WARN, skip to fallback
- Progress: log every 100 institutions with running totals
- Final summary: `"5842 success, 196 failed, 6038 total (97% hit rate)"`
- `errors.jsonl`: structured failure details per institution

### Downstream Integration (not in scope for this tool)

Favicons will be served via `GET /api/favicon/{unit_id}.png` as a static
file route in the API. Frontend constructs URL from unit_id — no GraphQL
schema change needed. This is a separate task.

### Open Questions

1. **Output directory location** — `common/favicons/`? Or a separate
   deploy artifact referenced by env var? Suggest gitignored dir that gets
   deployed separately (6K PNGs at 64x64 ~ 10-20MB total).
2. **Google globe detection** — worth hashing the generic globe icon to
   filter it out, or accept it for the ~3-5% failure case?
3. **SVG-only sites** — punt to Google fallback (SVG rasterization needs
   CGO). Acceptable?

## Verification

1. `cd tools/favicon && go build ./...` — compiles
2. `cd tools/favicon && go test ./...` — unit tests pass
3. `go run . --database-url "$DATABASE_URL" --limit 10 --log-level debug` — test run on 10 schools
4. Inspect output PNGs: correct size, reasonable quality, no corrupt files
5. Full run: `go run . --database-url "$DATABASE_URL" --output-dir ../../common/favicons`
6. Check `errors.jsonl` for failure rate (target: < 5%)
