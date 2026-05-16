# Shatter

Automatic exploratory testing via concolic execution.

Shatter analyzes functions, discovers branch behavior, and generates inputs that exercise distinct paths without hand-written test cases.

## Who This README Is For

This file is the user-facing entry point.

- Want to try Shatter on a function today? Start with [QUICKSTART.md](QUICKSTART.md).
- Want to know what files and directories Shatter creates? See [docs/PROJECT-LAYOUT.md](docs/PROJECT-LAYOUT.md).
- Want to handle database clients, LSP clients, HTTP contexts, or other live resources? See [docs/resource-parameters.md](docs/resource-parameters.md).
- Want precise command behavior and output contracts? See [SPEC.md](SPEC.md).
- Want to work on Shatter itself? See [CONTRIBUTING.md](CONTRIBUTING.md).

## Current Status

Shatter is under active development. The current frontend status is:

| Language | Status | Notes |
|----------|--------|-------|
| TypeScript | Supported | `shatter-ts` frontend |
| Go | Supported | `shatter-go` frontend |
| Rust | Supported | `shatter-rust` frontend for `.rs` targets; see SPEC for tracked parity gaps |

See [SPEC.md](SPEC.md) for the canonical behavior reference.

## Install

### Install the latest continuous binary

Shatter publishes continuously built GitHub Release archives for common Linux
and macOS platforms. These releases are named like
`continuous-20260512-1735-abc123def456`: the name identifies exactly what was
built, but it does not imply semver compatibility or long-term API stability.

```bash
curl -sSL https://raw.githubusercontent.com/shatterproof-ai/shatter/main/install.sh | bash
```

For repeatable CI, pin the exact build tag:

```bash
curl -sSL https://raw.githubusercontent.com/shatterproof-ai/shatter/main/install.sh | BUILD=continuous-20260512-1735-abc123def456 bash
```

The installer reads the release manifest, verifies the archive checksum, and
installs `shatter` into `~/.local/bin` by default. Set `INSTALL_DIR` to choose a
different destination.

Projects can also consume the same GitHub Release payloads through a
registryless npm tarball dependency, a Go tool wrapper, or the GitHub setup
action. Pick one repo-level owner rather than adding Shatter to every language
manifest in a mixed repo. See [docs/distribution.md](docs/distribution.md).

### Build from source

Requires the [Rust toolchain](https://rustup.rs/), Node.js 22+, Go 1.24+, and `libclang`.

```bash
git clone https://github.com/shatterproof-ai/shatter.git
cd shatter
cargo build --release
./target/release/shatter --help
```

Shatter's demo, smoke, and E2E flows fetch the separate
`https://github.com/shatterproof-ai/examples` repository into `/tmp` on demand.
Set `SHATTER_EXAMPLES_DIR` if you want those flows to use an existing checkout
instead of the default `/tmp/shatter-examples-main` cache.

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

Shatter uses two configuration files with distinct responsibilities:

| File | Scope | Controls |
|------|-------|----------|
| `shatter.config.json` | Project root | Scan-global: file discovery, output, caching, resource limits |
| `.shatter/config.yaml` | Hierarchical (any directory level) | Per-function: iterations, timeouts, mocks, genetic, generators, setup, opaque types, execution profiles |

### `shatter.config.json` — scan-global defaults

Place this in your project root. All fields are optional — missing fields use
built-in defaults. CLI flags always override config file values.

```json
{
  "include": ["src/**/*.ts"],
  "exclude": ["**/*.test.ts", "node_modules/**"],
  "language": "typescript",
  "max_depth": 5,
  "timeout_total": 600,
  "exec_timeout": 15,
  "parallelism": 4,
  "output": {
    "format": "markdown",
    "paths": ["reports/scan.html", "reports/scan.json"],
    "stdout": true
  },
  "cache_dir": ".shatter-cache",
  "no_cache": false,
  "seeds_dir": ".shatter/seeds",
  "capture_side_effects": true
}
```

| Field | Type | Description |
|-------|------|-------------|
| `include` | `string[]` | Glob patterns for files to include |
| `exclude` | `string[]` | Glob patterns for files to exclude |
| `language` | `string` | Language filter: `typescript`, `go`, or `rust` |
| `max_depth` | `number` | Maximum directory traversal depth |
| `timeout_total` | `number` | Total scan timeout in seconds (default: 300) |
| `exec_timeout` | `number` | Function execution timeout in seconds (default: 10) |
| `parallelism` | `number` | Parallel frontend processes (0 = auto) |
| `output.format` | `string` | Stdout format: `markdown`, `json`, `html`, `text` |
| `output.paths` | `string[]` | Report file paths (format inferred from extension) |
| `output.stdout` | `boolean` | Write to stdout alongside output files |
| `cache_dir` | `string` | Behavior map cache directory |
| `no_cache` | `boolean` | Disable caching entirely |
| `seeds_dir` | `string` | Cross-function seed pool directory |
| `capture_side_effects` | `boolean` | Enable rich side-effect capture |

### `.shatter/config.yaml` — hierarchical per-function settings

Created by `shatter init`. Can be placed at multiple levels in the project tree;
the nearest config to each target file wins on conflicts. Per-function settings
like iteration limits, timeouts, mocks, genetic algorithm config, generators,
setup files, opaque type declarations, and execution profiles belong here. See
[docs/PROJECT-LAYOUT.md](docs/PROJECT-LAYOUT.md) for the full schema, and
[docs/resource-parameters.md](docs/resource-parameters.md) for guidance on live
resource parameters.

### Override Precedence

```
CLI flags > --set overrides > .shatter/config.yaml (nearest first) > shatter.config.json > built-in defaults
```

The two files do not overlap: `shatter.config.json` owns discovery/output/cache
settings, while `.shatter/config.yaml` owns per-function analysis behavior.
CLI flags override both.

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

`explore` and `properties` also accept quoted glob patterns over file paths,
which are expanded against the filesystem and filtered to supported source
extensions:

```bash
shatter explore 'src/**/*.ts'
```

For repository-wide discovery, point `shatter scan` at a directory and narrow
the file set with `--include` / `--exclude`:

```bash
shatter scan --include '**/*.ts' --exclude '**/vendor/**' src/
```

Single-target commands (`observe`, `revalidate`, `stale`) require a concrete
`<file>` or `<file>:<function>` and reject wildcard inputs with an actionable
error — use `explore`/`properties` for glob targets and `scan --include` for
repository discovery.

## Core Commands

Use `shatter --help` and `shatter <command> --help` for the current CLI surface. The commands most users start with are:

- `shatter explore`: analyze one file or function and generate inputs for distinct behaviors
- `shatter scan`: explore a project or directory in dependency order
- `shatter diff` and `shatter spec-diff`: compare current behavior against a saved baseline
- `shatter observe`, `analyze`, `solve`, and `specify`: lower-level pipeline commands for offline and staged workflows

## Live Output and Resume

When running `shatter scan`, you can track progress in real time with
`--progress`, which emits structured NDJSON events to stderr as each function
starts, completes, skips, or fails.

If a scan is interrupted (Ctrl-C, timeout, crash), partial results are
preserved automatically. Use `--resume auto` on the next run to skip
already-completed functions and continue from where the scan left off.

```bash
# First run — interrupted
shatter scan --resume auto --progress src/
^C

# Second run — picks up where it left off
shatter scan --resume auto --progress src/
```

See [SPEC.md, Section 6](SPEC.md#6-live-output-and-resume) for the full
progress event format, partial artifact layout, checkpoint structure, and
resume semantics.

## How Shatter Works

Shatter combines concrete execution with symbolic reasoning:

1. Analyze the target and identify branches and parameter shapes.
2. Execute with concrete inputs while collecting path information.
3. Generate new inputs to reach uncovered behavior.
4. Report observed behaviors, clusters, and optional specs or tests.

## Documentation

- [QUICKSTART.md](QUICKSTART.md): first successful run as a user
- [docs/PROJECT-LAYOUT.md](docs/PROJECT-LAYOUT.md): project directories, config files, caches, and artifacts
- [docs/resource-parameters.md](docs/resource-parameters.md): setup, generators, opaque types, and adapters for live resource parameters
- [SPEC.md](SPEC.md): detailed command and output behavior
- [docs/INDEX.md](docs/INDEX.md): documentation map
- [CONTRIBUTING.md](CONTRIBUTING.md): contributor setup and workflow

## License

Not yet determined.
