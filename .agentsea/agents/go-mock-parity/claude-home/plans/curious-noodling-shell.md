# Plan: Embedding + Semantic Search (flt-it8.8)

## Context

The Flotsam knowledge management system has full-text search (FTS via PostgreSQL `tsvector`) but lacks semantic search. The DB schema already has an `embedding vector(1536)` column with an IVFFlat index, and a stub `EmbedWorker` that does nothing. This issue implements the full pipeline: EmbeddingProvider interface, OpenAI implementation, embed worker, semantic search query, and job chaining so embeddings are generated after content is ready.

## Changes

### 1. EmbeddingProvider interface (`api/internal/provider/embedding.go`)

```go
type Embedder interface {
    Embed(ctx context.Context, text string) ([]float32, error)
    Dimensions() int
}
```

Keep it simple — no `EmbedBatch` yet (YAGNI; items are embedded one at a time by worker jobs).

### 2. OpenAI embedding implementation (`api/internal/provider/openai_embedding.go`)

- Uses `net/http` + `encoding/json` directly (no OpenAI SDK dep — same pattern as `whisper_api.go`)
- Model: `text-embedding-3-small` (1536 dimensions, matches DB column)
- Config: uses existing `OPENAI_API_KEY` env var
- Truncates input to avoid API limits (8191 tokens ≈ ~30K chars)

### 3. EmbeddingProvider tests (`api/internal/provider/embedding_test.go`)

- Unit test for OpenAI provider with HTTP test server mock
- Test error handling, response parsing, dimension reporting

### 4. Implement EmbedWorker (`api/internal/worker/embed.go`)

- Add `Pool *pgxpool.Pool` and `Embedder provider.Embedder` fields
- `Work()`: call `Embedder.Embed()`, then `UPDATE items SET embedding = $1 WHERE id = $2`
- Use pgx with `pgvector.Vector` type for the embedding column
- Add dependency: `github.com/pgvector/pgvector-go` for vector type encoding

### 5. Wire Embedder into worker deps (`api/internal/worker/registry.go`)

- Add `Embedder provider.Embedder` to `Deps` struct
- Pass to `EmbedWorker` in `Workers()` function

### 6. Wire Embedder in flotsam-worker main (`api/cmd/flotsam-worker/main.go`)

- Initialize `provider.NewOpenAIEmbedder(cfg.OpenAIAPIKey)` when key is available
- Pass to worker deps

### 7. Enqueue embed jobs after content is ready

- **PageFetchWorker** (`api/internal/worker/page_fetch.go`): after successful content update, insert `EmbedArgs` job via River client
- **TranscribeWorker** (`api/internal/worker/transcribe.go`): after successful transcription, insert `EmbedArgs` job
- **CaptureNote resolver** (`api/graph/resolver/schema.resolvers.go`): notes are immediately `ready`, so enqueue embed job right after creation
- Workers need `RiverClient` in their deps to enqueue follow-up jobs

### 8. Add semantic search to item service (`api/internal/item/service.go`)

- New method: `SemanticSearch(ctx, ownerID, embedding []float32, filter, limit, threshold)`
- Query: `SELECT ... FROM items WHERE owner_id = $1 AND embedding IS NOT NULL AND (1 - (embedding <=> $2)) >= $3 ORDER BY embedding <=> $2 LIMIT $4`
- Uses cosine distance operator `<=>` from pgvector

### 9. Hybrid search in resolver (`api/graph/resolver/schema.resolvers.go`)

- The `Search` resolver already accepts `threshold` parameter
- When threshold > 0 and Embedder is available on Resolver: generate query embedding, call `SemanticSearch`
- When threshold == 0 or no Embedder: fall back to FTS (current behavior)
- Add `Embedder provider.Embedder` to Resolver struct, wire in router

### 10. Config and wiring (`api/internal/config/config.go`, `api/internal/router/router.go`)

- Add `EmbeddingModel` config field (default: `text-embedding-3-small`)
- Wire Embedder into Resolver in router

### 11. Tests

- `api/internal/provider/embedding_test.go` — mock HTTP server tests for OpenAI embedder
- `api/internal/worker/embed_test.go` — unit test with mock embedder + mock pool
- `api/internal/item/service_test.go` — add test for SemanticSearch empty/validation cases

## Files to modify

| File | Action |
|---|---|
| `api/internal/provider/embedding.go` | **Create** — Embedder interface + OpenAI impl |
| `api/internal/provider/embedding_test.go` | **Create** — Unit tests |
| `api/internal/worker/embed.go` | **Modify** — Implement stub |
| `api/internal/worker/embed_test.go` | **Create** — Unit tests |
| `api/internal/worker/registry.go` | **Modify** — Add Embedder + RiverClient to Deps |
| `api/internal/worker/page_fetch.go` | **Modify** — Enqueue embed job after fetch |
| `api/internal/worker/transcribe.go` | **Modify** — Enqueue embed job after transcribe |
| `api/internal/item/service.go` | **Modify** — Add SemanticSearch method |
| `api/internal/item/service_test.go` | **Modify** — Add SemanticSearch tests |
| `api/graph/resolver/resolver.go` | **Modify** — Add Embedder field |
| `api/graph/resolver/schema.resolvers.go` | **Modify** — Hybrid search, embed on note capture |
| `api/internal/router/router.go` | **Modify** — Wire Embedder |
| `api/internal/config/config.go` | **Modify** — Add EmbeddingModel field |
| `api/cmd/flotsam-worker/main.go` | **Modify** — Init embedder, pass to deps |
| `api/go.mod` / `api/go.sum` | **Modify** — Add pgvector-go dep |
| `.env.example` | **Modify** — Document EMBEDDING_MODEL |

## Dependencies to add

- `github.com/pgvector/pgvector-go` — for `pgvector.Vector` type (pgx-compatible encoding)

## Key decisions

1. **No batch embedding** — items are processed one at a time via worker jobs; batch adds complexity without benefit here
2. **Direct HTTP for OpenAI** — follows the existing `whisper_api.go` pattern; no SDK dependency
3. **Hybrid search via threshold** — threshold > 0 triggers semantic, 0 triggers FTS. Simple, backward-compatible
4. **Job chaining** — workers enqueue embed jobs as follow-ups rather than a pipeline abstraction
5. **pgvector-go** — needed for proper vector type encoding with pgx; raw `[]float32` won't work with pgx's type system

## Verification

```bash
# Quality gate (must pass)
make api-test-unit && make api-lint

# Manual verification (requires DB + OPENAI_API_KEY)
# 1. Start dev env: make dev
# 2. Capture a note via GraphQL → verify embed job runs
# 3. Search with threshold=0.5 → verify semantic results
```
