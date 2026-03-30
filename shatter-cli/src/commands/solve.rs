use std::path::Path;

use shatter_core::pipeline::{self, SolveOutcome, SolveStageOutput};

/// Run the solve command: read Stage 1 observation output and use Z3 to find
/// inputs for uncovered branches. No frontend needed — pure offline computation.
pub(crate) fn run_solve(
    input_path: &Path,
    output_path: Option<&Path>,
    solver_timeout_ms: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let stage_input = pipeline::read_observe_stage(input_path)?;

    let solve_output = pipeline::solve(&stage_input, Some(solver_timeout_ms));
    let metrics = &solve_output.metrics;

    // Print summary.
    println!(
        "Solve: {} uncovered branch directions",
        metrics.total_uncovered
    );
    if metrics.total_uncovered > 0 {
        println!(
            "  sat: {}, unsat: {}, opaque: {}, unreachable: {}, error: {}",
            metrics.sat_count,
            metrics.unsat_count,
            metrics.opaque_count,
            metrics.unreachable_count,
            metrics.error_count,
        );
    }

    // Print per-branch details for sat results.
    for sb in &solve_output.solved_branches {
        if let SolveOutcome::Sat { inputs } = &sb.outcome {
            let direction = if sb.target_taken { "true" } else { "false" };
            let inputs_str = serde_json::to_string(inputs).unwrap_or_else(|_| "?".into());
            println!(
                "  branch {} (line {}, taken={}): {}",
                sb.branch_id, sb.line, direction, inputs_str
            );
        }
    }

    // Write solve stage output if requested.
    if let Some(out_path) = output_path {
        let stage_output = SolveStageOutput {
            solve: solve_output,
            function_name: stage_input.observation.function_name,
            file: stage_input.file,
        };
        pipeline::write_solve_stage(&stage_output, out_path)?;
        log::info!("Wrote solve output: {}", out_path.display());
    }

    Ok(())
}
