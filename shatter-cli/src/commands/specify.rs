use std::path::Path;

use crate::helpers::print_markdown;

/// Options for the specify command.
pub(crate) struct SpecifyOptions<'a> {
    pub observation_path: &'a Path,
    pub analyze_path: Option<&'a Path>,
    pub solve_path: Option<&'a Path>,
    pub as_json: bool,
    pub as_yaml: bool,
    pub detect_invariants: bool,
    pub output_path: Option<&'a Path>,
    pub use_color: bool,
}

/// Run the specify command: read an ObserveStageOutput JSON file and build a
/// FunctionSpec from it, optionally integrating solve results for provenance
/// enrichment and coverage completeness.
pub(crate) fn run_specify(opts: SpecifyOptions<'_>) -> Result<(), Box<dyn std::error::Error>> {
    let stage_input = shatter_core::pipeline::read_observe_stage(opts.observation_path)?;
    let observation = &stage_input.observation;
    let analysis = &stage_input.analysis;

    let analyze_out = if let Some(af) = opts.analyze_path {
        shatter_core::pipeline::read_analyze_stage(af)?.analyze
    } else {
        shatter_core::pipeline::analyze(observation, analysis)
    };

    if let Some(sf) = opts.solve_path {
        let solve_stage = shatter_core::pipeline::read_solve_stage(sf)?;
        let specify_out = shatter_core::pipeline::specify(
            &stage_input,
            &analyze_out,
            &solve_stage.solve,
            opts.detect_invariants,
        );

        let formatted = if opts.as_json {
            serde_json::to_string_pretty(&specify_out)?
        } else if opts.as_yaml {
            shatter_core::spec::format_spec_yaml(&specify_out.spec)?
        } else {
            let mut out = shatter_core::spec::format_spec_markdown(&specify_out.spec);
            out.push_str(&format_completeness_footer(
                &specify_out.coverage_completeness,
            ));
            out
        };

        write_output(
            &formatted,
            opts.output_path,
            opts.as_json || opts.as_yaml,
            opts.use_color,
        )
    } else {
        let eq_classes = analyze_out.eq_classes;
        let location = Some(format!(
            "{}:{}-{}",
            stage_input.file, analysis.start_line, analysis.end_line
        ));

        let spec = if opts.detect_invariants {
            shatter_core::spec::build_spec_with_invariants(observation, &eq_classes, location, None)
        } else {
            shatter_core::spec::build_spec(observation, &eq_classes, location, None)
        };

        let formatted = if opts.as_json {
            shatter_core::spec::format_spec_json(&spec)?
        } else if opts.as_yaml {
            shatter_core::spec::format_spec_yaml(&spec)?
        } else {
            shatter_core::spec::format_spec_markdown(&spec)
        };

        write_output(
            &formatted,
            opts.output_path,
            opts.as_json || opts.as_yaml,
            opts.use_color,
        )
    }
}

fn write_output(
    formatted: &str,
    output_path: Option<&Path>,
    is_structured: bool,
    use_color: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(out_path) = output_path {
        std::fs::write(out_path, formatted)?;
        log::info!("Wrote spec: {}", out_path.display());
    } else if is_structured {
        println!("{formatted}");
    } else {
        print_markdown(formatted, use_color);
    }
    Ok(())
}

fn format_completeness_footer(cc: &shatter_core::pipeline::CoverageCompleteness) -> String {
    format!(
        "\n\n---\n\n## Coverage Completeness\n\n\
         | Metric | Count |\n|---|---|\n\
         | Total branch directions | {} |\n\
         | Observed | {} |\n\
         | Proven SAT (Z3) | {} |\n\
         | Proven UNSAT | {} |\n\
         | Opaque | {} |\n\
         | Unreachable | {} |\n\
         | Solver errors | {} |\n\
         | **Completeness** | **{:.1}%** |\n",
        cc.total_branch_directions,
        cc.observed,
        cc.proven_sat,
        cc.proven_unsat,
        cc.opaque,
        cc.unreachable,
        cc.solver_errors,
        cc.completeness_pct,
    )
}
