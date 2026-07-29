//! Regression test for str-79t9: a `defaults.<param>` literal configured in
//! `.shatter/config.yaml` for a FREE FUNCTION must be applied by `shatter
//! explore` and actually tried as an input — without requiring the opt-in
//! `--planner go` flag.
//!
//! Before the fix, `fetch_planner_extra_seeds` short-circuited whenever
//! `--planner` was absent, so `explore` never issued `get_invocation_plan` and
//! configured inputs were silently ignored. `scan` had no such gate: it
//! consults the planner whenever the frontend advertises the
//! `get_invocation_plan` capability. That divergence made the whole
//! configured-input lever dead on the default `explore` invocation.
//!
//! The discriminator is deliberate: the configured value is an absolute path
//! that appears NOWHERE in the target's source, so neither the core's
//! string-literal mining nor the generic string family can produce it. If it
//! shows up among the tried inputs, it can only have come from the config.

use std::path::Path;
use std::process::Command;

fn shatter_binary() -> &'static str {
    env!("CARGO_BIN_EXE_shatter")
}

/// Recursively collect the contents of every `.json` file under `dir`.
fn read_json_files(dir: &Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            read_json_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "json")
            && let Ok(contents) = std::fs::read_to_string(&path)
        {
            out.push(contents);
        }
    }
}

#[test]
fn explore_applies_configured_default_for_free_function_without_planner_flag() {
    if Command::new("go").arg("version").output().is_err() {
        eprintln!("skipping: no Go toolchain available");
        return;
    }

    let tmp = tempfile::tempdir().expect("create tempdir for fixture project");
    let root = tmp.path();
    let fixture_dir = root.join("internal").join("fixture");
    let data_dir = root.join("testdata").join("sample");
    std::fs::create_dir_all(&fixture_dir).expect("create source dir");
    std::fs::create_dir_all(&data_dir).expect("create data dir");
    std::fs::create_dir_all(root.join(".shatter")).expect("create .shatter");

    std::fs::write(root.join("go.mod"), "module example.com/repro\n\ngo 1.22\n")
        .expect("write go.mod");

    // loadOne reads <dir>/meta.yaml. Only the configured directory contains
    // one, so a successful (non-error) outcome proves the configured value was
    // tried. The path is absolute and absent from the source text.
    let src = r#"package fixture

import (
	"os"
	"path/filepath"
)

func loadOne(dir string) (int, error) {
	data, err := os.ReadFile(filepath.Join(dir, "meta.yaml"))
	if err != nil {
		return 0, err
	}
	if len(data) == 0 {
		return 0, nil
	}
	return len(data), nil
}
"#;
    std::fs::write(fixture_dir.join("loader.go"), src).expect("write loader.go");
    std::fs::write(data_dir.join("meta.yaml"), "name: sample\n").expect("write meta.yaml");

    let configured_dir = data_dir.to_string_lossy().to_string();
    let config = format!(
        "functions:\n  \"loader.go:loadOne\":\n    defaults:\n      dir: \"{configured_dir}\"\n"
    );
    std::fs::write(root.join(".shatter").join("config.yaml"), config).expect("write config.yaml");

    let output = Command::new(shatter_binary())
        .current_dir(root)
        // Targets run in a throwaway working directory; the fixture only reads.
        .env("SHATTER_ALLOW_HOST_WRITES", "1")
        .args([
            "explore",
            "internal/fixture/loader.go:loadOne",
            "--max-iterations",
            "20",
        ])
        .output()
        .expect("run shatter explore");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    let mut artifacts = Vec::new();
    read_json_files(&root.join("shatter-artifacts"), &mut artifacts);
    let artifact_blob = artifacts.join("\n");

    assert!(
        artifact_blob.contains(&configured_dir),
        "configured default {configured_dir:?} was never tried as an input.\n\
         --- stdout ---\n{stdout}\n--- stderr ---\n{stderr}\n\
         --- artifacts ({} file(s)) ---\n{}",
        artifacts.len(),
        artifact_blob.chars().take(4000).collect::<String>(),
    );
}
