# str-zwgc: Test Impact Analysis (TIA)

**Status: DEFERRED** — Plan saved to `docs/plans/str-zwgc-test-impact-analysis.md`. Issue `str-zwgc` deferred in beads.

## Context

Running all tests (~1,487 Rust + 195 Go + TS) on every change is slow. TIA maintains a coverage map (test → source files) so `shatter test` runs only tests affected by changed files. Cold start falls back to running everything; the map populates incrementally via `--record`.

## Design Decisions

- **Keep Jest** (not Vitest) — the project already uses Jest; migrating is orthogonal
- **Forward map only on disk** — reverse map derived in memory to avoid consistency issues
- **Two new modules** in shatter-core: `tia.rs` (data model + query) and `tia_runner.rs` (runner filter generation)
- **Reuse** `scm.rs` (GitProvider + `run_git`), `file_lock.rs` (FileLock for atomic writes)
- **Add** `blob_hash()` and `tree_hash()` helpers to `scm.rs`

## Phase 1: Core Data Model + CLI (MVP)

### New files

1. **`shatter-core/src/tia.rs`** — Coverage map types, load/save/query
2. **`shatter-core/src/tia_runner.rs`** — Runner-specific filter formatting
3. Register both in **`shatter-core/src/lib.rs`**

### Coverage map schema (`.shatter/test-markers/coverage-map.yaml`)

```yaml
version: 1
recorded_at: "2026-03-07T14:30:00Z"
tests:
  "shatter-core::solver::tests::solve_basic":
    files:
      "shatter-core/src/solver.rs": "da39a3e..."
      "shatter-core/src/protocol.rs": "b4c3d2e..."
```

### Tier marker schema (`.shatter/test-markers/<tier>`)

```yaml
tree_hash: "abc123..."
passed_at: "2026-03-07T14:30:00Z"
```

### Types in `tia.rs`

```rust
const COVERAGE_MAP_FILENAME: &str = "coverage-map.yaml";
const TEST_MARKERS_DIR: &str = "test-markers";
const COVERAGE_MAP_VERSION: u32 = 1;

struct TestCoverage { files: BTreeMap<String, String> }  // path -> blob hash
struct CoverageMap { version: u32, recorded_at: String, tests: BTreeMap<String, TestCoverage> }
struct AffectedTests { tests: Vec<String>, unmapped_files: Vec<String>, stale_files: Vec<String> }
struct TierMarker { tree_hash: String, passed_at: String }
```

### Key functions

**`tia.rs`:**
- `load_coverage_map(project_root) -> Result<CoverageMap>` — cold start returns empty map
- `save_coverage_map(project_root, map) -> Result<()>` — atomic via FileLock + tempfile + rename
- `build_reverse_map(map) -> BTreeMap<String, Vec<String>>` — derived in memory
- `query_affected(map, changed_files, project_root) -> Result<AffectedTests>` — reverse lookup + blob hash staleness check
- `write_tier_marker(project_root, tier) -> Result<()>`
- `read_tier_marker(project_root, tier) -> Result<Option<TierMarker>>`
- `is_tier_fresh(project_root, tier) -> Result<bool>`

**`tia_runner.rs`:**
- `format_cargo_nextest_filter(tests) -> String` — `-E 'test(=n1) | test(=n2)'`
- `format_cargo_test_filter(tests) -> Vec<String>` — `-- n1 n2`
- `format_jest_filter(tests) -> Vec<String>` — `--testPathPattern`
- `format_go_test_filter(tests) -> (String, String)` — `./pkg/ -run 'T1|T2'`

**`scm.rs` additions:**
- `blob_hash(root, file) -> Result<String, ScmError>` — `git hash-object <file>`
- `tree_hash(root) -> Result<String, ScmError>` — `git write-tree` (or `git rev-parse HEAD^{tree}` for committed state)

### CLI addition (`shatter-cli/src/main.rs`)

Add `Test` variant to `CliCommand`:
```rust
Test {
    #[arg(long)] all: bool,
    #[arg(long)] record: bool,
    #[arg(long, value_enum)] tier: Option<TestTier>,
    #[arg(long)] dry_run: bool,
}
```

Add `run_test()` handler:
1. Detect project root
2. Load coverage map
3. Get changed files via `GitProvider::changed_files()`
4. `query_affected()` → affected tests
5. If `--dry-run`, print and exit
6. If `--all` or cold start, run everything
7. Format runner-specific filters, execute via `Command`
8. If `--tier` and all pass, write tier marker

## Phase 2: Coverage Recording (`--record`)

Parse coverage output from each runner:
- Rust: `cargo llvm-cov --json` → parse JSON for file-level coverage per test
- TS: `npx jest --coverage --json` → parse coverage map
- Go: `go test -coverprofile` → parse profile

New functions in `tia.rs`:
- `parse_cargo_llvm_cov(json) -> Result<Vec<(test, files)>>`
- `parse_jest_coverage(json) -> Result<Vec<(test, files)>>`
- `parse_go_coverprofile(text) -> Result<Vec<(test, files)>>`
- `update_coverage_map(map, test, files)` — update forward map entries

## Phase 3: Tiered Markers + Pre-commit (Enhancement)

Pre-commit integration via `shatter test --tier=standard --dry-run` to check tier freshness. Lower priority — markers work from Phase 1, hook integration can follow.

## Tests

### Unit tests
- Load/save roundtrip (empty map, populated map)
- Query with no changes → empty
- Query with changed file → correct tests returned
- Query with unmapped file → appears in `unmapped_files`
- Cold start (missing file) → empty map, no panic
- Tier marker write/read roundtrip
- Filter formatting for each runner

### Proptest
- Roundtrip: CoverageMap serialize → deserialize → equality
- Reverse map completeness: every test with file F → F in reverse map → that test
- Query soundness: query_affected(map, [f]) ⊇ {t | f ∈ map.tests[t].files}
- Cold start safety: empty map + any changed files → all unmapped
- Filter non-empty: non-empty test list → non-empty filter string

## Verification

1. `cargo test -p shatter-core` — unit + proptest pass
2. `cargo clippy -p shatter-core -p shatter-cli -- -D warnings` — clean
3. `shatter test --dry-run` in the shatter repo — prints affected tests or cold-start message
4. `shatter test --all` — runs all tests successfully
5. Update `demo/walkthrough.sh` to exercise `shatter test --dry-run`

## Files to modify

| File | Change |
|------|--------|
| `shatter-core/src/tia.rs` | New: coverage map types, load/save/query |
| `shatter-core/src/tia_runner.rs` | New: runner filter formatting |
| `shatter-core/src/lib.rs` | Add `pub mod tia; pub mod tia_runner;` |
| `shatter-core/src/scm.rs` | Add `blob_hash()`, `tree_hash()` |
| `shatter-cli/src/main.rs` | Add `Test` command + `run_test()` handler |
| `demo/walkthrough.sh` | Add `shatter test --dry-run` exercise |
