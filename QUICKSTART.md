# Quickstart

This guide is for someone using Shatter, not developing Shatter itself.

If you want repo setup, beads, or contributor workflow, see [CONTRIBUTING.md](CONTRIBUTING.md).

## 1. Install Shatter

### Latest continuous binary

This is the normal path for trying Shatter or adding it to a project quickly:

```bash
curl -sSL https://raw.githubusercontent.com/shatterproof-ai/shatter/main/install.sh | bash
shatter --version
```

Shatter's published binaries are continuous development builds, not semver
releases. Build tags look like `continuous-20260512-1735-abc123def456` and are
intended to be easy to find, install, and pin.

For repeatable CI, pin one exact build tag:

```bash
curl -sSL https://raw.githubusercontent.com/shatterproof-ai/shatter/main/install.sh | BUILD=continuous-20260512-1735-abc123def456 bash
shatter scan src/
```

Use the unpinned latest build for local use or non-blocking scheduled jobs; use
an exact `BUILD` value for required CI gates.

If your repository already has a root package manager or CI setup, install
Shatter through that owner instead of every language subproject. See
[docs/distribution.md](docs/distribution.md) for npm tarball, Go tool wrapper,
GitHub Action, and update-bot examples.

### Build from source

Use this when you are developing Shatter itself or need a local source build.

```bash
git clone https://github.com/shatterproof-ai/shatter.git
cd shatter
cargo build --release
./target/release/shatter --version
```

Prerequisites:

- Rust toolchain
- Node.js 22+
- Go 1.24+
- `libclang`
- Z3 (the development package — `shatter-core` links the system Z3 library, so
  `cargo build` fails without it)

On Ubuntu/Debian: `sudo apt install libclang-dev libz3-dev`. On macOS:
`brew install llvm z3`. Running the quality gates also needs `go-task` and
Python 3 with `pyyaml` + `jsonschema` (`pip install pyyaml jsonschema`); see
[CONTRIBUTING.md](CONTRIBUTING.md) for the full contributor toolchain.

## 2. Explore One Function

Create a small TypeScript file:

```bash
cat > shipping.ts <<'EOF'
export function calculateShipping(weight: number, country: string): number {
  if (weight <= 0) throw new Error("invalid weight");
  if (country === "US") return weight < 5 ? 5.99 : 12.99;
  return weight < 5 ? 15.99 : 29.99;
}
EOF
```

Run Shatter:

Commands that execute target functions (`explore`, `scan`, `run`, `observe`,
`properties`, `revalidate`, `bench`) refuse to run unsandboxed unless you opt
in — Shatter calls your code with generated inputs, and an unconfined target
that writes a relative-path file can leave stray files in your working
directory. Pass `--allow-host-writes` (each unsandboxed run then executes in a
throwaway directory that's deleted afterward); or, if you have Docker or
bubblewrap available, `export SHATTER_SANDBOX_BACKEND=docker` once per shell
instead. See [Executing Target Functions Safely](README.md#executing-target-functions-safely)
in README.md for the full policy.

```bash
./target/release/shatter explore shipping.ts:calculateShipping --allow-host-writes
```

Or, if `shatter` is already on your `PATH`:

```bash
shatter explore shipping.ts:calculateShipping --allow-host-writes
```

What to expect:

- Shatter analyzes the function signature and branches.
- It executes the function with generated inputs.
- It reports distinct observed behaviors and example inputs that reach them.

## 3. Initialize a Project for Persistent Shatter State

If you want repo-local Shatter configuration and reusable state, initialize the
project explicitly:

```bash
shatter init
```

This is the installed-project path. It opts the repository into persistent
Shatter state under paths such as:

- `.shatter/config.yaml` for project configuration
- `.shatter-cache/` for caches
- `shatter-artifacts/` for preserved outputs such as recorded mocks and reports

Use this when you want durable project-local settings, repeatable runs, or
other Shatter-managed state to live alongside the code.

## 4. Save a Behavioral Spec

Generate a spec you can diff later:

```bash
shatter explore --concolic --spec shipping.ts:calculateShipping --allow-host-writes
```

Write JSON instead:

```bash
shatter explore --concolic --spec-json --spec-out shipping-spec.json shipping.ts:calculateShipping --allow-host-writes
```

## 5. Scan More Than One File

For a handful of specific files, pass a quoted glob to `explore` (or
`properties`) — it expands against the filesystem and filters to supported
source extensions:

```bash
shatter explore 'src/**/*.ts' --allow-host-writes
```

For repository-wide discovery, point `shatter scan` at a directory and narrow
the file set with `--include` / `--exclude` (repeatable glob patterns):

```bash
shatter scan src/ --allow-host-writes
shatter scan --include '**/*.ts' --exclude '**/vendor/**' src/ --allow-host-writes
```

Useful follow-ons:

```bash
shatter scan --changed src/ --allow-host-writes
shatter scan --language rust crates/my-crate/src/ --allow-host-writes
shatter diff snapshots/shipping.json current/shipping.json
```

(`diff` is analysis-only and does not execute target functions, so it needs
neither flag.)

Single-target commands (`observe`, `revalidate`, `stale`) require a concrete
`<file>` or `<file>:<function>` and reject wildcard inputs.

## 6. Know Where To Look Next

- `shatter --help`: top-level command list
- `shatter explore --help`: current `explore` flags
- [docs/PROJECT-LAYOUT.md](docs/PROJECT-LAYOUT.md): directories, config files, caches, and generated artifacts
- [SPEC.md](SPEC.md): command behavior and output contract
- [README.md](README.md): high-level overview

## Notes

- File targets use `<file>:<function>` for one function or `<file>` for all exported functions in that file.
- Initializing a project is separate from installing the `shatter` binary.
- TypeScript, Go, and Rust are the current user-facing frontends.
- Rust scans require a reachable `shatter-rust` frontend. From a source checkout, build it with `cargo build --manifest-path shatter-rust/Cargo.toml`; pass `--require-rust` when a missing Rust frontend should fail the command instead of skipping `.rs` files.
