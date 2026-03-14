# Plan: GraphQL Schema + Resolvers (flt-it8.4)

## Context

The Flotsam API has a working HTTP server with auth middleware, database connection, and a placeholder `/graphql` endpoint returning 501. This plan adds the real GraphQL layer: SDL schema, gqlgen codegen, an `internal/item` service for CRUD + search against the existing `items` table, and resolver implementations wired into the router.

## Steps

### 1. Add gqlgen dependency
```bash
cd api && go get github.com/99designs/gqlgen@latest
```

### 2. Create gqlgen config + generate directive
- **`api/gqlgen.yml`** — schema-first config pointing at `graph/schema/*.graphql`, output to `graph/generated/` and `graph/model/`, resolvers in `graph/resolver/`. Map `DateTime` → `graphql.Time`, `JSON` → `graphql.Map`.
- **`api/graph/generate.go`** — `//go:generate go run github.com/99designs/gqlgen generate`

### 3. Create GraphQL SDL files in `api/graph/schema/`
- **`scalars.graphql`** — `DateTime`, `JSON`
- **`enums.graphql`** — `ItemType`, `ItemStatus`, `SensitivityLevel`, `ItemSortField`, `SortDirection`
- **`item.graphql`** — `Item` type, `SearchResult`, input types (`ItemFilter`, `ItemSort`, `BookmarkInput`, `NoteInput`, `UpdateItemInput`)
- **`schema.graphql`** — `Query` (item, items, search) + `Mutation` (captureBookmark, captureNote, updateItem, deleteItem, updateSensitivity, tagItem)

Schema follows the spec from the issue description exactly.

### 4. Run gqlgen codegen
`make api-generate` → generates `graph/generated/generated.go`, `graph/model/models_gen.go`, and resolver stubs.

### 5. Create `api/internal/item/` package

**`model.go`** — Domain types: `Item` struct (maps to DB columns), `Filter`, `Sort`, `SearchResult`, `BookmarkInput`, `NoteInput`, `UpdateInput`.

**`service.go`** — `Service` struct with `*pgxpool.Pool`. Methods:
- `GetByID(ctx, ownerID, itemID)` — SELECT with owner_id filter
- `List(ctx, ownerID, filter, limit, offset, sort)` — dynamic WHERE + ORDER BY + COUNT
- `Search(ctx, ownerID, query, filter, limit, threshold)` — FTS via `search_vector @@ plainto_tsquery`
- `CaptureBookmark(ctx, ownerID, input)` — INSERT with type=bookmark, status=pending
- `CaptureNote(ctx, ownerID, input)` — INSERT with type=text_note, status=ready
- `Update(ctx, ownerID, itemID, input)` — partial UPDATE (non-nil fields only)
- `Delete(ctx, ownerID, itemID)` — DELETE
- `UpdateSensitivity(ctx, ownerID, itemID, sensitivity)` — UPDATE sensitivity
- `Tag(ctx, ownerID, itemID, tags)` — UPDATE tags array

All SQL parameterized. All queries filter by `owner_id`. Use `RETURNING *` to avoid round-trips.

**`query.go`** — unexported query builder helper for dynamic filter → WHERE clause construction.

**`service_test.go`** — Unit tests for validation logic, query builder, enum mapping. Integration tests guarded by `testing.Short()`.

### 6. Implement resolvers in `api/graph/resolver/`

**`resolver.go`** — `Resolver` struct with `Items *item.Service`

**`helpers.go`** — `requireAuth(ctx)`, enum string↔GraphQL mappers, `toGraphQLItem()` domain→model mapper

**`schema.resolvers.go`** — Fill in generated stubs. Each resolver:
1. Calls `requireAuth(ctx)` for mutations (and item-access queries)
2. Maps GraphQL inputs → domain types
3. Calls `item.Service` method
4. Maps domain result → GraphQL model

### 7. Wire into router (`api/internal/router/router.go`)
- Add `Items *item.Service` to `Deps`
- Replace `placeholderHandler("graphql")` with real gqlgen `handler.New()`
- Add playground route if `GQLPlayground` is enabled
- Enable introspection if `GQLIntrospection` is enabled

### 8. Wire into main.go (`api/cmd/flotsamd/main.go`)
- Create `item.NewService(pool)` after pool connection
- Pass to `router.Deps{..., Items: itemService}`

## Files to create
| File | Purpose |
|---|---|
| `api/gqlgen.yml` | gqlgen config |
| `api/graph/generate.go` | go:generate directive |
| `api/graph/schema/scalars.graphql` | DateTime, JSON scalars |
| `api/graph/schema/enums.graphql` | All enum types |
| `api/graph/schema/item.graphql` | Item type, inputs, SearchResult |
| `api/graph/schema/schema.graphql` | Query + Mutation root types |
| `api/graph/resolver/resolver.go` | Resolver struct |
| `api/graph/resolver/helpers.go` | Auth check, mappers |
| `api/internal/item/model.go` | Domain types |
| `api/internal/item/service.go` | CRUD + search service |
| `api/internal/item/query.go` | Dynamic query builder |
| `api/internal/item/service_test.go` | Unit tests |

## Files to modify
| File | Change |
|---|---|
| `api/go.mod` | Add gqlgen dependency |
| `api/internal/router/router.go` | Wire gqlgen handler, add Items to Deps |
| `api/cmd/flotsamd/main.go` | Create item service, pass to router |

## Generated files (never hand-edit)
- `api/graph/generated/generated.go`
- `api/graph/model/models_gen.go`
- `api/graph/resolver/schema.resolvers.go` (stubs generated, then filled in)

## Key existing code to reuse
- `auth.GetClaims(ctx)` → `api/internal/auth/context.go:13`
- `auth.Claims.UserID` → `api/internal/auth/auth.go`
- `router.Deps` → `api/internal/router/router.go:21`
- `config.GQLPlayground`, `config.GQLIntrospection` → `api/internal/config/config.go:37-38`

## Verification
```bash
make api-generate          # gqlgen codegen succeeds
make api-test-unit         # all tests pass
go vet ./...               # no issues (fallback if golangci-lint unavailable)
make api-build             # all binaries compile
```
