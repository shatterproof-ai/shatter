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
| `prepare` | optional | ✅ | ✅ | ✅ | All frontends implement reusable harness preparation |
| `execute` | required | ✅ | ✅ | ✅ | All frontends implement |
| `generate` | required | ✅ | ✅ | ✅ | All frontends implement |
| `instrument` | optional | ✅ | ✅ | ✅ | Core gates on capability |
| `setup` | optional | ✅ | ✅ | ✅ | Core gates on capability |
| `teardown` | optional | ✅ | ✅ | ✅ | All frontends implement |

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
| `file_write` | `path`, `content` | ❌ | ✅ | ❌ |
| `network_request` | `method`, `url`, `body` | ❌ | ✅ | ❌ |
| `environment_read` | `variable`, `value` | ❌ | ✅ | ❌ |

All supported kinds use the field names above — no frontend-specific aliases.

### Divergences in capture coverage

- **`console_output` granularity**: TypeScript and Go capture per-call output with individual log levels. Rust captures process-level stdout/stderr bulk output (all stdout → level `"log"`, all stderr → level `"error"`), and the Rust crate-bridge harness does not capture console output.
- **OS-level capture**: Go captures `file_write`, `network_request`, and `environment_read` through source-overlay rewrites of high-confidence stdlib calls only: `os.WriteFile`, `os.Getenv`, `os.LookupEnv`, and package-level `net/http.Get`/`Post`/`PostForm`. Lower-level file writes, raw sockets, custom clients, `http.Client.Do`, `os.Environ`, and uninstrumented dependencies are accepted limits of the current AST-only interception. See divergence `go-side-effects-partial`.

---

## Protocol Behaviors

### Execution Timeout

All frontends must read `SHATTER_EXEC_TIMEOUT` (seconds) and apply it to function execution. Invalid values (non-numeric, zero, negative) fall back to the default silently.

| Frontend | Default | Implementation |
|----------|---------|----------------|
| TypeScript | 15s | `getExecTimeoutMs()` in `src/executor.ts` |
| Go | 5s | `execTimeout()` in `instrument/executor.go` |
| Rust | 5s | `exec_timeout_from_env()` in `src/handler.rs`; applied by the harness runner |

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

While a `resolved` entry sits inside its 30-day grace window the validator only emits a warning ("schedule removal within N day(s)"). Because that warning is otherwise seen only by whoever happens to run `task parity` locally, the hard failure can first surface as a red gate on an unrelated parity-touching branch. The **Parity Expiry Watch** scheduled workflow (`.github/workflows/parity-expiry.yml`) closes that governance gap (str-5dx0): it runs `scripts/validate-parity.py --warn-as-error-within-days 14` weekly, escalating any resolved entry within 14 days of its removal deadline to a failure. The red *scheduled* run (and its failure notification) is the owner-actionable signal to remove the entry before the deadline blocks someone else's change. Run the same check locally at any time with `python3 scripts/validate-parity.py --warn-as-error-within-days N`.

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

### `go-side-effects-partial`

**Description:** Go captures all canonical side-effect kinds at the protocol level. For `file_write`, `network_request`, and `environment_read`, capture is limited to high-confidence stdlib package-level calls rewritten by the overlay AST instrumentation: `os.WriteFile`, `os.Getenv`, `os.LookupEnv`, and `net/http.Get`/`Post`/`PostForm`. Lower-level syscall/raw socket/custom client surfaces and uninstrumented dependencies are accepted permanent limits of the current source-overlay interception mechanism.

**Affected frontends:** go

**Affected commands:** execute

**Status:** accepted

**Owner:** Ketan Gangatirkar

**Tracking issue:** none

**Resolution condition:** Go continues to emit the supported stdlib call shapes and this matrix names the lower-level surfaces that the overlay AST mechanism does not attempt to capture.

**Resolution:** Keep Go capture source-level and deterministic. Do not add ptrace, eBPF, LD_PRELOAD, or platform-specific syscall interception unless a future issue explicitly introduces an OS-observation backend with a portability and sandboxing design.

---

### `go-symbolic-http-request-body`

**Description:** A direct `*http.Request` parameter in the Go frontend is analyzed as `{kind: "str", label: "*http.Request"}` and consumes one string input slot: the generated wrapper builds the request via `httptest.NewRequest("POST", "/", strings.NewReader(<body>))` with stub `x-api-key`, `Authorization: Bearer`, `x-goog-api-key`, and `Content-Type: application/json` headers, so the explorer/solver drive symbolic JSON bodies into HTTP handlers instead of a constant empty-body runtime value (str-e41w). The Go planner also seeds schema-agnostic JSON bodies ordered after project config hints. Nested `*http.Request` occurrences keep the fixed runtime-value expression. TypeScript and Rust have no equivalent native-request synthesis.

**Affected frontends:** typescript, rust

**Affected commands:** analyze, execute

**Status:** tracked

**Owner:** Ketan Gangatirkar

**Tracking issue:** str-e41w

**Resolution condition:** TS/Rust gain an equivalent symbolic-request-body synthesis for their native HTTP request types (e.g. express `Request`, `http::Request`), reporting the body param as a symbolic string in analyze output, or this stays a documented Go-only capability alongside `adapter_http_nethttp`.

**Resolution:** Mirror the design when TS/Rust handler exploration becomes a priority: analyzer reports the request param as a symbolic string body; the execution shim constructs the framework request around it; planner seeds stay schema-agnostic with config hints taking precedence.

---

### `ts-opaque-param-stub-registry`

**Description:** The TypeScript frontend carries a typed opaque-param stub registry (str-syj9b) — the TS analogue of Go's `runtimeval` registry / `go_runtime_values` config. A parameter whose declared type matches a registry entry (built-in `@playwright/test:Page` / `:Locator`, or a project `ts_runtime_values` entry in `.shatter/config.yaml`) is analyzed as `{kind: "object", fields: []}` instead of `{kind: "opaque"}`, so the Rust core no longer skips the function as unexecutable; at execute time the frontend overlays the generated argument with a structurally-valid recording proxy (per-type override map rotates the return values of branch-gating calls such as `locator().count()`). Entirely frontend-local: no wire schema change, no new protocol field — stub bindings travel analyzer→executor on the internal worker channel only. The only observable analyze-output change is for registry-covered handle params (object instead of opaque); no shared conformance fixture exercises such a type. Go has `runtimeval`; Rust has native-replay generators; neither reads `ts_runtime_values`.

**Affected frontends:** go, rust

**Affected commands:** analyze, execute

**Status:** accepted

**Owner:** Ketan Gangatirkar

**Tracking issue:** str-syj9b

**Resolution condition:** Permanent by design — the config key is language-neutral (`type key + language-specific factory payload`) so Go/Rust/other frontends could adopt the same `*_runtime_values` shape, but each frontend supplies its own per-language stub/expression mechanism. No cross-frontend wire contract is implied.

**Resolution:** If another frontend needs handle-param stubbing, mirror this design in that frontend: analyzer stops emitting bare opaque for registry-covered types and the execution shim supplies a structurally-valid value, keeping the config schema language-neutral.

---

### `medium-opacity-analyze-partial`

**Description:** The `medium_opacity` field on `TypeInfo` opaque variants is an optional advisory field. TypeScript and Go frontends emit it when medium-confidence heuristics fire. The Rust frontend performs static analysis but does not yet implement this medium-opacity heuristic, so it never emits `medium_opacity`. The field is intentionally non-skip-causing in the Rust core.

**Affected frontends:** rust

**Affected commands:** analyze

**Status:** accepted

**Owner:** Ketan Gangatirkar

**Tracking issue:** none (intentional permanent divergence)

**Resolution condition:** Permanent divergence until Rust adds medium-opacity analysis. If the Rust frontend gains this heuristic, this entry should be removed and `medium_opacity` emission added following TS/Go behavior.

**Resolution:** Intentional divergence for the current Rust analyzer.

---

### `ite-symexpr-production-partial`

**Description:** The `ite` `SymExpr` variant represents SSA phi-node merges from conditional variable reassignment. TypeScript and Go produce `ite` expressions via data-flow analysis. Rust can deserialize `ite` but does not yet perform the data-flow pass needed to produce it.

**Affected frontends:** rust

**Affected commands:** analyze

**Status:** tracked

**Owner:** Ketan Gangatirkar

**Tracking issue:** str-1hlk.17

**Resolution condition:** Rust frontend produces `ite` `SymExpr` nodes from conditional reassignment via data-flow analysis.

**Resolution:** Add data-flow tracking to the Rust frontend.

---

### `adapter-owned-instrumentation-coverage-partial`

**Description:** Adapter-owned Execute responses (`invocation_model = adapter`) originally returned empty `branch_path` / `lines_executed` / `path_constraints` / `calls_to_external` by construction, so targets invoked through invocation-model adapters contributed zero line coverage (epic str-j49xg). TypeScript now closes this for its only invocation-owned adapter, the react-hook adapter (str-26fhi): `executeAdapterOwned` threads the instrumented source through a shared instrumentation sandbox (`buildInstrumentedSandbox` / `loadInstrumentedModuleInSandbox`) that the adapter mounts/rerenders via `ctx.loadInstrumentedExports`, so adapter-owned coverage now matches direct calls. TS specifics kept honest: `branch_path` / `path_constraints` are scoped to the initial props-driven render (via `ctx.markInitialRenderComplete`) so state-driven rerender branch flips don't emit contradictory `X ∧ ¬X` constraints (UNSAT for the core's negation search); `path_constraints` is the per-branch constraint of `branch_path` (1:1). `lines_executed` / `scope_events` / `calls_to_external` are the full cross-render union. `loop_body_states` is intentionally **not** emitted on the adapter path (direct-path-only). The TS browser-globals adapter is a `SandboxProvider`, not an `InvocationHook`, so its targets stay `direct` and have always reported real coverage. Go (net/http, gin) and Rust (axum) adapter-owned launchers still return empty instrumentation fields.

**Affected frontends:** go, rust

**Affected commands:** execute

**Status:** tracked

**Owner:** Ketan Gangatirkar

**Tracking issue:** str-j49xg

**Resolution condition:** Go and Rust adapter-owned executions propagate `branch_path` / `lines_executed` from their instrumented launcher/overlay builds into `ExecuteResult`, matching the TypeScript react-hook adapter.

**Resolution:** Implement instrumentation propagation in the Go adapter launchers (str-1qd5i) and audit/fix the Rust axum adapter (str-3eki5).

---

### `rust-execute-response-fields-partial`

**Description:** The Rust frontend `Response` flattens the execute result and carries `return_value`, `thrown_error`, `branch_path`, `lines_executed`, `calls_to_external`, `path_constraints`, `side_effects`, `loop_body_states`, `performance`, and `outcome`, but omits five `ExecuteResult` fields the core defines: `scope_events` (scope-annotated trace for scope-aware path collapsing), `capture_truncation` (`TruncationInfo` for captured side effects), `discovered_dependencies` (deps observed at execution time that static analysis missed), `connection_failures` (drives the core's LiveFirst fallback), and `runtime_crypto_boundaries` (runtime-intercepted encrypt/decrypt APIs for boundary splitting). Genuine capability gaps, not wire bugs: the Rust execute path never produces these signals, and the core deserializes each via `#[serde(default)]` / omit-when-empty, so the absence is tolerated. TS/Go emit the subset each supports.

**Affected frontends:** rust

**Affected commands:** execute

**Status:** tracked

**Owner:** Ketan Gangatirkar

**Tracking issue:** str-924ca

**Resolution condition:** The Rust execute/instrument path produces and emits `scope_events`, `capture_truncation`, `discovered_dependencies`, `connection_failures`, and `runtime_crypto_boundaries` where the language allows, matching TS/Go capture; `e2e_concolic_rust` covers at least `scope_events`.

**Resolution:** Implement per-field capture in the Rust frontend (str-924ca). Remove this entry once the Rust frontend emits the fields it can support and this matrix names any that stay permanently unsupported.

---

### `rust-protocol-enum-vocabulary-narrower`

**Description:** Three Rust frontend protocol enums are narrower than their `shatter-core` counterparts, so the Rust analyzer can never emit the missing variants: `ConstValue` lacks `Undefined` and `Complex` (core `sym_expr.rs`); `BranchType` lacks `Select` (core `protocol.rs`); `TypeInfo::Opaque` carries only `label`, omitting the optional `static_opacity` (`StaticOpacityReason`) discriminator. Emit-only limitations — the Rust frontend produces these types in its analyze output and never deserializes them from the core, so the narrower vocabulary is a capability gap, not a wire-compat bug. The related `medium_opacity` Opaque field is tracked separately as `medium-opacity-analyze-partial`.

**Affected frontends:** rust

**Affected commands:** analyze

**Status:** tracked

**Owner:** Ketan Gangatirkar

**Tracking issue:** str-dcelm

**Resolution condition:** Rust `ConstValue`/`BranchType`/`TypeInfo::Opaque` are widened to match the core vocabularies and the Rust analyzer emits the variants where applicable (select branches, static-opacity reasons, undefined/complex constants), with round-trip coverage.

**Resolution:** Widen the Rust enums and add emission in the Rust analyzer (str-dcelm). Remove this entry once the vocabularies match.

---

### `rust-error-details-not-emitted`

**Description:** The core Error response variant (`ResponseResult::Error`) carries an optional `details` field for structured enrichment (stack trace, source location). The Rust frontend `Response` emits only `code` and `message` on error responses and never populates `details`. The field is optional on the core wire contract (`Option<serde_json::Value>`), so absence is tolerated; an accepted enrichment gap, not a wire bug. TS/Go likewise treat `details` as best-effort.

**Affected frontends:** rust

**Affected commands:** analyze, instrument, execute, setup, generate

**Status:** accepted

**Owner:** Ketan Gangatirkar

**Tracking issue:** none (accepted enrichment gap)

**Resolution condition:** Accepted while the Rust frontend surfaces errors as code + message only. If it gains structured error enrichment, it should populate `details` and this entry should be removed.

**Resolution:** Intentional divergence for the current Rust frontend. Error responses remain code + message; structured `details` is an optional enrichment not produced today.

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
4. Delete both entries within 30 days of `resolved_at` — the validator fails after the grace window expires. The weekly **Parity Expiry Watch** workflow (`.github/workflows/parity-expiry.yml`) turns red ~2 weeks before that deadline so the owner is notified in advance rather than discovering the failure on an unrelated branch.
