# Shatter Generator Examples

This directory contains example generators for use with `shatter explore`.

## WASM Generators (Layer 1)

WASM generators work with any frontend (TypeScript, Go, Rust). They produce
JSON values and run sandboxed via Extism.

### Building the Rust WASM example

```bash
cd examples/generators/wasm-rust
cargo build --target wasm32-wasip1 --release
cp target/wasm32-wasip1/release/example_generators.wasm ../../../.shatter/generators/
```

### Config usage

```yaml
# .shatter/config.yaml
defaults:
  generators:
    DbConfig: ./generators/db_config.wasm
  param_generators:
    authToken: ./generators/token.wasm
```

## Native Generators (Layer 2 — Custom Build)

Native generators are compiled into a custom frontend binary using
`shatter build-frontend`. They can return live, non-serializable objects
(DB connections, file handles, etc.).

### Go example

```bash
# Place generator files in .shatter/generators/
# Then build:
shatter build-frontend go
```

### Rust example

```bash
shatter build-frontend rust
```

See `native-go/` and `native-rust/` for example generator source files.
