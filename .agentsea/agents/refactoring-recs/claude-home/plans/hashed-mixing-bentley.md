# Autonomous External Dependency Mocking

## Context

Shatter runs on real-world projects with external dependencies (APIs, databases, filesystems, IPC, subprocesses). Currently, functions with external deps either fail at execution or get static auto-generated mocks that never vary — meaning Shatter can't explore branches that depend on *what the external call returns*. The goal is to treat mock return values as explorable parameters: generate diverse values, mutate them, and systematically discover code paths gated by external call outcomes.

"External" encompasses: network calls (HTTP, gRPC, WebSocket), database queries, filesystem I/O, IPC (named pipes, Unix sockets), subprocess spawning (`child_process.exec`, `os/exec`), and message queues.

## Design Principles

1. **Mock values are parameters** — they get the same treatment as input parameters: type-aware generation, mutation, crossover, coverage tracking, behavior map recording, exploration optimization. The only differences are (a) they appear mid-execution not at the start, (b) they may have expectation matching, and (c) loops mean a single mock may return multiple values.

2. **Type-driven first, category-aware second** — The primary generation strategy is type-driven: look at the `TypeInfo` return type and generate values that match it, using the existing `generate_random_value` infrastructure. Category classification (network, DB, etc.) is a *heuristic bonus* that produces more realistic shapes for known modules. The system works without it because real code wraps external calls behind layers of libraries and custom abstractions that category classification can't reliably detect.

3. **Try real first, then mock** — Default behavior is `LiveFirst`: attempt the real external call, classify the result (success → record for behavior map, connection failure → switch to autonomous mocking). A `--record` mode runs against a full integration stack and saves external call behavior maps as fixtures for future autonomous runs.

4. **Declared error types are explorable** — If a mock's return type includes error variants (Go `(T, error)`, Rust `Result<T, E>`, TS union with Error), explore that space like any other type variant. Undeclared errors (panics, RuntimeException) are not generated autonomously but are recorded in behavior maps when they occur.

5. **Report what you can't mock** — When a call is hard to mock (dynamic dispatch, closures, subprocess spawning), produce refactoring recommendations.

## What Exists Today

| Component | Status | Location |
|-----------|--------|----------|
| `ExternalDependency` protocol type | Done | `protocol.rs` |
| `MockConfig` / `MockBehavior` types | Done | `protocol.rs` |
| IoCategory classification | Done | `auto_mock.rs` — matches `source_module` against known strings |
| `generate_auto_mocks()` | Done | `auto_mock.rs` — produces static MockConfig |
| ScopeConfig (mock/passthrough globs) | Done | `scope.rs` |
| TS analyzer dep detection | Done | `analyzer.ts` — `extractDependencies()` via TypeChecker |
| TS executor mock registry | Done | `executor.ts` — VM sandbox, cycles through `return_values`, records via `mockCallFn` |
| Go executor mock generation | Done | `executor.go` — `generateMockFile()` writes Go source with mock functions |
| CLI wiring | Done | `main.rs` — generates auto_mocks, passes to ExploreConfig |
| `MockDecisionTree` | Done but unused | `mock_gen.rs` — flattened lossily to MockConfig |
| Input gen infrastructure | Done | `input_gen.rs` — generate, mutate, crossover, custom generators |
| Behavior maps | Done | `behavior.rs` — records inputs→outcomes |
| Dynamic mock variation | **Missing** | — |
| Error space exploration | **Missing** | — |
| Execution-time dep detection | **Missing** | — |
| Live-first / record mode | **Missing** | — |
| User fixture format | **Missing** | — |
| Refactoring recommendations | **Missing** | — |
| Rust frontend mock injection | **Missing** | execute unimplemented |

### Frontend Mock Registries

| Frontend | Mechanism | How it works |
|----------|-----------|--------------|
| **TypeScript** | Runtime VM sandbox | `mockRegistry` object injected into VM context. Instrumented code calls `mockRegistry[symbol]()`. Records calls via `mockCallFn`. |
| **Go** | Code generation | `generateMockFile()` writes a `.go` file with mock function implementations that index into a `retvals` slice. Compiled and executed. Each Execute call receives fresh mocks. |
| **Rust** | None yet | Execute unimplemented. When implemented, options: code generation (like Go), trait object injection, or proc-macro rewriting. |

## Plan

### Phase 1: Dynamic Mock Value Generation (MVP)

**Goal**: Vary mock return values per execution, treating them as explorable parameters with the same generation/mutation/crossover/tracking features as input parameters. Include declared error types in the exploration space.

#### 1A. `MockParam` type (`auto_mock.rs`)

New struct paralleling `ParamInfo`:

```rust
pub struct MockParam {
    pub symbol: String,
    pub return_type: TypeInfo,
    pub category: IoCategory,
    /// How many times this dep is called per execution (affects return_values length).
    pub call_count_estimate: u32,
    /// Where mock values come from (BuiltIn or Custom generator).
    pub value_source: ValueSource,
}
```

New function:
```rust
pub fn build_mock_params(
    deps: &[ExternalDependency],
    configs: &[MockConfig],
) -> Vec<MockParam>
```

Filters to only deps with non-passthrough MockConfig. Sets `call_count_estimate` from `dep.call_sites.len()`.

#### 1B. Mock value generation (`input_gen.rs`)

Three new functions mirroring the existing input generation API:

- **`generate_mock_values(mock_params, rng, caps) -> Vec<MockConfig>`**
  - For each `MockParam`, generate `call_count_estimate` return values
  - Primary strategy: `generate_random_value(&mock_param.return_type, rng, caps)`
  - **Declared error types**: If `return_type` is `Result<T,E>`, generate both `Ok(T)` and `Err(E)` variants. If it's a union including Error, generate error variants. If Go-style `(T, error)`, generate `(zero, error)` variants. This happens naturally through type-aware generation — `Result` and union types already have variant generation in `generate_random_value`.
  - Category-aware shaping as fallback when return_type is Unknown
  - For Custom `value_source`: call `prefetch_custom_values` for frontend-generated values

- **`mutate_mock_values(current, mock_params, rate, rng) -> Vec<MockConfig>`**
  - Mutate each MockConfig's return_values using `mutate_value`

- **`crossover_mock_values(a, b, mock_params, rng) -> Vec<MockConfig>`**
  - Crossover return_values between two MockConfig sets

**Loop handling (v1)**: When `call_count_estimate > 1`, each return value in the sequence is independently generated. Pattern-aware variation (e.g., "first succeeds, second fails") deferred to a later phase.

#### 1C. Explorer integration (`explorer.rs`)

- Add `mock_params: Vec<MockParam>` to `ExploreConfig`
- Generate fresh mock values per iteration:
  ```rust
  let mocks = if config.mock_params.is_empty() {
      config.mocks.clone()
  } else {
      generate_mock_values(&config.mock_params, &mut rng, &config.capabilities)
  };
  ```
- Extend `raw_results` to carry mock config: `Vec<(Vec<Value>, Vec<MockConfig>, ExecuteResult)>`

#### 1D. Orchestrator integration (`orchestrator.rs`)

- Add `mock_params: Vec<MockParam>` to orchestrator config
- Extend `WorklistEntry` with `mock_values: Vec<MockConfig>`
- Z3 solves for parameter values; mock values carried forward from parent
- On fuzz/mutate iterations, also mutate mock values

#### 1E. Behavior map integration (`behavior.rs`)

- `BehaviorMap::from_results` accepts the new triple format
- Each `Behavior` stores mock config used in `dependency_trace`
- **Undeclared errors**: When an execution produces a panic/RuntimeException, record it in the behavior map even though we didn't generate it autonomously. This gives visibility into unexpected failure modes.

#### 1F. CLI wiring (`main.rs`)

Build mock_params and pass to both explorer paths (parity contract):
```rust
let mock_params = auto_mock::build_mock_params(&func.dependencies, &auto_mocks);
```

#### 1G. Downstream consumer updates

- `interesting_pool.rs` — `harvest_from_exploration` signature
- `scan_orchestrator.rs` — builds mock_params for each function
- `ObservationOutput` — serialization of the new tuple
- Coverage metrics — mock values included in discovery attribution

**Files to modify:**
- `shatter-core/src/auto_mock.rs` — MockParam, build_mock_params
- `shatter-core/src/input_gen.rs` — generate_mock_values, mutate_mock_values, crossover_mock_values
- `shatter-core/src/explorer.rs` — ExploreConfig.mock_params, per-iteration mock variation, raw_results triple
- `shatter-core/src/orchestrator.rs` — WorklistEntry.mock_values, mock variation on fuzz iterations
- `shatter-core/src/behavior.rs` — from_results accepts triple, stores mock config
- `shatter-core/src/interesting_pool.rs` — raw_results signature update
- `shatter-core/src/scan_orchestrator.rs` — pass mock_params through
- `shatter-cli/src/main.rs` — build mock_params, pass to both explorer paths

**Tests:**
- Unit: `generate_mock_values` produces type-correct values for each TypeInfo variant
- Unit: declared error types (Result, union-with-Error) generate both success and error variants
- Proptest: arbitrary MockParam → generate → validate return types match; mutate preserves types
- E2E: Test function branching on mock return values — verify dynamic mocking discovers all branches
- E2E: Test function with `Result`-returning mock — verify both Ok and Err paths explored

---

### Phase 2: Live-First Execution & Recording Mode

**Goal**: Default to calling real external deps, switching to autonomous mocking on failure. Provide a recording mode for building external dep behavior maps.

#### 2A. Live-first default

Instead of always mocking, the default flow becomes:

1. **First execution**: Don't mock — let the real call go through
2. **Classify the result**:
   - `ECONNREFUSED`, DNS failure, auth error → external dep unavailable, switch to autonomous mocking
   - Success → record the real response as a behavior map entry for this dep
   - Application-level error (e.g., 404) → record it, continue with real calls
3. **Subsequent executions**: If dep is available, keep using it. If not, use autonomous mocks seeded with any recorded real responses.

New enum for how mocks are sourced:
```rust
pub enum MockValueSpace {
    /// Try real calls first, fall back to autonomous if unavailable.
    LiveFirst,
    /// Only user-supplied values. No autonomous generation.
    FixedSet,
    /// Only Shatter-generated values.
    Autonomous,
    /// User values as seeds, Shatter generates additional.
    Seeded,
}
```

Connection failure detection heuristics:
- `ECONNREFUSED` → service not running
- DNS resolution failure → service not reachable
- Auth/401/403 → service running but not configured for test
- Timeout after reasonable period → service unavailable

#### 2B. Recording mode (`--record`)

A special CLI mode that runs against a full integration stack:
- Execute functions with real external deps
- Record ALL external call inputs and outputs
- Build behavior maps for external deps (not just code under test)
- Save as `.shatter/recorded-mocks/` fixtures
- These become high-fidelity seeds for future autonomous runs

This bridges integration testing and autonomous testing — run once against a real stack, then test autonomously forever using recorded data augmented with generated variations.

#### 2C. External dep behavior maps

New type or extension of existing `BehaviorMap`:
```rust
pub struct ExternalDepBehavior {
    pub symbol: String,
    pub source_module: String,
    pub observations: Vec<DepObservation>,
}

pub struct DepObservation {
    pub args: Vec<Value>,
    pub return_value: Option<Value>,
    pub error: Option<String>,
    pub latency_ms: u64,
}
```

These feed back into mock generation: when autonomous mocking kicks in, it uses recorded real responses as seeds alongside type-driven generation.

**Files:**
- `shatter-core/src/auto_mock.rs` — MockValueSpace enum, live-first logic
- `shatter-core/src/explorer.rs` — connection failure detection, fallback to mocking
- `shatter-core/src/behavior.rs` — ExternalDepBehavior type
- `shatter-cli/src/main.rs` — `--record` mode
- `shatter-ts/src/executor.ts` — report connection failures distinctly from app errors

**Depends on**: Phase 1

---

### Phase 3: Execution-Time Gap Detection

**Goal**: Catch external calls that static analysis missed. Applies to ALL external calls (not just network).

**How TS `calls_to_external` works today**: `mockCallFn(moduleName, symbolName, args, returnValue)` is injected into the VM sandbox. The instrumented code calls it for every external call identified at instrumentation time. It records calls in `externalCalls[]` → `result.calls_to_external`. Calls not identified during instrumentation go through silently.

**Approach**: Frontends report calls to non-local modules that bypassed the mock registry.

1. Add `discovered_dependencies: Vec<ExternalDependency>` to `ExecuteResult`
2. TS executor: track calls to imports not in mock registry → report as discovered_dependencies
3. Core: after execution, create new MockParams for discovered deps, add to active set
4. Subsequent iterations mock the newly discovered deps

**strace-based detection** (supplementary, not primary):
- Relevant syscalls are small (~10-15): `connect`, `sendto`, `sendmsg`, `recvfrom`, `socket`, plus `write`/`read` on socket fds
- Useful as a **diagnostic/discovery tool** — run once to catalog all network calls a function makes
- Not practical for runtime interception: Linux-specific, stack trace from syscall back to source function is hard (especially for Node.js/Go), and ptrace has performance overhead
- Better as a one-shot `shatter discover-deps --strace` command that produces a dependency report
- The monkey-patching approach (intercepting `net.Socket.connect`, `net.Dial`, `TcpStream::connect`) is more reliable for actual mocking

**Subprocess detection**: Match known subprocess-spawning APIs at analysis time (`child_process.exec`, `os/exec.Command`, `std::process::Command`). At execution time, intercept and mock with plausible stdout/stderr/exit code combinations. Flag as "hard to mock" if deeply nested.

**Files:**
- `shatter-core/src/auto_mock.rs` — detect_unmocked_deps
- `shatter-core/src/protocol.rs` — discovered_dependencies on ExecuteResult
- `shatter-core/src/explorer.rs` — after execution, extend mock_params dynamically
- `shatter-ts/src/executor.ts` — report unmocked external calls

**Depends on**: Phase 1

---

### Phase 4: Error Case Generation

**Goal**: Exercise error-handling paths by injecting failures from external deps.

**Type-driven error generation** (not library-specific):
- Return type has `status` field → generate 400, 404, 500, 503 values
- Return type is `Result<T,E>` or union with Error → generate error variants (this is already covered in Phase 1 via type-driven generation, but Phase 4 adds *probability control* and *error templates*)
- Return type is nullable → generate null
- Fallback → `MockBehavior::ThrowError` with generic error message

Category-aware error shapes as fallback for Unknown return types:
- Network: `{status: 500, data: null}`, timeout error
- Database: `{rows: [], error: "connection refused"}`
- FileSystem: ENOENT, EACCES errors
- Subprocess: non-zero exit codes, stderr output

Configurable error injection probability (~15% default).

TS executor: handle `throw_error` behavior in mock registry:
```typescript
if (mock.default_behavior === "throw_error") {
    mockRegistry[mock.symbol] = () => { throw new Error(mock.return_values[0]); };
}
```

**Files:**
- `shatter-core/src/input_gen.rs` — error probability, error templates
- `shatter-ts/src/executor.ts` — throw_error handling

**Depends on**: Phase 1. Parallel with Phases 2/3.

---

### Phase 5: User-Supplied Mock Fixtures

**Goal**: Let users provide canned return values with configurable value space policy and scoping.

#### Value space policy:

```rust
pub enum MockValueSpace {
    /// Try real calls first, fall back to autonomous if unavailable.
    LiveFirst,
    /// Only user-supplied values. No autonomous generation.
    FixedSet,
    /// Only Shatter-generated values.
    Autonomous,
    /// User values as seeds, Shatter generates additional.
    Seeded,
}
```

Default: `Seeded` (when user provides values) or `LiveFirst` (when no user values, per Phase 2).

#### Scoping (global + file + function):

```yaml
mocks:
  "stripe.charges.create":
    scope: "src/payments/checkout.ts::processPayment"  # file::function
    values:
      - { id: "ch_1", status: "succeeded", amount: 2000 }
      - { id: "ch_2", status: "failed", amount: 0 }
    value_space: seeded

  "db.query":
    scope: "src/db/users.ts"  # file-level
    values:
      - { rows: [] }
      - { rows: [{ id: 1 }] }
    value_space: fixed_set

  "logger.info":
    # no scope = global
    value_space: autonomous
```

Call-ordinal scoping within a function deferred to later.

#### Expectations:

```yaml
mocks:
  "stripe.charges.create":
    values:
      - { id: "ch_1", status: "succeeded" }
    expectations:
      - called_with: [{ amount: { gte: 100 } }]
      - call_count: { min: 1, max: 3 }
```

Expectations validated after execution, reported in behavior map.

#### Type validation at load time:

```rust
pub fn validate_mock_types(
    overrides: &HashMap<String, MockOverride>,
    deps: &[ExternalDependency],
) -> Vec<MockTypeError>
```

**Files:**
- `shatter-core/src/auto_mock.rs` — MockValueSpace, MockOverride with scope/expectations, validate_mock_types
- `shatter-core/src/input_gen.rs` — respect MockValueSpace
- `shatter-core/src/config.rs` — parse mock fixtures
- `shatter-cli/src/main.rs` — validation at startup

**Depends on**: Phase 1. Independent of 2/3/4.

---

### Phase 6: Refactoring Recommendations & Hard-to-Mock Detection

**Goal**: When Shatter encounters calls it can't effectively mock, suggest code changes.

Detection criteria:
- Dynamic dispatch / computed property access
- Calls inside closures passed to external code
- Subprocess spawning with complex argument construction
- Multiple layers of indirection

Output in scan results:
```
Refactoring Recommendations:
  src/services/payment.ts::processOrder (line 42):
    Call to `externalService.process()` is through dynamic dispatch.
    Suggestion: Extract to a named function parameter for dependency injection.

  src/utils/shell.ts::runMigration (line 18):
    Subprocess call `exec("psql ...")` cannot be mocked.
    Suggestion: Wrap in a function that can be replaced in test configuration.
```

**Files:**
- `shatter-core/src/auto_mock.rs` or new `mock_analysis.rs`
- `shatter-core/src/report.rs`

**Depends on**: Phase 3 (runtime detection reveals what couldn't be mocked).

---

### Phase 7: Frontend Parity (Go)

Go frontend already has `generateMockFile` in `executor.go` (code generation approach, writes Go source with mock functions). Changes:
- Per-execution mock variation (already receives mocks per Execute call — no architectural change)
- ThrowError → `panic()` or error return type
- Report discovered_dependencies for unmocked calls
- Custom generator integration for mock values

Rust frontend deferred until execute is implemented.

**Depends on**: Phases 1 + 4.

---

## Sequencing

```
Phase 1 (MVP: Dynamic Generation + Error Types)
  ├── Phase 2 (live-first + recording mode)
  ├── Phase 3 (runtime gap detection)        ← parallel with 2
  ├── Phase 4 (error case injection)         ← parallel with 2/3
  ├── Phase 5 (user fixtures)                ← independent
  ├── Phase 6 (refactoring recs)             ← after 3
  └── Phase 7 (Go parity)                    ← after 1+4
```

Phase 1 is ~50% of the work. Phase 2 (live-first) is the next most impactful.

## Resolved Decisions

- **Loop handling v1**: Independent variation per call. Pattern-aware variation deferred.
- **Mock scoping v1**: Global + file + function. Call-ordinal deferred.
- **Plan storage**: Save to `docs/plans/`.
- **Error types**: Declared error types explored in Phase 1. Undeclared errors recorded but not generated.
- **strace**: Useful as diagnostic tool (`shatter discover-deps --strace`), not for runtime interception.
- **Live-first default**: Try real calls, fall back to autonomous mocking on connection failure.

## Open Questions

1. **Custom generators for mocks**: Reuse `generate` command with a flag, or add `generate_mock` command?
2. **Recording mode UX**: How does the user specify the integration stack is available? Environment variable? CLI flag?
3. **Subprocess mocking depth**: Mock exec (stdout/stderr/exit code) or try to understand the subprocess?
4. **Pattern-aware loop variation**: When to add "first succeeds, second fails" patterns?

## Verification

1. **Unit**: generate_mock_values type correctness, error variant generation
2. **Proptest**: arbitrary MockParam → generate → validate return types; mutate preserves types
3. **E2E**: Test function branching on mock return values — dynamic mocking discovers all branches
4. **E2E**: Test function with Result-returning mock — both Ok and Err paths explored
5. **E2E loop**: Test function calling mock in a loop — different return values per iteration
6. **Walkthrough**: Run on `12-external-deps.ts` and `17-external-deps-npm.ts`
