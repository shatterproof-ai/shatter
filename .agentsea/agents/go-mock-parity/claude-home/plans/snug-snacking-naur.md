# Plan: Auto-classification pipeline (flt-it8.12)

## Context
The ClassifyWorker stub exists but does nothing. We need a Classifier provider (OpenAI gpt-4o-mini) and wiring so that items get auto-classified with category, topics, people, action items, sensitivity, and summary — stored in the existing `auto_metadata` JSONB column.

## Files to create
1. `api/internal/provider/classification.go` — Classifier interface + OpenAIClassifier
2. `api/internal/provider/classification_test.go` — httptest-based tests

## Files to modify
3. `api/internal/worker/classify.go` — Real implementation with Classifier + Pool deps
4. `api/internal/worker/classify_test.go` — New test file with mock classifier
5. `api/internal/worker/registry.go` — Add Classifier to Deps, pass to ClassifyWorker
6. `api/internal/item/service.go` — Add UpdateAutoMetadata method
7. `api/cmd/flotsam-worker/main.go` — Initialize OpenAIClassifier, pass in Deps
8. `api/cmd/flotsamd/main.go` — Same (for insert-only mode, Deps still needs Classifier field)

## Implementation

### Step 1: Classification provider (`api/internal/provider/classification.go`)

Define interface and struct following existing patterns from `embedding.go`:

```go
type Classification struct {
    Category    string   `json:"category"`
    Topics      []string `json:"topics"`
    People      []string `json:"people"`
    ActionItems []string `json:"action_items"`
    Sensitivity string   `json:"sensitivity_suggestion"`
    Summary     string   `json:"summary"`
}

type Classifier interface {
    Classify(ctx context.Context, content string) (*Classification, error)
}
```

OpenAIClassifier:
- Functional options: `WithClassificationModel`, `WithClassificationHTTPClient`
- Default model: `gpt-4o-mini`
- Uses Chat Completions API with `response_format: {"type": "json_object"}`
- System prompt instructs extraction of the 6 fields
- Manual HTTP (no SDK) — consistent with OpenAIEmbedder pattern
- Truncate content to ~30k chars to stay within context
- Error wrapping: `fmt.Errorf("provider: classify: ...")`

### Step 2: Classification tests (`api/internal/provider/classification_test.go`)

Follow `embedding_test.go` pattern:
- httptest.NewServer mock returning valid JSON
- URL rewriter to redirect to test server
- Test cases: happy path, empty input, API error, malformed JSON response, truncation

### Step 3: Update ClassifyWorker (`api/internal/worker/classify.go`)

Add fields:
```go
type ClassifyWorker struct {
    river.WorkerDefaults[ClassifyArgs]
    Classifier provider.Classifier
    Pool       *pgxpool.Pool
}
```

Work() logic:
1. Return nil if Classifier is nil or Content is empty (graceful skip, like EmbedWorker)
2. Call `Classifier.Classify(ctx, job.Args.Content)`
3. Marshal Classification to `map[string]any` via JSON round-trip
4. Update DB: `UPDATE items SET auto_metadata = $1, updated_at = now() WHERE id = $2`
5. Log success with slog

### Step 4: ClassifyWorker tests (`api/internal/worker/classify_test.go`)

Follow `embed_test.go` pattern:
- Mock classifier (returns fixed Classification or error)
- Test: nil classifier → no-op, empty content → no-op, happy path, classifier error

### Step 5: UpdateAutoMetadata in item service (`api/internal/item/service.go`)

```go
func (s *Service) UpdateAutoMetadata(ctx context.Context, itemID uuid.UUID, metadata map[string]any) error
```
- Serialize metadata to JSON, UPDATE auto_metadata column
- Error wrap: `fmt.Errorf("item: update auto metadata: %w", err)`

Actually — looking at the EmbedWorker pattern, it does the DB update directly (not via service). The ClassifyWorker should do the same for consistency. Skip the service method; update DB directly in the worker. This is simpler and matches existing patterns.

### Step 6: Wire in registry.go

- Add `Classifier provider.Classifier` to `Deps` struct
- In `NewRegistry()`: `&ClassifyWorker{Classifier: deps.Classifier, Pool: deps.Pool}`

### Step 7: Wire in main binaries

In `cmd/flotsam-worker/main.go` and `cmd/flotsamd/main.go`:
- If `cfg.OpenAIAPIKey != ""`, create `provider.NewOpenAIClassifier(cfg.OpenAIAPIKey)`
- Pass to `worker.Deps{Classifier: classifier, ...}`

## Verification
```bash
make api-test-unit && make api-lint
```
