# flt-it8.13: MCP Server — search + browse + capture

## Context

The Flotsam API needs an MCP (Model Context Protocol) server so AI assistants can search, browse, and capture items in the knowledge base. The router already has a `/mcp` route group with auth middleware and rate limiting, but it serves a 501 placeholder. The `api/internal/mcp/` package doesn't exist yet.

## Approach

Use `github.com/mark3labs/mcp-go` (latest v0.45.0) with Streamable HTTP transport. Define local interfaces for dependencies (per project convention: "interfaces defined where consumed"). Three tools: search, browse, capture.

## Files to Create

### 1. `api/internal/mcp/mcp.go` — Interfaces + Deps

- `ItemSearcher` interface (subset of `item.Service` methods needed)
- `Embedder` interface (mirrors `provider.Embedder`)
- `Deps` struct holding `Items ItemSearcher` and `Embedder Embedder` (nil-able)

### 2. `api/internal/mcp/server.go` — NewHandler constructor

- `NewHandler(deps Deps) http.Handler`
- Creates `server.NewMCPServer("flotsam", version, ...)` with tool capabilities
- Registers three tools via `s.AddTool()`
- Returns `server.NewStreamableHTTPServer(s, ...)` with:
  - Stateless mode (auth is per-request via chi middleware)
  - `WithHTTPContextFunc` to forward auth claims from `r.Context()` into MCP handler context

### 3. `api/internal/mcp/tools.go` — Tool definitions + handlers

**search tool**: `query` (required string), `limit` (optional int 1-50, default 10), `type` (optional enum filter)
- If embedder available: embed query → `SemanticSearch`, fall back to FTS on failure or empty results
- If no embedder: straight to `Search` (FTS)
- Returns formatted markdown text with item summaries

**browse tool**: `id` (required UUID string)
- Calls `GetByID` with authenticated user's owner ID
- Returns full item details as markdown

**capture tool**: `url` (optional), `content` (optional), `title` (optional), `tags` (optional comma-separated), `sensitivity` (optional enum)
- URL provided → `CaptureBookmark` with source "mcp"
- Content provided → `CaptureNote` with source "mcp"
- Neither → error
- Returns captured item as markdown

All handlers: extract claims via `auth.GetClaims(ctx)`, return tool error if nil.

### 4. `api/internal/mcp/format.go` — Output formatting

- `formatSearchResults(*item.SearchResult) string` — markdown list of items (title, type, tags, snippet)
- `formatItem(*item.Item) string` — full item detail as markdown
- `formatItemSummary(*item.Item) string` — one-line summary for search results

### 5. `api/internal/mcp/tools_test.go` — Unit tests (>=80% coverage)

Mock `ItemSearcher` and `Embedder` with function-field structs. Test each handler directly by constructing `mcp.CallToolRequest` and injecting auth claims via `auth.WithClaims(ctx, claims)`.

Key test cases:
- search: semantic search path, FTS fallback (embedder nil), FTS fallback (embed error), FTS fallback (semantic empty), type filter, limit clamping, no auth
- browse: found, not found, invalid UUID, no auth
- capture: bookmark (url), note (content), missing input, with tags, no auth
- format helpers: search results, full item

## File to Modify

### 6. `api/internal/router/router.go`

- Add import: `flotsammcp "github.com/ketang/flotsam/api/internal/mcp"`
- Replace `r.Handle("/*", placeholderHandler("mcp"))` with:
  ```go
  mcpHandler := flotsammcp.NewHandler(flotsammcp.Deps{
      Items:    deps.Items,
      Embedder: deps.Embedder,
  })
  r.Handle("/*", mcpHandler)
  ```

## Key Design Decisions

1. **Stateless MCP**: No server-side sessions. Auth is per-request (JWT via chi middleware). Simplifies deployment.
2. **Context bridging**: `WithHTTPContextFunc` copies auth claims from HTTP request context into MCP tool handler context. Same pattern as GraphQL resolvers.
3. **Semantic-first search**: Try embedding + vector search first, fall back to FTS. Graceful degradation when embedder is nil.
4. **Text output**: MCP tool results are markdown text (not JSON). More readable for AI consumers.
5. **Chi path stripping**: Chi's `r.Route("/mcp", ...)` strips the prefix. May need `server.WithEndpointPath("/")` on the StreamableHTTPServer to align. Will verify during implementation.

## Dependencies

```bash
cd api && go get github.com/mark3labs/mcp-go@latest
```

## Verification

```bash
# Unit tests with coverage
cd api && go test -short -race -count=1 -cover ./internal/mcp/...

# Full quality gate
cd api && go vet ./... && golangci-lint run ./...
make api-test-unit && make api-lint
```
