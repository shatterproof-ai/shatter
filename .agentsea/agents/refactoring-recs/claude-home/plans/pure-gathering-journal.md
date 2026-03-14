# Multi-Level Setup/Teardown — Issue Breakdown

## Context

Users need in-process setup and teardown for tests executed by Shatter — initializing databases, authentication keys, external services, etc. The current system supports two levels (`per_function` and `per_execution`) but lacks session-level and file-level setup, convention-based discovery, proper error propagation, separate timeouts, and implementations in Go/Rust frontends. The orchestrator path (`orchestrator.rs`) also completely ignores setup, violating the parallel-path parity rule.

No backward compatibility constraints — we can redesign the protocol cleanly.

Design reference: see the full implementation plan in the user's message for protocol types, config schema, SetupManager design, error propagation rules, timeout defaults, CLI flags, convention patterns, and user-facing examples.

---

## Epic

**Title:** Multi-level setup/teardown
**Type:** epic
**Priority:** 1
**Labels:** core, protocol, setup
**Description:** Four-level setup/teardown lifecycle (session → file → function → execution) with convention-based discovery, per-level timeouts, error propagation, and implementations in all frontends. Replaces the current two-level SetupMode system.
**Gate:** `--waits-for-gate all-children`

---

## Issues

### 1. SetupLevel protocol types

**Type:** feature | **Priority:** 1 | **Labels:** core, protocol
**Parent:** epic
**Files:** `shatter-core/src/protocol.rs`, `shatter-core/src/test_arbitraries.rs`
**Description:**
Replace `SetupMode` with `SetupLevel` enum (session/file/function/execution). Add `SetupContextStack` type. Update Setup command: rename `function` → `scope`, replace `mode` → `level`, add `parent_context: Option<SetupContextStack>`. Update Teardown: rename `function` → `scope`, replace `mode` → `level`. Change Execute `setup_context` from `Option<Value>` to `Option<SetupContextStack>`. Update test_arbitraries: replace `arb_setup_mode()` with `arb_setup_level()`, add `arb_setup_context_stack()`. Include proptest roundtrips for all new types.
**Acceptance:** `cargo test -p shatter-core` passes, `cargo clippy -p shatter-core -- -D warnings` clean.
**Blocked by:** nothing

### 2. Config schema for multi-level setup

**Type:** feature | **Priority:** 1 | **Labels:** core, config
**Parent:** epic
**Files:** `shatter-core/src/config.rs`
**Description:**
Replace `SetupMode` with `SetupLevel` throughout config types. Add `SessionSetupConfig { file: String, timeout: Option<u64> }`. Add `session_setup: Option<SessionSetupConfig>` and `file_setup: Option<HashMap<String, String>>` (glob → setup file path) to `ShatterConfig`. Add `setup_timeout: Option<u64>` to `DefaultsConfig` and `FunctionConfig`. Update `ResolvedFunctionConfig`: rename `setup_mode` → `setup_level`, add `setup_timeout: u64`. Add config deserialization tests for new fields.
**Acceptance:** Config round-trips with new fields. `cargo test -p shatter-core` passes.
**Blocked by:** #1 (uses SetupLevel type)

### 3. Convention-based setup discovery

**Type:** feature | **Priority:** 2 | **Labels:** core, config
**Parent:** epic
**Files:** `shatter-core/src/config.rs`
**Description:**
Add `discover_setup_files(project_root, language_ext)` function. Conventions: session-level checks `shatter.setup.{ext}` in project root and `.shatter/setup.{ext}`; file-level checks `<stem>.shatter.setup.{ext}` co-located with source file. Config-based setup takes precedence over convention-discovered files. Unit tests with temp directories verifying discovery for each level and each language extension.
**Acceptance:** Discovery finds convention files, config overrides convention, tests pass.
**Blocked by:** #2 (integrates with config resolution)

### 4. SetupManager module

**Type:** feature | **Priority:** 1 | **Labels:** core, setup
**Parent:** epic
**Files:** `shatter-core/src/setup_manager.rs` (new), `shatter-core/src/lib.rs`
**Description:**
New module centralizing setup lifecycle. `SetupManager` struct with: session/file/function context caches, `SetupFailures` tracker, `SetupTimeouts` with per-level defaults (120s/30s/15s/5s), `fail_on_error` flag. Methods: `setup_session()`/`teardown_session()`, `setup_file()`/`teardown_file()`, `setup_function()`/`teardown_function()`, `setup_execution()`/`teardown_execution()`, `context_stack(file, fn)` → `SetupContextStack`, `should_skip_file()`/`should_skip_function()`. All setup calls wrapped with `tokio::time::timeout()`. Supports `SHATTER_SETUP_TIMEOUT` env var as global ceiling. Unit tests for lifecycle ordering, context merging, failure propagation, timeout behavior. Proptest for arbitrary setup/teardown sequences maintaining valid state.
**Acceptance:** `cargo test -p shatter-core` passes including SetupManager unit + proptest. Clippy clean.
**Blocked by:** #1 (uses SetupLevel, SetupContextStack), #2 (uses config types)

### 5. Explorer SetupManager integration

**Type:** feature | **Priority:** 1 | **Labels:** core, explorer
**Parent:** epic
**Files:** `shatter-core/src/explorer.rs`
**Description:**
Remove `send_setup`/`send_teardown` helper functions and inline setup lifecycle from explorer. Replace `setup_file`/`setup_mode` in `ExploreConfig` with `SetupManager` reference. Use `SetupManager::context_stack()` when building Execute commands instead of flat `setup_context`. Call `setup_function()`/`teardown_function()` per function and `setup_execution()`/`teardown_execution()` per execution in the explore loop. Report setup failures in exploration output with skip reason.
**Acceptance:** `cargo test -p shatter-core` passes. Explorer uses SetupManager for all setup lifecycle.
**Blocked by:** #4 (SetupManager)

### 6. Orchestrator setup parity

**Type:** feature | **Priority:** 1 | **Labels:** core, orchestrator
**Parent:** epic
**Files:** `shatter-core/src/orchestrator.rs`
**Description:**
Add setup support to orchestrator path (currently sends `setup_context: None`). Use `SetupManager` for function/execution setup, mirroring explorer's integration. Replace `setup_context: None` in Execute commands with `context_stack()`. This fixes the parallel-path parity violation where orchestrator ignores setup entirely.
**Acceptance:** `cargo test -p shatter-core` passes. Orchestrator and explorer produce identical setup behavior (parity test).
**Blocked by:** #4 (SetupManager), #5 (follow explorer's pattern)

### 7. Scan orchestrator setup lifecycle

**Type:** feature | **Priority:** 2 | **Labels:** core, scan
**Parent:** epic
**Files:** `shatter-core/src/scan_orchestrator.rs`
**Description:**
Wire session and file setup lifecycle into scan orchestrator. Call `SetupManager::setup_session()` before scan starts, `setup_file()` when entering each source file, `teardown_file()` when leaving, and `teardown_session()` at end. Teardowns run in reverse order. If session setup fails and `fail_on_error` is set, abort scan immediately. Otherwise skip all functions and continue to next file.
**Acceptance:** `cargo test -p shatter-core` passes. Scan orchestrator handles session+file setup lifecycle.
**Blocked by:** #4 (SetupManager)

### 8. CLI setup flags and wiring

**Type:** feature | **Priority:** 1 | **Labels:** cli
**Parent:** epic
**Files:** `shatter-cli/src/main.rs`
**Description:**
Add `--setup-timeout <seconds>` (global ceiling) and `--fail-on-error` (abort on any setup failure, default: skip + continue) flags to explore and scan commands. Set `SHATTER_SETUP_TIMEOUT` env var before spawning frontends. Resolve setup files from config + convention discovery. Wire `SetupManager` construction into both explore and scan paths (both explorer and orchestrator code paths in `run_explore`).
**Acceptance:** CLI accepts new flags, passes them through to SetupManager. `cargo test` passes. `cargo clippy -- -D warnings` clean.
**Blocked by:** #3 (convention discovery), #4 (SetupManager)

### 9. TS frontend multi-level setup

**Type:** feature | **Priority:** 1 | **Labels:** typescript, frontend
**Parent:** epic
**Files:** `shatter-ts/src/protocol.ts`, `shatter-ts/src/handlers.ts`, `shatter-ts/src/setup-loader.ts`, `shatter-ts/src/executor.ts`
**Description:**
Update TS protocol types: replace `SetupMode` with `SetupLevel`, add `SetupContextStack` interface, update Setup/Teardown/Execute request types. Update handlers: handle `level` field and `parent_context`, maintain separate context caches per level, pass parent contexts to setup functions. Update setup-loader: `runSetup()` accepts `SetupLevel` and `parentContext`, setup function signature is `setup(scope, parentContext?)`, support async setup/teardown. Read `SHATTER_SETUP_TIMEOUT` env var for setup-specific timeout. Protocol roundtrip tests + handler tests for each level.
**Acceptance:** `cd shatter-ts && npm test` passes. Protocol roundtrips work for all new types.
**Blocked by:** #1 (wire format defined by core protocol types)

### 10. Go frontend multi-level setup

**Type:** feature | **Priority:** 2 | **Labels:** go, frontend
**Parent:** epic
**Files:** `shatter-go/protocol/types.go`, `shatter-go/protocol/handler.go`, `shatter-go/setup/loader.go` (new)
**Description:**
Mirror protocol changes in Go types. Replace setup/teardown stubs with real implementations. Setup files loaded as compiled Go plugins or subprocess execution. Maintain context caches per level. Pass parent contexts to setup functions. New `setup/loader.go` module for Go setup file loading. Protocol roundtrip tests (rapid) + handler tests + fuzz target for setup protocol messages.
**Acceptance:** `cd shatter-go && go test ./...` passes. Protocol roundtrips work.
**Blocked by:** #1 (wire format)

### 11. Rust frontend multi-level setup

**Type:** feature | **Priority:** 2 | **Labels:** rust, frontend
**Parent:** epic
**Files:** `shatter-rust/src/protocol.rs`, `shatter-rust/src/handler.rs`, `shatter-rust/src/setup.rs` (new)
**Description:**
Mirror protocol changes in Rust frontend types. Replace setup/teardown stubs with real implementations. Rust setup files compiled and executed as subprocesses that return JSON context on stdout. New `setup.rs` module for setup execution logic. Protocol roundtrip tests + handler tests.
**Acceptance:** `cd shatter-rust && cargo test` passes. Clippy clean.
**Blocked by:** #1 (wire format)

### 12. E2E tests for multi-level setup

**Type:** feature | **Priority:** 1 | **Labels:** core, testing, e2e
**Parent:** epic
**Files:** `shatter-core/tests/e2e_concolic.rs`, example setup files in `examples/`
**Description:**
Add E2E test cases exercising the full pipeline with multi-level setup. Create example setup files (at least session + file level) in `examples/typescript/`. Test cases: (1) session setup provides context to all functions, (2) file-level setup scoped per source file, (3) setup failure skips dependent functions, (4) teardown runs in reverse order. Verify both explorer and orchestrator paths produce correct setup behavior (parity).
**Acceptance:** `cargo test --test e2e_concolic` passes with new setup test cases.
**Blocked by:** #5 (explorer integration), #6 (orchestrator integration), #8 (CLI wiring), #9 (TS frontend — needed for E2E subprocess)

### 13. Walkthrough setup demo

**Type:** feature | **Priority:** 2 | **Labels:** cli, docs
**Parent:** epic
**Files:** `demo/walkthrough.sh`
**Description:**
Add a setup/teardown demo step to the walkthrough script. Create a minimal example with session-level setup (e.g., sets a config value) and file-level setup. Demonstrate `--setup-timeout` and `--fail-on-error` flags. Verify the walkthrough ERROR SUMMARY reports no unexpected errors.
**Acceptance:** `bash demo/walkthrough.sh --auto --delay 0` passes with new setup demo step.
**Blocked by:** #8 (CLI flags), #9 (TS frontend handles setup)

---

## Dependency Graph

```
#1 Protocol types
├── #2 Config schema
│   ├── #3 Convention discovery
│   │   └── #8 CLI wiring ──────────────┐
│   └── #4 SetupManager                 │
│       ├── #5 Explorer integration     │
│       │   └── #12 E2E tests ◄─────────┤
│       ├── #6 Orchestrator parity      │
│       │   └── #12 E2E tests ◄─────────┤
│       ├── #7 Scan orchestrator        │
│       └── #8 CLI wiring              │
│           ├── #12 E2E tests          │
│           └── #13 Walkthrough demo   │
├── #9 TS frontend ─────────────────────┤
│   ├── #12 E2E tests                  │
│   └── #13 Walkthrough demo           │
├── #10 Go frontend                     │
└── #11 Rust frontend                   │
```

## Parallelization Opportunities

- **#9, #10, #11** (all three frontends) can run in parallel — they only depend on #1
- **#5, #6, #7** can run in parallel — they all depend on #4 but not each other (though #6 should follow #5's pattern)
- **#2 and #9/10/11** can run in parallel — #2 depends on #1, frontends depend on #1
- **#3** can run in parallel with #4 — both depend on #2 but not each other
