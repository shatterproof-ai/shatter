//! str-vr7vq follow-up regression (found during code review of the
//! implicit-init directory-resolution fix): implicit init's project-root
//! resolution must canonicalize the scan target the same way `run_scan`
//! does, or the two can disagree about which ancestor is the project root.
//!
//! `Path::parent()` on a *relative* path bottoms out at `""` instead of
//! climbing past the process's cwd, so resolving the project root from a
//! raw relative target (e.g. `shatter scan sub` where neither `sub` nor the
//! cwd has a marker file, but an ancestor of the cwd does) fails to find
//! that ancestor -- and `maybe_implicit_init` then falls back to
//! initializing the cwd itself. `run_scan` canonicalizes its scan directory
//! before resolving the project root (str-6vl7p) and correctly finds the
//! ancestor marker, so the two disagreed on which directory is the project
//! root. Canonicalizing before implicit-init resolution too (str-vr7vq)
//! keeps both resolutions in agreement.

use std::process::Command;

fn shatter_binary() -> &'static str {
    env!("CARGO_BIN_EXE_shatter")
}

const TS_FIXTURE: &str = "export function classify(value: number): boolean { return value > 0; }\n";

#[test]
fn scan_relative_target_implicit_init_finds_the_real_ancestor_root() {
    let outer = tempfile::tempdir().expect("create outer project tempdir");
    // `outer` is the real project root: it has the only marker file.
    std::fs::write(outer.path().join("package.json"), "{}\n").expect("write package.json");

    let inner = outer.path().join("inner");
    std::fs::create_dir_all(&inner).expect("create inner dir");
    let sub = inner.join("sub");
    std::fs::create_dir_all(&sub).expect("create sub dir");
    std::fs::write(sub.join("lib.ts"), TS_FIXTURE).expect("write lib.ts");

    // Run from `inner` (no marker) scanning the relative target "sub" (no
    // marker either) -- only `outer`, two levels up, has one.
    let output = Command::new(shatter_binary())
        .env("SHATTER_ALLOW_HOST_WRITES", "1")
        .current_dir(&inner)
        .args([
            "scan",
            "sub",
            "--language",
            "typescript",
            "--no-cache",
            "--no-seeds",
            "--dry-run",
            "--fail-on-failures=50",
        ])
        .output()
        .expect("invoke shatter scan --dry-run");

    assert!(
        output.status.success(),
        "dry-run must exit 0; stderr=\n{}",
        String::from_utf8_lossy(&output.stderr),
    );

    assert!(
        !inner.join(".shatter").exists(),
        "implicit init must not fall back to the ambient cwd when the \
         relative target has no marker of its own"
    );
    assert!(
        !sub.join(".shatter").exists(),
        "implicit init must not target the scanned directory itself when \
         an ancestor has the marker file"
    );
    assert!(
        outer.path().join(".shatter").exists(),
        "implicit init must resolve to the same ancestor project root that \
         `run_scan` itself finds via the canonicalized scan directory"
    );
}
