use std::path::Path;

use shatter_core::snapshot;

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
    let previous = snapshot::Snapshot::read_from_file(snapshot_path)
        .map_err(|e| format!("failed to read previous snapshot '{}': {e}", snapshot_path.display()))?;
    let current = snapshot::Snapshot::read_from_file(current_path)
        .map_err(|e| format!("failed to read current snapshot '{}': {e}", current_path.display()))?;

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

/// Run the spec-diff command: compare two function specifications.
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

    let old_spec: shatter_core::spec::FunctionSpec = serde_json::from_str(&old_contents)
        .map_err(|e| format!("failed to parse old spec '{}': {e}", old_path.display()))?;
    let new_spec: shatter_core::spec::FunctionSpec = serde_json::from_str(&new_contents)
        .map_err(|e| format!("failed to parse new spec '{}': {e}", new_path.display()))?;

    let result = shatter_core::spec_diff::diff_specs(&old_spec, &new_spec);

    if output_json {
        let json = shatter_core::spec_diff::format_spec_diff_json(&result)
            .map_err(|e| format!("failed to serialize spec diff: {e}"))?;
        println!("{json}");
    } else {
        print_markdown(&shatter_core::spec_diff::format_spec_diff_text(&result), use_color);
    }

    Ok(result.has_regressions())
}
