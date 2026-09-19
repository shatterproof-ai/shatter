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

// str-nfg4y: same branch path, same function, but the two sides' canonical
// examples recorded different inputs ([1] vs [2]) — the diff must report an
// inconclusive comparison note, not a changed postcondition, and must not
// treat that note as a regression.
const SAMPLE_SPEC_JSON_IDENTITY_INPUT_1: &str = r#"{"function_name":"identity","location":null,"classes":[{"label":"Class 1","branch_path":[{"branch_id":1,"taken":true}],"preconditions":[],"postcondition":{"kind":"returns","value":1},"side_effects":[],"examples":[{"inputs":[1],"return_value":1,"thrown_error":null}],"sample_count":1,"precondition_provenance":"observed","postcondition_provenance":"observed"}],"iterations":1,"lines_covered":1,"total_lines":1}"#;

const SAMPLE_SPEC_JSON_IDENTITY_INPUT_2: &str = r#"{"function_name":"identity","location":null,"classes":[{"label":"Class 1","branch_path":[{"branch_id":1,"taken":true}],"preconditions":[],"postcondition":{"kind":"returns","value":2},"side_effects":[],"examples":[{"inputs":[2],"return_value":2,"thrown_error":null}],"sample_count":1,"precondition_provenance":"observed","postcondition_provenance":"observed"}],"iterations":1,"lines_covered":1,"total_lines":1}"#;

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
fn spec_diff_note_only_json_exits_zero_with_comparison_note() {
    let a = write_temp(SAMPLE_SPEC_JSON_IDENTITY_INPUT_1);
    let b = write_temp(SAMPLE_SPEC_JSON_IDENTITY_INPUT_2);
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
        "mismatched canonical inputs must produce an inconclusive note, not a \
         regression, and must exit 0; stderr=\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("comparison_notes"),
        "JSON output must carry the comparison note; stdout=\n{stdout}"
    );
    assert!(
        !stdout.contains("changed_postconditions\":[{"),
        "a mismatched-input pair must not be reported as a changed postcondition; \
         stdout=\n{stdout}"
    );
}

#[test]
fn spec_diff_note_only_text_exits_zero_and_reports_inconclusive() {
    let a = write_temp(SAMPLE_SPEC_JSON_IDENTITY_INPUT_1);
    let b = write_temp(SAMPLE_SPEC_JSON_IDENTITY_INPUT_2);
    let output = Command::new(shatter_binary())
        .env("SHATTER_ALLOW_HOST_WRITES", "1") // str-gg9v: opt into unsandboxed host execution
        .args(["spec-diff", "--color", "never", "--render", "plain"])
        .arg(a.path())
        .arg(b.path())
        .output()
        .expect("invoke shatter spec-diff");
    assert_eq!(
        output.status.code(),
        Some(0),
        "note-only text output must still exit 0; stderr=\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("INCONCLUSIVE") || stdout.contains("insufficient comparison evidence"),
        "text output must explicitly flag the inconclusive comparison, not just \
         say 'No changes detected'; stdout=\n{stdout}"
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
