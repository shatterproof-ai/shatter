# shatter-go

Go language frontend. Go binary subprocess implementing the JSON-over-stdio protocol.

## Key Files

- `protocol/handler.go` — Protocol handler, `logf()` function that writes `[shatter-go]` lines to stderr
- `instrument/executor.go` — Function execution and instrumentation

## Timeout Contract

Execution timeout: 5s default, overridden by `SHATTER_EXEC_TIMEOUT` env var (seconds). See `execTimeout()` in `instrument/executor.go`.
