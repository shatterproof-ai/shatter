# Flotsam Project Memory

## Project Overview
Personal knowledge management system ("second brain"). Go API + React frontend monorepo.
Full architecture: `docs/specs/product-overview.md`

## Key Paths
- API: `api/` (Go module `github.com/ketang/flotsam/api`)
- Web: `web/` (React + TypeScript + Vite + Mantine)
- Docker Compose dev: `compose/db.yml` (PG 16 + pgvector on :5433, MinIO on :9000/:9001)
- Specs: `docs/specs/product-overview.md`

## Beads Issue Tracking
- Prefix: `flt`
- Epic: `flt-it8` (Flotsam MVP, 17 children)
- Ready issues (no deps): flt-it8.1 (DB schema), flt-it8.2 (API scaffold), flt-it8.3 (Background jobs)
- `bd export` instead of `bd sync` (sync not available in this version)
- Child issues use sequential numbering (flt-it8.1, flt-it8.2, ... flt-it8.17), not hex

## Tech Stack
- Go 1.24, chi/v5, gqlgen, pgx/v5, River (bg jobs), goose (migrations)
- React 18, TypeScript 5, Vite 5, Mantine 7, urql + gql.tada, Zustand 5
- PostgreSQL 16 + pgvector, MinIO (dev) / Cloudflare R2 (prod)

## Conventions
- Adapted from kapow project (`/home/ketan/project/kapow/`)
- Same patterns: gqlgen schema-first, chi middleware, goose embedded, pnpm
- Same testing standards: >=80% coverage, unit + integration, test tiers

## Issue ID Mapping (plan # → beads ID)
1. DB schema → flt-it8.1
2. API scaffold → flt-it8.2
3. GraphQL → flt-it8.4
4. Auth → flt-it8.5
5. S3 storage → flt-it8.6
6. Bookmark capture → flt-it8.7
7. Embedding + search → flt-it8.8
8. Chrome extension → flt-it8.9
9. Voice capture → flt-it8.10
10. Android app → flt-it8.11
11. Background jobs → flt-it8.3
12. Auto-classification → flt-it8.12
13. MCP server → flt-it8.13
14. MCP auth → flt-it8.14
15. Web search UI → flt-it8.15
16. Web capture UI → flt-it8.16
17. Local AI providers → flt-it8.17
