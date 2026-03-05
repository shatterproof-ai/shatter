# shatter-go

Go language frontend. Go binary subprocess implementing the JSON-over-stdio protocol.

## Key Files

- `protocol/handler.go` — Protocol handler, uses `log/slog` for `[shatter-go]` prefixed stderr logging
- `protocol/log.go` — slog configuration: `LevelTrace` constant, `prefixHandler` for `[shatter-go]` format
- `instrument/executor.go` — Function execution and instrumentation

## Timeout Contract

Execution timeout: 5s default, overridden by `SHATTER_EXEC_TIMEOUT` env var (seconds). See `execTimeout()` in `instrument/executor.go`.
