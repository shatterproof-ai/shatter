use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let ts_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("..")
        .join("shatter-ts");

    let bundle_src = ts_dir.join("dist").join("bundle.js");

    // Re-run if any TS source files change
    println!("cargo:rerun-if-changed={}", ts_dir.join("src").display());
    println!("cargo:rerun-if-changed={}", ts_dir.join("package.json").display());

    // Install deps if node_modules missing
    if !ts_dir.join("node_modules").exists() {
        run_npm(&ts_dir, &["install", "--silent"]);
    }

    // Always run bundle (esbuild is fast, ~500ms)
    run_npm(&ts_dir, &["run", "bundle", "--silent"]);

    assert!(
        bundle_src.exists(),
        "esbuild bundle not found at {}",
        bundle_src.display()
    );

    // Compute hash for cache-busting at runtime
    let bundle_bytes = std::fs::read(&bundle_src).expect("failed to read bundle.js");
    let hash = sha256_hex(&bundle_bytes);

    // Copy bundle into OUT_DIR so include_bytes! can reference it
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let out_bundle = out_dir.join("frontend-bundle.js");
    std::fs::copy(&bundle_src, &out_bundle).expect("failed to copy bundle to OUT_DIR");

    println!("cargo:rustc-env=FRONTEND_BUNDLE_HASH={hash}");
}

fn run_npm(dir: &Path, args: &[&str]) {
    let status = Command::new("npm")
        .args(args)
        .current_dir(dir)
        .status()
        .expect("failed to run npm — is Node.js installed?");

    assert!(status.success(), "npm {} failed", args.join(" "));
}

/// Simple SHA-256 without external crate (uses the sha256sum command).
fn sha256_hex(data: &[u8]) -> String {
    use std::io::Write;
    use std::process::Stdio;

    let mut child = Command::new("sha256sum")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to run sha256sum");

    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(data)
        .expect("failed to write to sha256sum");

    let output = child.wait_with_output().expect("sha256sum failed");
    let stdout = String::from_utf8(output.stdout).expect("invalid utf8 from sha256sum");

    // sha256sum output: "<hash>  -\n"
    stdout.split_whitespace().next().unwrap().to_string()
}
