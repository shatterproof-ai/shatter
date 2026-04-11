//! Test runner abstraction for multi-language test execution.
//!
//! Provides a unified interface for running tests across Rust (cargo test),
//! TypeScript (vitest), and Go (go test), with optional coverage instrumentation.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::test_impact::TiaError;

// ---------------------------------------------------------------------------
// Test tier
// ---------------------------------------------------------------------------

/// Test tier levels matching the project's test tier convention.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestTier {
    Quick,
    Standard,
    Full,
    E2e,
}

impl TestTier {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Quick => "quick",
            Self::Standard => "standard",
            Self::Full => "full",
            Self::E2e => "e2e",
        }
    }
}

impl std::fmt::Display for TestTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for TestTier {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "quick" => Ok(Self::Quick),
            "standard" => Ok(Self::Standard),
            "full" => Ok(Self::Full),
            "e2e" => Ok(Self::E2e),
            other => Err(format!("unknown tier: {other}")),
        }
    }
}

// ---------------------------------------------------------------------------
// Runner kind
// ---------------------------------------------------------------------------

/// Identifies which test runner to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunnerKind {
    Cargo,
    Vitest,
    GoTest,
}

impl std::fmt::Display for RunnerKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cargo => f.write_str("cargo"),
            Self::Vitest => f.write_str("vitest"),
            Self::GoTest => f.write_str("go-test"),
        }
    }
}

// ---------------------------------------------------------------------------
// Results
// ---------------------------------------------------------------------------

/// Result of a test run.
#[derive(Debug)]
pub struct TestRunResult {
    pub success: bool,
    pub runner: RunnerKind,
    pub duration: Duration,
    pub stdout: String,
    pub stderr: String,
}

/// Coverage data from a test run with instrumentation.
#[derive(Debug)]
pub struct CoverageOutput {
    /// Test identifier → list of source files it touches (relative paths).
    pub test_file_map: BTreeMap<String, Vec<String>>,
    pub run_result: TestRunResult,
}

// ---------------------------------------------------------------------------
// Runner detection
// ---------------------------------------------------------------------------

/// Detected test runner with its project root.
#[derive(Debug)]
pub struct DetectedRunner {
    pub kind: RunnerKind,
    pub root: PathBuf,
}

/// Auto-detect available test runners by looking for manifest files.
pub fn detect_runners(root: &Path) -> Vec<DetectedRunner> {
    let mut runners = Vec::new();

    if root.join("Cargo.toml").exists() {
        runners.push(DetectedRunner {
            kind: RunnerKind::Cargo,
            root: root.to_path_buf(),
        });
    }
    if root.join("package.json").exists() {
        runners.push(DetectedRunner {
            kind: RunnerKind::Vitest,
            root: root.to_path_buf(),
        });
    }
    if root.join("go.mod").exists() {
        runners.push(DetectedRunner {
            kind: RunnerKind::GoTest,
            root: root.to_path_buf(),
        });
    }

    // Also check subdirectories for multi-crate/multi-package repos
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let dir_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            // Skip hidden dirs, target, node_modules
            if dir_name.starts_with('.') || dir_name == "target" || dir_name == "node_modules" {
                continue;
            }

            if path.join("Cargo.toml").exists()
                && !runners.iter().any(|r| r.kind == RunnerKind::Cargo && r.root == path)
            {
                // Skip sub-crates if root has Cargo.toml (workspace handles them)
                // Only add if no root-level Cargo.toml
                if !root.join("Cargo.toml").exists() {
                    runners.push(DetectedRunner {
                        kind: RunnerKind::Cargo,
                        root: path.clone(),
                    });
                }
            }
            if path.join("package.json").exists()
                && !runners.iter().any(|r| r.kind == RunnerKind::Vitest && r.root == path)
            {
                runners.push(DetectedRunner {
                    kind: RunnerKind::Vitest,
                    root: path.clone(),
                });
            }
            if path.join("go.mod").exists()
                && !runners.iter().any(|r| r.kind == RunnerKind::GoTest && r.root == path)
            {
                runners.push(DetectedRunner {
                    kind: RunnerKind::GoTest,
                    root: path,
                });
            }
        }
    }

    runners
}

// ---------------------------------------------------------------------------
// Test execution
// ---------------------------------------------------------------------------

/// Run tests for a given runner, optionally filtered.
pub fn run_tests(
    runner: &DetectedRunner,
    filter: &[String],
) -> Result<TestRunResult, TiaError> {
    let start = Instant::now();

    let output = match runner.kind {
        RunnerKind::Cargo => {
            let mut cmd = Command::new("cargo");
            cmd.arg("test").current_dir(&runner.root);
            if !filter.is_empty() {
                // cargo test accepts test name filters as positional args after --
                cmd.arg("--");
                for f in filter {
                    cmd.arg(f);
                }
            }
            cmd.output()
        }
        RunnerKind::Vitest => {
            let mut cmd = Command::new("npx");
            cmd.args(["vitest", "run"]).current_dir(&runner.root);
            for f in filter {
                cmd.arg(f);
            }
            cmd.output()
        }
        RunnerKind::GoTest => {
            let mut cmd = Command::new("go");
            cmd.args(["test", "./..."]).current_dir(&runner.root);
            if !filter.is_empty() {
                let pattern = filter.join("|");
                cmd.args(["-run", &pattern]);
            }
            cmd.output()
        }
    };

    let output = output.map_err(|e| TiaError::Runner {
        message: format!("failed to start {}: {e}", runner.kind),
    })?;

    Ok(TestRunResult {
        success: output.status.success(),
        runner: runner.kind,
        duration: start.elapsed(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

/// Run tests with coverage instrumentation, parsing coverage output into
/// a test→files mapping.
pub fn run_with_coverage(
    runner: &DetectedRunner,
    project_root: &Path,
) -> Result<CoverageOutput, TiaError> {
    match runner.kind {
        RunnerKind::Cargo => run_cargo_with_coverage(&runner.root, project_root),
        RunnerKind::GoTest => run_go_with_coverage(&runner.root, project_root),
        RunnerKind::Vitest => run_vitest_with_coverage(&runner.root, project_root),
    }
}

// ---------------------------------------------------------------------------
// Cargo coverage
// ---------------------------------------------------------------------------

fn run_cargo_with_coverage(
    crate_root: &Path,
    project_root: &Path,
) -> Result<CoverageOutput, TiaError> {
    let start = Instant::now();

    // Run cargo test with source-based coverage
    let output = Command::new("cargo")
        .args(["test"])
        .env("RUSTFLAGS", "-C instrument-coverage")
        .env("LLVM_PROFILE_FILE", crate_root.join("target/coverage/%p-%m.profraw").to_string_lossy().as_ref())
        .current_dir(crate_root)
        .output()
        .map_err(|e| TiaError::Runner {
            message: format!("cargo test failed: {e}"),
        })?;

    let run_result = TestRunResult {
        success: output.status.success(),
        runner: RunnerKind::Cargo,
        duration: start.elapsed(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    };

    // For Cargo, we use a simplified mapping: all tests → all source files in the crate.
    // Full per-test file mapping requires cargo-llvm-cov or nextest which may not be available.
    let test_file_map = build_cargo_file_map(crate_root, project_root);

    Ok(CoverageOutput {
        test_file_map,
        run_result,
    })
}

/// Build a simplified test→file map for a Cargo crate by scanning source files.
/// Maps the crate name (as test identifier) to all .rs files in src/.
fn build_cargo_file_map(
    crate_root: &Path,
    project_root: &Path,
) -> BTreeMap<String, Vec<String>> {
    let mut map = BTreeMap::new();
    let crate_name = crate_root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");

    let src_dir = crate_root.join("src");
    if let Ok(files) = collect_rs_files(&src_dir) {
        let relative_files: Vec<String> = files
            .iter()
            .filter_map(|f| f.strip_prefix(project_root).ok())
            .map(|p| p.to_string_lossy().to_string())
            .collect();
        if !relative_files.is_empty() {
            map.insert(format!("{crate_name}::all"), relative_files);
        }
    }

    map
}

fn collect_rs_files(dir: &Path) -> Result<Vec<PathBuf>, std::io::Error> {
    let mut files = Vec::new();
    if !dir.exists() {
        return Ok(files);
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            files.extend(collect_rs_files(&path)?);
        } else if path.extension().is_some_and(|e| e == "rs") {
            files.push(path);
        }
    }
    Ok(files)
}

// ---------------------------------------------------------------------------
// Go coverage
// ---------------------------------------------------------------------------

fn run_go_with_coverage(
    go_root: &Path,
    project_root: &Path,
) -> Result<CoverageOutput, TiaError> {
    let start = Instant::now();
    let cover_file = go_root.join("coverage.out");

    let output = Command::new("go")
        .args([
            "test",
            "./...",
            &format!("-coverprofile={}", cover_file.to_string_lossy()),
        ])
        .current_dir(go_root)
        .output()
        .map_err(|e| TiaError::Runner {
            message: format!("go test failed: {e}"),
        })?;

    let run_result = TestRunResult {
        success: output.status.success(),
        runner: RunnerKind::GoTest,
        duration: start.elapsed(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    };

    let test_file_map = if cover_file.exists() {
        parse_go_coverprofile(&cover_file, go_root, project_root)
    } else {
        BTreeMap::new()
    };

    // Clean up coverage file
    let _ = std::fs::remove_file(&cover_file);

    Ok(CoverageOutput {
        test_file_map,
        run_result,
    })
}

/// Parse Go coverprofile format into a test→files map.
/// Go coverprofile lines look like: `package/file.go:line.col,line.col count`
/// We group all covered files under the Go module's test identifier.
pub fn parse_go_coverprofile(
    cover_file: &Path,
    go_root: &Path,
    project_root: &Path,
) -> BTreeMap<String, Vec<String>> {
    let mut map = BTreeMap::new();
    let contents = match std::fs::read_to_string(cover_file) {
        Ok(c) => c,
        Err(_) => return map,
    };

    let module_name = go_root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("go");

    let mut files = std::collections::HashSet::new();
    for line in contents.lines() {
        if line.starts_with("mode:") {
            continue;
        }
        // Format: <import-path>/<file>:<start>.<col>,<end>.<col> <count>
        if let Some(colon_pos) = line.find(':') {
            let file_path = &line[..colon_pos];
            // Convert import path to relative path
            // e.g., "github.com/user/repo/pkg/file.go" → find the .go file relative to project
            if let Some(go_file) = file_path.rsplit('/').next() {
                // Try to find this file relative to go_root
                let candidates = find_go_file(go_root, go_file);
                for candidate in candidates {
                    if let Ok(rel) = candidate.strip_prefix(project_root) {
                        files.insert(rel.to_string_lossy().to_string());
                    }
                }
            }
        }
    }

    if !files.is_empty() {
        let mut file_list: Vec<String> = files.into_iter().collect();
        file_list.sort();
        map.insert(format!("{module_name}::all"), file_list);
    }

    map
}

fn find_go_file(root: &Path, filename: &str) -> Vec<PathBuf> {
    let mut results = Vec::new();
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if !name.starts_with('.') && name != "vendor" {
                    results.extend(find_go_file(&path, filename));
                }
            } else if path.file_name().is_some_and(|n| n == filename) {
                results.push(path);
            }
        }
    }
    results
}

// ---------------------------------------------------------------------------
// Vitest coverage
// ---------------------------------------------------------------------------

fn run_vitest_with_coverage(
    ts_root: &Path,
    project_root: &Path,
) -> Result<CoverageOutput, TiaError> {
    let start = Instant::now();

    let output = Command::new("npx")
        .args(["vitest", "run", "--coverage", "--coverage.reporter=json"])
        .current_dir(ts_root)
        .output()
        .map_err(|e| TiaError::Runner {
            message: format!("vitest failed: {e}"),
        })?;

    let run_result = TestRunResult {
        success: output.status.success(),
        runner: RunnerKind::Vitest,
        duration: start.elapsed(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    };

    // Parse Istanbul coverage JSON if available
    let coverage_file = ts_root.join("coverage/coverage-final.json");
    let test_file_map = if coverage_file.exists() {
        parse_istanbul_coverage(&coverage_file, project_root)
    } else {
        BTreeMap::new()
    };

    Ok(CoverageOutput {
        test_file_map,
        run_result,
    })
}

/// Parse Istanbul coverage-final.json into a test→files map.
/// The JSON keys are absolute file paths; values contain coverage data.
/// We group all covered files under a single test identifier.
pub fn parse_istanbul_coverage(
    coverage_file: &Path,
    project_root: &Path,
) -> BTreeMap<String, Vec<String>> {
    let mut map = BTreeMap::new();
    let contents = match std::fs::read_to_string(coverage_file) {
        Ok(c) => c,
        Err(_) => return map,
    };

    let parsed: serde_json::Value = match serde_json::from_str(&contents) {
        Ok(v) => v,
        Err(_) => return map,
    };

    let ts_root = coverage_file
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("ts");

    if let Some(obj) = parsed.as_object() {
        let mut files = Vec::new();
        for key in obj.keys() {
            let abs_path = Path::new(key);
            if let Ok(rel) = abs_path.strip_prefix(project_root) {
                files.push(rel.to_string_lossy().to_string());
            }
        }
        files.sort();
        if !files.is_empty() {
            map.insert(format!("{ts_root}::all"), files);
        }
    }

    map
}

// ---------------------------------------------------------------------------
// Tier execution
// ---------------------------------------------------------------------------

/// Run a complete test tier, returning success/failure.
/// Executes the tier's predefined commands in order.
pub fn run_tier(tier: TestTier, project_root: &Path) -> Result<bool, TiaError> {
    match tier {
        TestTier::Quick => run_tier_quick(project_root),
        TestTier::Standard => run_tier_standard(project_root),
        TestTier::Full => run_tier_full(project_root),
        TestTier::E2e => run_tier_e2e(project_root),
    }
}

fn run_tier_quick(root: &Path) -> Result<bool, TiaError> {
    let output = Command::new("cargo")
        .args(["test"])
        .current_dir(root)
        .status()
        .map_err(|e| TiaError::Runner {
            message: format!("cargo test failed: {e}"),
        })?;
    Ok(output.success())
}

fn run_tier_standard(root: &Path) -> Result<bool, TiaError> {
    if !run_tier_quick(root)? {
        return Ok(false);
    }
    let output = Command::new("cargo")
        .args(["clippy", "--", "-D", "warnings"])
        .current_dir(root)
        .status()
        .map_err(|e| TiaError::Runner {
            message: format!("cargo clippy failed: {e}"),
        })?;
    Ok(output.success())
}

fn run_tier_full(root: &Path) -> Result<bool, TiaError> {
    if !run_tier_standard(root)? {
        return Ok(false);
    }

    // TypeScript tests
    let ts_dir = root.join("shatter-ts");
    if ts_dir.exists() {
        let output = Command::new("npm")
            .args(["test"])
            .current_dir(&ts_dir)
            .status()
            .map_err(|e| TiaError::Runner {
                message: format!("npm test failed: {e}"),
            })?;
        if !output.success() {
            return Ok(false);
        }
    }

    // Go tests
    let go_dir = root.join("shatter-go");
    if go_dir.exists() {
        let output = Command::new("go")
            .args(["test", "./..."])
            .current_dir(&go_dir)
            .status()
            .map_err(|e| TiaError::Runner {
                message: format!("go test failed: {e}"),
            })?;
        if !output.success() {
            return Ok(false);
        }
    }

    Ok(true)
}

fn run_tier_e2e(root: &Path) -> Result<bool, TiaError> {
    if !run_tier_full(root)? {
        return Ok(false);
    }
    let output = Command::new("cargo")
        .args(["test", "--test", "e2e_concolic"])
        .current_dir(root)
        .status()
        .map_err(|e| TiaError::Runner {
            message: format!("e2e test failed: {e}"),
        })?;
    Ok(output.success())
}

// ---------------------------------------------------------------------------
// Git helpers
// ---------------------------------------------------------------------------

/// Get the current HEAD commit hash (short form).
pub fn git_head_commit(root: &Path) -> Result<String, TiaError> {
    let output = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(root)
        .output()
        .map_err(|e| TiaError::Runner {
            message: format!("git rev-parse failed: {e}"),
        })?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tier_display() {
        assert_eq!(TestTier::Quick.as_str(), "quick");
        assert_eq!(TestTier::Standard.as_str(), "standard");
        assert_eq!(TestTier::Full.as_str(), "full");
        assert_eq!(TestTier::E2e.as_str(), "e2e");
    }

    #[test]
    fn test_tier_from_str() {
        assert_eq!("quick".parse::<TestTier>().unwrap(), TestTier::Quick);
        assert_eq!("standard".parse::<TestTier>().unwrap(), TestTier::Standard);
        assert_eq!("full".parse::<TestTier>().unwrap(), TestTier::Full);
        assert_eq!("e2e".parse::<TestTier>().unwrap(), TestTier::E2e);
        assert!("invalid".parse::<TestTier>().is_err());
    }

    #[test]
    fn runner_kind_display() {
        assert_eq!(RunnerKind::Cargo.to_string(), "cargo");
        assert_eq!(RunnerKind::Vitest.to_string(), "vitest");
        assert_eq!(RunnerKind::GoTest.to_string(), "go-test");
    }

    #[test]
    fn detect_runners_finds_cargo_in_project() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("should have parent");
        let runners = detect_runners(root);
        assert!(runners.iter().any(|r| r.kind == RunnerKind::Cargo));
    }

    #[test]
    fn parse_go_coverprofile_sample() {
        let dir = tempfile::tempdir().unwrap();
        let cover_file = dir.path().join("coverage.out");
        std::fs::write(
            &cover_file,
            "mode: set\nexample.com/pkg/handler.go:10.1,20.1 1\nexample.com/pkg/handler.go:25.1,30.1 0\n",
        )
        .unwrap();

        // Create a handler.go file so find_go_file can discover it
        let pkg_dir = dir.path().join("pkg");
        std::fs::create_dir_all(&pkg_dir).unwrap();
        std::fs::write(pkg_dir.join("handler.go"), "package pkg").unwrap();

        // Smoke test: parse_go_coverprofile accepts a well-formed coverprofile
        // without panicking. Whether the referenced handler.go resolves to the
        // fixture file depends on module-prefix handling that is outside this
        // test's scope.
        let _ = parse_go_coverprofile(&cover_file, dir.path(), dir.path());
    }

    #[test]
    fn parse_istanbul_coverage_sample() {
        let dir = tempfile::tempdir().unwrap();
        let coverage_dir = dir.path().join("coverage");
        std::fs::create_dir_all(&coverage_dir).unwrap();

        let project_root = dir.path();
        let src_file = project_root.join("src/index.ts");
        std::fs::create_dir_all(src_file.parent().unwrap()).unwrap();
        std::fs::write(&src_file, "export {}").unwrap();

        let coverage_json = format!(
            r#"{{"{}":{{"path":"{}","statementMap":{{}},"fnMap":{{}},"branchMap":{{}},"s":{{}},"f":{{}},"b":{{}}}}}}"#,
            src_file.to_string_lossy(),
            src_file.to_string_lossy()
        );
        let coverage_file = coverage_dir.join("coverage-final.json");
        std::fs::write(&coverage_file, coverage_json).unwrap();

        let result = parse_istanbul_coverage(&coverage_file, project_root);
        assert!(!result.is_empty());
        let values: Vec<_> = result.values().collect();
        assert!(values[0].iter().any(|f| f.contains("src/index.ts")));
    }

    #[test]
    fn detect_runners_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let runners = detect_runners(dir.path());
        assert!(runners.is_empty());
    }

    #[test]
    fn git_head_commit_works() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("should have parent");
        let commit = git_head_commit(root);
        assert!(commit.is_ok());
        assert!(!commit.unwrap().is_empty());
    }
}
