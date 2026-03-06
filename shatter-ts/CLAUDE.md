# shatter-ts

TypeScript language frontend. Node.js subprocess implementing the JSON-over-stdio protocol.

## Key Files

- `src/main.ts` — Entry point, protocol handler
- `src/logger.ts` — pino logger configured for stderr with `[shatter-ts]` prefix, reads `SHATTER_LOG_LEVEL`

## Instrumentor Parity Contract

`instrumentor.ts` has two symbolic expression builders that **must handle the same AST node types**:

| Function | Purpose | Location |
|---|---|---|
| `buildSymExprWithFlow()` | Data flow analysis (tracks variables through assignments) | Lines ~278-352 |
| `buildSymExpr()` | Branch condition analysis (builds SymExpr for Z3) | Lines ~860-951 |

When adding support for a new AST node type (e.g. `CallExpression`, `ElementAccessExpression`), update **both** functions. If `buildSymExprWithFlow()` returns `{kind: "unknown"}` for a node type that `buildSymExpr()` handles, variables assigned from that expression become invisible to the solver, silently degrading concolic coverage.

## Property-Based Testing (fast-check)

Property tests live in `src/property.test.ts` using `fast-check`. Use `fc.letrec` for recursive types (SymExpr, TypeInfo). Priority targets:

- Protocol message roundtrips (table stakes)
- **SymExpr builder parity**: `buildSymExpr` and `buildSymExprWithFlow` must handle the same AST node types — test that neither returns `unknown` for nodes the other handles
- Instrumentor output structural validity
- Any function processing untrusted protocol input from the core engine

When adding a new AST node type or protocol message, add corresponding fast-check properties.

## Timeout Contract

Execution timeout: 15s default, overridden by `SHATTER_EXEC_TIMEOUT` env var (seconds). See `getExecTimeoutMs()` in `src/executor.ts`.
