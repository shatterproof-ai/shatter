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
| `execute` | required | ✅ | ✅ | ✅ | All frontends implement |
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
| `console_output` | `level`, `message` | ✅ | ✅ | ✅ |
| `thrown_error` | `error_type`, `message`, `stack` | ✅ | ✅ | ✅ |
| `global_mutation` | `name` | ✅ | ✅ | ❌ |

All supported kinds use the field names above — no frontend-specific aliases.

### Divergences in capture coverage

- **`console_output` granularity**: TypeScript captures per-call via a proxy console (individual log levels); Go and Rust capture process-level stdout/stderr bulk output (all stdout → level `"log"`, all stderr → level `"error"`). Rust crate-bridge harness does not capture console output. See divergence `side-effect-console-coverage`.
- **OS-level capture**: Go does not yet capture `file_write`, `network_request`, or `environment_read`; these require OS-level or runtime interception. See divergence `go-side-effects-partial`.

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

Every entry below mirrors a record in `parity-matrix.yaml` `allowed_divergences:` and carries the same required metadata (Owner, Tracking issue, Status, Resolution condition). The validator (`scripts/validate-parity.py`) fails when:

- a divergence entry is missing any required metadata field;
- an id appears in this document but not in `parity-matrix.yaml`, or vice versa;
- a `resolved` entry remains in either file more than 30 days past its `resolved_at` date.

### `ts-missing-teardown`

**Description:** TypeScript does not advertise the `teardown` capability and does not implement a teardown handler. State initialized with `setup` in a TypeScript session is cleaned up at the process level rather than via an explicit teardown call.

**Affected frontends:** typescript

**Affected commands:** teardown

**Status:** tracked

**Owner:** Ketan Gangatirkar

**Tracking issue:** str-1hlk.13

**Resolution condition:** `teardown` is present in `SUPPORTED_CAPABILITIES` in `shatter-ts/src/handlers.ts` and a teardown handler returns success.

**Resolution:** Implement teardown in the TypeScript frontend and add `teardown` to its `SUPPORTED_CAPABILITIES`.

---

### `ts-rust-execute-plan-not-implemented`

**Description:** The `Command::Execute.plan` field carries an optional `InvocationPlan` that Go consumes for method-receiver invocation. TypeScript and Rust accept the field on the wire but do not consume it; Execute behavior on those frontends is unchanged. Method targets observe a second axis: Go yields `runtime_failed`/`completed` based on plan presence; TS/Rust always yield `unsupported`/`method_not_supported`.

**Affected frontends:** typescript, rust

**Affected commands:** execute

**Status:** tracked

**Owner:** Ketan Gangatirkar

**Tracking issue:** str-1hlk.16

**Resolution condition:** TypeScript and Rust execute handlers consume `Command::Execute.plan` and dispatch method targets via plan-driven invocation, matching Go's `runtime_failed`/`completed` outcome contract for method targets.

**Resolution:** Implement plan-aware execute in TypeScript and Rust when planners are added in those frontends.

---

### `side-effect-console-coverage`

**Description:** All frontends that capture console output use the same `{ kind, level, message }` shape, but differ in granularity. TypeScript intercepts each `console.*` call individually (per-call granularity, distinct log levels). Go captures process-level stdout/stderr in bulk (all stdout → `"log"`, all stderr → `"error"`). Rust crate-bridge harness does not capture console output.

**Affected frontends:** go

**Affected commands:** execute (side_effects field)

**Status:** tracked

**Owner:** Ketan Gangatirkar

**Tracking issue:** str-1hlk.20

**Resolution condition:** Go executor emits per-call `console_output` entries with distinct log levels (log/info/warn/error/debug), matching TypeScript granularity.

**Resolution:** Improve Go to capture per-call console output via harness injection.

---

### `side-effect-thrown-error-placement`

**Description:** TypeScript and Rust emit thrown errors/panics as a `{ kind: "thrown_error", error_type, message, stack }` entry inside the `side_effects` array AND via the top-level `thrown_error` response field. Go now reports thrown errors via both placements as well.

**Affected frontends:** go

**Affected commands:** execute (side_effects field)

**Status:** resolved

**Resolved at:** 2026-05-07

**Owner:** Ketan Gangatirkar

**Tracking issue:** str-1hlk.14

**Resolution condition:** Go executor emits `thrown_error` entries inside the `side_effects` array in addition to the top-level `thrown_error` field.

**Resolution:** Add `thrown_error` side effect emission to Go executor, mirroring TypeScript/Rust behavior. The top-level field can be retained for backwards compatibility.

---

### `go-side-effects-partial`

**Description:** Go captures `console_output`, `global_state_change`, `thrown_error`, and `global_mutation` side effects. The kinds `file_write`, `network_request`, and `environment_read` are defined in the protocol struct and wire format but not yet emitted by the Go executor because they require OS-level or runtime interception rather than the existing wrapper-level observation.

**Affected frontends:** go

**Affected commands:** execute

**Status:** tracked

**Owner:** Ketan Gangatirkar

**Tracking issue:** str-1hlk.21

**Resolution condition:** Go executor captures and emits `file_write`, `network_request`, and `environment_read` side effects, or the parity contract documents a permanent limitation for a kind that cannot be captured safely.

**Resolution:** Plan and implement OS-level or runtime interception for file writes, network requests, and environment variable reads, potentially by using the sandbox runner's observation boundary.

---

### `medium-opacity-analyze-partial`

**Description:** The `medium_opacity` field on `TypeInfo` opaque variants is an optional advisory field. TypeScript and Go frontends emit it when medium-confidence heuristics fire. The Rust frontend analyze handler is a stub and does not yet perform static analysis, so it never emits `medium_opacity`. The field is intentionally non-skip-causing in the Rust core.

**Affected frontends:** rust

**Affected commands:** analyze

**Status:** accepted

**Owner:** Ketan Gangatirkar

**Tracking issue:** none (intentional permanent divergence)

**Resolution condition:** Permanent divergence — Rust analyze is a stub. If the Rust frontend ever gains full static analysis, this entry should be removed and `medium_opacity` emission added following TS/Go heuristics.

**Resolution:** Intentional permanent divergence for the Rust frontend analyze stub.

---

### `ite-symexpr-production-partial`

**Description:** The `ite` `SymExpr` variant represents SSA phi-node merges from conditional variable reassignment. TypeScript produces `ite` expressions via data flow analysis. Go and Rust frontends can deserialize `ite` but do not produce it because they lack data flow tracking.

**Affected frontends:** go, rust

**Affected commands:** analyze

**Status:** tracked

**Owner:** Ketan Gangatirkar

**Tracking issue:** str-1hlk.17

**Resolution condition:** Go frontend produces `ite` `SymExpr` nodes from conditional reassignment via data flow analysis. Rust portion is deferred until full analyze implementation lands and can be split into a follow-up entry then.

**Resolution:** Add data flow tracking to Go frontend. Rust frontend deferred until full analyze implementation lands.

---

### `error-code-preflight-failed-typescript-only`

**Description:** The `preflight_failed` error code is declared in all three frontend bindings for wire compatibility, but only TypeScript currently runs an environment preflight that emits it. Go and Rust accept and round-trip the code on the wire but never produce it.

**Affected frontends:** go, rust

**Affected commands:** analyze, instrument, prepare, execute, setup

**Status:** tracked

**Owner:** Ketan Gangatirkar

**Tracking issue:** str-1hlk.18

**Resolution condition:** Go and Rust frontends emit `preflight_failed` for equivalent missing-dependency or missing-toolchain conditions (missing `go.sum` / module cache for Go; missing toolchain or target dir for Rust).

**Resolution:** Add an environment preflight in the Go and Rust frontends that emits `preflight_failed` for the equivalent conditions.

---

### `loop-body-states-typescript-only`

**Description:** TypeScript emits `loop_body_states` in execute responses for supported canonical counted loops, combining cached `LoopInfo` metadata with observed runtime iteration counts. Go and Rust include the field in protocol structs for wire compatibility but do not populate it yet.

**Affected frontends:** go, rust

**Affected commands:** execute

**Status:** tracked

**Owner:** Ketan Gangatirkar

**Tracking issue:** str-1hlk.19

**Resolution condition:** Go and Rust execute paths populate `loop_body_states` with the same `loop_id` and zero-based `iteration` contract as TypeScript, or the reconstruction logic moves into a shared core-side postprocessor.

**Resolution:** Add loop snapshot emission to Go and Rust execute paths.

---

## Adding a New Divergence

When a new cross-frontend mismatch is discovered:

1. Add an entry to `parity-matrix.yaml` under `allowed_divergences` with **all required metadata fields**: `id`, `description`, `affected_frontends`, `affected_commands`, `status`, `resolution`, `owner`, `tracking_issue`, `resolution_condition`. `resolved` entries also need `resolved_at` (ISO YYYY-MM-DD).
2. Mirror the entry as a `### <id>` block in the **Allowed Divergences** section above with the same metadata. The validator fails when ids in the two files diverge.
3. File a bd issue tracking the concrete work and reference its id in `tracking_issue`. Use the literal string `"none"` only for `accepted` (intentional permanent) divergences.
4. If a corresponding `known_drifts` entry is added in `conformance/conformance_cases.yaml`, reference the divergence id in its comment.

When resolving a divergence:

1. Set `status: resolved` and add `resolved_at: YYYY-MM-DD` in `parity-matrix.yaml`. Mirror in this document.
2. Remove the matching `known_drifts` entry from `conformance_cases.yaml`.
3. Verify conformance harness passes without warnings.
4. Delete both entries within 30 days of `resolved_at` — the validator fails after the grace window expires.
