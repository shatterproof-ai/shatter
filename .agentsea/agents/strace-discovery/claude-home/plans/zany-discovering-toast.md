# Plan: S3 Storage Integration (flt-it8.6)

## Context
Flotsam needs an S3-compatible storage abstraction for uploading bookmark HTML, voice note audio, and generic files. Config fields and .env.example already exist. The `aws-sdk-go-v2` SDK needs to be added to go.mod.

## Files to Create

### 1. `api/internal/storage/storage.go`
- `Config` struct (Endpoint, Bucket, AccessKey, SecretKey, Region, ForcePathStyle)
- `Client` struct wrapping `*s3.Client` + `*s3.PresignClient` + bucket string
- `New(cfg Config) (*Client, error)` — builds AWS config with custom endpoint resolver, static credentials, path-style option
- `Upload(ctx, key, body io.Reader, contentType string) error` — `PutObject`
- `Download(ctx, key) (io.ReadCloser, error)` — `GetObject`
- `Delete(ctx, key) error` — `DeleteObject`
- `Exists(ctx, key) (bool, error)` — `HeadObject`, return false on `NotFound`
- `PresignedURL(ctx, key, expiry time.Duration) (string, error)` — presign `GetObject`

### 2. `api/internal/storage/keys.go`
- `KeyFor(ownerID uuid.UUID, itemType string, itemID uuid.UUID, filename string) string`
- `BookmarkKey(ownerID, itemID uuid.UUID) string` → `{owner_id}/bookmarks/{item_id}/content.html`
- `AudioKey(ownerID, itemID uuid.UUID, format string) string` → `{owner_id}/voice_notes/{item_id}/audio.{format}`
- `FileKey(ownerID, itemID uuid.UUID, filename string) string` → `{owner_id}/files/{item_id}/{filename}`

### 3. `api/internal/storage/keys_test.go`
- Table-driven tests for all key generation functions

### 4. `api/internal/storage/storage_test.go`
- Unit tests: `TestNew` with valid/invalid configs, test that `New` returns error on empty bucket
- Integration tests (guarded by `testing.Short()` + `S3_ENDPOINT` env): Upload → Exists → Download → Delete round-trip against MinIO

## Files to Modify

### 5. `api/internal/router/router.go`
- Add `Storage *storage.Client` to `Deps` struct (imported from `internal/storage`)

### 6. `api/cmd/flotsamd/main.go`
- After DB connect (step 4), add storage init (step 5):
  - If `cfg.S3Endpoint == ""` → log warning, skip
  - Otherwise call `storage.New(...)`, pass to `router.Deps`

### 7. `api/go.mod` / `api/go.sum`
- `go get` to add `aws-sdk-go-v2` core + `s3`, `config`, `credentials` packages

## Dependencies (aws-sdk-go-v2)
```
github.com/aws/aws-sdk-go-v2
github.com/aws/aws-sdk-go-v2/config
github.com/aws/aws-sdk-go-v2/credentials
github.com/aws/aws-sdk-go-v2/service/s3
```

## Verification
```bash
cd /home/ketan/project/flotsam/.claude/worktrees/s3-storage
make api-test-unit && make api-lint
```
