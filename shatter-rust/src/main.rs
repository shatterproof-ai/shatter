// Binary re-exports the lib modules; many public APIs are only exercised via
// the library crate's tests, not from `main()` directly.
#![allow(dead_code)]

mod analyzer;
mod executor;
mod generators;
mod handler;
mod instrument;
mod protocol;
mod setup;
mod timing;
mod wasm_generator;

use std::io;

/// Process-global mutex used by tests that mutate environment variables.
/// See lib.rs for the canonical declaration; this mirrors it for the bin test binary.
#[cfg(test)]
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn main() {
    let handler = handler::Handler::new(io::stdin().lock(), io::stdout().lock(), io::stderr());
    if let Err(e) = handler.run() {
        eprintln!("[shatter-rust] Fatal: {e}");
        std::process::exit(1);
    }
}
