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
