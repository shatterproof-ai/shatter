# Shatter

Automatic exploratory testing via concolic execution. Shatter analyzes your functions, discovers branches, and generates inputs that exercise every code path — no test authoring required.

## Installation

Shatter is not yet published to a package registry or public repository. Build from source:

```bash
cd shatter
cargo build --release
# Binary is at target/release/shatter
```

## Quickstart

```bash
# 1. Install shatter (see above)

# 2. Write a function
cat > shipping.ts <<'EOF'
export function calculateShipping(weight: number, country: string): number {
  if (weight <= 0) throw new Error("Invalid weight");
  if (country === "US") return weight < 5 ? 5.99 : 12.99;
  return weight < 5 ? 15.99 : 29.99;
}
EOF

# 3. Explore it
shatter explore shipping.ts:calculateShipping
```

Shatter finds every branch and generates inputs that reach each one.

## How It Works

Shatter uses **concolic execution** — a hybrid of concrete and symbolic execution:

1. **Analyze**: Parses your function and identifies branch points (if/else, switch, try/catch, ternary, etc.)
2. **Explore**: Runs the function concretely while collecting symbolic path constraints. After each run, it negates a constraint and solves for a new input (via Z3) that takes a different path.
3. **Report**: Groups discovered paths into behavior clusters and shows which inputs trigger each behavior.

This approach combines the precision of real execution with the systematic coverage of symbolic analysis.

## Documentation

See [docs/INDEX.md](docs/INDEX.md) for a map of all project documentation — what each file covers, who it's for, and whether it describes current behavior or planned work.

## Supported Languages

| Language   | Frontend      | Status |
|------------|---------------|--------|
| TypeScript | `shatter-ts`  | Supported |
| Go         | `shatter-go`  | Supported |
| Rust       | `shatter-rust`| Stub (protocol handler only) |

See [SPEC.md §1.3](SPEC.md#13-supported-languages) for the canonical language support matrix including file extensions and implementation details.

## CLI Reference

All commands accept targets in `<file>:<function>` format (e.g., `src/math.ts:add`) or `<file>` to target all functions. File extension determines the language frontend (`.ts` = TypeScript, `.go` = Go).

**Global options:** `--log-level <LEVEL>` (error/warn/info/debug/trace), `-v` (debug), `-vv` (trace), `-q` (quiet), `--timing <MODE>` (show timing stats), `--project-dir <DIR>` (override project root), `--color <WHEN>` (always/auto/never).

See [`SPEC.md`](SPEC.md) for the full behavioral specification.

### `shatter explore`

Explore functions to discover branches and generate test inputs.

```
shatter explore [OPTIONS] <TARGETS>...
```

| Flag | Default | Description |
|------|---------|-------------|
| `--max-iterations N` | 100 | Maximum exploration iterations per function |
| `--timeout SECS` | 60 | Timeout in seconds for entire exploration |
| `--timeout-explore SECS` | -- | Per-function wall-clock timeout (whichever of this or `--max-iterations` triggers first) |
| `--concolic` | -- | Use the Z3-backed concolic explorer instead of the random explorer |
| `--genetic` | -- | Enable the genetic algorithm explorer |
| `--analyze-only` | -- | Only analyze branches, skip exploration |
| `--show-clusters` | -- | Display behavior clusters in output |
| `--spec` | -- | Output a behavioral specification (markdown) |
| `--spec-json` | -- | Output the behavioral specification as JSON |
| `-o, --output PATH` | -- | Write per-file spec JSON to a file (implies `--spec-json`) |
| `--invariants` | -- | Enable Daikon-style invariant detection |
| `--scope PATH` | -- | Path to a `shatter.scope.yaml` file |
| `--config PATH` | -- | Path to `.shatter/config.yaml` |
| `--inputs PATH` | -- | Path to a candidate inputs JSON file |
| `--cache-dir DIR` | `.shatter-cache/behavior-maps/` | Directory for caching behavior maps |
| `--no-cache` | -- | Disable behavior map caching |
| `--no-boundary-values` | -- | Disable built-in boundary values as seeds |
| `--seeds-dir DIR` | `.shatter/seeds` | Directory for cross-function seed pool |
| `--no-seeds` | -- | Disable the cross-function seed pool |
| `--solver-timeout SECS` | -- | Z3 solver timeout per query |
| `--memory-limit MB` | -- | Memory limit for the frontend process |
| `--request-timeout SECS` | 30 | Per-request timeout for frontend responses |
| `--exec-timeout SECS` | 10 | Per-invocation timeout in the frontend |
| `--build-timeout SECS` | 30 | Timeout for compiling instrumented code |
| `--setup-timeout SECS` | -- | Override setup/teardown timeouts |
| `--fail-on-setup-error` | -- | Treat setup failures as fatal (abort immediately) |
| `--clean` | -- | Ignore existing spec and force full re-exploration |
| `--dry-run` | -- | Print stale/fresh/removed functions, then exit (requires `--output`) |
| `--loop-buckets LIST` | `0,1,2,5` | Loop iteration bucket boundaries for path hashing |

Run `shatter explore --help` for the complete option list including genetic algorithm tuning flags.

```bash
shatter explore src/shipping.ts:calculateShipping
shatter explore --concolic --spec --invariants src/math.ts:add
shatter explore --analyze-only src/shipping.ts
```

### `shatter scan`

Scan a directory for source files, analyze and explore all functions in dependency order, using behavior maps as mocks.

```
shatter scan [OPTIONS] <DIRECTORY>
```

| Flag | Default | Description |
|------|---------|-------------|
| `--language LANG` | auto | Language to scan: typescript, go (auto-detected from extensions if omitted) |
| `--include GLOB` | -- | Glob patterns for files to include (repeatable) |
| `--exclude GLOB` | -- | Glob patterns for files to exclude (repeatable) |
| `--changed` | -- | Scan only files with uncommitted changes (staged + unstaged) |
| `--since REF` | -- | Scan only files changed between `<ref>` and HEAD |
| `--include-untracked` | -- | Include untracked files when using `--changed` |
| `--all` | -- | Scan all functions, including non-exported ones |
| `--max-depth N` | -- | Maximum directory traversal depth |
| `--max-iterations N` | 100 | Maximum iterations per function |
| `--timeout-total SECS` | 300 | Total scan timeout |
| `--timeout-per-fn SECS` | 30 | Per-function timeout (skip on exceed) |
| `--timeout-explore SECS` | -- | Per-function exploration wall-clock timeout |
| `--parallelism N` | auto | Number of parallel frontend subprocesses |
| `-o, --output DIR` | `./shatter-report/` | Output directory for reports |
| `--format FMT` | json | Report format: json, markdown, or both |
| `--emit-tests FRAMEWORK` | -- | Generate test files: jest, vitest, or gotest |
| `--progress` | -- | Emit progress events to stderr |
| `--dry-run` | -- | Show what would be scanned without executing |
| `--resume FILE` | -- | Resume a previous scan from a state file |
| `--mock-config PATH` | -- | Path to a mock configuration YAML file |
| `--core-sample SIZE` | -- | Representative sample: percentage (`"50%"`) or count (`"20"`) |
| `--seed N` | -- | Seed for deterministic core sample selection |
| `--batch RANGE` | -- | Progressive batch index (`"0"`, `"next"`, `"0-2"`). Requires `--core-sample` |
| `--stratum RANGE` | -- | Call graph layer filter (`"0"` = leaves, `"0..3"`, `"-2..-0"`) |
| `--cache-dir DIR` | `.shatter-cache/behavior-maps/` | Behavior map cache directory |
| `--no-cache` | -- | Disable caching |
| `--genetic` | -- | Enable the genetic algorithm explorer |
| `--solver-timeout SECS` | -- | Z3 solver timeout per query |
| `--memory-limit MB` | -- | Memory limit for the frontend process |
| `--seeds-dir DIR` | `.shatter/seeds` | Cross-function seed pool directory |
| `--no-seeds` | -- | Disable the cross-function seed pool |
| `--setup-timeout SECS` | -- | Override setup/teardown timeouts |
| `--fail-on-setup-error` | -- | Treat setup failures as fatal (abort immediately) |

Run `shatter scan --help` for the complete option list.

```bash
shatter scan src/
shatter scan --format both -o ./reports src/
shatter scan --changed --emit-tests jest src/
```

### `shatter export-tests`

Export generated tests from behavior maps produced by exploration.

```
shatter export-tests [OPTIONS] <TARGETS>...
```

| Flag | Default | Description |
|------|---------|-------------|
| `--framework NAME` | jest | Test framework: jest, vitest, or gotest |
| `--module-path PATH` | `.` | Module path for imports |
| `-o, --output PATH` | stdout | Write output to a file |
| `--max-iterations N` | 100 | Maximum iterations for exploration |
| `--timeout SECS` | 60 | Exploration timeout |
| `--scope PATH` | -- | Scope config YAML file |
| `--request-timeout SECS` | 30 | Per-request frontend timeout |
| `--exec-timeout SECS` | 10 | Per-invocation timeout |
| `--build-timeout SECS` | 30 | Build timeout |
| `--memory-limit MB` | -- | Memory limit for the frontend process |

Run `shatter export-tests --help` for the complete option list.

```bash
shatter export-tests --framework jest -o shipping.test.ts src/shipping.ts:calculateShipping
shatter export-tests --framework gotest pkg/utils.go:Validate
```

### `shatter run`

Discover, analyze, and explore an entire repository in one shot.

```
shatter run [OPTIONS] <PATH>
```

| Flag | Default | Description |
|------|---------|-------------|
| `--output-dir DIR` | -- | Write per-function reports to this directory |
| `--max-iterations N` | 50 | Maximum iterations per function |
| `--timeout SECS` | 300 | Overall timeout |
| `--analyze-only` | -- | Only discover and analyze, skip exploration |
| `--request-timeout SECS` | 30 | Per-request frontend timeout |
| `--exec-timeout SECS` | 10 | Per-invocation timeout |
| `--build-timeout SECS` | 30 | Build timeout |
| `--solver-timeout SECS` | -- | Z3 solver timeout per query |
| `--memory-limit MB` | -- | Memory limit for the frontend process |

Run `shatter run --help` for the complete option list.

```bash
shatter run .
shatter run --analyze-only --output-dir ./reports ./my-project
```

### `shatter diff`

Compare current behaviors against a previous snapshot to detect regressions.

```
shatter diff [OPTIONS] <SNAPSHOT> <CURRENT>
```

| Flag | Default | Description |
|------|---------|-------------|
| `--json` | -- | Output diff as JSON instead of text |

Exit code 0 when all behaviors match, nonzero when regressions are found. Run `shatter diff --help` for the complete option list.

```bash
shatter diff baseline.json current.json
shatter diff --json old.json new.json
```

### `shatter spec-diff`

Compare two function specifications and report behavioral changes.

```
shatter spec-diff [OPTIONS] <OLD> <NEW>
```

| Flag | Default | Description |
|------|---------|-------------|
| `--json` | -- | Output diff as JSON instead of text |

Accepts spec JSON files produced by `explore --spec-json`. Exit code 0 when specs are equivalent, nonzero when regressions are found. Run `shatter spec-diff --help` for the complete option list.

```bash
shatter spec-diff old-spec.json new-spec.json
```

### `shatter build-frontend`

Build a custom frontend binary with user-provided native generators. Reads generator paths from `.shatter/config.yaml`, compiles a custom frontend, and writes it to `.shatter-cache/bin/`.

```
shatter build-frontend [OPTIONS] <LANGUAGE>
```

| Flag | Default | Description |
|------|---------|-------------|
| `<LANGUAGE>` | -- | Target language: `go` or `rust` |
| `--config PATH` | -- | Path to the `.shatter/` directory (auto-discovers if omitted) |
| `-o, --output DIR` | `.shatter-cache/bin/` | Output directory for the built binary |

Run `shatter build-frontend --help` for the complete option list.

```bash
shatter build-frontend go
shatter build-frontend --output ./bin rust
```

### `shatter stale`

Check which functions in a source file are stale relative to a spec file. Analyzes the source, computes fingerprints, and compares against the spec.

```
shatter stale [OPTIONS] <SOURCE> <SPEC>
```

| Flag | Default | Description |
|------|---------|-------------|
| `--format FMT` | text | Output format: `text` or `json` |
| `--cache-dir DIR` | -- | Cache directory for cross-file dependency fingerprints |
| `--no-cache` | -- | Disable cache (skip cross-file dependency tracking) |
| `--request-timeout SECS` | 30 | Per-request frontend timeout |
| `--exec-timeout SECS` | 10 | Per-invocation timeout |
| `--build-timeout SECS` | 30 | Build timeout |
| `--memory-limit MB` | -- | Memory limit for the frontend process |

Exit code 0 = all fresh, 1 = some stale or removed. Run `shatter stale --help` for the complete option list.

```bash
shatter stale src/math.ts math-spec.json
shatter stale --format json src/utils.go utils-spec.json
```

### `shatter revalidate`

Re-execute cached behaviors to detect regressions or drift. Loads behavior maps from the cache, replays each recorded input, and compares observed behavior against the cached expectation.

```
shatter revalidate [OPTIONS] <SOURCE>
```

| Flag | Default | Description |
|------|---------|-------------|
| `--cache-dir DIR` | `.shatter-cache/behavior-maps/` | Cache directory for loading behavior maps |
| `--format FMT` | text | Output format: `text` or `json` |
| `--request-timeout SECS` | 30 | Per-request frontend timeout |
| `--exec-timeout SECS` | 10 | Per-invocation timeout |
| `--build-timeout SECS` | 30 | Build timeout |
| `--memory-limit MB` | -- | Memory limit for the frontend process |

Exit code 0 = no regressions, 1 = issues found. Run `shatter revalidate --help` for the complete option list.

```bash
shatter revalidate src/shipping.ts
shatter revalidate --format json src/utils.go
```

### `shatter test`

Run tests with impact analysis: only execute tests affected by changed files. Uses a coverage map to determine which tests touch which source files.

```
shatter test [OPTIONS]
```

| Flag | Default | Description |
|------|---------|-------------|
| `--all` | -- | Run all tests, bypassing impact analysis |
| `--record` | -- | Force coverage recording to refresh the coverage map |
| `--tier TIER` | -- | Run a specific test tier and write a success marker |
| `--base REF` | HEAD | Base git ref for change detection |
| `--include-untracked` | -- | Include untracked files in change detection |
| `--dry-run` | -- | Show which tests would run without executing them |
| `--prioritize` | -- | Prioritize test execution order by marginal coverage per unit time |
| `--budget DURATION` | -- | Time budget for test execution (e.g., `"10s"`, `"2m"`). Implies `--prioritize` |

Run `shatter test --help` for the complete option list.

```bash
shatter test
shatter test --all
shatter test --dry-run --base main
shatter test --budget 30s
```

## Development

### Prerequisites

- Rust toolchain (via [rustup](https://rustup.rs/))
- Node.js 22+
- Go 1.24+
- libclang (`sudo apt install libclang-dev` on Ubuntu/Debian)
- [beads](https://github.com/steveyegge/beads) (`npm install -g @beads/bd`) — issue tracker

Or use the **devcontainer** — open in VS Code with the Dev Containers extension and everything is pre-configured.

### First-Time Setup

After cloning, initialize the beads issue tracker:

```bash
bd init --prefix str --from-jsonl --quiet
bd import -i .beads/issues.jsonl
bd config set beads.role maintainer
```

This bootstraps the local database from the checked-in issue history and installs git hooks that keep it in sync. The devcontainer does this automatically.

### Project Structure

```
shatter-core/     Rust core engine (library crate)
shatter-cli/      Rust CLI binary
shatter-ts/       TypeScript frontend (Node.js subprocess)
shatter-go/       Go frontend (Go binary subprocess)
shatter-rust/     Rust frontend (stub — protocol handler only)
examples/         Example target functions for testing
demo/             Walkthrough scripts
```

### Build & Test

```bash
# Build
cargo build

# Quick test (during development)
cargo test

# Standard test (before committing)
cargo test && cargo clippy -- -D warnings

# Full test (before merging — includes all frontends)
cargo test && cargo clippy -- -D warnings
cd shatter-ts && npm test && cd ..
cd shatter-go && go test ./... && cd ..
```

### Demo

Run the interactive walkthrough to see Shatter in action:

```bash
./demo/walkthrough.sh          # Interactive (pauses between steps)
./demo/walkthrough.sh --auto   # Continuous
```

## License

Not yet determined.
