//! str-w5jt9 regression: a plain `shatter scan`/`explore`/`analyze` against an
//! uninitialized project (no `.shatter/`) implicitly runs `init`. Before this
//! fix, that implicit init appended a managed block to a `.gitignore` even
//! when the `.gitignore` was already tracked in git — dirtying a tracked file
//! the user never asked to change.
//!
//! Decision (b) from the issue: keep implicit init (so first-run ergonomics
//! are unchanged — `.shatter/` and a brand-new `.gitignore` are still
//! created), but never modify a `.gitignore` that git already tracks.
//!
//! Strategy: build a real git repo fixture with a Go module and a tracked
//! `.gitignore`, run the real `shatter` binary's `scan` subcommand against it
//! with *none* of the external-audit-mode flags (so implicit init fires), and
//! assert `git status --porcelain` shows the tracked `.gitignore` untouched
//! afterward.

use std::path::Path;
use std::process::Command;

mod common;

const GO_FIXTURE: &str = "package toy\n\n\
func Add(a, b int) int {\n\
\tif a > 0 {\n\
\t\treturn a + b\n\
\t}\n\
\treturn b\n\
}\n";

fn shatter_binary() -> &'static str {
    env!("CARGO_BIN_EXE_shatter")
}

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .status()
        .unwrap_or_else(|e| panic!("failed to run git {args:?}: {e}"));
    assert!(status.success(), "git {args:?} failed in {}", dir.display());
}

fn init_git_repo_with_tracked_gitignore(dir: &Path) {
    git(dir, &["init", "-q"]);
    git(dir, &["config", "user.email", "test@example.com"]);
    git(dir, &["config", "user.name", "Test User"]);
    std::fs::write(dir.join(".gitignore"), "node_modules/\n*.log\n").expect("write .gitignore");
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", "initial commit"]);
}

#[test]
fn plain_scan_on_uninitialized_project_does_not_touch_tracked_gitignore() {
    let project = tempfile::tempdir().expect("create project tempdir");
    let project_root = project.path();

    std::fs::write(project_root.join("go.mod"), "module toy\n\ngo 1.21\n").expect("write go.mod");
    std::fs::write(project_root.join("toy.go"), GO_FIXTURE).expect("write toy.go");
    init_git_repo_with_tracked_gitignore(project_root);

    let tracked_gitignore_before =
        std::fs::read_to_string(project_root.join(".gitignore")).expect("read .gitignore before");

    let command_tmp = tempfile::tempdir().expect("create command tmpdir");
    let _host_tmp_lock = common::host_tmp_shatter_lock();
    let output = Command::new(shatter_binary())
        .env("SHATTER_ALLOW_HOST_WRITES", "1") // str-gg9v: opt into unsandboxed host execution
        .env("TMPDIR", command_tmp.path())
        .current_dir(project_root)
        .args([
            "scan",
            ".",
            "--language",
            "go",
            "--max-iterations",
            "1",
            "--timeout-total",
            "60",
            "--timeout-per-fn",
            "10",
            "--fail-on-failures=0",
        ])
        .output()
        .expect("invoke shatter scan");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Implicit init must still have fired (first-run ergonomics preserved).
    assert!(
        project_root.join(".shatter").exists(),
        "plain scan on an uninitialized project must still create .shatter/ \
         (implicit init should still run for a genuinely new project)\n\
         status={:?}\nstderr=\n{stderr}\nstdout=\n{stdout}",
        output.status,
    );

    // But the pre-existing, git-tracked .gitignore must be byte-for-byte
    // unchanged — no managed block appended.
    let tracked_gitignore_after =
        std::fs::read_to_string(project_root.join(".gitignore")).expect("read .gitignore after");
    assert_eq!(
        tracked_gitignore_before, tracked_gitignore_after,
        "implicit init from a plain `scan` must never modify a .gitignore \
         that is already tracked in git\nstatus={:?}\nstderr=\n{stderr}\nstdout=\n{stdout}",
        output.status,
    );

    // `git status --porcelain` must not show .gitignore as modified — the
    // acceptance criterion is "never modifies a file that is tracked in
    // git", checked the same way a human would (`git status`).
    let status_output = Command::new("git")
        .args(["status", "--porcelain", "--", ".gitignore"])
        .current_dir(project_root)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .output()
        .expect("run git status");
    let status_text = String::from_utf8_lossy(&status_output.stdout);
    assert!(
        status_text.trim().is_empty(),
        "tracked .gitignore must show no changes in `git status --porcelain`, got: {status_text:?}"
    );
}
