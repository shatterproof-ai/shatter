# shatter-go

Go language frontend. Go binary subprocess implementing the JSON-over-stdio protocol.

## Key Files

- `protocol/handler.go` — Protocol handler, uses `log/slog` for `[shatter-go]` prefixed stderr logging
- `protocol/log.go` — slog configuration: `LevelTrace` constant, `prefixHandler` for `[shatter-go]` format
- `instrument/executor.go` — Function execution and instrumentation

## Property-Based Testing (rapid + native fuzzing)

Two complementary approaches:
- **rapid** (`protocol/property_test.go`, `instrument/property_test.go`): semantic property tests — roundtrips, behavioral invariants, idempotency. Use for logical properties.
- **Native fuzzing** (`testing.F` in `*_fuzz_test.go`): byte-level mutation for crash/panic discovery at parsing boundaries. Use for any code that deserializes untrusted input.

When adding a protocol message type or parsing function, add both: a rapid property test for semantic correctness and a fuzz target for crash resistance. Seed corpus from existing test fixtures.

## Timeout Contract

Execution timeout: 5s default, overridden by `SHATTER_EXEC_TIMEOUT` env var (seconds). See `execTimeout()` in `instrument/executor.go`.
