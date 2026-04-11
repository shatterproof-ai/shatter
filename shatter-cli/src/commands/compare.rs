use std::path::Path;

use crate::helpers::*;

/// Run the compare command: cross-language behavioral comparison of two specs.
///
/// Returns `Ok(true)` if there are divergent behaviors (nonzero exit), `Ok(false)` if clean.
pub(crate) fn run_compare(
    spec_a_path: &Path,
    spec_b_path: &Path,
    output_json: bool,
    use_color: bool,
) -> Result<bool, Box<dyn std::error::Error>> {
    let a_contents = std::fs::read_to_string(spec_a_path)
        .map_err(|e| format!("failed to read spec A '{}': {e}", spec_a_path.display()))?;
    let b_contents = std::fs::read_to_string(spec_b_path)
        .map_err(|e| format!("failed to read spec B '{}': {e}", spec_b_path.display()))?;

    let spec_a: shatter_core::spec::FunctionSpec = serde_json::from_str(&a_contents)
        .map_err(|e| format!("failed to parse spec A '{}': {e}", spec_a_path.display()))?;
    let spec_b: shatter_core::spec::FunctionSpec = serde_json::from_str(&b_contents)
        .map_err(|e| format!("failed to parse spec B '{}': {e}", spec_b_path.display()))?;

    let result = shatter_core::compare::compare_specs(&spec_a, &spec_b);

    if output_json {
        let json = shatter_core::compare::format_compare_json(&result)
            .map_err(|e| format!("failed to serialize comparison: {e}"))?;
        println!("{json}");
    } else {
        print_markdown(
            &shatter_core::compare::format_compare_text(&result),
            use_color,
        );
    }

    Ok(result.has_divergences())
}
