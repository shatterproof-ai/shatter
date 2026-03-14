# str-bat: --stratum range parsing

## Context

The stratum range parser is **already fully implemented** at `shatter-core/src/stratum.rs` (317 lines) with all requested syntax supported:
- `0..5` — absolute range
- `-3..` — last 3 to end
- `..5` — start to index 5
- `-3..-1` — negative indices on both sides
- Single values like `2` or `-1`

It's already integrated into the CLI (`shatter-cli/src/main.rs` `run_scan()`) and used by `ScanConfig` in `scan_orchestrator.rs`.

The module has **22 comprehensive unit tests** covering parsing, resolution, clamping, filtering, and error cases.

## What's Missing

The only gap per CLAUDE.md standards is **proptest coverage**. The project requires property-based tests for non-trivial public functions, and `stratum.rs` has three public functions (`parse_stratum_spec`, `resolve_range`, `filter_layers`) with no proptest properties.

## Plan

1. **Add proptest properties** to `shatter-core/src/stratum.rs`:
   - **Roundtrip invariant**: any valid spec string that parses successfully produces a `StratumSpec` that resolves without panic for any `max_layer`
   - **Resolve range bounds**: resolved range is always within `0..=max_layer`
   - **Monotonicity**: if `start <= end` (absolute), resolved range is non-empty
   - **filter_layers subset**: filtered output indices are always within the input range
   - **Negative index resolution**: `-0` always resolves to `max_layer`, `-N` resolves to `max_layer - N` (clamped to 0)

2. **Run verification**: `cargo test -p shatter-core` and `cargo clippy -p shatter-core -- -D warnings`

## Files to Modify

- `shatter-core/src/stratum.rs` — add `proptest!` block to existing `#[cfg(test)] mod tests`

## Verification

```bash
cd /path/to/worktree
cargo test -p shatter-core stratum -- --nocapture
cargo clippy -p shatter-core -- -D warnings
```
