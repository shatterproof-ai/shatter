# Quickstart

This guide is for someone using Shatter, not developing Shatter itself.

If you want repo setup, beads, or contributor workflow, see [CONTRIBUTING.md](CONTRIBUTING.md).

## 1. Install Shatter

### Build from source

This is the most reliable path on the current repository state.

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

### Optional release installer

If you want a published release build instead of a local source build:

```bash
curl -sSL https://raw.githubusercontent.com/shatterproof-ai/shatter/main/install.sh | bash
```

If no release artifact exists for your platform or desired version, fall back to the source build above.

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

```bash
./target/release/shatter explore shipping.ts:calculateShipping
```

Or, if `shatter` is already on your `PATH`:

```bash
shatter explore shipping.ts:calculateShipping
```

What to expect:

- Shatter analyzes the function signature and branches.
- It executes the function with generated inputs.
- It reports distinct observed behaviors and example inputs that reach them.

## 3. Save a Behavioral Spec

Generate a spec you can diff later:

```bash
shatter explore --concolic --spec shipping.ts:calculateShipping
```

Write JSON instead:

```bash
shatter explore --concolic --spec-json --spec-out shipping-spec.json shipping.ts:calculateShipping
```

## 4. Scan More Than One File

Once the single-function flow works, move up to a directory:

```bash
shatter scan src/
```

Useful follow-ons:

```bash
shatter scan --changed src/
shatter export-tests --framework jest shipping.ts:calculateShipping
```

## 5. Know Where To Look Next

- `shatter --help`: top-level command list
- `shatter explore --help`: current `explore` flags
- [docs/PROJECT-LAYOUT.md](docs/PROJECT-LAYOUT.md): directories, config files, caches, and generated artifacts
- [SPEC.md](SPEC.md): command behavior and output contract
- [README.md](README.md): high-level overview

## Notes

- File targets use `<file>:<function>` for one function or `<file>` for all exported functions in that file.
- TypeScript and Go are the current primary user-facing frontends.
- Rust support is not yet complete for normal end-user exploration workflows.
