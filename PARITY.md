# Frontend Feature Parity Matrix

Implementation status of protocol features across Shatter's three language frontends.

**Legend:** Y = implemented | N = not implemented | P = partial | S = stub (command accepted, returns "not yet implemented")

Last updated: 2026-03-03

## Protocol Commands

| Command | TypeScript | Go | Rust | Notes |
|---------|-----------|-----|------|-------|
| handshake | Y | Y | Y | All return capabilities list |
| analyze | Y | Y | Y | Branch detection, type inference, literals |
| instrument | Y | Y | Y | Source-to-source with branch/constraint tracking |
| execute | Y | Y | S | Rust returns "not yet implemented" |
| setup | Y | Y | S | Rust returns "not yet implemented" |
| teardown | Y | Y | S | Rust returns "not yet implemented" |
| generate | Y | Y | S | Rust returns "not yet implemented" |
| shutdown | Y | Y | Y | |

## Execution Features

| Feature | TypeScript | Go | Rust | Notes |
|---------|-----------|-----|------|-------|
| Function execution | Y | Y | N | Go uses subprocess harness; TS uses vm.runInContext |
| SHATTER_EXEC_TIMEOUT | Y | Y | Y | TS default 15s, Go default 5s, Rust default 5s (stored, not applied yet) |
| Performance metrics | Y | Y | N | wall_time_ms, setup_time_ms |
| Mock support | Y | Y | N | Symbol-level function mocking with return values |
| Setup/teardown lifecycle | Y | Y | N | Per-function and per-execution modes |

## Side-Effect Capture

| Side Effect | TypeScript | Go | Rust | Notes |
|-------------|-----------|-----|------|-------|
| Console output | Y | N | N | TS proxies console.log/warn/error/info/debug |
| process.stdout.write | N | N | N | TS only captures console methods, not raw stdout |
| File I/O | N | N | N | Not captured in any frontend |
| Network calls | N | N | N | Not captured in any frontend |
| Environment variable access | N | N | N | Not captured in any frontend |
| Global mutation | N | N | N | Not captured in any frontend |
| Thrown errors | Y | P | N | TS captures in thrown_error + side_effects; Go captures in thrown_error only |
| Global state changes | N | N | N | Not captured in any frontend |

## Analysis Features

| Feature | TypeScript | Go | Rust | Notes |
|---------|-----------|-----|------|-------|
| Branch detection | Y | Y | Y | if/else, switch/match, loops |
| Type inference | Y | Y | Y | Parameter and return types |
| Literal extraction | Y | Y | Y | String, number, bool literals from source |
| Symbolic constraint reporting | Y | Y | N | TS and Go extract symbolic expressions from branches |
| Call graph / dependency extraction | Y | Y | Y | Function-level call dependencies |
| Generator support | Y | Y | N | Custom input generators from setup |

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
