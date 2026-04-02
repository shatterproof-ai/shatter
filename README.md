# Shatter

Automatic exploratory testing via concolic execution.

Shatter analyzes functions, discovers branch behavior, and generates inputs that exercise distinct paths without hand-written test cases.

## Who This README Is For

This file is the user-facing entry point.

- Want to try Shatter on a function today? Start with [QUICKSTART.md](QUICKSTART.md).
- Want to know what files and directories Shatter creates? See [docs/PROJECT-LAYOUT.md](docs/PROJECT-LAYOUT.md).
- Want precise command behavior and output contracts? See [SPEC.md](SPEC.md).
- Want to work on Shatter itself? See [CONTRIBUTING.md](CONTRIBUTING.md).

## Current Status

Shatter is under active development. The current frontend status is:

| Language | Status | Notes |
|----------|--------|-------|
| TypeScript | Supported | `shatter-ts` frontend |
| Go | Supported | `shatter-go` frontend |
| Rust | Partial | `shatter-rust` protocol handler exists, but execution support is not complete |

See [SPEC.md](SPEC.md) for the canonical behavior reference.

## Install

The most reliable path today is to build from source.

### Build from source

Requires the [Rust toolchain](https://rustup.rs/), Node.js 22+, Go 1.24+, and `libclang`.

```bash
git clone --recurse-submodules https://github.com/shatterproof-ai/shatter.git
cd shatter
cargo build --release
./target/release/shatter --help
```

If you already cloned without `--recurse-submodules`, initialize submodules manually:

```bash
git submodule update --init
```

### Install a published release

The repository also includes an installer script for GitHub Release assets:

```bash
curl -sSL https://raw.githubusercontent.com/shatterproof-ai/shatter/main/install.sh | bash
```

If release assets are not available for your version or platform, build from source instead.

## Initialize a Project

Installing the `shatter` binary and initializing a project are separate steps.

- Installing Shatter puts the CLI on your machine.
- Initializing a project opts that repository into persistent Shatter state.

Run this from the project root when you want Shatter to keep repo-local state:

```bash
shatter init
```

Today that creates `.shatter/config.yaml` if it does not already exist. Other
commands may also create repo-local cache or artifact directories when using the
initialized project path, including `.shatter-cache/` and `shatter-artifacts/`.

Use initialization when you want durable project-local configuration, cached
results, custom generators, or other repeatable Shatter state in the repo.

## Project Configuration

Place a `shatter.config.json` in your project root to set persistent scan
defaults. All fields are optional — missing fields use built-in defaults. CLI
flags always override config file values.

```json
{
  "include": ["src/**/*.ts"],
  "exclude": ["**/*.test.ts", "node_modules/**"],
  "language": "typescript",
  "max_depth": 5,
  "max_iterations": 200,
  "timeout_total": 600,
  "timeout_per_fn": 60,
  "timeout_explore": 30.0,
  "exec_timeout": 15,
  "parallelism": 4,
  "output": {
    "format": "markdown",
    "paths": ["reports/scan.html", "reports/scan.json"],
    "stdout": true
  },
  "mocks": {
    "db.query": { "return_values": ["{\"id\": 1}"] }
  },
  "cache_dir": ".shatter-cache",
  "no_cache": false,
  "seeds_dir": ".shatter/seeds",
  "capture_side_effects": true,
  "genetic": {
    "enabled": true,
    "population_size": 100
  }
}
```

### Schema Reference

| Field | Type | Description |
|-------|------|-------------|
| `include` | `string[]` | Glob patterns for files to include |
| `exclude` | `string[]` | Glob patterns for files to exclude |
| `language` | `string` | Language filter: `typescript`, `go`, or `rust` |
| `max_depth` | `number` | Maximum directory traversal depth |
| `max_iterations` | `number` | Max iterations per function (default: 100) |
| `timeout_total` | `number` | Total scan timeout in seconds (default: 300) |
| `timeout_per_fn` | `number` | Per-function timeout in seconds (default: 30) |
| `timeout_explore` | `number` | Per-function exploration wall-clock timeout |
| `exec_timeout` | `number` | Function execution timeout in seconds (default: 10) |
| `parallelism` | `number` | Parallel frontend processes (0 = auto) |
| `output.format` | `string` | Stdout format: `markdown`, `json`, `html`, `text` |
| `output.paths` | `string[]` | Report file paths (format inferred from extension) |
| `output.stdout` | `boolean` | Write to stdout alongside output files |
| `mocks` | `object` | Per-symbol mock overrides (`{ "symbol": { "return_values": [...] } }`) |
| `cache_dir` | `string` | Behavior map cache directory |
| `no_cache` | `boolean` | Disable caching entirely |
| `seeds_dir` | `string` | Cross-function seed pool directory |
| `capture_side_effects` | `boolean` | Enable rich side-effect capture |
| `genetic` | `object` | Genetic algorithm settings (`enabled`, `population_size`, etc.) |

### Override Precedence

CLI flags > `shatter.config.json` > `.shatter/config.yaml` > built-in defaults

For list fields (`include`, `exclude`, output `paths`): CLI-provided values
replace the config entirely (they are not appended). For boolean flags
(`--no-cache`, `--capture-side-effects`): passing the flag on the CLI sets the
value to true, overriding the config.

## Quick Start

See [QUICKSTART.md](QUICKSTART.md) for a copy-paste first run.

Minimal example:

```ts
export function calculateShipping(weight: number, country: string): number {
  if (weight <= 0) throw new Error("invalid weight");
  if (country === "US") return weight < 5 ? 5.99 : 12.99;
  return weight < 5 ? 15.99 : 29.99;
}
```

```bash
shatter explore shipping.ts:calculateShipping
```

## Core Commands

Use `shatter --help` and `shatter <command> --help` for the current CLI surface. The commands most users start with are:

- `shatter explore`: analyze one file or function and generate inputs for distinct behaviors
- `shatter scan`: explore a project or directory in dependency order
- `shatter export-tests`: turn discovered behaviors into test files
- `shatter diff` and `shatter spec-diff`: compare current behavior against a saved baseline
- `shatter observe`, `analyze`, `solve`, and `specify`: lower-level pipeline commands for offline and staged workflows

## How Shatter Works

Shatter combines concrete execution with symbolic reasoning:

1. Analyze the target and identify branches and parameter shapes.
2. Execute with concrete inputs while collecting path information.
3. Generate new inputs to reach uncovered behavior.
4. Report observed behaviors, clusters, and optional specs or tests.

## Documentation

- [QUICKSTART.md](QUICKSTART.md): first successful run as a user
- [docs/PROJECT-LAYOUT.md](docs/PROJECT-LAYOUT.md): project directories, config files, caches, and artifacts
- [SPEC.md](SPEC.md): detailed command and output behavior
- [docs/INDEX.md](docs/INDEX.md): documentation map
- [CONTRIBUTING.md](CONTRIBUTING.md): contributor setup and workflow

## License

Not yet determined.
