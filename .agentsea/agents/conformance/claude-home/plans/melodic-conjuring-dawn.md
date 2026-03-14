# flt-it8.17 — Local AI Provider Support (Ollama)

## Context
The provider package has OpenAI implementations for embedding, classification, and transcription. Local transcription exists via whisper.cpp. This task adds Ollama-based local providers for embedding and classification, plus a routing layer that directs sensitive content to local providers. Config already has `OLLAMA_URL`.

## Implementation

### 1. Ollama Embedder (`api/internal/provider/ollama_embedding.go`)
- Implements `Embedder` interface
- `POST /api/embed` with `{"model": "nomic-embed-text", "input": "text"}`
- Response: `{"embeddings": [[...]]}`
- 768 dimensions (nomic-embed-text)
- Constructor: `NewOllamaEmbedder(baseURL string, opts ...OllamaEmbedderOption)`
- Options: `WithOllamaEmbeddingModel(string)`, `WithOllamaEmbeddingHTTPClient(*http.Client)`
- Same truncation pattern as OpenAI (30k chars)
- Timeout: 30s default

### 2. Ollama Classifier (`api/internal/provider/ollama_classifier.go`)
- Implements `Classifier` interface
- `POST /api/chat` with `{"model": "llama3.2", "messages": [...], "format": "json", "stream": false}`
- Response: `{"message": {"content": "{...}"}}`
- Reuse same system prompt from OpenAI classifier
- Constructor: `NewOllamaClassifier(baseURL string, opts ...OllamaClassifierOption)`
- Options: `WithOllamaClassificationModel(string)`, `WithOllamaClassificationHTTPClient(*http.Client)`
- Same truncation (30k chars), timeout 60s

### 3. Provider Router (`api/internal/provider/router.go`)
- `RoutingEmbedder` struct wrapping default + local `Embedder`
  - `NewRoutingEmbedder(defaultProvider, localProvider Embedder) *RoutingEmbedder`
  - Method: `EmbedWithSensitivity(ctx, text, sensitivity string) ([]float32, error)` — routes based on sensitivity
  - `Embed(ctx, text)` — delegates to default (for backward compat)
  - `Dimensions()` — returns default provider's dimensions (callers need consistent dims for DB column)
  - Note: routing embedder returns dimensions from whichever provider will be used; callers must handle mixed dimensions
- `RoutingClassifier` struct wrapping default + local `Classifier`
  - `NewRoutingClassifier(defaultProvider, localProvider Classifier) *RoutingClassifier`
  - `ClassifyWithSensitivity(ctx, content, sensitivity string) (*Classification, error)`
  - `Classify(ctx, content)` — delegates to default
- Sensitivity check: if sensitivity is "sensitive" or "private" → use local; otherwise default
- If local provider is nil, always use default (graceful degradation)

### 4. Config additions (`api/internal/config/config.go`)
- Add `OllamaEmbeddingModel string` (`env:"OLLAMA_EMBEDDING_MODEL" envDefault:"nomic-embed-text"`)
- Add `OllamaClassificationModel string` (`env:"OLLAMA_CLASSIFICATION_MODEL" envDefault:"llama3.2"`)

### 5. Wire into worker (`api/cmd/flotsam-worker/main.go`)
- If `cfg.OllamaURL` is set:
  - Create `OllamaEmbedder` and `OllamaClassifier`
  - Wrap with `RoutingEmbedder` / `RoutingClassifier` if both cloud and local are available
- If only Ollama (no OpenAI key): use Ollama directly as default
- Log which providers are initialized

### 6. Wire into server (`api/cmd/flotsamd/main.go`)
- Same pattern for embedder (server uses embedder for search)

### 7. Update `.env.example`
- Add `OLLAMA_URL`, `OLLAMA_EMBEDDING_MODEL`, `OLLAMA_CLASSIFICATION_MODEL` with comments

### 8. Tests
- `ollama_embedding_test.go` — httptest server mocking `/api/embed`. Test: success, empty input, API error, truncation, dimensions, custom model.
- `ollama_classifier_test.go` — httptest server mocking `/api/chat`. Test: success, empty input, API error, malformed JSON, custom model.
- `router_test.go` — mock embedders/classifiers. Test: normal→default, sensitive→local, private→local, nil local→default fallback, Dimensions().

## Files to modify/create
- **Create**: `api/internal/provider/ollama_embedding.go`
- **Create**: `api/internal/provider/ollama_embedding_test.go`
- **Create**: `api/internal/provider/ollama_classifier.go`
- **Create**: `api/internal/provider/ollama_classifier_test.go`
- **Create**: `api/internal/provider/router.go`
- **Create**: `api/internal/provider/router_test.go`
- **Modify**: `api/internal/config/config.go` (add 2 env vars)
- **Modify**: `api/cmd/flotsam-worker/main.go` (wire Ollama providers)
- **Modify**: `api/cmd/flotsamd/main.go` (wire Ollama embedder)
- **Modify**: `.env.example` (document new vars)

## Key patterns to reuse
- `urlRewriter` helper from `embedding_test.go` for redirecting HTTP in tests
- Functional options pattern from all existing providers
- System prompt from `classification.go` (extract as package-level const if not already)
- Error wrapping: `fmt.Errorf("provider: ollama: ...: %w", err)`
- Input truncation with `maxEmbeddingInputLen` / `maxClassifyInputLen` constants

## Verification
```bash
make api-test-unit   # all unit tests pass
make api-lint        # zero lint issues
```
