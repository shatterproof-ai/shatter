//! Installation test: verifies the bundled TS frontend can be invoked end-to-end.
//!
//! After `cargo build -p shatter-cli`, the `shatter` binary embeds the esbuild-bundled
//! TypeScript frontend. This test proves that the bundle extracts correctly and Node.js
//! can execute it — no separate `npm install` required.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

/// Send a JSON-over-stdio handshake to the TS frontend bundle and verify it responds
/// with a valid handshake containing "typescript" as the language.
#[test]
fn bundled_ts_frontend_responds_to_handshake() {
    // Build the CLI binary (which embeds the TS bundle)
    let binary = env!("CARGO_BIN_EXE_shatter");

    // The CLI doesn't expose the frontend directly, so we replicate what it does:
    // extract the bundle and spawn node against it.
    //
    // We use the cache directory that build.rs + embedded_frontend.rs populate.
    // The bundle is extracted on first use. Since `cargo test` compiles the binary
    // (triggering build.rs → esbuild → embed), the bundle is baked in.
    //
    // Instead of reaching into private modules, we test via the CLI itself:
    // `shatter explore --help` proves the binary loads, but for a true installation
    // test we need to verify the TS frontend subprocess works.
    //
    // Strategy: find the bundle in the cache dir and invoke it directly with node.
    let cache_dir = cache_dir();
    let bundle = find_bundle(&cache_dir);

    // First, verify the CLI binary itself is functional
    let cli_output = Command::new(binary)
        .env("SHATTER_ALLOW_HOST_WRITES", "1") // str-gg9v: opt into unsandboxed host execution
        .arg("--version")
        .output()
        .expect("failed to run shatter --version");
    assert!(
        cli_output.status.success(),
        "shatter --version failed: {}",
        String::from_utf8_lossy(&cli_output.stderr)
    );

    // Now test the TS frontend directly: spawn node with the bundle and do a handshake
    let mut child = Command::new("node")
        .args(["--no-warnings", &bundle])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn node — is Node.js installed?");

    let stdin = child.stdin.as_mut().expect("failed to open stdin");
    let stdout = child.stdout.take().expect("failed to open stdout");
    let mut reader = BufReader::new(stdout);

    // Send handshake request
    let handshake = serde_json::json!({
        "protocol_version": "0.1.0",
        "id": 1,
        "command": "handshake",
        "capabilities": ["analyze", "execute", "instrument"]
    });
    writeln!(stdin, "{}", handshake).expect("failed to write handshake");

    // Read handshake response
    let mut response_line = String::new();
    reader
        .read_line(&mut response_line)
        .expect("failed to read handshake response");

    let response: serde_json::Value =
        serde_json::from_str(&response_line).expect("handshake response is not valid JSON");

    assert_eq!(response["status"], "handshake", "expected handshake status");
    assert_eq!(
        response["language"], "typescript",
        "expected typescript language"
    );
    assert_eq!(response["id"], 1, "response id should match request");

    let capabilities = response["capabilities"]
        .as_array()
        .expect("capabilities should be an array");
    assert!(
        capabilities.iter().any(|c| c.as_str() == Some("analyze")),
        "capabilities should include 'analyze'"
    );
    assert!(
        capabilities.iter().any(|c| c.as_str() == Some("execute")),
        "capabilities should include 'execute'"
    );

    // Send shutdown and verify clean exit
    let shutdown = serde_json::json!({
        "protocol_version": "0.1.0",
        "id": 2,
        "command": "shutdown"
    });
    writeln!(stdin, "{}", shutdown).expect("failed to write shutdown");

    let status = child.wait().expect("failed to wait for node process");
    assert!(status.success(), "node process exited with error: {status}");
}

/// Locate the shatter cache directory.
fn cache_dir() -> String {
    if let Ok(xdg) = std::env::var("XDG_CACHE_HOME")
        && !xdg.is_empty()
    {
        return format!("{xdg}/shatter");
    }
    let home = std::env::var("HOME").expect("HOME not set");
    format!("{home}/.cache/shatter")
}

/// Find the frontend bundle in the cache directory.
///
/// The bundle may not be extracted yet (it's extracted on first CLI use).
/// To ensure it exists, we run `shatter --help` first which triggers extraction
/// via the embedded_frontend module. If still not found, we look for any
/// `frontend-*.js` file.
fn find_bundle(cache_dir: &str) -> String {
    // Trigger extraction by running a command that initializes the frontend config.
    // `shatter explore --help` doesn't actually spawn the frontend but does call
    // frontend_config() which calls ensure_extracted().
    //
    // Actually, --help doesn't trigger extraction. Let's look for existing bundles
    // or extract manually by running a minimal explore command that will fail fast.
    let dir = std::path::Path::new(cache_dir);

    // If cache dir exists, look for the bundle
    if dir.is_dir()
        && let Some(bundle) = find_frontend_js(dir)
    {
        return bundle;
    }

    // Bundle not yet extracted. Run the CLI binary to trigger extraction.
    // We use `shatter explore` with a nonexistent file — it will fail, but not
    // before extracting the embedded frontend.
    let binary = env!("CARGO_BIN_EXE_shatter");
    let _ = Command::new(binary)
        .env("SHATTER_ALLOW_HOST_WRITES", "1") // str-gg9v: opt into unsandboxed host execution
        .args(["explore", "/nonexistent/file.ts"])
        .output();

    // Now look again
    if dir.is_dir()
        && let Some(bundle) = find_frontend_js(dir)
    {
        return bundle;
    }

    panic!(
        "could not find frontend-*.js bundle in {cache_dir} — \
         is the TS frontend embedded in the binary?"
    );
}

/// Search for `frontend-<hash>.js` in the given directory.
fn find_frontend_js(dir: &std::path::Path) -> Option<String> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("frontend-") && name.ends_with(".js") && !name.ends_with(".tmp") {
            return Some(entry.path().to_string_lossy().into_owned());
        }
    }
    None
}
