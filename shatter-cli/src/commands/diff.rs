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

    /// Schema version of the input, when known. A [`FileSpecBundle`] always
    /// carries a `version` (legacy bundles deserialize to `0`); a bare
    /// [`FunctionSpec`] has no bundle-level version, so returns `None`.
    fn version(&self) -> Option<u32> {
        match self {
            SpecInput::Function(_) => None,
            SpecInput::Bundle(b) => Some(b.version),
        }
    }

    fn into_specs(self) -> Vec<FunctionSpec> {
        match self {
            SpecInput::Function(s) => vec![s],
            SpecInput::Bundle(b) => b.functions,
        }
    }
}

/// A spec-schema-version mismatch between the two spec-diff inputs.
///
/// Reported distinctly (not folded into the per-function diff) so a consumer
/// — e.g. the spec-diff CI regression workflow — can tell "the schema itself
/// changed between these two Shatter builds" apart from "the analyzed code's
/// behavior changed". A mismatch is tolerated (it does not by itself force a
/// regression exit); it is surfaced for the operator to interpret.
#[derive(serde::Serialize)]
struct SpecVersionMismatch {
    /// Schema version of the old (baseline) spec bundle.
    old: u32,
    /// Schema version of the new (current) spec bundle.
    new: u32,
}

/// Aggregated multi-function spec diff result.
#[derive(serde::Serialize)]
struct MultiSpecDiff {
    /// Schema-version mismatch between the two bundles, if any. Omitted from
    /// JSON when both sides report the same version (or a version is unknown).
    #[serde(skip_serializing_if = "Option::is_none")]
    version_mismatch: Option<SpecVersionMismatch>,
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
        if let Some(vm) = &self.version_mismatch {
            out.push_str(&format!(
                "Spec schema version mismatch: old=v{} new=v{} \
                 — comparing specs from different schema versions; \
                 differences may reflect schema changes, not behavior changes.\n\n",
                vm.old, vm.new
            ));
        }
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

    let old_input = SpecInput::from_str(&old_contents, old_path)?;
    let new_input = SpecInput::from_str(&new_contents, new_path)?;

    // Capture bundle schema versions before consuming the inputs. A mismatch is
    // reported distinctly but tolerated — it does not by itself force a
    // regression exit.
    let version_mismatch = compute_version_mismatch(old_input.version(), new_input.version());

    let old_specs = old_input.into_specs();
    let new_specs = new_input.into_specs();

    let mut result = diff_spec_collections(&old_specs, &new_specs);
    result.version_mismatch = version_mismatch;

    if output_json {
        let json = serde_json::to_string_pretty(&result)
            .map_err(|e| format!("failed to serialize spec diff: {e}"))?;
        println!("{json}");
    } else {
        print_markdown(&result.format_text(), use_color);
    }

    Ok(result.has_regressions())
}

/// Determine whether the two spec inputs report different schema versions.
///
/// Returns `Some` only when both sides carry a known bundle version and the
/// versions differ. A bare [`FunctionSpec`] input (version `None`) never
/// triggers a mismatch — there is no bundle-level version to compare.
fn compute_version_mismatch(
    old: Option<u32>,
    new: Option<u32>,
) -> Option<SpecVersionMismatch> {
    match (old, new) {
        (Some(old_v), Some(new_v)) if old_v != new_v => {
            Some(SpecVersionMismatch { old: old_v, new: new_v })
        }
        _ => None,
    }
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
        version_mismatch: None,
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

    // --- str-12us: spec schema version mismatch reporting ---

    /// A versioned bundle (no `version` field present → legacy v0) diffed
    /// against a current bundle should expose a distinct version mismatch.
    const LEGACY_BUNDLE_JSON: &str = r#"{
        "file": "src/math.ts",
        "functions": [
            {
                "function_name": "add",
                "location": "src/math.ts:1",
                "classes": [],
                "iterations": 1,
                "lines_covered": 0,
                "total_lines": 1
            }
        ]
    }"#;

    const VERSIONED_BUNDLE_JSON: &str = r#"{
        "version": 1,
        "file": "src/math.ts",
        "functions": [
            {
                "function_name": "add",
                "location": "src/math.ts:1",
                "classes": [],
                "iterations": 1,
                "lines_covered": 0,
                "total_lines": 1
            }
        ]
    }"#;

    #[test]
    fn bundle_input_reports_version_single_does_not() {
        let bundle = SpecInput::from_str(VERSIONED_BUNDLE_JSON, Path::new("b.json")).unwrap();
        assert_eq!(bundle.version(), Some(1));
        let legacy = SpecInput::from_str(LEGACY_BUNDLE_JSON, Path::new("l.json")).unwrap();
        assert_eq!(legacy.version(), Some(0), "legacy bundle decodes to v0");
        let single = SpecInput::from_str(SAMPLE_FUNCTION_SPEC_JSON, Path::new("s.json")).unwrap();
        assert_eq!(single.version(), None, "bare FunctionSpec has no version");
    }

    #[test]
    fn compute_version_mismatch_only_on_differing_known_versions() {
        assert!(compute_version_mismatch(Some(0), Some(1)).is_some());
        assert!(compute_version_mismatch(Some(1), Some(1)).is_none());
        assert!(compute_version_mismatch(None, Some(1)).is_none());
        assert!(compute_version_mismatch(Some(1), None).is_none());
        assert!(compute_version_mismatch(None, None).is_none());
    }

    #[test]
    fn version_mismatch_surfaced_in_text_and_tolerated() {
        let old = write_temp(LEGACY_BUNDLE_JSON);
        let new = write_temp(VERSIONED_BUNDLE_JSON);

        let old_input = SpecInput::from_str(LEGACY_BUNDLE_JSON, old.path()).unwrap();
        let new_input = SpecInput::from_str(VERSIONED_BUNDLE_JSON, new.path()).unwrap();
        let version_mismatch =
            compute_version_mismatch(old_input.version(), new_input.version());
        let mut result =
            diff_spec_collections(&old_input.into_specs(), &new_input.into_specs());
        result.version_mismatch = version_mismatch;

        let text = result.format_text();
        assert!(
            text.contains("Spec schema version mismatch: old=v0 new=v1"),
            "mismatch line should be distinct, got: {text}"
        );
        // Tolerated: a version mismatch alone is not a regression.
        assert!(
            !result.has_regressions(),
            "version mismatch alone must not force a regression exit"
        );

        // And it must appear in the JSON output, separate from per-function diffs.
        let json = serde_json::to_value(&result).expect("serialize");
        assert_eq!(json["version_mismatch"]["old"], 0);
        assert_eq!(json["version_mismatch"]["new"], 1);
    }

    #[test]
    fn no_version_mismatch_field_when_versions_match() {
        let a = write_temp(VERSIONED_BUNDLE_JSON);
        let b = write_temp(VERSIONED_BUNDLE_JSON);
        let a_input = SpecInput::from_str(VERSIONED_BUNDLE_JSON, a.path()).unwrap();
        let b_input = SpecInput::from_str(VERSIONED_BUNDLE_JSON, b.path()).unwrap();
        let version_mismatch = compute_version_mismatch(a_input.version(), b_input.version());
        let mut result = diff_spec_collections(&a_input.into_specs(), &b_input.into_specs());
        result.version_mismatch = version_mismatch;

        let json = serde_json::to_value(&result).expect("serialize");
        assert!(
            json.get("version_mismatch").is_none(),
            "matching versions should omit the field, got: {json}"
        );
    }
}
