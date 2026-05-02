use std::path::Path;

use shatter_core::snapshot;
use shatter_core::spec::{FileSpecBundle, FunctionSpec};

use crate::helpers::*;

/// Run the diff command: compare two snapshots and report regressions.
///
/// Returns `Ok(true)` if there are regressions (nonzero exit), `Ok(false)` if clean.
pub(crate) fn run_diff(
    snapshot_path: &Path,
    current_path: &Path,
    output_json: bool,
    use_color: bool,
) -> Result<bool, Box<dyn std::error::Error>> {
    let previous = snapshot::Snapshot::read_from_file(snapshot_path).map_err(|e| {
        format!(
            "failed to read previous snapshot '{}': {e}",
            snapshot_path.display()
        )
    })?;
    let current = snapshot::Snapshot::read_from_file(current_path).map_err(|e| {
        format!(
            "failed to read current snapshot '{}': {e}",
            current_path.display()
        )
    })?;

    let result = snapshot::diff(&previous, &current);

    if output_json {
        let json = serde_json::to_string_pretty(&result)
            .map_err(|e| format!("failed to serialize diff result: {e}"))?;
        println!("{json}");
    } else {
        print_markdown(&result.format_report(), use_color);
    }

    Ok(result.has_regressions())
}

/// Parsed shape of a spec input file: either a single [`FunctionSpec`] or a
/// [`FileSpecBundle`] (as produced by `explore --spec-out`).
enum SpecInput {
    Function(FunctionSpec),
    Bundle(FileSpecBundle),
}

impl SpecInput {
    /// Detect shape and parse. A bundle is identified by the presence of a
    /// top-level `functions` array (the `FileSpecBundle` schema); anything
    /// else is parsed as a single `FunctionSpec`.
    fn from_str(contents: &str, path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let value: serde_json::Value = serde_json::from_str(contents)
            .map_err(|e| format!("failed to parse spec '{}': {e}", path.display()))?;
        if value.get("functions").is_some_and(|v| v.is_array()) {
            let bundle: FileSpecBundle = serde_json::from_value(value).map_err(|e| {
                format!(
                    "failed to parse spec bundle '{}': {e}",
                    path.display()
                )
            })?;
            Ok(SpecInput::Bundle(bundle))
        } else {
            let spec: FunctionSpec = serde_json::from_value(value).map_err(|e| {
                format!("failed to parse spec '{}': {e}", path.display())
            })?;
            Ok(SpecInput::Function(spec))
        }
    }

    fn into_specs(self) -> Vec<FunctionSpec> {
        match self {
            SpecInput::Function(s) => vec![s],
            SpecInput::Bundle(b) => b.functions,
        }
    }
}

/// Aggregated multi-function spec diff result.
#[derive(serde::Serialize)]
struct MultiSpecDiff {
    /// Per-function diffs for functions present in both inputs.
    diffs: Vec<shatter_core::spec_diff::SpecDiff>,
    /// Function names present in the new spec but missing from the old.
    added_functions: Vec<String>,
    /// Function names present in the old spec but missing from the new.
    removed_functions: Vec<String>,
}

impl MultiSpecDiff {
    fn has_regressions(&self) -> bool {
        !self.removed_functions.is_empty() || self.diffs.iter().any(|d| d.has_regressions())
    }

    fn format_text(&self) -> String {
        let mut out = String::new();
        for diff in &self.diffs {
            out.push_str(&shatter_core::spec_diff::format_spec_diff_text(diff));
            out.push('\n');
        }
        if !self.added_functions.is_empty() {
            out.push_str(&format!(
                "Added functions: {}\n",
                self.added_functions.join(", ")
            ));
        }
        if !self.removed_functions.is_empty() {
            out.push_str(&format!(
                "Removed functions: {}\n",
                self.removed_functions.join(", ")
            ));
        }
        if self.diffs.is_empty()
            && self.added_functions.is_empty()
            && self.removed_functions.is_empty()
        {
            out.push_str("Spec diff: (no functions to compare)\n");
        }
        out
    }
}

/// Run the spec-diff command: compare two function specifications.
///
/// Accepts either a single [`FunctionSpec`] JSON or a [`FileSpecBundle`] JSON
/// (as produced by `explore --spec-out`). When both inputs are bundles (or
/// mixed), functions are matched by `function_name` and each matched pair is
/// diffed independently.
///
/// Returns `Ok(true)` if there are regressions (nonzero exit), `Ok(false)` if clean.
pub(crate) fn run_spec_diff(
    old_path: &Path,
    new_path: &Path,
    output_json: bool,
    use_color: bool,
) -> Result<bool, Box<dyn std::error::Error>> {
    let old_contents = std::fs::read_to_string(old_path)
        .map_err(|e| format!("failed to read old spec '{}': {e}", old_path.display()))?;
    let new_contents = std::fs::read_to_string(new_path)
        .map_err(|e| format!("failed to read new spec '{}': {e}", new_path.display()))?;

    let old_specs = SpecInput::from_str(&old_contents, old_path)?.into_specs();
    let new_specs = SpecInput::from_str(&new_contents, new_path)?.into_specs();

    let result = diff_spec_collections(&old_specs, &new_specs);

    if output_json {
        let json = serde_json::to_string_pretty(&result)
            .map_err(|e| format!("failed to serialize spec diff: {e}"))?;
        println!("{json}");
    } else {
        print_markdown(&result.format_text(), use_color);
    }

    Ok(result.has_regressions())
}

/// Match functions by `function_name` and produce a per-function diff plus
/// added/removed function lists.
fn diff_spec_collections(old: &[FunctionSpec], new: &[FunctionSpec]) -> MultiSpecDiff {
    use std::collections::HashMap;

    let old_by_name: HashMap<&str, &FunctionSpec> =
        old.iter().map(|s| (s.function_name.as_str(), s)).collect();
    let new_by_name: HashMap<&str, &FunctionSpec> =
        new.iter().map(|s| (s.function_name.as_str(), s)).collect();

    let mut diffs = Vec::new();
    let mut added_functions = Vec::new();
    let mut removed_functions = Vec::new();

    // Walk new in input order so output is deterministic and follows the
    // producer's natural ordering.
    for new_spec in new {
        match old_by_name.get(new_spec.function_name.as_str()) {
            Some(old_spec) => {
                diffs.push(shatter_core::spec_diff::diff_specs(old_spec, new_spec));
            }
            None => added_functions.push(new_spec.function_name.clone()),
        }
    }
    for old_spec in old {
        if !new_by_name.contains_key(old_spec.function_name.as_str()) {
            removed_functions.push(old_spec.function_name.clone());
        }
    }

    MultiSpecDiff {
        diffs,
        added_functions,
        removed_functions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Minimal FileSpecBundle JSON, mirroring what `explore --spec-out` writes.
    /// Two functions are included so name-keyed matching is exercised, not just
    /// the trivial single-element case.
    const SAMPLE_BUNDLE_JSON: &str = r#"{
        "file": "src/math.ts",
        "functions": [
            {
                "function_name": "add",
                "location": "src/math.ts:1",
                "classes": [],
                "iterations": 1,
                "lines_covered": 0,
                "total_lines": 1
            },
            {
                "function_name": "sub",
                "location": "src/math.ts:5",
                "classes": [],
                "iterations": 1,
                "lines_covered": 0,
                "total_lines": 1
            }
        ]
    }"#;

    /// Minimal single-FunctionSpec JSON — the legacy shape `spec-diff`
    /// historically accepted. Backward-compat coverage.
    const SAMPLE_FUNCTION_SPEC_JSON: &str = r#"{
        "function_name": "add",
        "location": "src/math.ts:1",
        "classes": [],
        "iterations": 1,
        "lines_covered": 0,
        "total_lines": 1
    }"#;

    fn write_temp(contents: &str) -> tempfile::NamedTempFile {
        let mut tmp = tempfile::Builder::new()
            .suffix(".json")
            .tempfile()
            .expect("tempfile");
        tmp.write_all(contents.as_bytes()).expect("write");
        tmp.flush().expect("flush");
        tmp
    }

    /// Regression test for str-wfqh: `spec-diff` must accept the file-level
    /// spec bundle that `explore --spec-json --spec-out` produces. Diffing the
    /// bundle against itself must succeed (Ok) with no regressions (false).
    #[test]
    fn spec_diff_accepts_explore_spec_out_bundle_against_itself() {
        let a = write_temp(SAMPLE_BUNDLE_JSON);
        let b = write_temp(SAMPLE_BUNDLE_JSON);
        let has_regressions = run_spec_diff(a.path(), b.path(), true, false)
            .expect("spec-diff should accept FileSpecBundle JSON");
        assert!(
            !has_regressions,
            "diffing a spec bundle against itself must report no regressions"
        );
    }

    /// Backward compatibility: a single FunctionSpec JSON (the historical
    /// shape) still works.
    #[test]
    fn spec_diff_accepts_single_function_spec_against_itself() {
        let a = write_temp(SAMPLE_FUNCTION_SPEC_JSON);
        let b = write_temp(SAMPLE_FUNCTION_SPEC_JSON);
        let has_regressions = run_spec_diff(a.path(), b.path(), true, false)
            .expect("spec-diff should accept single FunctionSpec JSON");
        assert!(!has_regressions);
    }

    /// Mixed shapes: a single FunctionSpec on one side and a bundle on the
    /// other should match by `function_name`.
    #[test]
    fn spec_diff_matches_function_across_bundle_and_single_shape() {
        let bundle = write_temp(SAMPLE_BUNDLE_JSON);
        let single = write_temp(SAMPLE_FUNCTION_SPEC_JSON);
        // Old=single("add"), new=bundle("add","sub") → "sub" appears as added,
        // not a regression. Should succeed with no regressions.
        let has_regressions = run_spec_diff(single.path(), bundle.path(), true, false)
            .expect("spec-diff should handle mixed shapes");
        assert!(
            !has_regressions,
            "an added function is not a regression"
        );
    }
}
