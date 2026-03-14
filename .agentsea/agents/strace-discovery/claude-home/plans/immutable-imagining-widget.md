# flt-it8.7: Bookmark Capture + Page Fetch

## Context

The `captureBookmark` mutation and `PageFetchWorker` stub already exist. This task wires them together: the resolver enqueues a River job after creating the bookmark, and the worker fetches the page, extracts readable content, stores HTML in S3, and updates the item.

## Changes

### 1. Add `go-readability` dependency
```bash
cd api && go get github.com/go-shiori/go-readability
```
[go-shiori/go-readability](https://github.com/go-shiori/go-readability) — most popular Go readability library (port of Mozilla Readability). No other strong candidates in the Go ecosystem.

### 2. Add `UpdateContent` method to `item.Service`
**File**: `api/internal/item/service.go`

The worker needs to update `status`, `title`, `content_text`, and `content_url` (for S3 key) after fetching. The existing `Update` method only handles user-facing fields (title, tags, sensitivity). Add:

```go
type ContentUpdate struct {
    Status      string
    Title       *string
    ContentText *string
    ContentURL  *string
}

func (s *Service) UpdateContent(ctx, ownerID, itemID, input ContentUpdate) (*Item, error)
```

This is an internal method used by workers, not exposed via GraphQL.

### 3. Wire River client into Resolver
**Files**: `api/graph/resolver/resolver.go`, `api/internal/router/router.go`

- Add `RiverClient *river.Client[pgx.Tx]` to `Resolver` struct
- Pass `deps.RiverClient` when constructing the Resolver in `router.go`

### 4. Enqueue PageFetch job in CaptureBookmark resolver
**File**: `api/graph/resolver/schema.resolvers.go`

After `r.ItemService.CaptureBookmark(...)` succeeds, enqueue:
```go
_, err = r.RiverClient.Insert(ctx, &worker.PageFetchArgs{
    ItemID: it.ID, URL: *it.ContentURL,
}, nil)
```
Log warning on enqueue failure but still return the item (don't fail the mutation).

### 5. Implement PageFetch worker
**File**: `api/internal/worker/page_fetch.go`

Add dependencies to worker struct:
```go
type PageFetchWorker struct {
    river.WorkerDefaults[PageFetchArgs]
    Pool    *pgxpool.Pool
    Storage *storage.Client // nil = skip S3 upload
}
```

`Work()` implementation:
1. Set item status to `"processing"` via `item.Service.UpdateContent`
2. HTTP GET the URL (with timeout, User-Agent)
3. Parse with `go-readability` → title, text content, HTML
4. If Storage != nil: upload HTML to S3 at key `items/{item_id}/page.html`
5. Update item: status=`"ready"`, title (if not set), content_text=extracted text, content_url=S3 key (or original URL)
6. On any error: set status to `"error"`, log, return error (River will retry)

### 6. Update worker registry to accept deps
**File**: `api/internal/worker/registry.go`

Change `Workers()` to accept deps so `PageFetchWorker` gets pool + storage:
```go
type Deps struct {
    Pool    *pgxpool.Pool
    Storage *storage.Client
}

func Workers(deps Deps) *river.Workers {
    workers := river.NewWorkers()
    river.AddWorker(workers, &PageFetchWorker{Pool: deps.Pool, Storage: deps.Storage})
    // ... other workers unchanged
}
```

**File**: `api/cmd/flotsam-worker/main.go` — pass deps when calling `Workers()`

### 7. Tests
**File**: `api/internal/worker/page_fetch_test.go`

- Test successful fetch with httptest server serving HTML
- Test fetch failure (server returns 404)
- Test with nil Storage (S3 disabled)
- Mock pgxpool via interface or test against the item service methods

**File**: `api/internal/item/service_test.go`

- Test `UpdateContent` method (unit test with mock pool or integration test)

**File**: `api/graph/resolver/schema.resolvers_test.go`

- Test CaptureBookmark enqueues a job (verify River insert called)

### 8. Update existing worker tests
**File**: `api/internal/worker/worker_test.go`

Update `TestPageFetchWorkerWork` since the worker now has real logic and deps.

## File Summary

| File | Action |
|---|---|
| `api/go.mod` | Add `go-shiori/go-readability` |
| `api/internal/item/service.go` | Add `ContentUpdate` struct + `UpdateContent` method |
| `api/internal/item/model.go` | Add `ContentUpdate` type (or keep in service.go) |
| `api/graph/resolver/resolver.go` | Add `RiverClient` field |
| `api/internal/router/router.go` | Wire `RiverClient` to resolver |
| `api/graph/resolver/schema.resolvers.go` | Enqueue PageFetch job in CaptureBookmark |
| `api/internal/worker/page_fetch.go` | Full implementation |
| `api/internal/worker/registry.go` | Accept deps, pass to PageFetchWorker |
| `api/cmd/flotsam-worker/main.go` | Pass deps to `Workers()` |
| `api/internal/worker/page_fetch_test.go` | New: worker tests with httptest |
| `api/internal/worker/worker_test.go` | Update existing stub tests |

## Verification

```bash
cd api
go get github.com/go-shiori/go-readability
make api-generate   # only if schema changes (none expected)
make api-test-unit
make api-lint
```
