// NoTargetReason::BuildScript — a Cargo `build.rs` script. The Rust
// frontend skips these.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
}
