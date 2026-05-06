---
name: frontend-parity
description: Shatter's cross-frontend parity contracts and divergence tracking. Use when adding or modifying a protocol message type, touching any frontend handler (TS/Go/Rust), changing side effect capture, changing prepare/execute behavior, or planning work that spans multiple frontends. Explains output parity vs implementation parity, names the capture matrices, and points to the authoritative tracking files.
user-invocable: true
---

# Frontend Parity Contract

Shatter has three language frontends: TypeScript (`shatter-ts/`), Go (`shatter-go/`), Rust (`shatter-rust/`). They communicate with `shatter-core` via a JSON-over-stdio protocol. **Output parity is required**; implementation parity is not.

## Output parity vs implementation parity

- **Output parity (required)**: JSON wire format, response field structure, error codes, observable behavior. Two frontends handling the same request should produce JSON that passes the same conformance checks.
- **Implementation parity (not required)**: internal types, helper functions, data structure choices. Frontends can diverge freely internally as long as wire output matches.

**Rule**: if a change affects wire output, error codes, or any observable behavior, it MUST update the parity contract. Internal refactors that leave JSON output identical do NOT require parity contract updates.

## Authoritative tracking files

| File | Role |
|---|---|
| `protocol/parity-matrix.yaml` | **The authoritative matrix.** `side_effect_capabilities` enumerates which frontend captures which side effect kinds. `allowed_divergences` lists known, tracked drift. |
| `protocol/conformance/conformance_cases.yaml` | Wire-format test cases run by `task conformance`. `known_drifts` section tracks accepted output differences. |
| `protocol/GOVERNANCE.md` | Checklist for adding a new protocol message type (registry, schemas, fixtures, per-frontend handlers, round-trip tests, conformance cases). |
| `shatter-ts/CLAUDE.md`, `shatter-go/CLAUDE.md`, `shatter-rust/CLAUDE.md` | Per-frontend parity contracts: timeout, instrumentor, side effects, prepare, invocation model, ite. |

**When in doubt, `protocol/parity-matrix.yaml` wins.** If this skill, a `CLAUDE.md` file, and `parity-matrix.yaml` disagree, trust the matrix.

## Capability summary (as of last audit)

### Side effect capture (7 canonical kinds)

| Kind | TS | Go | Rust |
|---|---|---|---|
| `console_output` | ✓ (capturing console, 4096 B/msg) | ✓ (stdout/stderr buffers, no trunc) | ✓ (fd redirection via libc dup, standalone/dispatch modes only; crate-bridge skips) |
| `global_state_change` | ✓ (pre/post diff of exported module vars) | ✓ (pre/post diff of exported package vars) | ✓ (`static mut` snapshots via serde) |
| `thrown_error` | ✓ (catch block → `error_type`/`message`/`stack`) | ✗ (panics handled internally, not surfaced — `go-side-effects-partial`) | ✓ (`catch_unwind`, `error_type: "runtime_error"`) |
| `global_mutation` | ✓ (name-only, exported module names) | ✗ | ✗ |
| `file_write` | ✗ (not intercepted) | ✗ | ✗ |
| `network_request` | ✗ | ✗ | ✗ |
| `environment_read` | ✗ | ✗ | ✗ |

### Invocation model dispatch

TS is the **reference implementation**. The executor inspects `FunctionAnalysis.invocation_model` and routes direct calls through `executeInstrumented` and adapter calls through `executeAdapterOwned` → `InvocationHook`. Go and Rust will reach parity in a later wave.

### Prepare command

All three frontends implement `prepare`. The `prepare_id` format is standardized: SHA-256 of `file:function:sorted-mock-symbols`, first 16 hex chars. Each frontend pre-builds or pre-warms something different:

- **TS**: pre-warms the compiled script cache (`compiledScriptCache` in `src/executor.ts`), skipping TypeScript → JS transpilation on subsequent executes.
- **Go**: pre-builds the instrumented harness binary once (`PreparedHarness` in `instrument/executor.go`), skipping `go build` on subsequent executes.
- **Rust**: pre-builds the harness binary, skipping rustc compilation on subsequent executes.

Lifecycle: all three clear prepared state on function-level teardown and shutdown.

### Ite SymExpr production (data flow tracking)

**TS is the only frontend that produces `ite` SymExpr nodes**, representing SSA phi-node merges from conditional variable reassignment. Go and Rust deserialize `ite` but don't produce it — Go lacks data flow tracking, Rust's analyze handler is a stub. See `protocol/parity-matrix.yaml` `ite-symexpr-production-partial`.

### Timeout contract

All frontends read `SHATTER_EXEC_TIMEOUT` env var (seconds), set by the CLI from `--timeout`.

| Frontend | Default | Impl |
|---|---|---|
| TS | 15 s | `getExecTimeoutMs()` in `src/executor.ts` |
| Go | 5 s | `execTimeout()` in `instrument/executor.go` |
| Rust | 5 s | `exec_timeout_from_env()` in `src/handler.rs` (stored, not yet applied — execute unimpl) |

Invalid values (non-numeric, zero, negative) fall back to the default silently.

## When you're about to…

- **Add a new protocol message type** → follow `protocol/GOVERNANCE.md` step by step; update all three frontends or document the drift in `known_drifts`; run `task parity` + `task conformance`.
- **Change execute response shape** → update `shatter-core/src/protocol.rs`, round-trip tests in all three frontends, conformance cases, and the affected frontend `CLAUDE.md`'s side effect / invocation / prepare section as relevant.
- **Add a new side effect capture kind in one frontend** → confirm the cross-frontend story in `protocol/parity-matrix.yaml`, update the affected frontend's `CLAUDE.md`, add known drift if other frontends won't match immediately.
- **Refactor a frontend internally without touching wire format** → no parity contract update needed. Confirm with `task conformance` that output is identical.
- **Plan work that spans multiple frontends** → Read one file in each target frontend's subtree before reasoning, so each crate's nested CLAUDE.md loads. Otherwise you plan without the per-frontend contracts.

## Validation gates

- `task parity` — registry consistency + capability contract
- `task conformance` — wire format parity across frontends
- `task check` — Full tier: runs both of the above plus cross-language tests
- `cargo test --test e2e_concolic` — full pipeline E2E through the TS frontend subprocess
