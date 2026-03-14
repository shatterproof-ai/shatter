# Security Review Skill Enhancement — flt-it8.32.2

## Context
The existing `.claude/skills/security-review/SKILL.md` has a good skeleton (trigger patterns, 5-step workflow, output expectations) but lacks the detailed trust-boundary checklist and per-surface-area checks required by the governance hardening plan. The referenced policy file `docs/policies/trust-boundaries.md` doesn't exist yet.

## Deliverables

### 1. Expand SKILL.md with per-surface checklists
**File**: `.claude/skills/security-review/SKILL.md` (in worktree)

Keep existing frontmatter and trigger patterns. Add:

- **Trust Boundary Checklist** section with a table mapping each surface to its trust boundaries, untrusted inputs, and required controls
- **Per-Surface Review Checklists** covering all 9 surfaces:
  1. **Auth** (`api/internal/auth/`): JWT validation (algorithm pinning, expiry, audience), password hashing (bcrypt cost), session/token lifecycle
  2. **Middleware/Router** (`api/internal/middleware/`, `api/internal/router/`): input validation, path traversal, SSRF in redirects, rate limiting, CORS
  3. **Uploads** (`api/internal/router/upload.go`): content-type validation (server-side, not trusting client), size limits, filename sanitization, storage path injection
  4. **Storage** (`api/internal/storage/`): key ownership (owner_id in path), no user-controlled key components, access control on download
  5. **Workers** (`api/internal/worker/`): input sanitization (URLs in page_fetch = SSRF), resource limits (timeout, size), no credential leakage in logs
  6. **Providers** (`api/internal/provider/`): API key handling (no logging, no response leakage), sensitivity routing enforcement, response validation
  7. **MCP** (`api/internal/mcp/`): per-request auth enforcement, tool-level authorization, input validation on tool args
  8. **GraphQL Resolvers** (`api/graph/resolver/`): ownership filtering on all queries, input validation, N+1/DoS via deep queries
  9. **Client Token Storage** (web/android/chrome/CLI): secure storage choice, XSS exposure, token rotation, no plaintext secrets in logs
- **OWASP Top 10 Quick Reference** — project-specific mapping of which OWASP categories apply to which surfaces
- Expand trigger file patterns to include `api/internal/middleware/**`, `api/internal/mcp/**`, `api/internal/item/**`, `api/internal/cli/**`, `web/src/stores/**`, `web/src/lib/urqlClient.ts`
- Add trigger code patterns: `bcrypt`, `jwt`, `pgx.`, `river.`, `os.Getenv`, `SharedPreferences`

### 2. Create trust-boundaries policy
**File**: `docs/policies/trust-boundaries.md`

Reference document that SKILL.md points to. Contains:
- Trust boundary diagram (text-based) showing: Internet → Reverse Proxy → API → PostgreSQL/S3/AI Providers
- Per-boundary table: boundary name, what crosses it, required controls, example attack
- Client-specific boundaries (web browser, Android app, Chrome extension, CLI)

### 3. Create issue-closure policy (stub)
**File**: `docs/policies/issue-closure.md`

Referenced by both security-review and ship-gates skills. Minimal policy:
- Security-sensitive issues require security-review skill run before closure
- Must have test evidence for adversarial paths
- Residual risk must be documented

## Files to Modify/Create
- **Modify**: `.claude/skills/security-review/SKILL.md`
- **Create**: `docs/policies/trust-boundaries.md`
- **Create**: `docs/policies/issue-closure.md`

## Verification
1. Confirm all three files are valid markdown (no broken formatting)
2. Confirm SKILL.md trigger patterns cover all identified security-sensitive paths
3. Confirm cross-references between files are consistent
4. Commit and push on `worktree/sec-review` branch
