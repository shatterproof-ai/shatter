# Flotsam: Architecture Design & Project Setup

## 1. Context & Vision

**Flotsam** is a personal knowledge management system — a "second brain" for capturing thoughts, notes, URLs, and voice dictations, with semantic search and AI integration via MCP.

**Core problems it solves:**
- **Capture friction**: Too many clicks to save a thought, especially on mobile/while driving
- **Retrieval**: Information is scattered across Google Drive, browser bookmarks, various apps — can't find things semantically
- **AI integration**: AI tools (Claude, ChatGPT, Cursor) can't access personal knowledge
- **Data fragmentation**: No unified place for all types of personal information
- **Data control**: User owns the data end-to-end, self-hosted on VPS

**MVP scope**: Browser bookmarks (Chrome extension) + voice notes (Android phone app with voice trigger for driving)

**Full vision** (post-MVP): Web app with query interface, Google Drive integration, iOS app, Firefox/Safari extensions, MCP with defense-in-depth authorization, content-level access control, multi-user support.

**Inspiration**: Nate B. Jones' "Open Brain" (Supabase + Slack + MCP), Obsidian (graph view, backlinks, hierarchical tags, daily notes), Readwise (capture + highlights).

---

## 2. Architecture Overview

```
                    ┌─────────────────────────────────────────────┐
                    │                  Clients                     │
                    ├──────────┬──────────┬──────────┬────────────┤
                    │ Chrome   │ Android  │ Web App  │ AI Tools   │
                    │Extension │   App    │ (React)  │(via MCP)   │
                    └────┬─────┴────┬─────┴────┬─────┴─────┬──────┘
                         │          │          │           │
                    ┌────▼──────────▼──────────▼───────────▼──────┐
                    │              Go API Server                    │
                    │  ┌──────────┐ ┌────────┐ ┌───────────────┐  │
                    │  │ GraphQL  │ │REST    │ │  MCP Server   │  │
                    │  │ (gqlgen) │ │Upload  │ │  (go-sdk)     │  │
                    │  └────┬─────┘ └───┬────┘ └──────┬────────┘  │
                    │       │           │             │            │
                    │  ┌────▼───────────▼─────────────▼────────┐  │
                    │  │          Service Layer                  │  │
                    │  │  ┌──────┐ ┌──────┐ ┌──────┐ ┌───────┐ │  │
                    │  │  │Items │ │Search│ │Auth  │ │MCP    │ │  │
                    │  │  │CRUD  │ │Engine│ │/RBAC │ │AuthZ  │ │  │
                    │  │  └──┬───┘ └──┬───┘ └──┬───┘ └───┬───┘ │  │
                    │  └─────┼────────┼────────┼─────────┼─────┘  │
                    │        │        │        │         │         │
                    │  ┌─────▼────────▼────────▼─────────▼─────┐  │
                    │  │        Provider Interfaces              │  │
                    │  │  Embedding │ Transcription │ Classify   │  │
                    │  │  (pluggable: OpenAI / local models)    │  │
                    │  └────────────────────────────────────────┘  │
                    │        │                                     │
                    │  ┌─────▼──────────────────────────────────┐  │
                    │  │     Background Job Processor            │  │
                    │  │     (PG-backed, async processing)       │  │
                    │  └────────────────────────────────────────┘  │
                    └──────┬──────────────────────────┬───────────┘
                           │                          │
                    ┌──────▼──────┐            ┌──────▼──────┐
                    │ PostgreSQL  │            │ S3-Compatible│
                    │ + pgvector  │            │   Storage    │
                    │ (metadata,  │            │ (page HTML,  │
                    │  embeddings,│            │  audio files,│
                    │  FTS, jobs) │            │  documents)  │
                    └─────────────┘            └─────────────┘
```

---

## 3. Data Model

### Core table: `items`

Everything is an "item" — a unified type with JSONB metadata for type-specific fields.

```sql
CREATE EXTENSION IF NOT EXISTS vector;
CREATE EXTENSION IF NOT EXISTS pg_trgm;

CREATE TYPE item_type AS ENUM (
  'bookmark', 'voice_note', 'text_note', 'drive_doc', 'highlight', 'file'
);

CREATE TYPE item_status AS ENUM (
  'pending',      -- just captured, processing not complete
  'processing',   -- async processing in progress
  'ready',        -- fully processed, searchable
  'error'         -- processing failed
);

CREATE TYPE sensitivity_level AS ENUM (
  'public',       -- shareable, external APIs ok
  'normal',       -- default, external APIs ok
  'sensitive',    -- use local models, restricted MCP access
  'private'       -- local models only, no MCP access
);

CREATE TABLE items (
  id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  owner_id      UUID NOT NULL REFERENCES users(id),
  type          item_type NOT NULL,
  status        item_status NOT NULL DEFAULT 'pending',
  sensitivity   sensitivity_level NOT NULL DEFAULT 'normal',

  -- Content
  title         TEXT,
  content_text  TEXT,                    -- searchable text (page extract, transcription, note body)
  content_url   TEXT,                    -- S3 key for large content (HTML, audio, images)

  -- Search
  embedding     vector(1536),            -- semantic search vector
  search_vector tsvector GENERATED ALWAYS AS (
    setweight(to_tsvector('english', coalesce(title, '')), 'A') ||
    setweight(to_tsvector('english', coalesce(content_text, '')), 'B')
  ) STORED,

  -- Organization
  tags          TEXT[] DEFAULT '{}',     -- hierarchical tags: 'project/flotsam', 'person/sarah'
  metadata      JSONB NOT NULL DEFAULT '{}',  -- type-specific fields
  auto_metadata JSONB NOT NULL DEFAULT '{}',  -- LLM-extracted classification

  -- Provenance
  source        TEXT NOT NULL,           -- 'chrome_extension', 'android_app', 'web_app', 'mcp', 'api'
  source_url    TEXT,                    -- original URL for bookmarks, referrer, etc.

  -- Timestamps
  created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
  captured_at   TIMESTAMPTZ NOT NULL DEFAULT now()  -- when user actually captured (may differ from created_at)
);

-- Indexes
CREATE INDEX idx_items_owner ON items(owner_id);
CREATE INDEX idx_items_type ON items(owner_id, type);
CREATE INDEX idx_items_created ON items(owner_id, created_at DESC);
CREATE INDEX idx_items_tags ON items USING GIN(tags);
CREATE INDEX idx_items_metadata ON items USING GIN(metadata);
CREATE INDEX idx_items_search ON items USING GIN(search_vector);
CREATE INDEX idx_items_embedding ON items USING ivfflat(embedding vector_cosine_ops) WITH (lists = 100);
CREATE INDEX idx_items_sensitivity ON items(owner_id, sensitivity);
```

### Type-specific metadata examples

**Bookmark** (`metadata`):
```json
{
  "url": "https://example.com/article",
  "referrer": "https://news.ycombinator.com",
  "selection": "The key insight is that...",
  "page_title": "Article Title",
  "favicon_url": "https://example.com/favicon.ico",
  "content_type": "article"
}
```

**Voice note** (`metadata`):
```json
{
  "duration_seconds": 45,
  "audio_format": "webm",
  "transcription_provider": "whisper-local",
  "device": "Pixel 8",
  "language": "en"
}
```

**Auto-extracted metadata** (`auto_metadata`):
```json
{
  "category": "career",
  "people": ["Sarah"],
  "action_items": ["Follow up with Sarah about consulting"],
  "sentiment": "neutral",
  "topics": ["career-change", "consulting"],
  "sensitivity_suggestion": "normal"
}
```

### Users table

```sql
CREATE TABLE users (
  id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  email         TEXT UNIQUE NOT NULL,
  display_name  TEXT,
  password_hash TEXT,                    -- bcrypt, nullable if using external auth
  auth_provider TEXT DEFAULT 'local',    -- 'local', 'google', 'github'
  auth_subject  TEXT,                    -- external provider subject ID
  settings      JSONB NOT NULL DEFAULT '{}',
  created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

### MCP client registry

```sql
CREATE TABLE mcp_clients (
  id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  owner_id      UUID NOT NULL REFERENCES users(id),
  name          TEXT NOT NULL,           -- 'Claude Desktop', 'Cursor', 'ChatGPT'
  api_key_hash  TEXT NOT NULL,           -- bcrypt hash of client API key
  permissions   JSONB NOT NULL DEFAULT '{}',  -- allowed tools, sensitivity levels
  -- e.g.: {"tools": ["search", "browse", "capture"], "max_sensitivity": "normal"}
  requires_approval TEXT[] DEFAULT '{}', -- operations requiring human approval
  -- e.g.: ["delete", "update_sensitivity", "export"]
  active        BOOLEAN NOT NULL DEFAULT true,
  last_used_at  TIMESTAMPTZ,
  created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

### Audit log (for MCP accountability)

```sql
CREATE TABLE audit_log (
  id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id       UUID NOT NULL REFERENCES users(id),
  client_id     UUID REFERENCES mcp_clients(id),
  action        TEXT NOT NULL,           -- 'search', 'read', 'create', 'update', 'delete'
  resource_type TEXT,                    -- 'item', 'user', 'mcp_client'
  resource_id   UUID,
  request       JSONB,                   -- sanitized request details
  response_summary JSONB,               -- summary of what was returned/changed
  approved      BOOLEAN,                -- null if no approval needed, true/false otherwise
  created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

---

## 4. API Design

### GraphQL Schema (core types)

```graphql
type Item {
  id: ID!
  type: ItemType!
  status: ItemStatus!
  sensitivity: SensitivityLevel!
  title: String
  contentText: String
  contentUrl: String
  tags: [String!]!
  metadata: JSON!
  autoMetadata: JSON!
  source: String!
  sourceUrl: String
  createdAt: DateTime!
  updatedAt: DateTime!
  capturedAt: DateTime!
}

enum ItemType { BOOKMARK, VOICE_NOTE, TEXT_NOTE, DRIVE_DOC, HIGHLIGHT, FILE }
enum ItemStatus { PENDING, PROCESSING, READY, ERROR }
enum SensitivityLevel { PUBLIC, NORMAL, SENSITIVE, PRIVATE }

type SearchResult {
  items: [Item!]!
  total: Int!
  hasMore: Boolean!
}

type Query {
  item(id: ID!): Item
  items(filter: ItemFilter, limit: Int, offset: Int, sort: ItemSort): SearchResult!
  search(query: String!, filter: ItemFilter, limit: Int, threshold: Float): SearchResult!
  me: User!
}

type Mutation {
  captureBookmark(input: BookmarkInput!): Item!
  captureNote(input: NoteInput!): Item!
  updateItem(id: ID!, input: UpdateItemInput!): Item!
  deleteItem(id: ID!): Boolean!
  updateSensitivity(id: ID!, sensitivity: SensitivityLevel!): Item!
  tagItem(id: ID!, tags: [String!]!): Item!
}

input BookmarkInput {
  url: String!
  title: String
  selection: String
  referrer: String
  tags: [String!]
  sensitivity: SensitivityLevel
}

input ItemFilter {
  types: [ItemType!]
  tags: [String!]
  sensitivity: [SensitivityLevel!]
  source: String
  dateFrom: DateTime
  dateTo: DateTime
  status: [ItemStatus!]
}
```

### REST Endpoints

| Method | Path | Purpose |
|--------|------|---------|
| POST | `/upload` | Multipart file upload (audio, images) → returns S3 URL |
| GET | `/health` | Health check + version |
| * | `/graphql` | GraphQL endpoint |
| GET | `/playground` | GraphQL Playground (dev only) |
| * | `/mcp` | MCP streamable HTTP |

---

## 5. Auth & Authorization Architecture

### Authentication flow

1. **Web app**: Email/password login → JWT (access + refresh tokens)
2. **Chrome extension**: User logs in via web app, extension gets token via `chrome.identity` or manual paste
3. **Android app**: Email/password → JWT
4. **MCP clients**: Per-client API keys (registered in web UI)
5. **API direct**: Bearer JWT or API key

### JWT claims

```go
type Claims struct {
    jwt.RegisteredClaims
    UserID     uuid.UUID `json:"uid"`
    Email      string    `json:"email"`
    ClientType string    `json:"ctype"` // "web", "extension", "mobile", "api"
}
```

### Authorization model

Multi-tenant from day 1:
- Every item has `owner_id`
- All queries filter by authenticated user's ID
- Row-level security in PostgreSQL as defense-in-depth
- RBAC columns in users table for future multi-user scenarios

---

## 6. MCP Authorization Model

Defense-in-depth with three layers:

### Layer 1: Per-client tool permissions

Each MCP client has an allowlist of tools and maximum sensitivity level:
```json
{
  "tools": ["search", "browse", "capture"],
  "max_sensitivity": "normal",
  "read_only": false
}
```
- Client can only invoke allowed tools
- Search results automatically filtered to items at or below `max_sensitivity`

### Layer 2: Human-in-the-loop approval

Certain operations require real-time user approval:
- Configured per-client in `requires_approval` array
- When triggered: request is held, push notification sent to user's device
- User approves/denies via web app or mobile app
- Timeout after configurable period (default: 5 minutes) → deny

Implementation: WebSocket from web/mobile app to API server, PG LISTEN/NOTIFY for approval events.

### Layer 3: Content-level filtering

- Items with `sensitivity = 'private'` are never returned via MCP
- Items with `sensitivity = 'sensitive'` only returned to clients with `max_sensitivity >= 'sensitive'`
- Auto-classification suggests sensitivity on capture; user can override
- Audit log records every MCP access for accountability

---

## 7. Content Processing Pipeline

### Sync operations (block API response)
- Text embedding generation (< 1s for short content)
- Tag extraction from metadata
- Input validation

### Async operations (background jobs)
- Page content fetching for bookmarks (fetch URL, extract readable content)
- Audio transcription (Whisper API or local)
- Auto-metadata extraction (LLM classification)
- Sensitivity auto-classification
- Large content upload to S3
- Re-embedding after content changes

### Job processing

PostgreSQL-backed job queue (River or similar):
- Jobs stored in PG (no new infrastructure)
- Retries with exponential backoff
- Dead letter queue for failed jobs
- Status tracking (items move from `pending` → `processing` → `ready` / `error`)

---

## 8. Storage Architecture

### PostgreSQL (structured data + search)
- Item metadata, user data, MCP client registry, audit log
- pgvector embeddings for semantic search
- Full-text search indexes
- Background job queue
- Managed via goose migrations

### S3-Compatible Object Storage (large content)
- Development: MinIO in Docker
- Production: Cloudflare R2 (S3-compatible, no egress fees)
- Stores: page HTML/content, audio files, images, document snapshots
- Items reference S3 objects via `content_url` column

### Key hierarchy
```
s3://flotsam-{env}/
  {owner_id}/
    bookmarks/{item_id}/content.html
    voice_notes/{item_id}/audio.webm
    voice_notes/{item_id}/audio.mp3
    files/{item_id}/{filename}
```

---

## 9. Provider Abstraction

Pluggable interfaces for AI services, with local fallback for sensitive content:

```go
type EmbeddingProvider interface {
    Embed(ctx context.Context, text string) ([]float32, error)
    EmbedBatch(ctx context.Context, texts []string) ([][]float32, error)
    Dimensions() int
}

type TranscriptionProvider interface {
    Transcribe(ctx context.Context, audio io.Reader, opts TranscribeOpts) (string, error)
}

type ClassificationProvider interface {
    Classify(ctx context.Context, content string) (*Classification, error)
}
```

**Implementations:**
| Provider | Embedding | Transcription | Classification |
|----------|-----------|---------------|----------------|
| OpenAI | text-embedding-3-small (1536d) | Whisper API | gpt-4o-mini |
| Local | nomic-embed-text via Ollama | whisper.cpp | llama via Ollama |

**Routing logic**: If item sensitivity >= `sensitive`, use local providers. Otherwise, use configured default (OpenAI).

---

## 10. Ideas Stolen from Obsidian

- **Hierarchical tags**: `project/flotsam`, `person/sarah`, `topic/career` — not flat tags
- **Backlinks**: When a bookmark references another captured URL, create a link. Surface "what links to this" in the UI.
- **Daily digest**: Auto-generated summary of items captured each day
- **Graph view** (future): Visualize connections between items based on shared tags, backlinks, semantic similarity
- **Quick capture**: Obsidian's "quick add" pattern — minimal friction to save a thought

---

## 11. Technology Choices

### Core (proven in kapow)
| Component | Technology | Why |
|-----------|-----------|-----|
| API framework | Go + chi/v5 | Composable middleware, stdlib-compatible |
| GraphQL | gqlgen (schema-first) | Type-safe codegen, SDL source of truth |
| Database driver | pgx/v5 + pgxpool | Native PostgreSQL, connection pooling |
| Migrations | goose/v3 (embedded) | SQL-based, up/down, embedded in binary |
| JWT | golang-jwt/v5 | HS256 + RS256 support |
| Config | caarlos0/env | Struct-tag env var binding |
| Logging | log/slog (stdlib) | Structured, leveled |
| MCP | modelcontextprotocol/go-sdk | Official Go MCP SDK |

### New for flotsam
| Component | Technology | Why |
|-----------|-----------|-----|
| Background jobs | River (riverqueue/river) | PG-backed, Go-native, integrates with pgx |
| S3 client | aws-sdk-go-v2 | S3-compatible API (works with R2, MinIO) |
| Password hashing | golang.org/x/crypto/bcrypt | Standard library |
| HTTP client (page fetch) | go-readability | Extract readable content from HTML |
| Vector search | pgvector (via pgx) | No additional infrastructure |

### Frontend (proven in kapow)
| Component | Technology |
|-----------|-----------|
| Build | Vite 5 |
| Framework | React 18 + TypeScript 5 |
| UI | Mantine 7 |
| GraphQL client | urql + gql.tada |
| State | Zustand 5 |
| Forms | React Hook Form + Zod |
| Testing | Vitest + Playwright |

### Chrome Extension
| Component | Technology |
|-----------|-----------|
| Manifest | V3 |
| UI | React (shared with web app components where possible) |
| Communication | Chrome Extension APIs + GraphQL mutations to API |

### Android App (MVP)
| Component | Technology |
|-----------|-----------|
| Framework | TBD (comparison when issue is worked: Kotlin/Jetpack Compose vs React Native vs Flutter) |
| Voice | Android SpeechRecognizer or Google Speech API |
| Trigger | "Hey Google" routine, notification quick-action, or widget |

---

## 12. Project Structure

```
flotsam/
├── .beads/                  # Beads issue tracking
│   └── config.yaml
├── .claude/
│   └── skills/
│       └── swarm/           # Multi-agent skill
├── api/                     # Go API server
│   ├── cmd/server/          # Entrypoint
│   │   └── main.go
│   ├── internal/
│   │   ├── config/          # Env var binding
│   │   ├── auth/            # JWT validation, password hashing
│   │   ├── db/              # Connection pool, migrations
│   │   ├── item/            # Item CRUD, search
│   │   ├── middleware/      # Logging, auth, recovery, rate limit
│   │   ├── router/          # Chi router setup
│   │   ├── mcp/             # MCP server + authorization
│   │   ├── provider/        # AI provider interfaces + implementations
│   │   ├── storage/         # S3 client abstraction
│   │   ├── worker/          # Background job definitions
│   │   └── server/          # HTTP server lifecycle
│   ├── graph/               # GraphQL (gqlgen)
│   │   ├── schema/          # SDL files
│   │   ├── resolver/        # Hand-written resolvers
│   │   ├── model/           # Generated types
│   │   └── generated/       # Generated runtime
│   ├── migrations/          # SQL migrations (goose)
│   ├── go.mod
│   ├── Makefile
│   └── CLAUDE.md
├── web/                     # React/TypeScript frontend
│   ├── src/
│   │   ├── components/
│   │   ├── pages/
│   │   ├── hooks/
│   │   ├── stores/
│   │   └── assets/
│   ├── package.json
│   ├── vite.config.ts
│   ├── tsconfig.json
│   ├── Makefile
│   └── CLAUDE.md
├── extension/               # Chrome extension (Manifest V3)
│   └── (TBD when implemented)
├── mobile/                  # Android app
│   └── (TBD when implemented)
├── compose/                 # Docker Compose
│   ├── compose.yml          # Full stack
│   └── db.yml               # Dev database only
├── docs/
│   ├── specs/               # Product & architecture specs
│   │   └── product-overview.md
│   └── plans/               # Implementation plans
├── scripts/                 # Shell utilities
├── .gitignore
├── .env.example
├── CLAUDE.md                # Root project instructions
├── AGENTS.md                # Agent workflow instructions
├── Makefile                 # Root makefile
└── README.md
```

---

## 13. What This Session Creates

### Files to create

| # | File | Description |
|---|------|-------------|
| 1 | `.beads/config.yaml` | Via `bd init --prefix flt --no-db` |
| 2 | `CLAUDE.md` | Root project instructions (adapted from kapow) |
| 3 | `AGENTS.md` | Agent workflow (adapted from kapow/shatter) |
| 4 | `README.md` | Project overview |
| 5 | `Makefile` | Root build targets |
| 6 | `.gitignore` | Ignore rules |
| 7 | `.env.example` | Environment template |
| 8 | `api/go.mod` | Go module |
| 9 | `api/Makefile` | API build targets |
| 10 | `api/CLAUDE.md` | API-specific instructions |
| 11 | `web/package.json` | Node scaffold |
| 12 | `web/Makefile` | Web build targets |
| 13 | `web/CLAUDE.md` | Web-specific instructions |
| 14 | `compose/db.yml` | Dev database (PG 16 + pgvector) |
| 15 | `docs/specs/product-overview.md` | Product spec (this architecture doc) |
| 16 | `.claude/skills/swarm/SKILL.md` | Multi-agent skill |

### Beads issues to create (product roadmap)

**Epic**: `Flotsam MVP` (waits-for-gate all-children)

| # | Title | Type | P | Depends on |
|---|-------|------|---|------------|
| 1 | DB schema: items + pgvector + users | task | 1 | — |
| 2 | API server scaffold | task | 1 | — |
| 3 | GraphQL schema + resolvers | task | 1 | 1, 2 |
| 4 | Auth: JWT + user registration | feature | 1 | 1, 2 |
| 5 | S3 storage integration | task | 1 | 2 |
| 6 | Bookmark capture + page fetch | feature | 1 | 3, 4, 5 |
| 7 | Embedding + semantic search | feature | 1 | 3, 6 |
| 8 | Chrome extension | feature | 1 | 6 |
| 9 | Voice capture + transcription | feature | 1 | 3, 4, 5 |
| 10 | Android voice capture app | feature | 1 | 9 |
| 11 | Background job processing | task | 1 | 2 |
| 12 | Auto-classification pipeline | feature | 2 | 7, 11 |
| 13 | MCP server: search + browse + capture | feature | 1 | 3, 7 |
| 14 | MCP authorization model | feature | 2 | 4, 13 |
| 15 | Web app: search + browse UI | feature | 2 | 3, 7 |
| 16 | Web app: capture UI | feature | 2 | 3, 4 |
| 17 | Local AI provider support | feature | 2 | 7, 9 |

**Separate epics (post-MVP):**
- Google Drive integration
- iOS app
- Firefox/Safari extensions
- Android Auto / CarPlay voice trigger
- Graph view + backlinks
- Multi-user support
- Deployment + CI/CD

---

## 14. Verification

After scaffolding:
- `bd list` shows all created issues with correct dependencies
- `bd ready` shows unblocked issues (DB schema, API scaffold)
- `make help` shows available targets
- `go mod tidy` succeeds in `api/`
- Directory structure matches plan
- Git commit with all scaffolding files
