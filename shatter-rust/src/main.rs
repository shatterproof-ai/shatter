mod analyzer;
mod generators;
mod handler;
mod instrument;
mod protocol;
mod wasm_generator;

use std::io;

fn main() {
    let handler = handler::Handler::new(
        io::stdin().lock(),
        io::stdout().lock(),
        io::stderr(),
    );
    if let Err(e) = handler.run() {
        eprintln!("[shatter-rust] Fatal: {e}");
        std::process::exit(1);
    }
}
