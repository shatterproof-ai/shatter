# str-jzd: explore --dry-run flag — Already Implemented

## Context

The `explore --dry-run` flag was requested to show what would be explored without actually running exploration. Upon investigation, **this feature is already fully implemented**.

## Evidence

### 1. Clap flag definition (`shatter-cli/src/main.rs:275-278`)
```rust
/// Analyze and compare fingerprints, print stale/fresh/removed functions, then exit
/// without exploring. Requires --output.
#[arg(long)]
dry_run: bool,
```

### 2. Implementation (`shatter-cli/src/main.rs:1063-1091`)
- Computes incremental plan via `compute_incremental_plan()` (fingerprint comparison)
- Prints stale/fresh/removed function lists to stderr
- Shuts down frontend without exploring

### 3. CLI parsing tests (`shatter-cli/src/main.rs`)
- `cli_parses_explore_with_dry_run_flag` (line 4856)
- `cli_clean_and_dry_run_default_to_false` (line 4875)

### 4. Walkthrough coverage (`demo/walkthrough.sh:389-392`)
```bash
# Stage 40: Dry-run mode
step 40 $TOTAL "Dry-Run Mode" \
    "Use --dry-run to preview which functions would be re-explored without actually exploring" \
    $SHATTER explore --output /tmp/shatter-spec.json --dry-run "${EXAMPLES[0]}"
```

## Plan

1. Run `cargo test` in shatter-cli to confirm tests pass
2. Run `cargo clippy -- -D warnings` to confirm clean
3. Close the beads issue with `bd close str-jzd`
4. No code changes required
