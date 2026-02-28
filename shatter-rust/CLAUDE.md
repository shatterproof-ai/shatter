# shatter-rust

Rust language frontend for Shatter. Standalone binary implementing the JSON-over-stdio protocol.

## Architecture

- `src/main.rs` — Entry point: creates Handler, calls `run()`, prints fatal errors to stderr
- `src/protocol.rs` — Protocol types (Request/Response) matching the JSON wire format
- `src/handler.rs` — Protocol handler: read lines, parse JSON, dispatch, write response

## Building & Testing

```bash
cargo test              # Run all tests
cargo clippy -- -D warnings  # Lint
cargo run               # Start the frontend (reads NDJSON from stdin)
```

## Protocol

This crate implements the same NDJSON protocol as `shatter-go/`. It does **not** depend on `shatter-core` — it defines its own protocol types that produce compatible JSON.

Commands: `handshake`, `analyze`, `instrument`, `execute`, `setup`, `teardown`, `generate`, `shutdown`
