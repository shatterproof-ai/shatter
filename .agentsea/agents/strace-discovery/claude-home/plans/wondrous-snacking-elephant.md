# Plan: MCP Walkthrough Demo (flt-12i)

## Context

The demo runner has boot-verification tests but no MCP coverage. This adds a Playwright spec that exercises the MCP JSON-RPC API (search + browse) against seeded data, proving the AI integration surface works without relying on live LLM output.

## Approach

Create `demo/tests/mcp-walkthrough.spec.ts` — a single new file using Playwright's `request` context (API-only, no browser).

### MCP Protocol Notes

The MCP server uses `mark3labs/mcp-go` StreamableHTTP transport at `/mcp`. The protocol requires a session handshake:

1. **POST `initialize`** — get server capabilities + session ID (`Mcp-Session-Id` header)
2. **POST `notifications/initialized`** — confirm (with session header)
3. **POST `tools/call`** — call tools (with session header)

All requests go to `http://localhost:8081/mcp` with `Authorization: Bearer <jwt>`.

### Test Flow

```
1. Login via GraphQL → get JWT token
2. MCP Initialize → get session ID
3. MCP Search "distributed systems" → verify results contain "Distributed Systems Are Hard"
4. Extract item ID from search result text (parse markdown)
5. MCP Browse item by ID → verify title, content, tags present
6. Log readable output for human observer
```

### Key Implementation Details

- **Auth**: POST to `http://localhost:8081/graphql` with login mutation
- **MCP requests**: POST JSON-RPC to `http://localhost:8081/mcp` with Bearer token + Mcp-Session-Id
- **Search response parsing**: Response is markdown text in `result.content[0].text`. Parse with regex to extract item IDs (UUID pattern after `id: `)
- **DEMO_MODE=manual**: Extra console.log output showing full responses
- **No config changes**: Only create the spec file

### File to Create

- `demo/tests/mcp-walkthrough.spec.ts`

### Seed Data Available

- User: `ketan@example.com` / `testpassword123`
- Items: "Distributed Systems Are Hard" (bookmark, tagged topic/distributed-systems), "Sourdough bread recipe" (text_note), "Go Performance Optimization" (bookmark), etc.

## Verification

```bash
cd demo && npx tsc --noEmit  # must pass with zero errors
```

Full runtime verification requires the demo environment (`scripts/demo.sh`).
