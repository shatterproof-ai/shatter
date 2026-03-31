use std::path::Path;

use shatter_core::explorer::{self, ReportOptions};

use crate::helpers::print_markdown;

/// Run the analyze command: read Stage 1 observation output and produce
/// equivalence classes, behavior map, coverage metrics, and optional spec.
/// No frontend or solver needed — pure offline computation.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_analyze(
    input_path: &Path,
    output_path: Option<&Path>,
    show_spec: bool,
    spec_as_json: bool,
    detect_invariants: bool,
    use_color: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let stage_input = shatter_core::pipeline::read_observe_stage(input_path)?;
    let observation = &stage_input.observation;
    let analysis = &stage_input.analysis;

    let analyze_output = shatter_core::pipeline::analyze(observation, analysis);

    // Print coverage metrics.
    let report_style = if use_color {
        shatter_core::report_style::ReportStyle::ansi()
    } else {
        shatter_core::report_style::ReportStyle::default()
    };
    let report_opts = ReportOptions {
        location: Some(format!("{}:{}-{}", stage_input.file, analysis.start_line, analysis.end_line)),
        show_perf: false,
        wall_time: None,
        coverage_metrics: Some(analyze_output.coverage_metrics.clone()),
        style: report_style.clone(),
        genetic_stats: None,
    };
    print!("{}", explorer::format_exploration_report(observation, &report_opts));
    print!(
        "{}",
        shatter_core::coverage_metrics::format_coverage_metrics(
            &analyze_output.coverage_metrics,
            &report_style,
        )
    );
    println!();

    // Build and display spec if requested.
    let spec = if show_spec || detect_invariants {
        let eq_classes = &analyze_output.eq_classes;
        let location = Some(format!("{}:{}-{}", stage_input.file, analysis.start_line, analysis.end_line));

        let spec = if detect_invariants {
            shatter_core::spec::build_spec_with_invariants(
                observation, eq_classes, location, None,
            )
        } else {
            shatter_core::spec::build_spec(observation, eq_classes, location, None)
        };

        if spec_as_json {
            match shatter_core::spec::format_spec_json(&spec) {
                Ok(json) => println!("{json}"),
                Err(e) => log::error!("Error serializing spec: {e}"),
            }
        } else {
            print_markdown(&shatter_core::spec::format_spec_markdown(&spec), use_color);
        }
        Some(spec)
    } else {
        None
    };

    // Write analyze stage output if requested.
    if let Some(out_path) = output_path {
        let stage_output = shatter_core::pipeline::AnalyzeStageOutput {
            analyze: analyze_output,
            spec,
            function_name: observation.function_name.clone(),
            file: stage_input.file,
        };
        shatter_core::pipeline::write_analyze_stage(&stage_output, out_path)?;
        log::info!("Wrote analyze output: {}", out_path.display());
    }

    Ok(())
}
