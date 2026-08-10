# shatter-cli

Rust CLI binary (clap). Thin layer that parses arguments and delegates to `shatter-core`.

## Key Files

- `src/args.rs` — clap CLI definition (`CliCommand` enum and per-command arg structs).
- `src/main.rs` — entry point: parses args and dispatches each `CliCommand` variant to a handler in `src/commands/`.
- `src/commands/` — one module per command. The handlers do the real work, e.g. `run_explore()` in `commands/explore.rs`, `run_scan()` in `commands/scan.rs`, `run_run()` in `commands/run.rs`, `run_analyze()` in `commands/analyze.rs`. See `commands/mod.rs` for the full list.
- `src/helpers.rs` — shared helpers used across command handlers.
- `src/render.rs` — output rendering.
- `src/telemetry_flush.rs` — PostHog telemetry flush: batch-sends queued events to the analytics endpoint.
- `src/embedded_frontend.rs`, `src/embedded_go_frontend.rs` — embedded frontend assets.
