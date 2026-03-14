# Plan: str-61v — shatter stale command

## Context

Issue str-61v requests a lightweight staleness check command. However, **this feature is already fully implemented** on main.

## Evidence

| Component | Status | Location |
|---|---|---|
| CLI subcommand definition | Done | `shatter-cli/src/main.rs:632-660` |
| Dispatch in `main()` | Done | `shatter-cli/src/main.rs:3239-3269` |
| Handler `run_stale()` | Done | `shatter-cli/src/main.rs:2883-2968` |
| Core logic `compute_incremental_plan()` | Done | `shatter-core/src/spec.rs:680-724` |
| Deep fingerprint computation | Done | `shatter-core/src/fingerprint.rs` |
| CLI parsing tests | Done | `shatter-cli/src/main.rs:4891-4924` |
| Walkthrough step 42 | Done | `demo/walkthrough.sh:399-405` |
| Exit codes (0=fresh, 1=stale) | Done | Implemented in main dispatch |
| Text and JSON output formats | Done | Both supported via `--format` flag |

## Action

No code changes needed. Close the issue with `bd close str-61v` noting it's already implemented.

## Verification

Run `cargo test` in shatter-cli and `cargo clippy -- -D warnings` to confirm everything passes on main.
