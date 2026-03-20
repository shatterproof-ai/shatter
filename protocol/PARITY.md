# Frontend Parity Contract

This document classifies every protocol-visible capability by parity status and explicitly documents allowed divergences between frontends.

**Relationship to other docs:**
- `registry.yaml` — single source of truth for commands, schemas, and enumerated values
- `GOVERNANCE.md` — mandatory process for changing the protocol
- `conformance/conformance_cases.yaml` — runtime cross-frontend conformance test cases
- `parity-matrix.yaml` — machine-readable form of the tables in this document

---

## Classification Definitions

| Status | Meaning |
|--------|---------|
| **required** | Every conforming frontend must implement this. A frontend that does not handle a required command is non-conforming and must not be deployed. |
| **optional** | Frontend advertises support via handshake capabilities. Core gates calls on capability presence, so a frontend that omits an optional command is still conforming. |
| **frontend-specific** | Only meaningful in a particular language's ecosystem. Other frontends are not expected to support it. |

---

## Command Parity

`handshake` and `shutdown` are not listed in the `capabilities` section of `registry.yaml` because they are part of the base protocol — every frontend must handle them regardless of what it advertises.

The remaining commands are advertised in handshake capabilities. Core checks the capability list before sending optional commands.

| Command | Status | TypeScript | Go | Rust | Notes |
|---------|--------|------------|----|------|-------|
| `handshake` | required | ✅ | ✅ | ✅ | Base protocol; not a capability |
| `shutdown` | required | ✅ | ✅ | ✅ | Base protocol; not a capability |
| `analyze` | required | ✅ | ✅ | ✅ | All frontends implement |
| `execute` | required | ✅ | ✅ | ⚠️ partial | Rust handler exists but execution logic is unimplemented (see divergence `rust-execute-unimplemented`) |
| `generate` | required | ✅ | ✅ | ✅ | All frontends implement |
| `instrument` | optional | ✅ | ✅ | ❌ | Rust does not advertise; core gates on capability |
| `setup` | optional | ✅ | ✅ | ❌ | Rust does not advertise; core gates on capability |
| `teardown` | optional | ❌ | ✅ | ❌ | Only Go advertises; core gates on capability (see divergence `ts-missing-teardown`) |

---

## Complex Type Capabilities

All complex type capabilities are **optional**. Frontends advertise which types they can generate; core only requests generation for types the frontend has declared support for. Many types are ecosystem-specific (e.g. `rune` and `go_byte` are Go-only; `symbol` and `buffer` are TypeScript-only).

### TypeScript

| Capability | Status |
|-----------|--------|
| `date` | ✅ |
| `date_time` | ✅ |
| `duration` | ✅ |
| `reg_exp` | ✅ |
| `url` | ✅ |
| `big_int` | ✅ |
| `buffer` | ✅ |
| `error` | ✅ |
| `symbol` | ✅ |

### Go

| Capability | Status |
|-----------|--------|
| `date` | ✅ |
| `duration` | ✅ |
| `url` | ✅ |
| `reg_exp` | ✅ |
| `ip_address` | ✅ |
| `big_int` | ✅ |
| `rational` | ✅ |
| `big_decimal` | ✅ |
| `error` | ✅ |

### Rust

No complex type capabilities are implemented yet.

---

## Branch Types

Most branch types are universal. One is language-specific:

| Branch Type | Status | Frontends |
|------------|--------|-----------|
| `if` | required | all |
| `else_if` | required | all |
| `switch` | required | all |
| `ternary` | required | all |
| `logical_and` | required | all |
| `logical_or` | required | all |
| `while` | required | all |
| `for` | required | all |
| `select` | frontend-specific | go only |

---

## Side Effect Capabilities

Side effects are optional observations emitted in the `side_effects` array of execute responses. Frontends differ in which kinds they capture and at what granularity.

### Normalized kinds (same payload shape across all supporting frontends)

| Kind | Payload fields | TypeScript | Go | Rust |
|------|---------------|------------|----|------|
| `global_state_change` | `variable`, `before`, `after` | ✅ | ✅ | ✅ |
| `console_output` | `level`, `message` | ✅ | ✅ | ❌ |
| `thrown_error` | `error_type`, `message`, `stack` | ✅ | ❌ | ❌ |

All supported kinds use the field names above — no frontend-specific aliases.

### Divergences in capture coverage

- **`console_output` granularity**: TypeScript captures per-call via a proxy console (individual log levels); Go captures process-level stdout/stderr bulk output (all stdout → level `"log"`, all stderr → level `"error"`); Rust does not capture console output at all. See divergence `side-effect-console-coverage`.
- **`thrown_error` placement**: TypeScript emits thrown errors as a `thrown_error` side effect inside the `side_effects` array. Go and Rust report them in the top-level `thrown_error` response field instead. See divergence `side-effect-thrown-error-placement`.

---

## Protocol Behaviors

### Execution Timeout

All frontends must read `SHATTER_EXEC_TIMEOUT` (seconds) and apply it to function execution. Invalid values (non-numeric, zero, negative) fall back to the default silently.

| Frontend | Default | Implementation |
|----------|---------|----------------|
| TypeScript | 15s | `getExecTimeoutMs()` in `src/executor.ts` |
| Go | 5s | `execTimeout()` in `instrument/executor.go` |
| Rust | 5s | `exec_timeout_from_env()` in `src/handler.rs` (stored; not yet applied) |

### Error Codes

All 11 error codes defined in `registry.yaml` apply to all frontends. Frontends should return the most specific applicable code and fall back to `internal_error` for unexpected failures.

### Version Compatibility

Core and frontends must agree on **major.minor** protocol version. Patch differences are tolerated. On mismatch, the frontend returns `version_mismatch`.

---

## Allowed Divergences

These are cross-frontend mismatches that are known, tracked, and explicitly accepted as temporary. They produce warnings (not failures) in the conformance harness. Each must have a resolution path.

### `go-extra-functions-field`

**Description:** The Go handler includes an extra `functions` field in all responses. TypeScript and Rust do not emit this field.

**Affected frontends:** go

**Affected commands:** all

**Status:** tracked — produces a conformance warning, not a failure

**Resolution:** Remove the extra field from Go responses in a future cleanup pass.

---

### `ts-missing-teardown`

**Description:** TypeScript does not advertise the `teardown` capability and does not implement a teardown handler. This means any state initialized with `setup` in a TypeScript session cannot be explicitly torn down via protocol — the frontend relies on process-level cleanup instead.

**Affected frontends:** typescript

**Affected commands:** teardown

**Status:** tracked

**Resolution:** Implement teardown in the TypeScript frontend and add `teardown` to its `SUPPORTED_CAPABILITIES`.

---

### `rust-execute-unimplemented`

**Description:** The Rust frontend advertises `execute` in its capabilities and has a handler that parses execute requests, but the actual execution logic is not implemented. Execute calls return a stub or error response.

**Affected frontends:** rust

**Affected commands:** execute

**Status:** tracked

**Resolution:** Implement the execute handler in `shatter-rust/src/handler.rs`. At that point also apply the stored exec timeout value.

---

### `side-effect-console-coverage`

**Description:** All three frontends use the same `console_output` shape (`{ kind, level, message }`), but differ in what they capture. TypeScript intercepts each `console.*` call individually (per-call granularity, distinct log levels). Go captures process-level stdout/stderr as a single blob after the subprocess exits (all stdout becomes level `"log"`, all stderr becomes level `"error"`). Rust does not capture console output at all.

**Affected frontends:** all

**Affected commands:** execute (side_effects field)

**Status:** tracked — shape is normalized; coverage granularity divergence is accepted

**Resolution:** Improve Go to capture per-call console output; add console capture to the Rust frontend. Both require harness code injection changes.

---

### `side-effect-thrown-error-placement`

**Description:** TypeScript emits thrown errors as a `{ kind: "thrown_error", error_type, message, stack }` entry inside the `side_effects` array. Go and Rust report thrown errors exclusively via the top-level `thrown_error` field in the execute response, not as a side effect entry.

**Affected frontends:** go, rust

**Affected commands:** execute (side_effects field)

**Status:** tracked

**Resolution:** Add `thrown_error` side effect emission to Go and Rust executors, mirroring the TypeScript behavior. The top-level field can be retained for backwards compatibility.

---

## Adding a New Divergence

When a new cross-frontend mismatch is discovered:

1. Add a `known_drifts` entry in `conformance/conformance_cases.yaml` (prevents CI failure)
2. Add an entry in the **Allowed Divergences** section above with description, affected scope, status, and resolution path
3. Add the same entry to `parity-matrix.yaml` under `allowed_divergences`

When resolving a divergence:

1. Remove from `known_drifts` in `conformance_cases.yaml`
2. Update status to `resolved` in the Allowed Divergences section and `parity-matrix.yaml`
3. Verify conformance harness passes without warnings
