# shatter-ts

TypeScript language frontend. Node.js subprocess implementing the JSON-over-stdio protocol.

## Key Files

- `src/main.ts` — Entry point, protocol handler, `log()` function that writes `[shatter-ts]` lines to stderr

## Timeout Contract

Execution timeout: 15s default, overridden by `SHATTER_EXEC_TIMEOUT` env var (seconds). See `getExecTimeoutMs()` in `src/executor.ts`.
