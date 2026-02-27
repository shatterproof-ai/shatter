# shatter-cli

Rust CLI binary (clap). Thin layer that parses arguments and delegates to `shatter-core`.

## Key Files

- `src/main.rs` — All CLI commands including `run_explore()`, `run_scan()`, `run_export_tests()`

## Output Review

After changing code that affects CLI output (flags, formatting, log levels, verbosity), run `/walkthrough-review` to validate the output is human-readable.
