# Plan: MCP Feature/Docs Alignment (flt-it8.27)

## Context
Project docs and README present MCP as available, while the implementation plan still describes a 501 placeholder. In reality, MCP is fully implemented with 3 tools, JWT auth, and rate limiting. The issue is stale docs, not missing code. The product-overview.md also describes advanced MCP auth features (per-client keys, HITL approval, audit logging) without clearly marking them as planned/future.

## Approach: Update docs to match reality

### 1. Update `docs/plans/flt-it8.13-mcp-server.md` — mark as completed
- Add a "Status: Completed" header or note at the top
- The context section says "serves a 501 placeholder" and "package doesn't exist yet" — add a note that this plan has been implemented

### 2. Update `docs/specs/product-overview.md` — clarify MCP implementation status
- In the MCP Authorization Model section (lines ~345-373), clearly mark which layers are implemented vs planned:
  - **Implemented**: JWT auth (mandatory), rate limiting, 3 tools (search/browse/capture)
  - **Planned (flt-it8.14)**: Per-client API keys, tool permission allowlists, HITL approval, sensitivity filtering, audit logging
- Ensure the MCP client registry table and audit log table are marked as planned schema

### 3. Update `api/CLAUDE.md` — minor clarification
- Line 83 says "mandatory JWT or API key" for /mcp/* — API key auth is NOT yet implemented. Fix to say "mandatory JWT" only.

### 4. README.md — minor clarification
- Line 11-12 mentions "defense-in-depth authorization" — this is aspirational. Soften to reflect current state (JWT auth) while noting the planned model.

## Files to modify
- `docs/plans/flt-it8.13-mcp-server.md` — add completion status
- `docs/specs/product-overview.md` — annotate MCP sections with implemented vs planned
- `api/CLAUDE.md` — fix "/mcp/* Auth" description (remove "API key" claim)
- `README.md` — adjust MCP description accuracy

## Verification
- `make api-test-unit` — ensure no code changes break anything
- `make api-lint` — clean lint
- Review each doc change for accuracy against actual code in `api/internal/mcp/`
