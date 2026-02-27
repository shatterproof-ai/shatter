# shatter-core

Rust core engine (library crate). Contains the concolic explorer, constraint solver integration, protocol types, batch analysis, call graph, and export logic.

## Key Modules

- `explorer.rs` — Concolic exploration loop and `format_exploration_report()`
- `protocol.rs` — Protocol message types shared across frontends
- `batch_analyze.rs` — Multi-file analysis orchestration
- `call_graph.rs` — Function dependency graph
- `export.rs` — Test generation from behavior maps

## Output Review

After changing code that affects CLI output (explore reports, log formatting, cluster summaries, export format), run `/walkthrough-review` to validate the output is human-readable.
