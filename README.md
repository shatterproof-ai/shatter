# Shatter

Automatic exploratory testing via concolic execution. Shatter analyzes your functions, discovers branches, and generates inputs that exercise every code path — no test authoring required.

## Installation

**Quick install** (Linux/macOS):

```bash
curl -sSL https://raw.githubusercontent.com/user/shatter/main/install.sh | bash
```

**Cargo install**:

```bash
cargo install --git https://github.com/user/shatter shatter-cli
```

**Build from source**:

```bash
git clone https://github.com/user/shatter.git
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

## Supported Languages

| Language   | Status |
|------------|--------|
| TypeScript | Supported (via Node.js frontend) |
| Go         | Supported (via Go frontend) |

## CLI Reference

All commands accept targets in `<file>:<function>` format (e.g., `src/math.ts:add`) or `<file>` to target all functions. File extension determines the language frontend (`.ts` = TypeScript, `.go` = Go).

**Global options:** `--log-level <LEVEL>` (error/warn/info/debug/trace), `-v` (debug), `-vv` (trace), `-q` (quiet), `--perf` (show timing stats).

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
| `--analyze-only` | -- | Only analyze branches, skip exploration |
| `--show-clusters` | -- | Display behavior clusters in output |
| `--scope PATH` | -- | Path to a `shatter.scope.yaml` file |
| `--cache-dir DIR` | `.shatter/cache/` | Directory for caching behavior maps |
| `--no-cache` | -- | Disable behavior map caching |
| `--request-timeout SECS` | 30 | Per-request timeout for frontend responses |
| `--exec-timeout SECS` | 10 | Per-invocation timeout in the frontend |
| `--build-timeout SECS` | 30 | Timeout for compiling instrumented code |
| `--inputs PATH` | -- | Path to a candidate inputs JSON file |
| `--config PATH` | -- | Path to `.shatter/config.yaml` |
| `--spec` | -- | Output a behavioral specification (markdown) |
| `--spec-json` | -- | Output the behavioral specification as JSON |
| `--no-boundary-values` | -- | Disable built-in boundary values as seeds |
| `--invariants` | -- | Enable Daikon-style invariant detection |

```bash
shatter explore src/shipping.ts:calculateShipping
shatter explore --analyze-only src/shipping.ts
shatter explore --spec --invariants src/math.ts:add
```

### `shatter scan`

Scan multiple functions in dependency order, using behavior maps as mocks.

```
shatter scan [OPTIONS] <TARGETS>...
```

| Flag | Default | Description |
|------|---------|-------------|
| `--max-iterations N` | 100 | Maximum iterations per function |
| `--timeout SECS` | 120 | Timeout for the entire scan |
| `--analyze-only` | -- | Only analyze, skip exploration |
| `--scope PATH` | -- | Path to a scope config YAML file |
| `--cache-dir DIR` | `.shatter/cache/` | Behavior map cache directory |
| `--no-cache` | -- | Disable caching |
| `--request-timeout SECS` | 30 | Per-request frontend timeout |
| `--exec-timeout SECS` | 10 | Per-invocation timeout |
| `--build-timeout SECS` | 30 | Build timeout |
| `--parallelism N` | auto | Number of parallel frontend subprocesses |
| `--timeout-per-fn SECS` | 30 | Per-function timeout (skip on exceed) |
| `--output-dir DIR` | `./shatter-report/` | Write reports to this directory |
| `--report FORMAT` | json | Report format: json, markdown, or both |
| `--progress-json` | -- | Emit structured JSON progress events |
| `--emit-tests FRAMEWORK` | -- | Generate tests: jest, vitest, or gotest |
| `--emit-tests-dir DIR` | -- | Output directory for emitted test files |

```bash
shatter scan src/shipping.ts src/utils.ts
shatter scan --report both --output-dir ./reports src/
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

Exit code 0 when all behaviors match, nonzero when regressions are found.

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

Accepts spec JSON files produced by `explore --spec-json`. Exit code 0 when specs are equivalent, nonzero when regressions are found.

```bash
shatter spec-diff old-spec.json new-spec.json
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

TBD
