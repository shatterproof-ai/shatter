# Plan: WebMCP Typed Filter Payloads (kapow-8e8)

## Context

The WebMCP client (`web/src/hooks/useWebMCP.ts`) registers the `search_colleges` tool with a schema that declares filter `value` as `type: 'string'` (line 97). The backend expects typed JSON objects per field kind — e.g., `{"values": ["CA"]}` for multi-enum, `{"min": 1000, "max": 50000}` for numeric-range. An LLM calling this tool via MCP would send plain strings, which fail backend validation.

The server-side MCP (`api/internal/mcp/tools.go`) uses `search.FilterInput` with `json.RawMessage` for `Value`, which is correct — the Go MCP SDK infers the schema from the struct. The problem is entirely in the **web-side** MCP tool registration.

Additionally, the integration test at line 494 of `tools_test.go` uses `["CA"]` (bare JSON array) instead of the correct `{"values":["CA"]}` typed shape for the `state` multi-enum field.

## Changes

### 1. Fix web MCP tool schema (`web/src/hooks/useWebMCP.ts`)

Change the `search_colleges` tool schema so `value` is not `type: 'string'` but instead describes the actual typed filter contract:

```typescript
value: {
  description: 'Filter value — shape depends on field kind. ' +
    'Text/substring: {"value": "..."}. ' +
    'Full-text: {"query": "..."}. ' +
    'Single enum: {"value": "option"}. ' +
    'Multi enum: {"values": ["a", "b"]}. ' +
    'Numeric range: {"min": N, "max": N} (at least one). ' +
    'Exact numeric: {"value": N}. ' +
    'Geo radius: {"kind": "radius", "radius": {"lat": N, "lng": N, "radius_km": N}}. ' +
    'Geo bbox: {"kind": "bbox", "bbox": {"min_lat": N, "max_lat": N, "min_lng": N, "max_lng": N}}. ' +
    'Use get_search_fields to discover field kinds.',
  oneOf: [
    { type: 'object', description: 'Text/substring/exact-string/single-enum', properties: { value: { type: 'string' } }, required: ['value'] },
    { type: 'object', description: 'Full-text search', properties: { query: { type: 'string' } }, required: ['query'] },
    { type: 'object', description: 'Multi-enum', properties: { values: { type: 'array', items: { type: 'string' } } }, required: ['values'] },
    { type: 'object', description: 'Numeric range', properties: { min: { type: 'number' }, max: { type: 'number' } } },
    { type: 'object', description: 'Exact numeric', properties: { value: { type: 'number' } }, required: ['value'] },
    { type: 'object', description: 'Geo filter', properties: { kind: { type: 'string', enum: ['radius', 'bbox'] } }, required: ['kind'] },
  ],
}
```

Also fix the handler's type cast from `f.value as string` to properly pass the object through (the value may already be an object from the LLM).

### 2. Fix integration test filter format (`api/internal/mcp/tools_test.go:494`)

Change `["CA"]` to `{"values":["CA"]}` to match the typed multi-enum contract.

### 3. Update web tests (`web/src/hooks/useWebMCP.test.ts`)

Update the schema shape assertion to check for the new `oneOf` or object-typed `value` field instead of expecting a plain string type. Add a test that validates the handler passes filter objects correctly to GraphQL.

## Files to modify

- `web/src/hooks/useWebMCP.ts` — fix schema + handler type cast
- `web/src/hooks/useWebMCP.test.ts` — update schema assertions
- `api/internal/mcp/tools_test.go` — fix integration test filter format (line 494)

## Verification

1. `cd api && go test -short ./internal/mcp/...` — all MCP unit + contract tests pass
2. `cd web && pnpm build && pnpm lint` — TypeScript + lint clean
3. `cd web && pnpm test` — Vitest passes
4. `make test-quick` — fast gate
