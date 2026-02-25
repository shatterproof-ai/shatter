---
name: check-rust
description: Run Rust quality gates (cargo test + cargo clippy). Use after making changes to shatter-core/ or shatter-cli/.
allowed-tools: Bash
disable-model-invocation: true
---

Run the following checks and examine the output for errors:

1. Run `cargo test` in the workspace root and capture output
2. Run `cargo clippy -- -D warnings` and capture output
3. Examine both outputs for failures, warnings, and errors
4. Summarize results:
   - Number of tests passed/failed
   - Any clippy warnings or errors
   - Suggested corrections for any failures
   - Overall PASS/FAIL status
