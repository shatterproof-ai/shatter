---
name: rust-conventions
description: Rust coding standards for Shatter. Use when writing or reviewing Rust code in shatter-core/ or shatter-cli/.
user-invocable: true
---

## Error Handling
- Use `thiserror` for error types in `shatter-core/` (library code)
- Use `anyhow` only in `shatter-cli/` (binary code)
- **No `unwrap()` in library code** — use `Result<T, E>` with `?`
- Define error enums: `#[derive(Debug, thiserror::Error)]`
- Propagate with `?`; let callers decide handling

## Testing
- Unit tests in `#[cfg(test)] mod tests` at bottom of each file
- Behavior-descriptive names: `fn rejects_negative_offset()`
- `proptest` for property-based tests where applicable
- Integration tests in `tests/` directory
- Every public function has tests

## Clippy & Style
- `cargo clippy -- -D warnings` must pass (zero warnings)
- Short, focused functions — extract helpers over section comments
- `///` doc comments on public APIs — document contracts, not signatures (see root CLAUDE.md "Inline Documentation")
- `//!` module-level doc comments when the module's purpose isn't obvious from its name
- `#[must_use]` on functions returning values callers shouldn't ignore
- Prefer `impl Trait` over `dyn Trait`

## Constants
- Define named constants for default values, timeouts, and configuration defaults
- Place constants at the top of the module that owns the concept (e.g., `DEFAULT_REQUEST_TIMEOUT` in `frontend.rs`)
- Use `pub const` for values referenced across modules; module-private `const` otherwise
- Tests reference constants, never duplicate the literal value
- Struct `Default` impls reference the constant, not a bare literal

## Module Structure
- One module = one responsibility
- Minimal `lib.rs` re-exports
- Dependencies: `cli → core`, never reverse
- Rust 2024 edition

## Serde / Protocol
- `#[serde(tag = "type")]` for tagged enums
- `#[serde(rename_all = "camelCase")]` for JSON fields
- Derive `Serialize, Deserialize` on protocol types

## Key Dependencies
- `z3` — SMT solver bindings
- `tokio` — async runtime
- `serde` / `serde_json` — serialization
- `thiserror` — error derive macros
- `clap` — CLI argument parsing
- `proptest` — property-based testing
