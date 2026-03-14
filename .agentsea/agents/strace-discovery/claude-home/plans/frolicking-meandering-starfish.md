# Plan: str-zwgc — Test Impact Analysis (TIA)

## Context

Local dev iteration is slow when running all tests after small changes. TIA maintains a coverage map (which tests touch which files) and uses git to detect changes, running only the affected test subset. This accelerates the Quick/Standard/Full tiers significantly.

## Architecture

Two new modules in `shatter-core`, one new CLI subcommand in `shatter-cli`.

### New Files

| File | Purpose |
|---|---|
| `shatter-core/src/test_impact.rs` | Coverage map data model, YAML persistence, query/update logic, tier markers |
| `shatter-core/src/test_runner.rs` | TestTier enum, runner trait, Cargo/Vitest/GoTest adapters, coverage parsers |

### Modified Files

| File | Change |
|---|---|
| `shatter-core/src/lib.rs` | Add `pub mod test_impact; pub mod test_runner;` |
| `shatter-core/src/scm.rs` | Add `pub fn blob_hash(root, file) -> Result<String, ScmError>` (wraps `git hash-object`) and make `run_git` pub(crate) |
| `shatter-cli/src/main.rs` | Add `Test` variant to `CliCommand`, add `run_test()` handler |
| `shatter-core/src/test_arbitraries.rs` | Add proptest strategies for coverage map types |

## Data Model (`test_impact.rs`)

```yaml
# .shatter/test-markers/coverage-map.yaml
version: 1
recorded_at: "2026-03-07T23:19:00Z"
entries:
  "shatter-core::solver::tests::solve_basic":
    files:
      "shatter-core/src/solver.rs": "abc123..."
      "shatter-core/src/sym_expr.rs": "789abc..."
```

Key types:
- `CoverageMapData` — serialized form (version, recorded_at, forward map as `BTreeMap<String, TestEntry>`)
- `TestEntry` — `files: BTreeMap<String, String>` (relative path → blob hash)
- `CoverageMap` — in-memory wrapper with derived `reverse: HashMap<String, Vec<String>>` (file → test IDs)
- `TierMarker` — tier name, timestamp, git commit hash
- `ImpactQuery` — result of querying: changed_files, affected_tests, stale_entries, unmapped_files
- `TiaError` — error enum using thiserror (Io, Yaml, Scm, Runner, NoCoverageMap)

Constants:
- `COVERAGE_MAP_REL_PATH = "test-markers/coverage-map.yaml"`
- `TIER_MARKER_DIR = "test-markers/tiers"`
- `COVERAGE_MAP_VERSION = 1`

Key functions:
- `CoverageMap::load(shatter_dir)` — load YAML, build reverse index
- `CoverageMap::save(shatter_dir)` — atomic write (tempfile + rename) under `FileLock`
- `CoverageMap::query_affected(changed_files)` → `ImpactQuery`
- `CoverageMap::update_from_coverage(coverage_output, root)` — update forward entries, recompute hashes
- `build_reverse_index(entries)` — pure function, file → [test_ids]
- `blob_hash(root, file)` — via `git hash-object` in scm.rs
- `find_stale_entries(&self)` — compare stored vs current hashes
- `write_tier_marker / read_tier_marker / is_tier_fresh` — tier marker CRUD

## Runner Abstraction (`test_runner.rs`)

```rust
pub enum TestTier { Quick, Standard, Full, E2e }

pub trait TestRunner {
    fn run_tests(&self, root: &Path, filter: &[String]) -> Result<TestRunResult, TiaError>;
    fn run_with_coverage(&self, root: &Path, filter: &[String]) -> Result<CoverageOutput, TiaError>;
}
```

Adapters:
- `CargoTestRunner` — `cargo test` with `--test-threads` and filter args; coverage via `RUSTFLAGS="-C instrument-coverage"` + `llvm-cov export`
- `VitestRunner` — `npx vitest run` with `--reporter=json`; coverage via `--coverage --coverage.reporter=json`
- `GoTestRunner` — `go test ./...` with `-run` filter; coverage via `-coverprofile`

Helper: `detect_runners(root)` — checks for `Cargo.toml`, `package.json`, `go.mod`

## CLI (`shatter-cli/src/main.rs`)

Add `Test` variant to `CliCommand`:
```rust
Test {
    #[arg(long)] all: bool,
    #[arg(long)] record: bool,
    #[arg(long, value_enum)] tier: Option<TestTier>,
    #[arg(long, default_value = "HEAD")] base: String,
    #[arg(long)] include_untracked: bool,
    #[arg(long)] dry_run: bool,
}
```

Handler `run_test()` flow:
1. Resolve project root and `.shatter/` dir
2. If `--tier`: run the tier's predefined commands (matching CLAUDE.md tiers), write marker on success
3. If `--record`: run all detected runners with coverage, build/update coverage map, save
4. If `--all`: run all detected runners without filtering
5. Default: load coverage map → query git changes via `ScmProvider` → compute affected tests → run filtered → print summary
6. If `--dry-run`: print affected tests without executing

## Reusable Infrastructure

- `scm.rs::GitProvider` — `changed_files()`, `diff_files()` for change detection
- `scm.rs::run_git()` — make `pub(crate)`, add `blob_hash()` wrapper
- `file_lock.rs::FileLock` — atomic coverage map writes
- `serde_yaml` — already a dependency for YAML round-trips

## Implementation Sequence

1. Add `blob_hash()` to `scm.rs` and make `run_git` pub(crate)
2. Create `test_impact.rs` with data model, persistence, query logic, tier markers
3. Create `test_runner.rs` with TestTier, runner trait, three adapters
4. Register modules in `lib.rs`
5. Add CLI subcommand and handler in `main.rs`
6. Add proptest strategies to `test_arbitraries.rs`
7. Write unit + property tests

## Testing Strategy

**Unit tests** (in `test_impact.rs`):
- `build_reverse_index` correctness
- `query_affected` returns correct union, deduplicates
- Coverage map YAML roundtrip
- `find_stale_entries` detects mismatches
- Tier marker write/read/freshness

**Proptest** (in `test_impact.rs`):
- Reverse index invariant: every (test, file) pair in forward map has file→test in reverse
- Roundtrip: arbitrary `CoverageMapData` → YAML → parse → equal
- Query monotonicity: `query(A ∪ B) ⊇ query(A)`

**Unit tests** (in `test_runner.rs`):
- Go coverprofile parser with sample input
- Istanbul JSON parser with sample input
- Filter formatting per runner

**CLI tests** (in `main.rs`):
- Parse `test`, `test --all`, `test --record`, `test --tier=quick`

## Verification

1. `cargo test -p shatter-core` — unit + property tests pass
2. `cargo test -p shatter-cli` — CLI parse tests pass
3. `cargo clippy -p shatter-core -p shatter-cli -- -D warnings` — no warnings
4. Manual: `cargo run -- test --dry-run` in the shatter repo shows affected tests
5. Manual: `cargo run -- test --record` creates `.shatter/test-markers/coverage-map.yaml`
6. Manual: `cargo run -- test --tier=quick` runs cargo test and writes tier marker
