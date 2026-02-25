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

### `shatter explore`

Explore functions to discover branches and generate test inputs.

```
shatter explore [OPTIONS] <TARGETS>...
```

**Arguments:**

- `<TARGETS>` — Functions to explore, as `file:function` or `file` (all functions)
  - Examples: `src/math.ts:add`, `src/math.ts`, `pkg/utils.go:Validate`

**Options:**

| Flag | Default | Description |
|------|---------|-------------|
| `--max-iterations N` | 100 | Maximum exploration iterations per function |
| `--timeout SECS` | 60 | Timeout in seconds per function |
| `--analyze-only` | — | Only analyze branches, skip exploration |
| `--show-clusters` | — | Display behavior clusters in output |

**Examples:**

```bash
# Explore a single function
shatter explore src/shipping.ts:calculateShipping

# Analyze branches without exploring
shatter explore --analyze-only src/shipping.ts

# Explore with custom limits
shatter explore --max-iterations 200 --timeout 120 src/utils.go:ParseConfig
```

## Development

### Prerequisites

- Rust toolchain (via [rustup](https://rustup.rs/))
- Node.js 22+
- Go 1.24+
- libclang (`sudo apt install libclang-dev` on Ubuntu/Debian)

Or use the **devcontainer** — open in VS Code with the Dev Containers extension and everything is pre-configured.

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
