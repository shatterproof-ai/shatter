# flt-it8.3: Background Job Processing

## Context
Flotsam needs background job processing for async tasks (page fetching, transcription, embedding, classification, S3 uploads). River is the chosen PG-backed job queue. This issue implements the worker infrastructure with stub implementations — actual job logic comes in later issues.

## Files to Create

### 1. `api/internal/worker/registry.go`
- `func Workers() *river.Workers` — registers all 5 job types, returns the collection

### 2. Job type files (one per job, each with Args + Worker structs)
- `api/internal/worker/page_fetch.go` — `PageFetchArgs{ItemID, URL}`, kind `"page_fetch"`
- `api/internal/worker/transcribe.go` — `TranscribeArgs{ItemID, AudioURL, Provider}`, kind `"transcribe"`
- `api/internal/worker/embed.go` — `EmbedArgs{ItemID, Content}`, kind `"embed"`
- `api/internal/worker/classify.go` — `ClassifyArgs{ItemID, Content}`, kind `"classify"`
- `api/internal/worker/s3upload.go` — `S3UploadArgs{ItemID, Key, ContentType}`, kind `"s3_upload"`

Each `Work()` method: log job details via slog, return nil (stub).

### 3. `api/internal/worker/worker_test.go`
- Test `Workers()` returns non-nil
- Test each `Kind()` returns correct string
- Test each `Work()` stub returns nil

## Files to Modify

### 4. `api/cmd/flotsam-worker/main.go` — Replace stub
1. Load `.env` via godotenv
2. Parse config via `config.Load()`
3. Set up slog logger
4. Connect to DB via `db.Connect(ctx, cfg.DatabaseURL)`
5. Parse flags: `--watch` and `--once` (default: `--once`)
6. Create River client with `worker.Workers()` registered
7. Start River client
8. `--watch`: block on SIGINT/SIGTERM, graceful shutdown
9. `--once`: use River's brief poll then exit

### 5. `api/cmd/flotsamd/main.go` — Add River + `--with-worker`
- Parse `--with-worker` flag
- Init River client in insert-only mode (no workers registered): `river.NewClient(riverpgxv5.New(pool), &river.Config{})`
- Pass River client to `router.Deps`
- If `--with-worker`: `exec.Command("flotsam-worker", "--watch")` as child process with stdout/stderr forwarded; defer kill; log PID
- Kill child on shutdown signal

### 6. `api/internal/router/router.go` — Add RiverClient to Deps
- Add `RiverClient` field (type from River, will be `*river.Client[pgx.Tx]`)
- Replace the comment placeholder

### 7. `api/go.mod` / `api/go.sum` — Add River dependency
- `go get github.com/riverqueue/river github.com/riverqueue/river/riverdriver/riverpgxv5`

## Implementation Notes

- River requires `river_job` and `river_leader` tables. River provides a migration helper, but since we use goose, we'll use River's `rivermigrate` to install its schema. Actually, River's client auto-creates tables or we can add a migration. Check River docs — likely need `rivermigrate.New(riverpgxv5.New(pool))` in the worker startup or a goose migration that calls River's SQL.
- For insert-only mode in flotsamd, use `river.NewClient` with empty `river.Config{}` (no workers).
- For worker mode, `river.Config{Workers: worker.Workers(), Queues: map[string]river.QueueConfig{river.QueueDefault: {MaxWorkers: 100}}}`.

## Verification
```bash
cd api && go build ./...                    # all 3 binaries compile
cd api && go test -race -short ./...        # unit tests pass
cd api && go vet ./...                      # no vet issues
```
