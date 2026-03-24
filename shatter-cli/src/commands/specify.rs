use std::path::Path;

use crate::helpers::print_markdown;

/// Run the specify command: read an ObserveStageOutput JSON file and build a
/// FunctionSpec from it, optionally using a pre-computed AnalyzeStageOutput.
pub(crate) fn run_specify(
    observation_path: &Path,
    analyze_path: Option<&Path>,
    as_json: bool,
    as_yaml: bool,
    detect_invariants: bool,
    output_path: Option<&Path>,
    use_color: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let stage_input = shatter_core::pipeline::read_observe_stage(observation_path)?;
    let observation = &stage_input.observation;
    let analysis = &stage_input.analysis;

    let eq_classes = if let Some(af) = analyze_path {
        let data = std::fs::read_to_string(af)?;
        let analyze_stage: shatter_core::pipeline::AnalyzeStageOutput =
            serde_json::from_str(&data)?;
        analyze_stage.analyze.eq_classes
    } else {
        shatter_core::pipeline::analyze(observation, analysis).eq_classes
    };

    let location = Some(format!(
        "{}:{}-{}",
        stage_input.file, analysis.start_line, analysis.end_line
    ));

    let spec = if detect_invariants {
        shatter_core::spec::build_spec_with_invariants(observation, &eq_classes, location, None)
    } else {
        shatter_core::spec::build_spec(observation, &eq_classes, location, None)
    };

    let formatted = if as_json {
        shatter_core::spec::format_spec_json(&spec)?
    } else if as_yaml {
        shatter_core::spec::format_spec_yaml(&spec)?
    } else {
        shatter_core::spec::format_spec_markdown(&spec)
    };

    if let Some(out_path) = output_path {
        std::fs::write(out_path, &formatted)?;
        log::info!("Wrote spec: {}", out_path.display());
    } else if as_json || as_yaml {
        println!("{formatted}");
    } else {
        print_markdown(&formatted, use_color);
    }

    Ok(())
}
