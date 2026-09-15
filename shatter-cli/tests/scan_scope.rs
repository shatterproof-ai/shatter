//! str-leozr regression: `scan` must auto-discover and apply a scope file.

use std::process::Command;

mod common;

fn shatter_binary() -> &'static str {
    env!("CARGO_BIN_EXE_shatter")
}

#[test]
fn scan_auto_discovers_scope_file_before_analysis() {
    let project = tempfile::tempdir().expect("create project");
    let root = project.path();
    let web = root.join("web");
    std::fs::create_dir(&web).expect("create source directory");
    std::fs::write(root.join("go.mod"), "module example.com/scoped\n\ngo 1.21\n")
        .expect("write module");
    std::fs::write(
        web.join("kept.go"),
        "package web\n\nfunc Keep(v int) int { return v }\n",
    )
    .expect("write kept source");
    std::fs::write(
        web.join("ignored.go"),
        "package web\n\nfunc Ignore(v int) int { return v }\n",
    )
    .expect("write ignored source");
    std::fs::create_dir(root.join(".shatter")).expect("create shatter config directory");
    std::fs::write(root.join(".shatter/config.yaml"), "").expect("write shatter config");
    std::fs::write(
        root.join("shatter.scope.yaml"),
        "scope:\n  exclude:\n    - web/ignored.go\n",
    )
    .expect("write scope config");

    let command_tmp = tempfile::tempdir().expect("create command tempdir");
    let _host_tmp_lock = common::host_tmp_shatter_lock();
    let output = Command::new(shatter_binary())
        .env("SHATTER_ALLOW_HOST_WRITES", "1")
        .env("TMPDIR", command_tmp.path())
        .current_dir(root)
        .args([
            "scan",
            root.to_str().expect("utf8 project path"),
            "--language",
            "go",
            "--dry-run",
            "--stdout",
            "--no-cache",
            "--no-seeds",
            "--color",
            "never",
            "--render",
            "plain",
        ])
        .output()
        .expect("run scoped scan");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(
        output.status.success(),
        "scoped scan failed with {:?}:\n{combined}",
        output.status
    );
    assert!(combined.contains("Keep"), "kept source was not scanned:\n{combined}");
    assert!(
        !combined.contains("Ignore"),
        "scope-excluded source reached analysis:\n{combined}"
    );
}
