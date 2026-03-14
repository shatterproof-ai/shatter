# Feedback Form + GitHub Issues Integration

## Context

Kapow has no user-facing way to submit feedback. Users need a simple form to
report bugs, request features, or send general comments. Feedback should flow
into GitHub Issues for visibility and triage. The existing beads tracker stays
for internal dev work; GitHub Issues handles user-reported feedback only.

The `internal.user_feedback` table already exists in the database with a
flexible JSONB `data` column — we build on top of it.

---

## Go GitHub Client Library

| Library | Stars | License | Pros | Cons | Fit |
|---|---|---|---|---|---|
| `google/go-github` | 11k+ | BSD-3 | Actively maintained by Google; full Issues API coverage; well-documented | REST only (v3); large dependency | Best fit — simple issue creation |
| `shurcooL/githubv4` | 1k+ | MIT | GraphQL v4; efficient for complex queries | Overkill for creating issues; less maintained | Over-engineered for this |
| `gh` CLI subprocess | N/A | N/A | No Go dependency; already installed | Fragile stdout parsing; hard to test | Fragile |
| Raw HTTP | N/A | N/A | Zero dependencies | Manual auth, pagination, error handling | Too much boilerplate |

**Recommendation**: `google/go-github` — proven, well-maintained, right level of abstraction.

---

## Architecture

```
User submits form
  → GraphQL mutation (submitFeedback)
  → Saved to internal.user_feedback table (JSONB)
  → Batch sync tool reads unsynced rows
  → Creates GitHub Issues with labels (feedback:bug, feedback:feature, feedback:general)
  → Claude triages via `gh` CLI + promotes actionable items to beads
```

---

## Implementation

### 1. Database migration (`api/migrations/00010_feedback_github_sync.sql`)

Add `github_issue_url` column + partial index for unsynced rows:

```sql
-- +goose Up
ALTER TABLE internal.user_feedback ADD COLUMN github_issue_url TEXT;
CREATE INDEX user_feedback_unsynced_idx ON internal.user_feedback (created)
  WHERE github_issue_url IS NULL;

-- +goose Down
DROP INDEX IF EXISTS internal.user_feedback_unsynced_idx;
ALTER TABLE internal.user_feedback DROP COLUMN IF EXISTS github_issue_url;
```

### 2. GraphQL schema (`api/graph/schema/feedback.graphql`)

```graphql
enum FeedbackCategory { BUG  FEATURE  GENERAL }

input SubmitFeedbackInput {
  category: FeedbackCategory!
  subject: String!
  description: String!
  email: String
  userAgent: String
  pageUrl: String
}

type FeedbackResult { id: String!  success: Boolean! }

extend type Mutation {
  submitFeedback(input: SubmitFeedbackInput!): FeedbackResult!
}
```

### 3. Feedback service (`api/internal/feedback/service.go`)

- `Service` struct with `*pgxpool.Pool` (follows `preference.Service` pattern)
- `Submit(ctx, input)` — validates, generates ID, marshals to JSONB, inserts
- Validation: subject required (max 200), description required (max 5000), email format if provided
- Pattern reference: `api/internal/preference/service.go`

### 4. Config flag (`api/internal/config/config.go`)

```go
FeedbackAnonymousEnabled bool `env:"FEEDBACK_ANONYMOUS_ENABLED" envDefault:"true"`
```

Document in `.env.example`.

### 5. Resolver + wiring

- Add `submitFeedback` resolver in `api/graph/resolver/` (auto-generated file after `make api-generate`)
- Read `auth.GetClaims(ctx)` — use user_id/email from JWT if present
- If not authenticated and `FEEDBACK_ANONYMOUS_ENABLED=false`, return error
- Add `Feedback *feedback.Service` and `Config *config.Config` to `Resolver` struct
- Wire in `api/internal/router/router.go`
- Pattern reference: `api/graph/resolver/preferences.resolvers.go`

### 6. Frontend feedback form

**New files**:
- `web/src/components/feedback/FeedbackButton.tsx` — floating `Affix` button (bottom-right)
- `web/src/components/feedback/FeedbackModal.tsx` — Mantine Modal with form
- Tests alongside each component

**Form fields**: Category (Select), Subject (TextInput), Description (Textarea), Email (TextInput, only shown for anonymous users)

**Integration**: Render `<FeedbackButton />` in `App.tsx`. Uses `gql.tada` for the mutation.

Pattern reference: `web/src/components/auth/PreferencesModal.tsx`

### 7. GitHub sync tool (`tools/feedbacksync/`)

Standalone Go module using `google/go-github`:

```
tools/feedbacksync/
  go.mod
  main.go       (CLI: --database-url, --github-token, --repo, --dry-run, --batch-size)
  sync.go       (query unsynced → create GH issue → update row with issue URL)
  sync_test.go
```

- Maps categories to labels: `feedback:bug`, `feedback:feature`, `feedback:general`
- Auto-creates labels on first run
- Issue title: `[Feedback] {subject}`
- Issue body: Markdown with all fields + DB record ID
- Batch size default: 50

**Makefile targets**: `feedbacksync-build`, `feedbacksync-test`

### 8. Claude Code triage workflow (`docs/specs/feedback-triage.md`)

Documented workflow using `gh` CLI:
- List open feedback issues by label
- Analyze, prioritize, add labels
- Promote actionable items to beads: `bd create --title "..." --description "From feedback: <url>"`
- Close duplicates/non-actionable with explanatory comment

---

## Key files to modify

| File | Change |
|---|---|
| `api/internal/config/config.go` | Add `FeedbackAnonymousEnabled` field |
| `api/internal/router/router.go` | Wire `feedback.Service` + Config into resolver |
| `api/graph/resolver/resolver.go` | Add `Feedback` + `Config` to Resolver struct |
| `api/graph/schema/feedback.graphql` | New file |
| `web/src/App.tsx` | Add `<FeedbackButton />` |
| `.env.example` | Document `FEEDBACK_ANONYMOUS_ENABLED` |
| `Makefile` | Add feedbacksync targets |

## New files

| File | Purpose |
|---|---|
| `api/migrations/00010_feedback_github_sync.sql` | Add github_issue_url column |
| `api/internal/feedback/service.go` | Feedback service |
| `api/internal/feedback/service_test.go` | Unit tests |
| `api/graph/schema/feedback.graphql` | GraphQL schema |
| `web/src/components/feedback/FeedbackButton.tsx` | Floating trigger button |
| `web/src/components/feedback/FeedbackButton.test.tsx` | Button tests |
| `web/src/components/feedback/FeedbackModal.tsx` | Modal with form |
| `web/src/components/feedback/FeedbackModal.test.tsx` | Modal tests |
| `tools/feedbacksync/` | GitHub sync tool (go.mod, main.go, sync.go, sync_test.go) |
| `docs/specs/feedback-triage.md` | Triage workflow docs |

---

## Verification

1. `make api-generate` — codegen succeeds
2. `make test-standard` — all tests pass
3. `pnpm build && pnpm lint` — zero errors/warnings
4. Manual test: open feedback modal, submit form, verify row in `user_feedback` table
5. `cd tools/feedbacksync && go test ./...` — sync tool tests pass
6. Dry run: `go run . --dry-run` against dev DB — verify issue format
7. Live run: sync one feedback item to GitHub, verify issue created with correct labels
