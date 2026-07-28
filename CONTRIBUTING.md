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
  quality gates (`task test-quick`, `task check`, `task parity`, etc.). Install
  the `go-task` package or run
  `sh -c "$(curl -ssL https://taskfile.dev/install.sh)"`.
- Python 3 with the `pyyaml` and `jsonschema` packages — used by repo tooling
  under `scripts/`, `protocol/`, and the `task parity` / `task conformance` /
  schema-validation gates. Install with `pip install pyyaml jsonschema` (or your
  distro's `python3-yaml` and `python3-jsonschema` packages). The schema and
  parity tasks skip with a warning when these imports are missing, so a clone
  without them silently runs fewer gates.
- [beads](https://github.com/steveyegge/beads): `npm install -g @beads/bd`

On Ubuntu/Debian the system packages can be installed together:

```bash
sudo apt install libclang-dev libz3-dev
pip install pyyaml jsonschema
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

The raw `cargo` / `npm` / `go` commands below build and test the individual
crates and frontends directly. For day-to-day development the **Taskfile targets
are canonical** — `task test-quick`, `task test-standard`, `task check`, and the
parity/conformance/E2E gates in [CLAUDE.md](CLAUDE.md) wrap these commands plus
the cross-language gates, and are what CI runs. Use the raw commands when you
want to drive a single crate; use `task ...` to reproduce the gates.

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

## Documentation examples smoke gate

`task docs-smoke` (part of `task check`) extracts fenced code blocks from the
user-facing docs — `README.md`, `QUICKSTART.md`, `SPEC.md`, and `docs/INDEX.md`
(configured in `scripts/docs-smoke.yaml`) — and validates them against the
**built CLI**:

- **Shell blocks** (` ```bash ` / `sh` / `shell` / `console`): every
  `shatter ...` invocation is checked against the real command surface. An
  unknown subcommand or flag fails the gate — this is what catches a
  reintroduced stale flag such as `shatter explore --timeout` (the real flag is
  `--timeout-explore`).
- **`json` / `yaml` blocks**: parsed for syntax; invalid snippets fail.
- **`smoke_commands`** from the config are executed in a throwaway temp
  directory to prove the surface is live.

This is a maintained allowlist, not blind execution of every block. Blocks in
other languages (`ts`, `markdown`, untagged) are not validated.

**Marking a non-runnable example.** When a block is intentionally illustrative
(pseudo-JSON, an NDJSON stream, sample output, a planned-but-unimplemented
command), exempt it with a directive on the line **immediately above** the
opening fence. A reason is **required** — a bare `skip` fails the gate:

```text
<!-- docs-smoke: skip reason="NDJSON stream — one object per line, not a single JSON document" -->
​```json
{"a":1}
{"a":2}
​```
```

Prefer fixing a stale example over exempting it. Reserve `skip` for blocks that
genuinely cannot be validated, and keep the reason specific.

## Contributor Notes

- Keep user-facing documentation in user-facing files. Do not put beads workflow or internal contributor setup into `README.md`.
- Treat [SPEC.md](SPEC.md) as the reference for current observable behavior.
- Treat [PLAN.md](PLAN.md) as roadmap material, not a statement of what already works.
