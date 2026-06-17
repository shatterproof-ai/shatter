# Contributing

This guide is for people working on Shatter itself.

If you want to use Shatter on your own code, start with [README.md](README.md) and [QUICKSTART.md](QUICKSTART.md).

## Read These First

- [AGENTS.md](AGENTS.md): issue tracking, branch workflow, and session completion rules
- [CLAUDE.md](CLAUDE.md): code quality standards, test tiers, and completion checklists
- [docs/INDEX.md](docs/INDEX.md): documentation map

## Prerequisites

- Rust toolchain via [rustup](https://rustup.rs/)
- Node.js 22+
- Go 1.24+
- `libclang` (`sudo apt install libclang-dev` on Ubuntu/Debian)
- Z3 (`sudo apt install libz3-dev` on Ubuntu/Debian; `brew install z3` on
  macOS). `shatter-core` links the system Z3 library, so without the Z3
  development package `cargo build` fails with a linker/bindgen error.
- [go-task](https://taskfile.dev/installation/) — the task runner used for all
  quality gates (`task test-quick`, `task check`, `task parity`, etc.)
- Python 3 — used by repo tooling under `scripts/` (parity validation,
  protocol codegen, release packaging, and so on)
- [beads](https://github.com/steveyegge/beads): `npm install -g @beads/bd`

On Ubuntu/Debian the system packages can be installed together:

```bash
sudo apt install libclang-dev libz3-dev
```

Or use the devcontainer in VS Code if you want a preconfigured environment.

## First-Time Setup

After cloning, initialize the local beads database:

```bash
bd init --prefix str --from-jsonl --quiet
bd import -i .beads/issues.jsonl
bd config set beads.role maintainer
```

The devcontainer performs this setup automatically.

When you need the shared example corpus, the repo's tasks and demo scripts fetch
`https://github.com/shatterproof-ai/examples` into `/tmp` automatically. Set
`SHATTER_EXAMPLES_DIR` if you want to point tests or demos at an existing
checkout instead of the default `/tmp/shatter-examples-main` cache.

## Project Structure

```text
shatter-core/     Rust core engine
shatter-cli/      Rust CLI binary
shatter-ts/       TypeScript frontend
shatter-go/       Go frontend
shatter-rust/     Rust frontend
demo/             Walkthrough scripts
docs/             Design notes, glossary, CI, plans
```

## Build And Test

```bash
# Build
cargo build

# Rust tests
cargo test

# Rust quality gate
cargo clippy -- -D warnings

# TypeScript frontend
cd shatter-ts
npm test
cd ..

# Go frontend
cd shatter-go
go test ./...
cd ..
```

See [CLAUDE.md](CLAUDE.md) and crate-specific `CLAUDE.md` files for the current quality-gate expectations.

## Demo

Run the gauntlet (broad CLI coverage):

```bash
./demo/gauntlet.sh
./demo/gauntlet.sh --auto
```

## Contributor Notes

- Keep user-facing documentation in user-facing files. Do not put beads workflow or internal contributor setup into `README.md`.
- Treat [SPEC.md](SPEC.md) as the reference for current observable behavior.
- Treat [PLAN.md](PLAN.md) as roadmap material, not a statement of what already works.
