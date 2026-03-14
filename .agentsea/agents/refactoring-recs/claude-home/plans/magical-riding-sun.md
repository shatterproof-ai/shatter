# Plan: flt-it8.5 — Auth: JWT + User Registration

## Context

The Flotsam API has JWT **validation** (HS256) and auth middleware already wired up, but lacks the ability to **create** users, **hash** passwords, **issue** tokens, or expose auth operations via GraphQL. This plan adds those missing pieces so users can register and log in.

## Files to Create

| File | Purpose |
|---|---|
| `api/internal/auth/password.go` | bcrypt hash/verify |
| `api/internal/auth/password_test.go` | Unit tests |
| `api/internal/auth/issuer.go` | JWT token minting |
| `api/internal/auth/issuer_test.go` | Unit tests (round-trip with HMACValidator) |
| `api/internal/user/service.go` | User struct + CRUD against pgx pool |
| `api/internal/user/service_test.go` | Integration tests (DB-guarded) |
| `api/graph/schema/auth.graphql` | Auth mutations + User type |

## Files to Modify

| File | Change |
|---|---|
| `api/internal/config/config.go` | Add `JWTExpiry time.Duration` field |
| `api/graph/resolver/resolver.go` | Add `UserService` + `Issuer` fields |
| `api/graph/resolver/auth.resolvers.go` | Implement register/login/me (generated stub, then hand-written) |
| `api/graph/resolver/helpers.go` | Add `toGraphQLUser` mapper |
| `api/internal/router/router.go` | Add `Users` + `Issuer` to Deps, wire into Resolver |
| `api/cmd/flotsamd/main.go` | Initialize user service + issuer |
| `.env.example` | Add `JWT_EXPIRY` |

## Implementation Steps

### 1. `auth/password.go` — bcrypt helpers

```go
func HashPassword(password string) (string, error)    // bcrypt.GenerateFromPassword, DefaultCost
func CheckPassword(hash, password string) error        // bcrypt.CompareHashAndPassword
```
Errors wrapped with `"auth: "` prefix.

### 2. `auth/issuer.go` — token minting

```go
type Issuer struct { secret []byte; duration time.Duration }
func NewIssuer(secret string, duration time.Duration) *Issuer
func (iss *Issuer) Issue(userID uuid.UUID, email, clientType string) (string, error)
```
Creates `Claims` with proper expiry, signs with HS256. Round-trip testable against existing `HMACValidator`.

### 3. `config.go` — add JWTExpiry

```go
JWTExpiry time.Duration `env:"JWT_EXPIRY" envDefault:"24h"`
```

### 4. `user/service.go` — user CRUD

```go
type User struct { ID, Email, DisplayName, PasswordHash, AuthProvider, Settings, CreatedAt, UpdatedAt }
type Service struct { pool *pgxpool.Pool }

func New(pool) *Service
func (s *Service) Create(ctx, email, password, displayName string) (*User, error)
func (s *Service) GetByEmail(ctx, email string) (*User, error)
func (s *Service) GetByID(ctx, id uuid.UUID) (*User, error)
```

- `Create`: validate email/password, hash password, INSERT with `auth_provider='local'`, handle unique violation → `ErrEmailTaken`
- `GetByEmail`/`GetByID`: return `nil, nil` if not found (matches item.Service pattern)
- Detect PG unique violation via `pgconn.PgError` code `23505`

### 5. `graph/schema/auth.graphql`

```graphql
type User { id, email, displayName, createdAt, updatedAt }
type AuthPayload { token: String!, user: User! }
input RegisterInput { email, password, displayName }
input LoginInput { email, password }
extend type Mutation { register, login }
extend type Query { me: User }
```

### 6. Run `make api-generate` then implement resolvers

gqlgen layout `follow-schema` → generates `auth.resolvers.go` with stubs.

**register**: Create user → issue token → return AuthPayload
**login**: GetByEmail → CheckPassword → issue token (generic "invalid credentials" on failure)
**me**: GetClaims from context → GetByID → return User (nil if unauthenticated)

### 7. Wire dependencies

- `resolver.Resolver`: add `UserService *user.Service`, `Issuer *auth.Issuer`
- `router.Deps`: add `Users *user.Service`, `Issuer *auth.Issuer`
- `main.go`: init `user.New(pool)` + `auth.NewIssuer(cfg.JWTSecret, cfg.JWTExpiry)`, pass into Deps

### 8. Run `make web-schema-sync`

Regenerate frontend schema types after GraphQL changes.

## Test Strategy

| Package | Type | Tests |
|---|---|---|
| `auth` (password) | Unit | Hash produces valid bcrypt, CheckPassword correct/wrong |
| `auth` (issuer) | Unit | Round-trip issue→validate, correct claims/expiry |
| `user` | Integration (DB-guarded) | Create, duplicate email, GetByEmail found/not-found, GetByID |

## Verification

```bash
make api-generate && make web-schema-sync
make api-test-unit && make api-lint
```
