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

### Adversarial String Generator

The `wasm-adversarial/` crate provides ~70 adversarial/boundary string payloads
across 8 categories for security testing:

| Category | Description |
|---|---|
| `sql_injection` | SQL injection probes (UNION, blind, comment-based) |
| `xss` | Cross-site scripting vectors (script, event handlers, protocol) |
| `unicode_edge` | BOM, zero-width, RTL override, combining marks, surrogates |
| `null_bytes` | Embedded nulls, URL-encoded `%00`, C-string terminators |
| `extreme_length` | Empty through 100K characters (built at runtime) |
| `format_string` | printf `%n`, template literals `${...}`, SSTI `{{...}}` |
| `path_traversal` | `../` sequences, double-encoding, null-byte bypass |
| `encoding` | URL-encoded, HTML entities, Unicode escapes, overlong UTF-8 |

**Exported functions:**
- `adversarial` — for `param_generators` config entries
- `AdversarialString` — for `generators` (type-name) config entries

**Recipe format:**
- Empty input: round-robin through all payloads
- `{"category": "xss"}` — round-robin within XSS category
- `{"category": "xss", "index": 2}` — specific XSS payload

**Building:**

```bash
cd examples/generators/wasm-adversarial
cargo build --target wasm32-wasip1 --release
cp target/wasm32-wasip1/release/adversarial_generators.wasm ./adversarial.wasm
```

A prebuilt `adversarial.wasm` is checked in so you don't need the Rust WASM toolchain.

**Config usage:**

```yaml
defaults:
  param_generators:
    adversarial: path/to/adversarial.wasm
  generators:
    AdversarialString: path/to/adversarial.wasm
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
