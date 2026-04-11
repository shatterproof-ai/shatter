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

When adding a protocol message type or parsing function, add both. Seed corpus from existing test fixtures.

## Ite SymExpr Parity Contract

Go can deserialize `ite` SymExpr nodes but does not produce them — Go lacks data flow tracking; `exprToSymExpr` only resolves function parameters. Adding SSA phi-node merging is a separate effort. See `protocol/parity-matrix.yaml` `ite-symexpr-production-partial`.

## Side Effect Parity Contract

Go captures 2 of 7 canonical kinds. Both `instrument.SideEffect` (in `executor.go`) and `protocol.SideEffect` (in `protocol/types.go`) carry fields for all 7; only capture logic is missing.

Captured: `console_output` (stdout/stderr buffers in `executor.go`, stdout→"log" stderr→"error", no per-message truncation), `global_state_change` (pre/post diff of exported package-level variables). Not captured: `thrown_error` (panics handled internally — see `go-side-effects-partial`), `global_mutation`, `file_write`, `network_request`, `environment_read`.

`convertSideEffects()` in `protocol/handler.go` maps all 7 fields, so adding a new capture kind only requires populating the struct in `executor.go`.

Authoritative matrix: `protocol/parity-matrix.yaml` `side_effect_capabilities` and `allowed_divergences: go-side-effects-partial`.

## Prepare Parity Contract

Go implements `prepare` to pre-build the instrumented harness binary so subsequent execute calls skip `go build`. Handler: `handlePrepare()` in `protocol/handler.go`. Advertised in `CommandCapabilities` (`protocol/constants.go`). `prepare_id` is SHA-256 of `file:function:sorted-mock-symbols`, first 16 hex chars (`computePrepareID`). Storage: `handler.preparedHarnesses map[string]*instrument.PreparedHarness`. Idempotent. `generateHarnessTemplate` generates code that reads `shatter_inputs.json` at runtime. `handleTeardown` (level=function) + `handleShutdown` call `Cleanup()` on all harnesses.

## Timeout Contract

5s default, overridden by `SHATTER_EXEC_TIMEOUT` env var (seconds). See `execTimeout()` in `instrument/executor.go`.
