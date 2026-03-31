---
name: rust-conventions
description: Rust coding standards for Shatter. Use when writing or reviewing Rust code in shatter-core/ or shatter-cli/.
user-invocable: true
---

## Tool-Verified Rules
- `cargo clippy -- -D warnings` must pass
- Treat `clippy::unwrap_used` as the default policy for `shatter-core/`: no `unwrap()` in library code
- Treat rustc `missing_docs` and rustdoc lints as the enforcement path for public API docs; `///` docs should explain contracts and non-obvious behavior, not restate signatures
- Prefer existing OSS lint engines over ad hoc scripts. If a Rust-only repo rule needs hard enforcement beyond Clippy, use `Dylint`

## Error Handling
- Use `thiserror` for error types in `shatter-core/` (library code)
- Use `anyhow` in `shatter-cli/` entrypoints and command wiring, not shared library surfaces
- Propagate with `?`; let callers decide handling where possible

## Testing
- Follow the repo-level testing policy in the root `CLAUDE.md`
- `proptest` is a primary testing tool here, not an optional extra. Add property tests for core invariants, round-trips, and malformed input where they matter
- Reuse shared generators such as `shatter-core/src/test_arbitraries.rs` instead of rebuilding them per file
- When touching protocol-visible behavior or parallel execution paths, run the repo's parity, conformance, and E2E gates called out in `CLAUDE.md`

## Protocol / Data Types
- Protocol-visible Rust types must stay aligned with the shared protocol contract; rely on the repo's parity and conformance tooling to verify behavior
- Tagged protocol enums should continue using `#[serde(tag = "type")]`
- JSON-facing protocol types should continue using `#[serde(rename_all = "camelCase")]` and derive `Serialize, Deserialize`

## Design Guidance
- Minimal `lib.rs` re-exports
- Dependencies flow `cli -> core`, never reverse
- Define named constants for defaults, timeouts, and shared configuration values; tests should reference the constants instead of duplicating important literals
- Use `#[must_use]` where ignored return values would hide mistakes
