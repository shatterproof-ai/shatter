# str-3lob: Revalidation CLI Command

## Context

The revalidation loop (`shatter-core/src/revalidation.rs`, from str-kab3) can re-execute previously-interesting inputs and classify drift into verdicts (Confirmed, ExpectedDrift, Flaky, PotentialRegression, SeverityDowngrade, SeverityUpgrade). This needs a CLI surface so users and CI pipelines can run `shatter revalidate` to check whether previously-found behaviors still hold after code changes.

## Approach

Follow the `stale` command pattern: analyze source → load cached behavior maps → spawn frontend → call `revalidate_behaviors()` → format output → set exit code.

## Files to Modify

1. **`shatter-cli/src/main.rs`** — Add `Revalidate` variant + `run_revalidate()` + dispatch
2. **`demo/walkthrough.sh`** — Add step 43 exercising the new command

## Implementation

### 1. Add `Revalidate` variant to `CliCommand` enum (~line 631)

```rust
/// Revalidate cached behaviors against current code.
///
/// Re-executes previously-interesting inputs and classifies each as
/// confirmed, drift, flaky, regression, or severity change.
/// Exit code: 0 = no regressions/upgrades, 1 = regression or severity upgrade found.
Revalidate {
    /// Source file to revalidate (e.g., "src/math.ts").
    #[arg(required = true)]
    source: String,

    /// Cache directory containing behavior maps.
    /// Falls back to SHATTER_CACHE_DIR env var, then `.shatter/cache/`.
    #[arg(long, env = "SHATTER_CACHE_DIR")]
    cache_dir: Option<PathBuf>,

    /// Output format: "text" (default) or "json".
    #[arg(long, default_value = "text")]
    format: String,

    /// Per-request timeout in seconds for frontend communication.
    #[arg(long, default_value_t = 30)]
    request_timeout: u64,

    /// Execution timeout in seconds for each function invocation.
    #[arg(long, default_value_t = 10)]
    exec_timeout: u64,

    /// Build timeout in seconds for compiling instrumented code.
    #[arg(long, default_value_t = 30)]
    build_timeout: u64,

    /// Memory limit in MB for the frontend process.
    #[arg(long)]
    memory_limit: Option<u64>,
},
```

### 2. Implement `run_revalidate()` function

Algorithm (mirrors `run_stale` pattern):

1. `parse_target(source)` to get file + optional function + language
2. `resolve_project_root()` for cache dir resolution
3. Determine cache dir: explicit flag > env var > `<project_root>/.shatter/cache/`
4. `frontend_config()` + `Frontend::spawn()` (needs `analyze` + `execute` capabilities)
5. Send `Analyze` to get current function list + fingerprints
6. Compute deep fingerprints via `compute_deep_fingerprints()`
7. For each function: load cached `BehaviorMap` from `BehaviorMapCache`
8. For each function with a cached behavior map: call `revalidate_behaviors()` with the frontend, behavior map, and current fingerprint
9. Collect all `RevalidationReport`s
10. `shutdown_frontend()`
11. Format output (text or JSON) with verdict labels
12. Return whether any PotentialRegression or SeverityUpgrade was found

**Text output format:**
```
src/math.ts:add [CONFIRMED] 3/3 behaviors confirmed
src/math.ts:divide [REGRESSION] 1 potential regression, 2 confirmed
  Input: [0] → potential_regression (was rare_path)
src/math.ts:parse [DRIFT] 2 expected drift, 1 confirmed
  Input: ["abc"] → expected_drift
  Input: [""] → expected_drift

Summary: 6 revalidated, 3 confirmed, 2 drift, 1 regression, 0 flaky, 0 severity changes
Exit code: 1 (regressions found)
```

Verdict label mapping:
- `Confirmed` → `[CONFIRMED]`
- `ExpectedDrift` → `[DRIFT]`
- `Flaky` → `[FLAKY]`
- `PotentialRegression` → `[REGRESSION]`
- `SeverityDowngrade` → `[SEVERITY↓]`
- `SeverityUpgrade` → `[SEVERITY↑]`

**JSON output format:**
```json
{
  "reports": [ /* Vec<RevalidationReport> serialized */ ],
  "summary": {
    "total": 6,
    "confirmed": 3,
    "expected_drift": 2,
    "flaky": 0,
    "potential_regression": 1,
    "severity_downgrade": 0,
    "severity_upgrade": 0
  },
  "has_regressions": true
}
```

**Exit code logic:** nonzero when any verdict is `PotentialRegression` or `SeverityUpgrade`.

### 3. Add dispatch in `main()` (~line 3259)

Follow the `Stale` pattern — match on `CliCommand::Revalidate`, extract fields, call `run_revalidate()`, convert `Ok(bool)` to `ExitCode`.

### 4. Update `demo/walkthrough.sh`

- Change `TOTAL=42` to `TOTAL=43`
- Add step 43 after the stale check:
```bash
step 43 $TOTAL "Revalidate" \
    "Revalidate cached behaviors for arithmetic example" \
    $SHATTER revalidate 'examples/standalone/ts/01-arithmetic.ts'
```

### 5. Add CLI argument parsing tests

Follow the `stale` test pattern (~line 4918): verify `Revalidate` variant is parsed correctly with default and explicit args.

## Reused Code

| What | Where |
|------|-------|
| `parse_target()` | `main.rs:701` |
| `resolve_project_root()` | `main.rs:99` |
| `frontend_config()` | `main.rs:749` |
| `Frontend::spawn()` | `shatter-core/src/frontend.rs:120` |
| `shutdown_frontend()` | `main.rs` (existing helper) |
| `BehaviorMapCache` | `shatter-core/src/cache.rs:80` |
| `revalidate_behaviors()` | `shatter-core/src/revalidation.rs` |
| `compute_deep_fingerprints()` | `shatter-core/src/fingerprint.rs` |
| `RevalidationVerdict`, `RevalidationReport` | `shatter-core/src/revalidation.rs` |
| `Colors` struct | `main.rs:118` |

## Verification

1. `cargo test -p shatter-cli` — unit + CLI arg parsing tests
2. `cargo clippy -p shatter-cli -- -D warnings` — no warnings
3. `bash demo/walkthrough.sh --auto --delay 0` — walkthrough passes with new step
4. Manual test: `cargo run -- revalidate examples/standalone/ts/01-arithmetic.ts` after running explore to populate cache
