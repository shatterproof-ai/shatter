# str-0s76.1 — SetupLevel protocol types

## Context
Replace the binary `SetupMode` (per_function/per_execution) with a richer `SetupLevel` enum supporting session/file/function/execution granularity. Add `SetupContextStack` for nested setup contexts. Update Setup, Teardown, and Execute commands accordingly.

## Files to modify
- `shatter-core/src/config.rs` — Replace `SetupMode` with `SetupLevel`
- `shatter-core/src/protocol.rs` — Update Setup/Teardown/Execute commands, add SetupContextStack, update tests
- `shatter-core/src/test_arbitraries.rs` — Update proptest strategies

## Changes

### 1. `config.rs` — Replace SetupMode with SetupLevel
- Remove `SetupMode` enum (PerFunction, PerExecution)
- Add `SetupLevel` enum: `Session`, `File`, `Function`, `Execution` (with `#[serde(rename_all = "snake_case")]`)

### 2. `protocol.rs` — New type: SetupContextStack
```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetupContextStack {
    pub contexts: Vec<SetupContextEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetupContextEntry {
    pub level: SetupLevel,
    pub context: serde_json::Value,
}
```

### 3. `protocol.rs` — Update Command::Setup
- Rename `function` → `scope`
- Replace `mode: SetupMode` → `level: SetupLevel`
- Add `parent_context: Option<SetupContextStack>` (serde default, skip_serializing_if)

### 4. `protocol.rs` — Update Command::Teardown
- Rename `function` → `scope`
- Add `level: SetupLevel`

### 5. `protocol.rs` — Update Command::Execute
- Change `setup_context: Option<serde_json::Value>` → `setup_context: Option<SetupContextStack>`

### 6. `test_arbitraries.rs` — Update strategies
- Replace `arb_setup_mode()` with `arb_setup_level()` (4 variants)
- Add `arb_setup_context_entry()` and `arb_setup_context_stack()`
- Update `arb_command()` for Setup (scope, level, parent_context) and Teardown (scope, level)
- Update Execute generation to use `arb_setup_context_stack()`

### 7. Update all tests in protocol.rs
- Fix setup/teardown round-trip tests to use new field names
- Fix serialization tests for new JSON shape
- Update execute tests for SetupContextStack

### 8. Grep for other usages
- Search for `SetupMode`, `setup_context`, and the old field names across shatter-core to catch any remaining references (explorer.rs, orchestrator.rs, etc.)

## Verification
1. `cargo test -p shatter-core`
2. `cargo clippy -p shatter-core -- -D warnings`
3. Run `/pre-completion` skill
