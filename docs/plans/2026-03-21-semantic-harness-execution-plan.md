# Semantic Harness Execution Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the legacy temp-project harness flow for Rust and Go with a semantic harness architecture that:

- passes execute requests over stdin
- compiles in the real crate/package context whenever possible
- uses lifecycle-specific storage instead of temp-dir reuse
- accepts one stable in-tree generated harness file per language
- fully replaces the legacy harness path rather than keeping it as the normal path

**Architecture summary:**

- **Rust:** one stable generated harness entrypoint inside the user's crate, built as a native bin target, request-driven over stdin, dispatching across supported functions.
- **Go:** one stable dispatch harness preserving package semantics, with tool-owned generated/build state outside the project tree.
- **Storage split:** reusable cache/build state in tool-owned cache, per-run scratch output in temp-scoped storage, preserved outputs in `shatter-artifacts/`.
- **Staleness split:** Shatter tracks instrumentation/generation staleness; Cargo and `go build` own compilation staleness.
- **Legacy replacement:** the old temp-project harness implementation is removed as the primary path. Standalone files remain a separate explicit fallback because they have no parent crate/module context.

**Tech stack:** Rust (`shatter-rust`), Go (`shatter-go`), serde_json, encoding/json

**Context documents:**

- `docs/plans/2026-03-21-harness-compilation-revised-plan.md`
- `docs/plans/2026-03-21-replicated-noodling-axolotl-review.md`

**Related issue:** `str-y3po` (split `.shatter` lifecycles)

**Design constraints:**

- Do not lock in temp-dir reuse as cache strategy.
- Do not keep the current temp-project Rust harness as the long-term path.
- One stable in-tree generated file per language is acceptable.
- Preserve semantic correctness over minimizing code generation.

---

## File Map

| File | Action | Responsibility |
|---|---|---|
| `shatter-rust/src/executor.rs` | Modify | Remove legacy temp-project execution flow; integrate semantic harness request/build/run flow |
| `shatter-rust/src/handler.rs` | Modify | Route execute requests through the new semantic harness manager |
| `shatter-rust/src/harness.rs` | New | Rust semantic harness manager: project detection, manifest integration, dispatch generation, instrumentation staleness |
| `shatter-rust/src/storage.rs` | New | Lifecycle-specific storage paths for cache/temp/artifacts |
| `shatter-go/instrument/executor.go` | Modify | Replace legacy temp-dir harness flow with semantic package-aware harness flow |
| `shatter-go/instrument/harness.go` | New | Go semantic harness manager: module detection, generated harness build/run, instrumentation staleness |
| `shatter-go/instrument/storage.go` | New | Lifecycle-specific storage paths for Go |
| `shatter-rust/src/executor.rs` tests | Modify | Replace legacy harness assertions with stdin + semantic harness tests |
| `shatter-go/instrument/executor_test.go` | Modify | Replace legacy harness assertions with stdin + semantic harness tests |
| `shatter-rust/Cargo.toml` or target user manifests | Consult only | No repo edit here unless Shatter’s own crates need new dependencies |
| `docs/` / CLI docs | Modify later | Document generated harness file, cleanup, and storage locations |

**Generated runtime files in user projects:**

- Rust: one stable file such as `src/bin/__shatter_exec.rs`
- Go: one stable harness entrypoint appropriate to package/module layout
- Preserved outputs: `shatter-artifacts/`

**Tool-owned generated state outside the project tree:**

- project-scoped cache root via lifecycle-specific storage helpers
- generated metadata
- reusable build inputs where externalization is required

---

## Storage Model

This plan requires lifecycle-specific storage from the start.

### Required locations

- [ ] **Tool-owned cache/build state**
  - deterministic per-project location
  - survives sessions
  - safe to delete and regenerate

- [ ] **Disposable temp output**
  - per-execute scratch files only
  - never used as durable cache

- [ ] **Preserved outputs**
  - project-visible `shatter-artifacts/`
  - behavior maps and other user-facing analysis outputs

### Rules

- [ ] Do not keep reusable harness binaries or generated metadata in `/tmp`.
- [ ] Do not mix behavior maps or preserved traces with tool-owned cache.
- [ ] Do not create a second `Cargo.toml` in the project tree.
- [ ] Do not reintroduce a mixed-purpose `.shatter/` working directory.

### Prepared harness lifecycle

The `prepare` command pre-builds a harness binary so subsequent `execute` calls
skip compilation. Prepared state is scoped by lifecycle level:

- **Function-level**: `prepared_harnesses` map keyed by `prepare_id` (SHA-256
  of `file:function:sorted-mock-symbols`, first 16 hex chars). Cleared on
  function-level teardown.
- **Session-level**: all prepared harnesses cleared on shutdown.

This is distinct from persistent subprocesses (deferred — see below). Prepare
caches the *compiled binary*, not a running process.

### Persistent subprocess (deferred)

Persistent subprocesses (keeping one harness process alive per project to avoid
process spawn overhead) are **not yet implemented**. The current architecture
uses single-shot harness execution per request. This optimization is deferred
until profiling data from the compile-once/run-many architecture justifies the
added complexity.

---

## Task 1: Add Lifecycle-Specific Storage Helpers

**Goal:** establish explicit storage APIs before changing executor behavior, so later tasks do not accidentally reuse temp directories as cache.

**Files:**

- New: `shatter-rust/src/storage.rs`
- New: `shatter-go/instrument/storage.go`
- Modify: `shatter-rust/src/lib.rs` or module declarations as needed
- Modify: `shatter-go/instrument/...` package declarations as needed

- [ ] **Step 1: Define storage responsibilities in code**

Implement helpers that return:

- cache root for a project
- temp root for one execute request
- preserved artifacts root (`shatter-artifacts/`) in the project

Rust API sketch:

```rust
pub struct StoragePaths {
    pub cache_root: PathBuf,
    pub temp_root: PathBuf,
    pub artifacts_root: PathBuf,
}

pub fn storage_paths(project_root: &Path, request_id: &str) -> io::Result<StoragePaths>;
```

Go API sketch:

```go
type StoragePaths struct {
    CacheRoot     string
    TempRoot      string
    ArtifactsRoot string
}

func StoragePathsFor(projectRoot string, requestID string) (StoragePaths, error)
```

- [ ] **Step 2: Write tests for path behavior**

Add tests that verify:

- cache path is deterministic for the same project
- temp path changes per request
- artifacts path is exactly `<projectRoot>/shatter-artifacts`
- no path points to `.shatter/`

- [ ] **Step 3: Run tests to verify failures before implementation**

Run focused unit tests for the new modules. Expected: fail until helpers exist.

- [ ] **Step 4: Implement path helpers**

Requirements:

- cache root should be outside the project tree
- temp root should be under system temp
- artifacts root should be in the project root

- [ ] **Step 5: Verify tests pass**

- [ ] **Step 6: Commit**

```
git add shatter-rust/src/storage.rs shatter-go/instrument/storage.go
git commit -m "feat(harness): add lifecycle-specific storage helpers"
```

---

## Task 2: Rust -- Introduce Semantic Harness Manager

**Goal:** replace ad hoc temp-project logic with a manager that builds and runs a stable in-tree harness in the user's real crate context.

**Files:**

- New: `shatter-rust/src/harness.rs`
- Modify: `shatter-rust/src/executor.rs`
- Modify: `shatter-rust/src/handler.rs`

- [ ] **Step 1: Add tests for project-root detection and harness file location**

Test cases:

- source file inside a crate with `Cargo.toml` resolves to that crate root
- stable harness path resolves to `src/bin/__shatter_exec.rs` (or the chosen final path)
- standalone files return a standalone mode result rather than crate mode

- [ ] **Step 2: Add tests for manifest integration requirements**

Define expected behavior:

- harness manager can detect whether manifest integration already exists
- integration is O(1), clearly marked, and idempotent
- no second project-local manifest is created

These tests can initially validate generated manifest fragments or detection logic.

- [ ] **Step 3: Implement crate-mode harness manager skeleton**

Responsibilities:

- detect project root
- determine stable harness file path
- determine whether crate-mode is available
- expose methods for:
  - `ensure_manifest_integration()`
  - `ensure_dispatch_source(...)`
  - `build_if_needed(...)`
  - `run_once(...)`

Rust sketch:

```rust
pub struct HarnessManager {
    project_root: PathBuf,
    harness_path: PathBuf,
    storage: StoragePaths,
}
```

- [ ] **Step 4: Implement atomic manifest integration**

Requirements:

- one-time or idempotent
- clearly marked autogenerated section
- atomic write via temp file + rename
- safe if already present

If a stable native bin target requires `Cargo.toml` integration, this task owns it.

- [ ] **Step 5: Verify tests pass**

- [ ] **Step 6: Commit**

```
git add shatter-rust/src/harness.rs shatter-rust/src/executor.rs shatter-rust/src/handler.rs
git commit -m "feat(rust): add semantic harness manager for crate-mode execution"
```

---

## Task 3: Rust -- Replace Embedded Inputs with Request-Driven Harness Dispatch

**Goal:** the Rust harness must read stdin requests and dispatch dynamically rather than baking inputs or generating per-function binaries.

**Files:**

- Modify: `shatter-rust/src/harness.rs`
- Modify: `shatter-rust/src/executor.rs`
- Modify tests in `shatter-rust/src/executor.rs`

- [ ] **Step 1: Update or add tests for generated harness source**

Assertions should cover:

- harness reads newline-delimited JSON from stdin
- harness expects `function`, `inputs`, and `mocks`
- harness does not embed JSON literals for inputs
- harness contains dispatch structure rather than a single hardcoded call

- [ ] **Step 2: Implement request schema generation**

The generated harness should consume requests like:

```json
{"function":"auth::validate::check","inputs":[...],"mocks":[...]}
```

- [ ] **Step 3: Implement generated dispatch source**

The generated stable harness file should:

- deserialize one request
- locate the target function in dispatch
- perform generated per-function input decoding
- call the target function
- serialize return value, errors, side effects, and performance

Do not create one file per function.

- [ ] **Step 4: Remove `inputs_json` / `mocks_json` baked-source flow**

`generate_harness()` should either be replaced entirely or transformed into dispatch-source generation owned by the harness manager.

- [ ] **Step 5: Add integration test for repeated executes**

Test:

- call the same function twice with different inputs
- verify output differs correctly
- verify no source regeneration is required between requests when instrumentation is unchanged

- [ ] **Step 6: Verify tests pass**

- [ ] **Step 7: Commit**

```
git add shatter-rust/src/harness.rs shatter-rust/src/executor.rs
git commit -m "feat(rust): replace baked harness inputs with stdin-driven dispatch"
```

---

## Task 4: Rust -- Replace Legacy Temp-Project Build/Run Flow Entirely

**Goal:** remove the current temp-project harness as the normal Rust execution path.

**Files:**

- Modify: `shatter-rust/src/executor.rs`
- Modify: `shatter-rust/src/handler.rs`
- Modify tests in `shatter-rust/src/executor.rs`

- [ ] **Step 1: Add failing test that execute uses crate-mode harness for crate files**

For a fixture crate:

- execute one function
- assert the stable harness file exists in the crate
- assert legacy temp-project layout is not used

- [ ] **Step 2: Rewire execute path**

Replace legacy flow:

- generate temp Cargo project
- write temp `Cargo.toml`
- compile in temp dir
- remove temp dir

with:

- detect crate mode
- ensure manifest integration
- ensure stable harness file exists and is current
- build via Cargo in real crate context
- execute harness with request over stdin

- [ ] **Step 3: Restrict standalone fallback to standalone files only**

If no parent crate exists:

- use standalone fallback

If a parent crate exists:

- do not use the legacy temp-project path

- [ ] **Step 4: Remove legacy helper code**

Delete or deprecate:

- temp `Cargo.toml` generation for crate-mode
- temp harness main generation for crate-mode
- crate-mode temp binary path logic

- [ ] **Step 5: Run Rust tests**

Run focused executor tests, then full `cargo test` for `shatter-rust`.

- [ ] **Step 6: Commit**

```
git add shatter-rust/src/executor.rs shatter-rust/src/handler.rs
git commit -m "refactor(rust): replace legacy temp-project harness with crate-mode execution"
```

---

## Task 5: Rust -- Instrumentation Staleness and Build Delegation

**Goal:** track only Shatter-specific invalidation, not full build invalidation.

**Files:**

- Modify: `shatter-rust/src/harness.rs`
- Modify: `shatter-rust/src/executor.rs`
- Tests in Rust harness/executor modules

- [ ] **Step 1: Add tests for instrumentation staleness**

Scenarios:

- unchanged source file skips reinstrumentation
- changed source file regenerates dispatch source
- changed generator version regenerates dispatch source

- [ ] **Step 2: Implement instrumentation metadata in tool-owned cache**

Store only:

- source hash
- generator version
- function/source registration metadata as needed

Do not store “binary is fresh” state separate from Cargo.

- [ ] **Step 3: Ensure build always delegates to Cargo**

Behavior:

- if instrumentation unchanged and dispatch unchanged, build step may still call Cargo
- Cargo decides whether work is needed

- [ ] **Step 4: Add test for dependency-change-safe behavior**

This may be a design-level test or a regression test that ensures the code path does not bypass Cargo simply because Shatter metadata is unchanged.

- [ ] **Step 5: Commit**

```
git add shatter-rust/src/harness.rs shatter-rust/src/executor.rs
git commit -m "feat(rust): track instrumentation staleness separately from build staleness"
```

---

## Task 6: Go -- Introduce Semantic Harness Manager and Storage Integration

**Goal:** replace the current temp-dir harness flow with a package-aware harness manager using lifecycle-specific storage.

**Files:**

- New: `shatter-go/instrument/harness.go`
- Modify: `shatter-go/instrument/executor.go`
- Modify: `shatter-go/instrument/executor_test.go`

- [ ] **Step 1: Add tests for module-root detection and storage use**

Scenarios:

- Go source under a module resolves module root
- cache/build state is outside the project tree
- artifacts root is `shatter-artifacts/`

- [ ] **Step 2: Implement Go harness manager skeleton**

Responsibilities:

- detect module root
- determine generated harness location
- determine tool-owned cache/build location
- expose:
  - `EnsureHarnessSource(...)`
  - `BuildIfNeeded(...)`
  - `RunOnce(...)`

- [ ] **Step 3: Preserve package semantics**

Requirements:

- no `cmd/` + `modules/` split that loses package-private access
- no recorder placement across package boundaries
- module configuration resolves correctly

If external cache generation is used, keep the generated package layout semantically faithful.

- [ ] **Step 4: Add tests for unexported-helper visibility**

Fixture should prove the harness can execute a function that depends on package-private helpers defined in sibling files.

- [ ] **Step 5: Commit**

```
git add shatter-go/instrument/harness.go shatter-go/instrument/executor.go
git commit -m "feat(go): add semantic harness manager with lifecycle-specific storage"
```

---

## Task 7: Go -- Replace Baked Inputs with Stdin-Driven Dispatch

**Goal:** make Go harness execution request-driven and reusable across input changes.

**Files:**

- Modify: `shatter-go/instrument/harness.go`
- Modify: `shatter-go/instrument/executor.go`
- Modify: `shatter-go/instrument/executor_test.go`

- [ ] **Step 1: Update generated-harness tests**

Assertions:

- generated harness reads stdin
- request contains `function`, `inputs`, and mocks if applicable
- generated source does not embed input literals
- dispatch exists for multiple functions

- [ ] **Step 2: Implement stdin request parsing**

The Go harness should read a request object from stdin and dispatch accordingly.

- [ ] **Step 3: Implement dynamic dispatch source generation**

Like Rust, this should be one stable harness entrypoint, not one generated file per function.

- [ ] **Step 4: Add repeated-execute integration test**

Call the same function twice with different inputs and verify no regeneration is needed when source is unchanged.

- [ ] **Step 5: Commit**

```
git add shatter-go/instrument/harness.go shatter-go/instrument/executor.go shatter-go/instrument/executor_test.go
git commit -m "feat(go): replace baked harness inputs with stdin-driven dispatch"
```

---

## Task 8: Go -- Replace Legacy Temp Harness Path Entirely

**Goal:** remove the current “instrument temp dir, build, run, delete” flow as the normal Go execution path.

**Files:**

- Modify: `shatter-go/instrument/executor.go`
- Modify tests in `shatter-go/instrument/executor_test.go`

- [ ] **Step 1: Add failing test that module-mode execute does not use legacy temp-dir flow**

For a fixture module:

- execute a function
- assert the new storage/cache locations are used
- assert legacy delete-on-exit behavior is gone for reusable state

- [ ] **Step 2: Rewire execute path to new harness manager**

Replace:

- temp output dir instrumentation
- flat one-shot harness generation
- one-shot build and immediate delete

with:

- semantic harness manager
- lifecycle-specific storage
- stdin-driven request execution

- [ ] **Step 3: Keep standalone fallback explicit**

Only use standalone fallback when there is no parent module.

- [ ] **Step 4: Remove legacy temp-flow helper code**

- [ ] **Step 5: Run Go tests**

Run focused executor tests, then full `go test ./...` in `shatter-go`.

- [ ] **Step 6: Commit**

```
git add shatter-go/instrument/executor.go shatter-go/instrument/executor_test.go
git commit -m "refactor(go): replace legacy temp harness with semantic package-aware execution"
```

---

## Task 9: Preserved Outputs and Behavior Maps

**Goal:** ensure durable analysis outputs land in `shatter-artifacts/`, not tool-owned cache or temp.

**Files:**

- Modify Rust and Go code paths that emit preserved analysis outputs
- Add tests in relevant modules

- [ ] **Step 1: Add tests for artifact location**

Assertions:

- behavior maps are written under `<projectRoot>/shatter-artifacts`
- cache/temp directories do not receive preserved outputs

- [ ] **Step 2: Rewire preserved output paths**

Any durable or user-facing analysis output should use the artifacts root from storage helpers.

- [ ] **Step 3: Add cleanup behavior tests**

Cleanup should not delete preserved artifacts by default when clearing tool-owned cache.

- [ ] **Step 4: Commit**

```
git add ...
git commit -m "feat(harness): write preserved outputs to shatter-artifacts"
```

---

## Task 10: Cleanup, Orphan Recovery, and Documentation

**Goal:** complete operational behavior for the new architecture.

**Files:**

- Modify Rust/Go harness managers
- Modify docs as needed

- [ ] **Step 1: Add orphan-handling tests**

Scenarios:

- source file removed after registration
- generated harness file missing
- cache metadata missing

The system should recover cleanly by regenerating or pruning.

- [ ] **Step 2: Implement cleanup entrypoints**

Support:

- clearing tool-owned cache for a project
- removing stable generated harness files if requested
- leaving `shatter-artifacts/` intact by default

- [ ] **Step 3: Document the new architecture**

Document:

- generated stable harness file location
- cache/temp/artifact locations
- cleanup semantics
- standalone fallback behavior

- [ ] **Step 4: Commit**

```
git add ...
git commit -m "docs(harness): document semantic harness storage and cleanup behavior"
```

---

## Execution Notes

### Rust-specific requirements

- [ ] `bin_only` mode (default): harness compiles as a `[[bin]]` target and can
  access `pub` items. Functions with `pub(crate)` signature types fail with a
  clear error and fall back to standalone.
- [ ] `crate_bridge` mode (opt-in): harness compiles inside the library crate
  and can access `pub(crate)` items.
- [ ] No import rewriting should be required for crate-mode execution.
- [ ] No second `Cargo.toml` should appear in the user project.
- [ ] Stable harness generation must be idempotent.

### Go-specific requirements

- [ ] Do not split generated code in a way that breaks package-private access.
- [ ] Module-relative configuration must still resolve correctly.
- [ ] Reusable build state must live in tool-owned cache, not temp.

### Standalone file policy

- [ ] Standalone files (no parent `Cargo.toml` or `go.mod`) are the only
  accepted non-semantic fallback.
- [ ] Standalone fallback is also used when `bin_only` mode encounters a build
  failure for crate-resident files.
- [ ] Standalone projects build in XDG cache (`$XDG_CACHE_HOME/shatter/standalone-{lang}/`).
- [ ] Their generated projects should still use lifecycle-specific storage.
- [ ] They must not reintroduce temp-dir reuse as durable cache.

---

## Verification Matrix

### Performance

- [ ] Walkthrough step 10 completes within budget.
- [ ] Repeated executes with different inputs do not trigger recompilation.
- [ ] Warm project rebuilds benefit from native incremental compilation.

### Semantic correctness

- [ ] Rust functions with `pub` signature types execute successfully in `bin_only` mode.
- [ ] Rust functions with `pub(crate)` signature types execute successfully in `crate_bridge` mode, or fail with a clear error and standalone fallback in `bin_only` mode.
- [ ] Rust functions using `crate::`, `self::`, and `super::` structures work without import rewriting.
- [ ] Go functions depending on unexported helpers in sibling files work.
- [ ] Package/module init behavior remains correct.

### Storage correctness

- [ ] Reusable state does not live in temp directories.
- [ ] Preserved outputs live under `shatter-artifacts/`.
- [ ] Tool-owned cache/build state is outside the project tree where appropriate.

### Replacement correctness

- [ ] Crate/module projects no longer use the legacy temp-project harness path.
- [ ] Standalone fallback is only used for true standalone files.
- [ ] Legacy crate/module helper code is removed or unreachable.

### Cleanup and recovery

- [ ] Missing generated harness files recover cleanly.
- [ ] Missing cache metadata recovers cleanly.
- [ ] Cleanup does not delete preserved artifacts by default.

---

## Recommended Issue Breakdown

The next step after this document is to split execution into small issues. The clean breakdown is:

1. lifecycle-specific storage helpers
2. Rust semantic harness manager skeleton
3. Rust stdin-driven dispatch source
4. Rust legacy-path replacement
5. Rust instrumentation-vs-build staleness split
6. Go semantic harness manager skeleton
7. Go stdin-driven dispatch source
8. Go legacy-path replacement
9. preserved outputs to `shatter-artifacts`
10. cleanup/orphan recovery/docs

Each of those should become one or more focused issues with acceptance criteria and tests.
