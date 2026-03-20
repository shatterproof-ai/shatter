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

## Side Effect Parity Contract

Go captures 2 of the 7 canonical side effect kinds. The `instrument.SideEffect` struct in `executor.go` and `protocol.SideEffect` in `protocol/types.go` both carry fields for all 7 kinds; only the capture logic is missing for the unimplemented kinds.

| Kind | Captured? | Source | Notes |
|---|---|---|---|
| `console_output` | Yes | `executor.go` stdout/stderr buffers | stdout → level "log", stderr → level "error"; no per-message truncation |
| `global_state_change` | Yes | `executor.go` pre/post diff | Tracks exported (capitalized) package-level variables |
| `thrown_error` | No | — | Panics handled internally; not yet surfaced as side effects (see `go-side-effects-partial` in parity-matrix.yaml) |
| `global_mutation` | No | — | Not yet captured |
| `file_write` | No | — | Requires OS-level interception; not yet planned |
| `network_request` | No | — | Requires HTTP interception; not yet planned |
| `environment_read` | No | — | Requires env-read interception; not yet planned |

The `convertSideEffects()` function in `protocol/handler.go` maps all 7 fields from `instrument.SideEffect` to `protocol.SideEffect`. Adding a new capture kind only requires populating the struct in `executor.go` — the conversion layer already handles it.

See `protocol/parity-matrix.yaml` `side_effect_capabilities` and `allowed_divergences: go-side-effects-partial` for tracking.

## Timeout Contract

Execution timeout: 5s default, overridden by `SHATTER_EXEC_TIMEOUT` env var (seconds). See `execTimeout()` in `instrument/executor.go`.
