# Plan: API Server Scaffold (flt-it8.2)

## Context

Flotsam has an empty `api/internal/` directory structure and stub binaries. This task scaffolds the Go API server (`flotsamd`) with config, auth, middleware, router, server, and graceful shutdown — adapting proven patterns from the kapow project (`/home/ketan/project/kapow/`).

## Files to Create (18 files)

### Phase 1: Leaf packages (no internal deps)

**`api/internal/version/version.go`**
- `Version = "dev"`, `BuildTime = "unknown"` — set via ldflags (already wired in Makefile)

**`api/internal/config/config.go`**
- Config struct with `caarlos0/env/v11` tags matching `.env.example`
- Fields: Host, Port, DatabaseURL (required), JWTSecret (required), S3 fields, OpenAIAPIKey, OllamaURL, LogLevel, LogFormat, Env, CORSOrigins, GQLPlayground, GQLIntrospection, MCPEnabled, RateLimitEnabled + 3 limit ints, ShutdownTimeout
- `Load() (*Config, error)`, `Addr() string`
- `LogHandler() slog.Handler` — JSON or text handler based on LogFormat, with `ParseLogLevel()` (simpler than kapow — no SwappableWriter/CategoryHandler)
- Adapt from: `/home/ketan/project/kapow/api/internal/config/config.go`

**`api/internal/config/config_test.go`**
- Test Load with required vars set, defaults, required field validation (DATABASE_URL, JWT_SECRET), Addr(), ParseLogLevel, LogHandler

### Phase 2: Auth (no internal deps)

**`api/internal/auth/auth.go`**
- `Claims`: `jwt.RegisteredClaims` + `UserID uuid.UUID` + `Email string` + `ClientType string`
- `Validator` interface, `HMACValidator` (HS256 only — no RSA unlike kapow)
- `NewHMACValidator(secret string) *HMACValidator`
- Adapt from: `/home/ketan/project/kapow/api/internal/auth/jwt.go` (remove RSA, Supabase fields)

**`api/internal/auth/context.go`**
- `WithClaims(ctx, claims) context.Context` (renamed from kapow's `SetClaims` per task spec)
- `GetClaims(ctx) *Claims`
- Adapt from: `/home/ketan/project/kapow/api/internal/auth/context.go`

**`api/internal/auth/auth_test.go`**
- Valid token, expired token, wrong secret, malformed token, wrong signing method
- Context round-trip tests

### Phase 3: Middleware (depends on auth)

**`api/internal/middleware/context.go`**
- Unexported `requestIDKey`, `withRequestID()`, `requestIDFromContext()`
- Identical to: `/home/ketan/project/kapow/api/internal/middleware/context.go`

**`api/internal/middleware/requestid.go`**
- `RequestID` middleware — generate or pass through `X-Request-ID`
- `GetRequestID(r)` exported helper
- Identical to: `/home/ketan/project/kapow/api/internal/middleware/requestid.go`

**`api/internal/middleware/logging.go`**
- `responseWriter` wrapper capturing status + bytes
- `Logging` middleware — logs method, path, status, duration_ms, bytes, request_id
- Identical to: `/home/ketan/project/kapow/api/internal/middleware/logging.go`

**`api/internal/middleware/recovery.go`**
- `Recovery` middleware — catches panics, logs stack trace, returns 500
- Identical to: `/home/ketan/project/kapow/api/internal/middleware/recovery.go`

**`api/internal/middleware/auth.go`**
- `Auth(validator)` — mandatory auth, 401 if missing/invalid
- `OptionalAuth(validator)` — sets claims if present, continues if not
- `bearerToken(r)` helper
- Adapt from: `/home/ketan/project/kapow/api/internal/middleware/auth.go` — change realm to `"flotsam"`, remove `resource_metadata` from WWW-Authenticate, use `auth.WithClaims` instead of `auth.SetClaims`

**`api/internal/middleware/ratelimit.go`** (NEW — kapow uses httprate)
- `RateLimiter` struct with `sync.Mutex`, `map[string]*visitor`, `rate.Limit`, `burst`
- `NewRateLimiter(requestsPerMinute int) *RateLimiter` — starts cleanup goroutine
- `Limit(next) http.Handler` — extract IP from `r.RemoteAddr`, check `rate.Limiter.Allow()`, 429 if exceeded
- Background cleanup every 60s removes visitors not seen for 3 minutes
- Uses `golang.org/x/time/rate`

**`api/internal/middleware/middleware_test.go`**
- Auth: valid token, missing header, malformed header (table-driven), invalid token
- OptionalAuth: no header (200), valid token, invalid token (401)
- RequestID: generates ID, passes through client ID
- Logging: status code passes through
- Recovery: no panic (200), catches panic (500)
- RateLimit: allows within limit, blocks exceeding, independent per-IP

### Phase 4: Infrastructure

**`api/internal/db/db.go`** (minimal stub)
- `Connect(ctx, databaseURL) (*pgxpool.Pool, error)` — real pgxpool connection with 3 retries and exponential backoff (adapting kapow's pattern)
- The real schema/migrations come from flt-it8.1; this just handles connection

**`api/internal/server/server.go`**
- `Server` struct wrapping `*http.Server`
- `New(addr string, handler http.Handler) *Server` — production timeouts: Read 15s, Write 30s, Idle 60s, ReadHeader 5s
- `Start() error` — calls `ListenAndServe`
- `Shutdown(ctx) error` — graceful drain
- Simpler than kapow (takes addr+handler, not config)

### Phase 5: Router

**`api/internal/router/router.go`**
- `Deps` struct: `Config *config.Config`, `Pool *pgxpool.Pool`, `Validator auth.Validator`
- `New(deps) http.Handler`
- Global middleware: RequestID → Logging → Recovery → RealIP → StripSlashes → CORS (if configured)
- CORS: parse `CORSOrigins` comma-separated → `go-chi/cors` middleware
- Routes:
  - `GET /health` → `{"status":"ok","version":"..."}` using `version.Version` (no auth)
  - `POST /graphql` → 501 placeholder + OptionalAuth + rate limit
  - `/mcp/*` → 501 placeholder + Auth + rate limit (if MCPEnabled)
  - `POST /upload` → 501 placeholder + Auth
- Adapt from: `/home/ketan/project/kapow/api/internal/router/router.go` (significantly simplified)

### Phase 6: Entry point

**`api/cmd/flotsamd/main.go`**
1. `godotenv.Load()` (no-op if absent)
2. `config.Load()` — exit on error
3. `slog.SetDefault(slog.New(cfg.LogHandler()))` — structured logging
4. `db.Connect(ctx, cfg.DatabaseURL)` — exit on error; `defer pool.Close()`
5. `auth.NewHMACValidator(cfg.JWTSecret)`
6. `router.New(deps)` — build chi router
7. `server.New(cfg.Addr(), router)` — create HTTP server
8. Start in goroutine; signal.Notify(SIGINT, SIGTERM); block; graceful shutdown
- Adapt from: `/home/ketan/project/kapow/api/cmd/server/main.go` (remove OTel, RSA, CIDR, user service, UI)

### Phase 7: Finalize
- `go mod tidy` — resolves all deps, creates go.sum

## Dependencies to Add

| Module | Package |
|---|---|
| `github.com/caarlos0/env/v11` | config |
| `github.com/golang-jwt/jwt/v5` | auth |
| `github.com/google/uuid` | auth |
| `github.com/go-chi/chi/v5` | router |
| `github.com/go-chi/cors` | router |
| `github.com/jackc/pgx/v5` | db, router |
| `github.com/joho/godotenv` | cmd/flotsamd |
| `golang.org/x/time` | middleware |

## Verification

```bash
cd api
go mod tidy
go vet ./...                    # must pass
go test -race -short ./...      # all tests pass
go build ./cmd/flotsamd         # compiles
go build ./cmd/flotsam-worker   # still compiles (unchanged)
go build ./cmd/flotsam          # still compiles (unchanged)
```
