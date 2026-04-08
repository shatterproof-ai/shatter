# GraphQL Conventions

## Backend: gqlgen (Schema-First)

### Adding a query or type

1. Edit or create a `.graphql` file under `graph/schema/`
2. `make generate` — regenerates `graph/generated/generated.go` and `graph/model/models_gen.go`
3. Implement the new resolver method in `graph/resolver/schema.resolvers.go`
4. Add new deps to the `Resolver` struct in `graph/resolver/resolver.go` and wire them in `router.go`

**Never edit** `graph/generated/generated.go` or `graph/model/models_gen.go` — these are generated files.

## Frontend: gql.tada (Typed Queries) — Mandatory

**Every GraphQL query and mutation MUST use `graphql()` from `gql.tada`.** Raw template-string queries are **forbidden** — they bypass compile-time schema validation and have caused production bugs (fields that don't exist in the API schema silently pass `tsc` and break at runtime).

### Correct pattern

```tsx
import { graphql } from 'gql.tada'
import { useQuery } from '@/lib/graphql'

const MyQuery = graphql(`query MyQuery { field1 field2 }`)
const [result] = useQuery({ query: MyQuery })  // types inferred from schema
```

### Wrong pattern (DO NOT USE)

```tsx
// WRONG — raw string bypasses schema validation
const MyQuery = `query MyQuery { field1 field2 }`
const [result] = useQuery<{ myQuery: Result }>({ query: MyQuery })
```

Use `ResultOf<typeof MyQuery>` to extract result types when needed.

### Schema sync

After any change to `graph/schema/*.graphql`, regenerate the frontend schema:

```bash
make web-schema-sync
```

Both `web/schema.graphql` and `web/src/graphql-env.d.ts` are committed.

### Testing with mocked useQuery

When `useQuery` is mocked, the `args.query` argument is a `TypedDocumentNode` (not a string). Dispatch on the operation name from the AST:

```ts
function getOperationName(query: unknown): string {
  const doc = query as { definitions?: Array<{ name?: { value?: string } }> }
  return doc.definitions?.[0]?.name?.value ?? ''
}
// then: if (getOperationName(args.query) === 'MyQuery') { ... }
```
