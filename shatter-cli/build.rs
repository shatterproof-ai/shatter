use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    build_ts_frontend(&manifest_dir, &out_dir);
    build_go_frontend(&manifest_dir, &out_dir);
}

/// Whether a file under an embedded-frontend source tree contributes to the
/// built artifact and should therefore drive rebuilds and the source hash.
///
/// Test files (`*_test.go`) are excluded: `go build .` ignores them, so a
/// change to a test must not trigger an embedded rebuild or flip the staleness
/// hash (which would produce false "stale binary" reports in `doctor`).
fn is_go_source_file(path: &Path) -> bool {
    let name = match path.file_name().and_then(|n| n.to_str()) {
        Some(n) => n,
        None => return false,
    };
    if name == "go.mod" || name == "go.sum" {
        return true;
    }
    name.ends_with(".go") && !name.ends_with("_test.go")
}

/// Whether a file under the TypeScript `src/` tree contributes to the bundle.
/// All files under `src/` are bundle inputs; the predicate exists for symmetry
/// with the Go side and to keep the walk explicit.
fn is_ts_source_file(_path: &Path) -> bool {
    true
}

/// Recursively collect every source file under `root` matching `predicate`,
/// returned sorted by path for a stable hash, alongside the set of directories
/// walked. Symlinks are not followed.
fn collect_source_tree(root: &Path, predicate: &dyn Fn(&Path) -> bool) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut files = Vec::new();
    let mut dirs = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        dirs.push(dir.clone());
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let file_type = match entry.file_type() {
                Ok(ft) => ft,
                Err(_) => continue,
            };
            if file_type.is_dir() {
                stack.push(path);
            } else if file_type.is_file() && predicate(&path) {
                files.push(path);
            }
        }
    }
    files.sort();
    dirs.sort();
    (files, dirs)
}

/// Compute a deterministic content hash over a sorted set of files.
///
/// For each file the relative path and its contents are folded into the digest
/// so the hash changes on edits, renames, additions, and removals. This is the
/// algorithm `commands::doctor` re-implements at runtime to detect staleness —
/// keep the two in lockstep.
fn hash_source_tree(root: &Path, files: &[PathBuf]) -> String {
    let mut buf: Vec<u8> = Vec::new();
    for file in files {
        let rel = file.strip_prefix(root).unwrap_or(file);
        buf.extend_from_slice(rel.to_string_lossy().as_bytes());
        buf.push(0);
        let bytes = std::fs::read(file)
            .unwrap_or_else(|e| panic!("failed to read {} for hashing: {e}", file.display()));
        buf.extend_from_slice(&bytes.len().to_le_bytes());
        buf.extend_from_slice(&bytes);
        buf.push(0);
    }
    sha256_hex(&buf)
}

fn build_ts_frontend(manifest_dir: &Path, out_dir: &Path) {
    let ts_dir = manifest_dir.join("..").join("shatter-ts");

    let bundle_src = ts_dir.join("dist").join("bundle.js");
    let worker_bundle_src = ts_dir.join("dist").join("worker-bundle.js");

    // Re-run if any TS source file changes. As with the Go tree (str-o09e),
    // the directory form of `rerun-if-changed` misses in-place edits to
    // existing files, so enumerate each source file and emit the directories
    // too (to catch additions/removals).
    let (ts_files, ts_dirs) = collect_source_tree(&ts_dir.join("src"), &is_ts_source_file);
    for dir in &ts_dirs {
        println!("cargo:rerun-if-changed={}", dir.display());
    }
    for file in &ts_files {
        println!("cargo:rerun-if-changed={}", file.display());
    }
    println!(
        "cargo:rerun-if-changed={}",
        ts_dir.join("package.json").display()
    );

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
    assert!(
        worker_bundle_src.exists(),
        "esbuild worker bundle not found at {}",
        worker_bundle_src.display()
    );

    // Compute hash for cache-busting at runtime. The cache-busting key must
    // change whenever EITHER the main bundle or the worker bundle changes,
    // because the runtime extracts both files into the same cache directory
    // keyed off this hash. Hashing only the main bundle (str-jeen.9 fix's
    // original setup) left worker.js stuck at the first hashless name a
    // user ever extracted; a later release that touched only worker-side
    // code (e.g. the str-jeen.9 trailer in instrumentor.ts, which bundles
    // into worker-bundle.js) would ship a new main bundle whose runtime
    // call to instrumentation in the worker still loaded the stale code.
    // That mode regressed in str-jeen.69 — old workers were missing the
    // private-target exposure trailer entirely. Combine both bundles into
    // the hash so any worker-only change forces a new cache directory and
    // a re-extraction of both files.
    let bundle_bytes = std::fs::read(&bundle_src).expect("failed to read bundle.js");
    let worker_bytes =
        std::fs::read(&worker_bundle_src).expect("failed to read worker-bundle.js");
    let mut combined = Vec::with_capacity(bundle_bytes.len() + worker_bytes.len());
    combined.extend_from_slice(&bundle_bytes);
    combined.extend_from_slice(&worker_bytes);
    let hash = sha256_hex(&combined);

    // Copy bundles into OUT_DIR so include_bytes! can reference them
    let out_bundle = out_dir.join("frontend-bundle.js");
    std::fs::copy(&bundle_src, &out_bundle).expect("failed to copy bundle to OUT_DIR");

    let out_worker = out_dir.join("frontend-worker-bundle.js");
    std::fs::copy(&worker_bundle_src, &out_worker)
        .expect("failed to copy worker bundle to OUT_DIR");

    println!("cargo:rustc-env=FRONTEND_BUNDLE_HASH={hash}");
}

fn build_go_frontend(manifest_dir: &Path, out_dir: &Path) {
    let go_dir = manifest_dir.join("..").join("shatter-go");

    // Track every Go source file individually. Cargo's directory-form
    // `rerun-if-changed=<dir>` does NOT detect in-place edits to files already
    // inside the directory (str-o09e) — editing an existing `.go` file then
    // left the embedded frontend stale. Emitting one `rerun-if-changed=<file>`
    // per source file makes Cargo re-run this script on any edit. We ALSO emit
    // the directory paths so that adding or removing a file (which the
    // file-level set cannot anticipate) still re-triggers the build.
    let (go_files, go_dirs) = collect_source_tree(&go_dir, &is_go_source_file);
    for dir in &go_dirs {
        println!("cargo:rerun-if-changed={}", dir.display());
    }
    for file in &go_files {
        println!("cargo:rerun-if-changed={}", file.display());
    }

    // Hash the Go source tree (not the built binary) so the staleness check in
    // `shatter doctor` can recompute the same value from a checkout and compare
    // it against what this binary was built from. The binary hash below is
    // non-reproducible across machines; the source hash is.
    let source_hash = hash_source_tree(&go_dir, &go_files);
    println!("cargo:rustc-env=GO_FRONTEND_SOURCE_HASH={source_hash}");
    // Record the source directory so `doctor` can locate the tree to re-hash in
    // a dev checkout. Absent (or pointing nowhere) in an installed binary, in
    // which case the staleness check is skipped.
    println!(
        "cargo:rustc-env=GO_FRONTEND_SOURCE_DIR={}",
        go_dir.display()
    );

    let go_binary = out_dir.join("shatter-go");

    let status = Command::new("go")
        .args(["build", "-buildvcs=false", "-o"])
        .arg(&go_binary)
        .arg(".")
        .current_dir(&go_dir)
        .status()
        .expect("failed to run go build — is Go installed?");

    assert!(status.success(), "go build failed");

    assert!(
        go_binary.exists(),
        "go binary not found at {}",
        go_binary.display()
    );

    // Compute hash for cache-busting at runtime
    let binary_bytes = std::fs::read(&go_binary).expect("failed to read shatter-go binary");
    let hash = sha256_hex(&binary_bytes);

    println!("cargo:rustc-env=GO_FRONTEND_HASH={hash}");
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
