# Flotsam Auto-Classification Pipeline - Exploration Report

## Summary of Current Codebase State

### 1. Configuration (`api/internal/config/config.go`)

**AI Provider Configuration Fields:**
- `OpenAIAPIKey` (line 28) — Required for embeddings and transcription
- `EmbeddingModel` (line 29) — Default: `text-embedding-3-small`
- `OllamaURL` (line 30) — Optional local AI fallback (not yet implemented)
- `WhisperModel` (line 32) — Default: `whisper-1` (transcription)
- `WhisperLocalBin` (line 33) — Optional local whisper.cpp binary
- `WhisperLocalModel` (line 34) — Optional local whisper model

**Gap:** No classification-specific config fields (e.g., `CLASSIFICATION_MODEL`, `CLASSIFICATION_PROVIDER`)

---

### 2. Provider Interfaces (`api/internal/provider/`)

**Existing Interfaces:**

1. **Embedder** (embedding.go, lines 14-16)
   - `Embed(ctx context.Context, text string) ([]float32, error)`
   - `Dimensions() int`
   - Implementation: `OpenAIEmbedder` (uses text-embedding-3-small API)

2. **Transcriber** (transcription.go, lines 15-16)
   - `Transcribe(ctx context.Context, audio io.Reader, opts TranscribeOpts) (string, error)`
   - Implementations: `WhisperAPI` and `WhisperLocal`

3. **Classifier** — **MISSING**
   - Need to define interface for classification provider
   - Should follow same pattern as Embedder/Transcriber

**Expected Classification Interface:**
```go
type Classifier interface {
    Classify(ctx context.Context, content string) (*Classification, error)
}

type Classification struct {
    Category string   // e.g., "work", "personal", "research"
    People   []string // extracted person names
    Topics   []string // extracted topics/keywords
    Summary  string   // brief summary
}
```

---

### 3. Worker Initialization

**flotsam-worker (`cmd/flotsam-worker/main.go`)**
- Lines 96-104: Embedder initialization (conditional on `OPENAI_API_KEY`)
- No classifier provider initialization yet
- Passes `provider.Embedder` to `worker.Deps` (line 111)

**flotsamd (`cmd/flotsamd/main.go`)**
- Lines 98-105: Same embedder initialization pattern
- Passes `provider.Embedder` to `router.Deps` (line 117)
- No classifier initialization

---

### 4. Worker Registry (`api/internal/worker/registry.go`)

**Current Structure:**
- `Deps` struct (lines 14-19): Holds `Embedder` but NO classifier
- `Registry.NewRegistry()` (lines 37-62):
  - Registers: PageFetch, Transcribe, Embed, Classify, S3Upload workers
  - `ClassifyWorker` registered (line 54) but **empty implementation** (just logs)

**Key Issue:** `ClassifyWorker.Work()` (classify.go, lines 24-29) is a stub:
```go
func (w *ClassifyWorker) Work(ctx context.Context, job *river.Job[ClassifyArgs]) error {
    slog.InfoContext(ctx, "worker: classify: processing", ...)
    return nil  // ← Does nothing!
}
```

---

### 5. Current ClassifyWorker (`api/internal/worker/classify.go`)

**Structure:**
- `ClassifyArgs` (lines 12-15): ItemID + Content
- `ClassifyWorker` (lines 20-22): Embeds `river.WorkerDefaults[ClassifyArgs]`
- **No dependencies**: Missing access to a classification provider

**Pattern to follow from EmbedWorker:**
- Check if provider is nil → warn and skip
- Call provider with content
- Update database with results

---

### 6. Router Dependency Injection (`api/internal/router/router.go`)

**Current `Deps` struct (lines 33-43):**
```go
type Deps struct {
    Config      *config.Config
    Pool        *pgxpool.Pool
    Validator   auth.Validator
    Storage     *storage.Client
    RiverClient *river.Client[pgx.Tx]
    Items       *item.Service
    Users       *user.Service
    Issuer      *auth.Issuer
    Embedder    provider.Embedder  // Optional, nil if no OPENAI_API_KEY
}
```

**Gap:** No `Classifier` field

---

### 7. Database Schema (`migrations/00001_initial_schema.sql`)

**Items Table Relevant Fields:**
- `auto_metadata JSONB` (line 51) — Where classification results should be stored
- `embedding vector(1536)` (line 44) — For semantic search
- No dedicated columns for classification fields

**Current approach:** Store classification results (category, people, topics) in `auto_metadata` JSONB field

**Example structure:**
```json
{
  "category": "work",
  "people": ["Alice", "Bob"],
  "topics": ["AI", "embeddings"],
  "summary": "Discussion about embedding models for classification"
}
```

---

### 8. GraphQL Schema

**Item Type** (graph/schema/item.graphql):
- Exposes `autoMetadata: JSON!` (line 11)
- Mutation endpoints: createBookmark, createNote, createVoiceNote, updateItem
- No specific classification fields in schema (stored in JSONB)

**No mutations or queries to trigger classification explicitly** — Only happens via River job queue

---

### 9. Dependencies in go.mod

**Current state:**
- No OpenAI Go library imported
- HTTP calls made manually using `net/http` + JSON marshaling (see embedding.go pattern)
- **Options for classification provider:**
  1. Manual HTTP calls (follow existing Embedder/Transcriber pattern)
  2. Add `github.com/openai/go-openai` library
  3. Build minimal wrapper for specific classification endpoint

---

## Implementation Requirements for Classification Pipeline

### Phase 1: Define Provider Interface
1. Create `api/internal/provider/classification.go`
2. Define `Classifier` interface + `Classification` struct
3. Create `OpenAIClassifier` implementation (following Embedder pattern)
4. Add config: `CLASSIFICATION_MODEL` env var

### Phase 2: Wire Dependencies
1. Update `config.Config` with classification model config
2. Initialize classifier in `cmd/flotsamd/main.go` (optional, like Embedder)
3. Initialize classifier in `cmd/flotsam-worker/main.go`
4. Add `Classifier` to `worker.Deps`
5. Add `Classifier` to `router.Deps`

### Phase 3: Implement ClassifyWorker
1. Update `ClassifyWorker` struct to include `Classifier` + `Pool`
2. Implement `Work()` method:
   - Skip if no classifier configured
   - Skip if content empty
   - Call classifier.Classify(content)
   - Update items table `auto_metadata` with results
   - Log completion

### Phase 4: Integrate with Item Creation Flow
1. Update resolver/schema.resolvers.go to enqueue Classify jobs
2. Classify happens AFTER content is ready (after PageFetch or audio transcription)
3. Update item status progression if needed

### Phase 5: Testing
1. Unit tests for OpenAIClassifier (with mocked HTTP)
2. Integration tests for ClassifyWorker with real DB
3. Test classification job enqueuing from mutations

---

## Key Architectural Decisions

### 1. Provider Pattern
All AI providers (Embedder, Transcriber, Classifier) follow:
- Interface-based design (pluggable)
- Constructor with options (e.g., `WithEmbeddingModel()`)
- Consistent error wrapping: `fmt.Errorf("provider: action: context: %w", err)`
- Optional initialization in main (skip if API key not set)

### 2. Worker Pattern
All workers follow:
- Struct embeds `river.WorkerDefaults[Args]`
- `Kind()` string method returns job type
- `Work()` processes the job
- Dependencies injected via struct fields
- Check for nil providers → warn & skip vs. return error

### 3. Classification Storage
Results stored in existing `auto_metadata` JSONB column:
- No schema migration needed
- Flexible for future classification variations
- Queryable via PostgreSQL JSONB operators

### 4. Manual HTTP vs. Library
Current codebase uses manual HTTP (no go-openai library):
- Pros: Minimal dependencies, simple request/response handling
- Cons: Manual JSON marshaling, error handling per API
- **Recommendation:** Continue with manual HTTP for consistency with Embedder

---

## Files to Create/Modify

### To Create:
1. `api/internal/provider/classification.go` — Classifier interface + OpenAIClassifier
2. `api/internal/provider/classification_test.go` — Unit tests with mocked HTTP

### To Modify:
1. `api/internal/config/config.go` — Add classification model config
2. `api/cmd/flotsamd/main.go` — Initialize classifier
3. `api/cmd/flotsam-worker/main.go` — Initialize classifier
4. `api/internal/worker/registry.go` — Add Classifier to Deps, pass to ClassifyWorker
5. `api/internal/worker/classify.go` — Implement Work() method
6. `api/internal/router/router.go` — Add Classifier to Deps
7. `.env.example` — Document CLASSIFICATION_MODEL config
8. Test files (existing stubs to be filled)

---

## Unresolved Questions / Next Steps

1. **Classification Prompt/System Message:** What exact system prompt should be used for classification? Examples:
   - Extract: category, people, topics, summary?
   - Use structured JSON output or parse unstructured response?
   - Should sensitivity level influence classification behavior?

2. **Sensitivity Level Routing:** Should classification use:
   - OpenAI for non-sensitive items
   - Local provider (Ollama) for sensitive items?
   - Or always use configured provider?

3. **Classification Model Choice:** Which OpenAI model?
   - `gpt-4o-mini` (cheaper, good for classification)
   - `gpt-4` (more powerful but expensive)
   - `gpt-3.5-turbo` (older, could still work)

4. **Batch Classification:** Should EmbedWorker also batch items for efficiency, or process one at a time?

5. **Job Dependency Chain:** Should classification always run after page fetch succeeds, or be optional based on item type?

