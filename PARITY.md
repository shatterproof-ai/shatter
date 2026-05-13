# Frontend Feature Parity Matrix

Implementation status of protocol features across Shatter's three language frontends.

**Legend:** Y = implemented | N = not implemented | P = partial

Last updated: 2026-05-13

## Protocol Commands

| Command | TypeScript | Go | Rust | Notes |
|---------|-----------|-----|------|-------|
| handshake | Y | Y | Y | All return capabilities list |
| analyze | Y | Y | Y | Branch detection, type inference, literals |
| instrument | Y | Y | Y | Source-to-source with branch/constraint tracking |
| prepare | Y | Y | Y | Pre-builds reusable harness artifacts where supported |
| execute | Y | Y | Y | Rust uses harness-backed execution; some plan surfaces remain unsupported |
| setup | Y | Y | Y | Rust compiles and executes setup files |
| teardown | Y | Y | Y | Rust acknowledges teardown and clears function-level caches |
| generate | Y | Y | Y | Rust supports WASM and native Rust generator dispatch |
| shutdown | Y | Y | Y | |

## Execution Features

| Feature | TypeScript | Go | Rust | Notes |
|---------|-----------|-----|------|-------|
| Function execution | Y | Y | Y | Go and Rust use subprocess harnesses; TS uses vm.runInContext |
| SHATTER_EXEC_TIMEOUT | Y | Y | Y | TS default 15s, Go default 5s, Rust default 5s |
| Performance metrics | Y | Y | Y | wall_time_ms and related timing fields |
| Mock support | Y | Y | P | Rust registers symbol-level return-value mocks in generated harnesses; some interception surfaces remain limited |
| Setup/teardown lifecycle | Y | Y | Y | Per-function and per-execution modes |

## Side-Effect Capture

| Side Effect | TypeScript | Go | Rust | Notes |
|-------------|-----------|-----|------|-------|
| Console output | Y | Y | P | Rust captures process stdout/stderr in standalone/dispatch harnesses; crate-bridge skips |
| process.stdout.write | N | N | N | TS only captures console methods, not raw stdout |
| File I/O | N | P | N | Go captures high-confidence `os.WriteFile` calls |
| Network calls | N | P | N | Go captures package-level `net/http` calls |
| Environment variable access | N | P | N | Go captures `os.Getenv` and `os.LookupEnv` |
| Global mutation | Y | Y | N | Rust emits `global_state_change`, not generic `global_mutation` |
| Thrown errors | Y | Y | Y | All frontends report thrown errors or panics |
| Global state changes | Y | Y | Y | Canonical `global_state_change` side effect |

## Analysis Features

| Feature | TypeScript | Go | Rust | Notes |
|---------|-----------|-----|------|-------|
| Branch detection | Y | Y | Y | if/else, switch/match, loops |
| Type inference | Y | Y | Y | Parameter and return types |
| Literal extraction | Y | Y | Y | String, number, bool literals from source |
| Symbolic constraint reporting | Y | Y | P | Rust extracts branch conditions but not ITE-producing data-flow expressions |
| Call graph / dependency extraction | Y | Y | Y | Function-level call dependencies |
| Generator support | Y | Y | P | Rust supports generator dispatch; native Rust generators require a custom frontend build |

## Instrumentation Features

| Feature | TypeScript | Go | Rust | Notes |
|---------|-----------|-----|------|-------|
| Branch recording | Y | Y | Y | Inserts recording calls at branch points |
| Constraint JSON embedding | Y | Y | Y | Symbolic constraints serialized into instrumented source |
| Source map / line tracking | Y | Y | Y | Original line numbers preserved |

## Configuration

| Feature | TypeScript | Go | Rust | Notes |
|---------|-----------|-----|------|-------|
| Truncation policy | N | N | N | Planned (str-ebtz) |
| Memory limits | N | N | N | Planned (str-9x76): --max-old-space-size for TS, GOMEMLIMIT for Go |
| Log level control | Y | Y | Y | SHATTER_LOG_LEVEL env var |

## CLI Parity Contract

The CLI always sets the following environment variables before spawning any frontend
subprocess. These are the **governed defaults** — the values used when the user does
not pass the corresponding flag. Any change to a default must update the constants in
`cli_parity_tests` in `shatter-cli/src/helpers.rs` (which fail CI on mismatch).

| Env var | CLI flag | Governed default | Governed commands |
|---------|----------|-----------------|-------------------|
| `SHATTER_LOG_LEVEL` | `--log-level` | `info` | all frontend-spawning commands |
| `SHATTER_EXEC_TIMEOUT` | `--exec-timeout` | `10` s | `explore`, `scan`, `fuzz`, `reduce`, `re-run`, `run` |
| `SHATTER_BUILD_TIMEOUT` | `--build-timeout` | `30` s | same as above |

**Intentional exception:** The `observe` command uses `exec-timeout=30s` and
`build-timeout=60s` as defaults because it executes many inputs in a single
long-running session. This divergence is permitted and documented here.

**Frontend fallback defaults** (when CLI env var is absent, e.g. in standalone use):

| Frontend | `SHATTER_EXEC_TIMEOUT` fallback | `SHATTER_BUILD_TIMEOUT` fallback |
|----------|--------------------------------|----------------------------------|
| TypeScript | 15 s (`DEFAULT_EXEC_TIMEOUT_MS` in `executor.ts`) | N/A (transpilation is synchronous) |
| Go | 5 s (`defaultExecTimeout` in `executor.go`) | 30 s (`defaultBuildTimeout` in `executor.go`) |
| Rust | 5 s (`DEFAULT_EXEC_TIMEOUT_MS` in `handler.rs`) | 30 s (`SHATTER_BUILD_TIMEOUT` fallback in `executor.rs`) |

Fallback defaults differ from CLI defaults intentionally: the CLI always passes the
env var, so the frontend fallback is only relevant when a frontend is invoked without
the CLI (e.g. in tests or direct subprocess calls).
