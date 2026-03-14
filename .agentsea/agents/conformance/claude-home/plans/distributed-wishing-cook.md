# Plan: Technical Contract Seed Data (kapow-pr9.2)

## Context
The contract registry framework exists (schemas, examples, README) but has no actual contract file yet. We need to seed `contracts/technical-contracts.json` with entries for 6 known backend guarantees that are already implemented and tested.

## Approach
Create `contracts/technical-contracts.json` as a JSON array of 6 contract objects, each following `contracts/schema/technical-contract.schema.json`.

### Contract entries

1. **search-validation-parity** — Based on the example, covers `contract.go` shared test matrix across GraphQL/CSV/MCP entry points
2. **route-auth-policy** — Auth modes per route: `/graphql` = OptionalAuth, `/mcp/*` = Auth, `/private/*` = IP allowlist, `/api/export/*` = OptionalAuth
3. **config-runtime-parity** — All config fields use `caarlos0/env` struct tags with defaults and docs in `.env.example`
4. **privacy-safe-logging** — `middleware/logging.go` logs method, path, status, duration, bytes, request_id, remote_addr — no request/response bodies, no query parameters containing user data
5. **startup-sequence-safety** — DB connect with 3-attempt retry + exponential backoff (`db/db.go`), graceful shutdown via SIGINT/SIGTERM with configurable timeout (`server/server.go`, `cmd/server/main.go`)
6. **jwt-validation** — RS256 (`RSAValidator`) and HS256 (`HMACValidator`) with `JWT_PUBLIC_KEY` taking precedence; signing method enforcement prevents algorithm confusion attacks

### Key source files to reference
- `api/internal/search/contract.go`, `contract_test.go`
- `api/internal/router/router.go`
- `api/internal/config/config.go`, `config_test.go`
- `api/internal/middleware/logging.go`, `middleware_test.go`
- `api/internal/db/db.go`
- `api/internal/server/server.go`
- `api/internal/auth/jwt.go`, `jwt_test.go`
- `api/cmd/server/main.go`

## Verification
- Validate JSON against schema: `python3 -c "import json, jsonschema; ..."`
- `make test-quick` (config-only change, no code affected)
