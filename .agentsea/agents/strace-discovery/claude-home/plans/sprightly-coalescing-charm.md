# Plan: GraphQL Error Tracking Exchange (kapow-d1i.3)

## Context

We need a urql exchange that captures GraphQL errors as PostHog `graphql_error` events. This is part of the PostHog integration epic (kapow-d1i). The exchange sits in the urql pipeline, inspects operation results for `graphQLErrors`, and sends them to PostHog with the operation name and error messages. It must be a transparent passthrough when PostHog is disabled.

## Implementation

### 1. Create `web/src/lib/posthogExchange.ts`

A urql `Exchange` function that:
- Takes `ExchangeInput` (`{ forward, client }`) and returns `ExchangeIO`
- Uses `pipe`, `tap` from `wonka` (already a urql dependency) to inspect results
- On each `OperationResult` with `result.error?.graphQLErrors?.length > 0`:
  - Extracts operation name from `operation.query` AST
  - Calls `posthog.capture('graphql_error', { operation_name, errors: [...messages] })`
- When `posthogEnabled` is `false`, just forwards operations unchanged (no tap)

Key types (from `@urql/core`):
```ts
type Exchange = (input: ExchangeInput) => ExchangeIO
type ExchangeInput = { forward: ExchangeIO; client: Client }
type ExchangeIO = (ops$: Source<Operation>) => Source<OperationResult>
```

Helper to extract operation name:
```ts
function getOperationName(op: Operation): string {
  return op.query.definitions.find(d => d.kind === 'OperationDefinition')?.name?.value ?? 'unknown'
}
```

### 2. Update `web/src/lib/urqlClient.ts`

Insert `posthogExchange` between `cacheExchange` and `authExchange`:
```ts
exchanges: [cacheExchange, posthogExchange, authExchange(...), fetchExchange]
```

### 3. Create `web/src/lib/posthogExchange.test.ts`

Test cases:
- **Captures graphql_error event** when result has graphQLErrors — verify `posthog.capture` called with correct event name, operation name, and error messages
- **Does not capture** when result has no errors
- **Passthrough when disabled** — when `posthogEnabled` is false, forward is called but capture is not
- **Handles unknown operation name** — when query has no name, uses 'unknown'

Use `vi.mock` for posthog-js (already globally mocked in setup.ts). Create mock wonka sources to simulate urql pipeline.

## Files to modify
- `web/src/lib/posthogExchange.ts` — **new** (the exchange)
- `web/src/lib/posthogExchange.test.ts` — **new** (unit tests)
- `web/src/lib/urqlClient.ts` — insert exchange into chain

## Existing code to reuse
- `web/src/lib/posthog.ts` — `posthog` instance and `posthogEnabled` flag
- `web/src/test/setup.ts` — global posthog-js mock (already in place)

## Verification
```bash
cd web && pnpm install && pnpm build && pnpm lint && pnpm test
```
