# flt-it8.10: Voice Capture + Transcription

## Context

Flotsam needs voice note capture: upload audio, store in S3, create a `voice_note` item, and asynchronously transcribe via Whisper. The DB schema already supports `voice_note` as an item type, `AudioKey()` exists in storage, and `TranscribeWorker` is stubbed. The provider package is empty — this is the first provider implementation.

## Implementation Plan

### 1. TranscriptionProvider interface + WhisperAPI implementation

**New file: `api/internal/provider/transcription.go`**
- `TranscribeOpts` struct: `Language string`, `Format string`
- `Transcriber` interface: `Transcribe(ctx, audio io.Reader, opts TranscribeOpts) (string, error)`

**New file: `api/internal/provider/whisper_api.go`**
- `WhisperAPIProvider` struct: `apiKey`, `model` (default "whisper-1"), `baseURL`
- `NewWhisperAPI(apiKey string) *WhisperAPIProvider`
- Implementation: multipart POST to `{baseURL}/audio/transcriptions`, parse `{"text":"..."}` response
- Use `net/http` directly (single endpoint, no SDK needed)

**New file: `api/internal/provider/whisper_local.go`**
- `WhisperLocalProvider` struct: `binaryPath`, `modelPath`
- `NewWhisperLocal(binaryPath, modelPath string) *WhisperLocalProvider`
- Implementation: write audio to temp file, `exec.CommandContext` whisper.cpp, read stdout, clean up

**New file: `api/internal/provider/transcription_test.go`**
- Mock HTTP server for WhisperAPI tests
- Test: success, bad status, timeout, empty response

### 2. Config additions

**Edit: `api/internal/config/config.go`** — add fields:
```go
WhisperModel      string `env:"WHISPER_MODEL" envDefault:"whisper-1"`
WhisperLocalBin   string `env:"WHISPER_LOCAL_BIN"`
WhisperLocalModel string `env:"WHISPER_LOCAL_MODEL"`
MaxUploadSize     int64  `env:"MAX_UPLOAD_SIZE" envDefault:"52428800"` // 50MB
```

**Edit: `.env.example`** — document new vars

### 3. Upload handler

**New file: `api/internal/router/upload.go`**
- `uploadHandler(store *storage.Client, maxSize int64) http.HandlerFunc`
- Extract claims from context (already behind Auth middleware)
- `http.MaxBytesReader` to enforce size limit
- `r.FormFile("file")` → validate MIME against allowlist: `audio/mpeg`, `audio/wav`, `audio/ogg`, `audio/webm`, `audio/mp4`, `audio/x-m4a`, `audio/flac`
- Generate temp key: `{ownerID}/uploads/{uuid}.{ext}`
- `store.Upload(ctx, key, file, contentType)`
- Return JSON: `{"key":"...", "contentType":"...", "size":N}`
- Edge: nil storage → 503

**New file: `api/internal/router/upload_test.go`**
- Tests: valid upload, bad MIME, nil storage, missing file

**Edit: `api/internal/router/router.go`** line 116
- Replace `placeholderHandler("upload")` with `uploadHandler(deps.Storage, deps.Config.MaxUploadSize)`

### 4. GraphQL schema + VoiceNoteInput

**Edit: `api/graph/schema/item.graphql`** — add:
```graphql
input VoiceNoteInput {
  audioKey: String!
  title: String
  tags: [String!]
  sensitivity: SensitivityLevel
  source: String
}
```

**Edit: `api/graph/schema/schema.graphql`** — add to Mutation:
```graphql
captureVoiceNote(input: VoiceNoteInput!): Item!
```

**Run: `make api-generate`**

### 5. Item service: CaptureVoiceNote

**Edit: `api/internal/item/model.go`** — add:
```go
type VoiceNoteInput struct {
    AudioKey    string
    Title       *string
    Tags        []string
    Sensitivity string
    Source      string
}
```

**Edit: `api/internal/item/service.go`** — add method following `CaptureBookmark` pattern:
```go
func (s *Service) CaptureVoiceNote(ctx context.Context, ownerID uuid.UUID, input VoiceNoteInput) (*Item, error)
```
- Validate `AudioKey != ""`
- Default source="api", sensitivity="normal"
- INSERT with `type='voice_note'`, `status='pending'`, `metadata='{"audio_key":"..."}'`
- Return scanned item

### 6. Resolver: CaptureVoiceNote + job enqueue

**Edit: `api/graph/resolver/resolver.go`** — add fields:
```go
RiverClient *river.Client[pgx.Tx]
Storage     *storage.Client
```

**Edit: `api/graph/resolver/schema.resolvers.go`** — implement generated stub:
- `requireAuth(ctx)`, call `ItemService.CaptureVoiceNote`, enqueue `TranscribeArgs` via RiverClient
- Include `OwnerID` in job args (add to `TranscribeArgs`)

**Edit: `api/internal/router/router.go`** — wire new Resolver fields:
```go
RiverClient: deps.RiverClient,
Storage:     deps.Storage,
```

### 7. TranscribeWorker: flesh out stub

**Edit: `api/internal/worker/transcribe.go`**
- Add `OwnerID uuid.UUID` to `TranscribeArgs`
- Add dep fields to `TranscribeWorker`: `Storage *storage.Client`, `Transcriber provider.Transcriber`, `Pool *pgxpool.Pool`
- `Work()` implementation:
  1. Download audio from S3 via `w.Storage.Download`
  2. Determine format from key extension
  3. Call `w.Transcriber.Transcribe`
  4. UPDATE item: `SET content_text=$1, status='ready', updated_at=now() WHERE id=$2 AND owner_id=$3`
  5. On error: UPDATE status='error', store error in metadata

**Edit: `api/internal/worker/registry.go`**
- Add `Deps` struct with `Storage`, `Transcriber`, `Pool`
- Change `Workers()` to `Workers(deps Deps)` — pass deps to TranscribeWorker
- Other workers remain zero-value (stubs)

**Edit: `api/cmd/flotsam-worker/main.go`**
- Initialize storage client (same pattern as flotsamd)
- Initialize WhisperAPI provider if `OPENAI_API_KEY` set
- Pass `worker.Deps{...}` to `worker.Workers(deps)`

### 8. Tests

| File | Tests |
|---|---|
| `api/internal/provider/transcription_test.go` | WhisperAPI with httptest, error paths |
| `api/internal/router/upload_test.go` | Valid upload, bad MIME, nil storage, oversize |
| `api/internal/worker/transcribe_test.go` | Worker with mock storage + mock transcriber |
| `api/internal/item/service_test.go` | Add CaptureVoiceNote test (input validation) |

### 9. Documentation

- Update `.env.example` with new config vars
- Update `README.md` if voice capture changes how things are run

## Files to modify

| File | Action |
|---|---|
| `api/internal/provider/transcription.go` | **CREATE** — interface + opts |
| `api/internal/provider/whisper_api.go` | **CREATE** — OpenAI implementation |
| `api/internal/provider/whisper_local.go` | **CREATE** — local whisper.cpp implementation |
| `api/internal/provider/transcription_test.go` | **CREATE** — unit tests |
| `api/internal/config/config.go` | **EDIT** — add 4 config fields |
| `api/internal/router/upload.go` | **CREATE** — upload handler |
| `api/internal/router/upload_test.go` | **CREATE** — upload tests |
| `api/internal/router/router.go` | **EDIT** — wire upload handler + resolver deps |
| `api/graph/schema/item.graphql` | **EDIT** — add VoiceNoteInput |
| `api/graph/schema/schema.graphql` | **EDIT** — add captureVoiceNote mutation |
| `api/internal/item/model.go` | **EDIT** — add VoiceNoteInput struct |
| `api/internal/item/service.go` | **EDIT** — add CaptureVoiceNote method |
| `api/graph/resolver/resolver.go` | **EDIT** — add RiverClient, Storage fields |
| `api/graph/resolver/schema.resolvers.go` | **EDIT** — implement CaptureVoiceNote |
| `api/internal/worker/transcribe.go` | **EDIT** — flesh out with real implementation |
| `api/internal/worker/registry.go` | **EDIT** — add Deps, pass to workers |
| `api/internal/worker/transcribe_test.go` | **CREATE** — worker unit tests |
| `api/cmd/flotsam-worker/main.go` | **EDIT** — init providers, pass deps |
| `.env.example` | **EDIT** — document new vars |

## Key reuse

- `storage.AudioKey()` — already exists for generating S3 keys
- `storage.Client.Upload/Download` — existing methods
- `CaptureBookmark` pattern — exact template for CaptureVoiceNote
- `toGraphQLItem()`, `requireAuth()`, `sensitivityToString()` — existing resolver helpers
- `scanItem()` — existing DB scanner

## Verification

```bash
make api-generate      # regenerate after schema changes
make api-test-unit     # all unit tests pass
make api-lint          # go vet + golangci-lint clean
```
