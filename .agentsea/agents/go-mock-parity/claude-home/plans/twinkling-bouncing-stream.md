# Plan: Bookmark Provenance Persistence (flt-it8.24)

## Context
Bookmark capture accepts `selection` and `referrer` in the GraphQL input but drops them — they're never written to the database. Additionally, `content_url` is overloaded: it stores the original bookmark URL at creation, then gets overwritten with an S3 key by the page-fetch worker. The `source_url` column exists in the DB schema but is never populated. This means the original URL is permanently lost after page fetch completes.

## Changes

### 1. `api/internal/item/service.go` — `CaptureBookmark()`
- Add `source_url` to the INSERT, setting it to `input.URL`
- Build a `metadata` map from `input.Selection` and `input.Referrer` (only non-nil fields), JSON-encode it, and pass it instead of `'{}'`
- `content_url` continues to receive `input.URL` initially (backward compat; page-fetch will overwrite it with S3 key)

**SQL becomes:**
```sql
INSERT INTO items (owner_id, type, status, sensitivity, title, content_url, source_url, tags, source, metadata, auto_metadata)
VALUES ($1, 'bookmark', 'pending', $2, $3, $4, $5, $6, $7, $8, '{}')
```

### 2. No migration needed
`source_url` column and `metadata` JSONB column already exist in `00001_initial_schema.sql`.

### 3. No GraphQL schema changes needed
`sourceUrl` is already on the `Item` type. `selection` and `referrer` are already on `BookmarkInput`. The schema is correct; only the backend was dropping the data.

### 4. No page-fetch worker changes needed
The worker only updates `content_url` (S3 key), `content_text`, `title`, and `status` via `UpdateContent()`. It never touches `source_url` or `metadata`, so provenance is naturally preserved.

### 5. Tests

**`api/internal/item/service_test.go`** — Add test for metadata construction:
- Test that `CaptureBookmark` with selection+referrer builds correct metadata map
- Test that nil selection/referrer produces empty metadata `{}`
- These are validation/logic tests that don't need a DB

**`api/internal/worker/page_fetch_test.go`** — Existing tests already verify page-fetch behavior. No changes needed since page-fetch doesn't touch `source_url` or `metadata`.

## Files to modify
- `api/internal/item/service.go` (CaptureBookmark method, ~15 lines changed)
- `api/internal/item/service_test.go` (add metadata construction tests)

## Verification
```bash
make api-test-unit && make api-lint
```
