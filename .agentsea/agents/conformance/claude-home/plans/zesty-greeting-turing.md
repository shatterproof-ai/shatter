# Coverage Debt Reduction Plan (flt-ncn)

## Context

9 Go packages fail the 80% coverage threshold, ranging from 6.7% to 72%. Web coverage is at 71.47% globally. This plan focuses on **unit tests only** (no DB required) to maximize coverage improvement with `-short` guard compliance.

## Baseline

| Package | Current | Target |
|---|---|---|
| internal/item | 22.6% | ~80% |
| internal/user | 11.4% | ~80% |
| graph/resolver | 16.3% | ~80% |
| internal/worker | 49.0% | ~80% |
| internal/audit | 6.7% | ~80% |
| internal/mcpclient | 16.4% | ~80% |
| internal/storage | 57.4% | ~70%+ |
| internal/provider | 72.0% | ~80% |
| internal/db | 70.3% | ~80% |

## Strategy

Most packages are service layers that talk to pgxpool. Since unit tests can't use a real DB, we need **interface-based mocking** for the pool/queries. However, the current services take `*pgxpool.Pool` directly — no interfaces.

**Key insight**: For unit tests without DB, we test:
1. **Validation paths** (before DB calls) — already partially done
2. **Helper/utility functions** — query builders, conversions, key generation
3. **Worker logic** around the DB calls (already uses mocks for providers)
4. **Resolver logic** by mocking the service layer (resolvers call services, not DB directly)

For services that are thin wrappers around SQL (item, user, mcpclient), getting to 80% with unit-only tests would require either (a) introducing interfaces or (b) accepting that integration tests cover the DB paths. The existing tests skip with `-short`. We'll focus on maximizing what we can test without DB and add integration tests that skip in `-short` mode.

### Approach per package

#### 1. `graph/resolver` (16.3% → ~80%) — BIGGEST WIN

The resolvers call service methods via struct fields. We can **mock the service layer** by creating test doubles. This is the highest-ROI target.

**File**: `api/graph/resolver/schema.resolvers_test.go`

Tests needed:
- All mutations: CaptureBookmark, CaptureNote, CaptureVoiceNote, UpdateItem, DeleteItem, UpdateSensitivity, TagItem
- All queries: Item, Items, Search
- Auth resolvers: Register, Login, Me
- MCP resolvers: CreateMCPClient, DeactivateMCPClient, McpClients
- Each with: (a) unauthenticated → error, (b) happy path, (c) service error → propagated

**Challenge**: Resolvers use concrete `*item.Service`, `*user.Service`, etc. — not interfaces. To unit test, we need to either:
- Option A: Introduce interfaces (invasive, but proper)
- Option B: Test through the GraphQL handler with a test server and real service mocks

**Chosen**: Option A — define small interfaces in the resolver package that the services already satisfy, then use mock implementations in tests. This is the idiomatic Go approach.

```go
// In resolver_test.go or a test helpers file
type mockItemService struct { ... }
func (m *mockItemService) CaptureBookmark(ctx, ownerID, input) (*item.Item, error) { ... }
```

Wait — the Resolver struct has `*item.Service` (concrete). We'd need to change it to an interface. That's a refactor.

**Revised approach**: Since the Resolver struct uses concrete types and changing that is out of scope for a coverage task, we'll test resolvers at the **GraphQL handler level** using `httptest` + the gqlgen handler, with a real `Resolver` struct wired to nil/mock services. Actually, this still requires interfaces.

**Final approach for resolvers**: Extract interfaces consumed by the resolver, update `resolver.go` to use them, and test with mocks. This is a one-time investment that pays off.

Interfaces to define (in `graph/resolver/deps.go`):
- `ItemQuerier` (GetByID, List, Search, SemanticSearch)
- `ItemMutator` (CaptureBookmark, CaptureNote, CaptureVoiceNote, Update, Delete, UpdateSensitivity, Tag)
- `UserQuerier` (GetByID, GetByEmail, Create)
- `MCPClientManager` (Create, GetByID, Deactivate, ListByOwner)
- `JobInserter` (Insert) — for River client

#### 2. `internal/user` (11.4% → ~50%+)

Current tests: 3 validation unit tests + 6 integration tests (skipped in `-short`).

Unit-testable additions:
- Email normalization logic (TrimSpace, ToLower)
- DisplayName nil vs empty handling
- `scanUser` can't be tested without a real row

The user package is a thin DB wrapper. Most paths require DB. We can add:
- More validation edge cases (whitespace-only email, exactly 8 char password)
- Test `HashAPIKey` equivalent if applicable
- Accept that this package will rely on integration tests for the DB paths

**File**: `api/internal/user/service_test.go` (extend)

#### 3. `internal/item` (22.6% → ~50%+)

Similar to user — thin DB wrapper. Already has good validation + query builder tests.

Unit-testable additions:
- `CaptureVoiceNote` validation (empty audioKey)
- `Update` with empty sets → delegates to GetByID
- `UpdateContent` validation paths
- More `buildBookmarkMetadata` edge cases (already well-tested)
- `joinSets` already tested

**File**: `api/internal/item/service_test.go` (extend)

#### 4. `internal/worker` (49% → ~80%)

Workers use mock providers. Main gaps:
- `S3UploadWorker` — trivial (just logs), add a basic test
- `NewRegistry` — test that it creates all workers
- `SetRiverClient` — test that it propagates
- Additional edge cases in existing workers

**Files**:
- `api/internal/worker/s3upload_test.go` (new)
- `api/internal/worker/registry_test.go` (new)
- Extend existing worker tests

#### 5. `internal/audit` (6.7% → ~80%)

`Log()` does JSON marshal + DB insert with error logging. Test with nil pool (will panic/error on insert, but we can test marshal paths).

Actually, `Log()` calls `l.pool.Exec()` — with nil pool, it panics. We need a mock pool or test only the marshal logic.

**Approach**: Test Entry construction (already done), test JSON marshaling of Request/ResponseSummary fields, and test that `Log` handles marshal errors gracefully. For the DB insert path, either add a pool interface or accept integration-only coverage.

**File**: `api/internal/audit/audit_test.go` (extend)

#### 6. `internal/mcpclient` (16.4% → ~60%+)

Key unit-testable functions:
- `generateAPIKey()` — test prefix, length, uniqueness, hash computation
- `HashAPIKey()` — deterministic, easy to test
- `Create` validation (empty name)
- Permissions JSON marshaling/unmarshaling

**File**: `api/internal/mcpclient/service_test.go` (extend or create)

#### 7. `internal/storage` (57.4% → ~70%+)

- `New()` with empty bucket → error (already tested?)
- `ValidateKeyOwnership` — if it exists
- Key generation functions in `keys.go`

**File**: `api/internal/storage/storage_test.go` (extend), `keys_test.go` (extend)

#### 8. `internal/provider` (72% → 80%)

Close to threshold. Check what's missing and add targeted tests.

#### 9. `internal/db` (70.3% → 80%)

Close to threshold. Check what's missing and add targeted tests.

---

## Implementation Order

1. **internal/mcpclient** — pure logic tests (generateAPIKey, HashAPIKey, validation)
2. **internal/audit** — marshal edge cases
3. **internal/worker** — S3Upload, Registry, edge cases
4. **internal/item** — more validation tests, CaptureVoiceNote
5. **internal/user** — more validation edge cases
6. **graph/resolver** — interface extraction + mock-based tests (largest effort, largest payoff)
7. **internal/storage** — key helpers, New() validation
8. **internal/provider** — targeted gap fills
9. **internal/db** — targeted gap fills

## Files to Create/Modify

| File | Action |
|---|---|
| `api/internal/mcpclient/service_test.go` | Extend with unit tests |
| `api/internal/audit/audit_test.go` | Extend with marshal/edge tests |
| `api/internal/worker/s3upload_test.go` | New — S3UploadWorker test |
| `api/internal/worker/registry_test.go` | New — NewRegistry + SetRiverClient |
| `api/internal/item/service_test.go` | Extend with more validation |
| `api/internal/user/service_test.go` | Extend with edge cases |
| `api/graph/resolver/deps.go` | New — interfaces for testability |
| `api/graph/resolver/resolver.go` | Update to use interfaces |
| `api/graph/resolver/schema.resolvers_test.go` | New — resolver tests with mocks |
| `api/graph/resolver/auth.resolvers_test.go` | New — auth resolver tests |
| `api/graph/resolver/mcp_client.resolvers_test.go` | New — MCP resolver tests |
| `api/internal/storage/keys_test.go` | Extend |
| `api/internal/storage/storage_test.go` | Extend |

## Verification

```bash
# Run unit tests
make api-test-unit

# Check coverage improvement
make coverage

# Run full standard gate
make test-standard
```
