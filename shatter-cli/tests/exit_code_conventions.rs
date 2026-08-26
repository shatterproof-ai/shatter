//! str-9fn2: exit-code convention — 0 = success, 1 = an opt-in gate/comparison
//! fired as designed (differences or failures found), 2 = a usage or tool
//! error (the command never reached a verdict). Drives the real `shatter`
//! binary against `spec-diff` to cover the acceptance criteria named in the
//! issue: a malformed spec file exits with the tool-error code, and real
//! differences exit with the gate code.

use std::io::Write;
use std::process::Command;

const SAMPLE_SPEC_JSON: &str = r#"{
    "function_name": "add",
    "location": "src/math.ts:1",
    "classes": [],
    "iterations": 1,
    "lines_covered": 0,
    "total_lines": 1
}"#;

const SAMPLE_SPEC_JSON_OTHER_FN: &str = r#"{
    "function_name": "subtract",
    "location": "src/math.ts:5",
    "classes": [],
    "iterations": 1,
    "lines_covered": 0,
    "total_lines": 1
}"#;

fn shatter_binary() -> &'static str {
    env!("CARGO_BIN_EXE_shatter")
}

fn write_temp(contents: &str) -> tempfile::NamedTempFile {
    let mut tmp = tempfile::Builder::new()
        .suffix(".json")
        .tempfile()
        .expect("tempfile");
    tmp.write_all(contents.as_bytes()).expect("write");
    tmp.flush().expect("flush");
    tmp
}

#[test]
fn spec_diff_clean_exits_zero() {
    let a = write_temp(SAMPLE_SPEC_JSON);
    let b = write_temp(SAMPLE_SPEC_JSON);
    let output = Command::new(shatter_binary())
        .env("SHATTER_ALLOW_HOST_WRITES", "1") // str-gg9v: opt into unsandboxed host execution
        .args(["spec-diff", "--json"])
        .arg(a.path())
        .arg(b.path())
        .output()
        .expect("invoke shatter spec-diff");
    assert_eq!(
        output.status.code(),
        Some(0),
        "identical specs must exit 0; stderr=\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn spec_diff_with_regressions_exits_with_gate_code() {
    let a = write_temp(SAMPLE_SPEC_JSON);
    let b = write_temp(SAMPLE_SPEC_JSON_OTHER_FN);
    let output = Command::new(shatter_binary())
        .env("SHATTER_ALLOW_HOST_WRITES", "1") // str-gg9v: opt into unsandboxed host execution
        .args(["spec-diff", "--json"])
        .arg(a.path())
        .arg(b.path())
        .output()
        .expect("invoke shatter spec-diff");
    assert_eq!(
        output.status.code(),
        Some(1),
        "a real regression (function removed) must exit 1 (gate fired), not the \
         tool-error code; stderr=\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn spec_diff_on_malformed_file_exits_with_tool_error_code() {
    let malformed = write_temp("{ not valid json");
    let valid = write_temp(SAMPLE_SPEC_JSON);
    let output = Command::new(shatter_binary())
        .env("SHATTER_ALLOW_HOST_WRITES", "1") // str-gg9v: opt into unsandboxed host execution
        .args(["spec-diff", "--json"])
        .arg(malformed.path())
        .arg(valid.path())
        .output()
        .expect("invoke shatter spec-diff");
    assert_eq!(
        output.status.code(),
        Some(2),
        "a malformed spec file must exit 2 (tool error), distinct from exit 1 \
         (differences found), so CI can tell 'spec-diff is broken' from \
         'spec-diff found a regression' without parsing stderr; stderr=\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
