# flt-it8.8: Embedding + Semantic Search

## Context

Flotsam items (bookmarks, notes, voice notes) currently support full-text search via PostgreSQL tsvector. The `embedding vector(1536)` column and IVFFlat index already exist in the DB schema but are never populated. The `EmbedWorker` is stubbed out (logs and returns nil). The `threshold` parameter exists in the GraphQL `search` query but is unused.

This change adds: (1) OpenAI embedding provider, (2) functional embed worker, (3) hybrid FTS+semantic search, (4) embedding enqueue from capture points.

## Implementation

### 1. Add pgvector-go dependency
```bash
cd api && go get github.com/pgvector/pgvector-go
```

### 2. Embedder interface — `api/internal/provider/embedding.go` (new)
- `Embedder` interface: `Embed(ctx, text) ([]float32, error)`, `EmbedBatch(ctx, texts) ([][]float32, error)`, `Dimensions() int`
- Follow `transcription.go` pattern (interface + opts in own file)

### 3. OpenAI implementation — `api/internal/provider/openai_embedding.go` (new)
- `OpenAIEmbeddingProvider` struct with apiKey, model, baseURL, dimensions, client
- Constructor `NewOpenAIEmbedding(apiKey, ...OpenAIEmbeddingOption)` with functional options
- Defaults: model `text-embedding-3-small`, 1536 dims, `https://api.openai.com/v1`
- POST to `/embeddings` endpoint, parse response `data[].embedding`
- Truncate content at 30k chars (safety for 8191-token model)
- Options: `WithEmbeddingModel`, `WithEmbeddingBaseURL`, `WithEmbeddingHTTPClient`

### 4. Provider tests — `api/internal/provider/openai_embedding_test.go` (new)
- httptest server mocking OpenAI response
- Cases: success, batch, API error, empty input, truncation, context cancel

### 5. Fill in EmbedWorker — `api/internal/worker/embed.go` (modify)
- Add fields: `Embedder provider.Embedder`, `Pool *pgxpool.Pool`
- `Work`: guard nil embedder, guard empty content, call `Embed()`, UPDATE with `pgvector.NewVector()`
- SQL: `UPDATE items SET embedding = $1::vector, updated_at = now() WHERE id = $2`

### 6. Update worker registry — `api/internal/worker/registry.go` (modify)
- Add `Embedder provider.Embedder` to `Deps` struct
- Pass `Embedder` and `Pool` to `EmbedWorker{}` registration

### 7. Enqueue embed jobs from capture/worker pipelines
**a) `api/graph/resolver/schema.resolvers.go`** — CaptureNote: enqueue embed job (content is immediately available)
**b) `api/internal/worker/page_fetch.go`** — after successful content update, enqueue embed via RiverClient on worker Deps
**c) `api/internal/worker/transcribe.go`** — after successful transcription, enqueue embed similarly

For enqueuing from workers: add `RiverClient *river.Client[pgx.Tx]` to `worker.Deps`. Workers receive the client and call `Insert()` directly. This is simpler than `ClientFromContext` which requires running within a River-managed transaction.

### 8. Hybrid search — `api/internal/item/service.go` (modify)
- Add `embedder provider.Embedder` optional field to `Service`
- New constructor `NewWithEmbedder(pool, embedder)`
- Refactor `Search`: if embedder is set, embed the query and run hybrid search; fallback to FTS on embedding error
- `hybridSearch` SQL: match on FTS OR cosine similarity > threshold, rank by combined score (cosine similarity boosted 1.2x when FTS also matches)
- Default semantic threshold: 0.3 when embedder is available

### 9. Wire embedder in entry points
**a) `api/cmd/flotsam-worker/main.go`**: init `OpenAIEmbeddingProvider` when `OPENAI_API_KEY` set, add to `workerDeps`
**b) `api/cmd/flotsamd/main.go`**: init embedder, use `item.NewWithEmbedder(pool, embedder)` for search-time embedding

### 10. Config — `api/internal/config/config.go` (modify)
- Add `EmbeddingModel string` with `env:"EMBEDDING_MODEL" envDefault:"text-embedding-3-small"`
- Update `.env.example` with `EMBEDDING_MODEL` documentation

### 11. Worker + search tests
- `api/internal/worker/embed_test.go` (new): mock embedder, test success/nil embedder/empty content/error cases
- `api/internal/item/service_test.go` (extend): test `Search` with nil embedder returns FTS path, test `NewWithEmbedder` constructor

## Files Changed

| File | Action |
|---|---|
| `api/go.mod` + `api/go.sum` | Add pgvector-go |
| `api/internal/provider/embedding.go` | New: Embedder interface |
| `api/internal/provider/openai_embedding.go` | New: OpenAI implementation |
| `api/internal/provider/openai_embedding_test.go` | New: tests |
| `api/internal/worker/embed.go` | Modify: implement real embedding |
| `api/internal/worker/embed_test.go` | New: worker tests |
| `api/internal/worker/registry.go` | Modify: add Embedder + RiverClient to Deps |
| `api/internal/worker/page_fetch.go` | Modify: enqueue embed after fetch |
| `api/internal/worker/transcribe.go` | Modify: enqueue embed after transcription |
| `api/internal/item/service.go` | Modify: hybrid search, embedder field |
| `api/internal/item/service_test.go` | Modify: add search tests |
| `api/graph/resolver/schema.resolvers.go` | Modify: enqueue embed for notes |
| `api/cmd/flotsamd/main.go` | Modify: init embedder for search |
| `api/cmd/flotsam-worker/main.go` | Modify: init embedder for worker |
| `api/internal/config/config.go` | Modify: add EMBEDDING_MODEL |
| `.env.example` | Modify: document EMBEDDING_MODEL |

## Verification

1. `make api-generate && make web-schema-sync` (no GraphQL schema changes, but verify clean)
2. `make api-test-unit` — all unit tests pass
3. `make api-lint` — zero warnings
4. Provider tests verify OpenAI API integration with httptest mocks
5. Worker tests verify embed job flow with mock embedder
