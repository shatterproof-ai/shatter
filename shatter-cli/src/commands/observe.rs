use std::path::Path;
use std::time::Duration;

use shatter_core::explorer::{self, ExploreConfig};
use shatter_core::frontend::Frontend;
use shatter_core::log_level::LogLevel;
use shatter_core::pipeline::{self, ObserveStageOutput};
use shatter_core::protocol::{Command as ProtoCommand, ResponseResult};

use crate::args::parse_target;
use crate::helpers::{frontend_config, resolve_project_root, shutdown_frontend};

/// Run the observe command: spawn a frontend, analyze the target function, explore it,
/// and write the resulting ObserveStageOutput JSON to a file or stdout.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_observe(
    target: &str,
    use_concolic: bool,
    max_iterations: u32,
    timeout: u64,
    request_timeout: u64,
    exec_timeout: u64,
    build_timeout: u64,
    release: bool,
    output_path: Option<&Path>,
    log_level: LogLevel,
    memory_limit: Option<u64>,
    project_dir: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    let parsed = parse_target(target)?;

    // The observe command requires a specific function name.
    let function_name = parsed
        .function
        .as_deref()
        .ok_or("target must specify a function: <file>:<function>")?;

    let file_str = parsed.file.to_string_lossy().into_owned();
    let project_root_str = resolve_project_root(project_dir, &parsed.file);

    let req_timeout = Duration::from_secs(request_timeout);
    let _wall_timeout = Duration::from_secs(timeout);

    let config = frontend_config(
        parsed.language,
        req_timeout,
        log_level,
        exec_timeout,
        build_timeout,
        memory_limit,
        None,
        false,
        release,
    )?;

    let mut frontend = Frontend::spawn(&config)
        .await
        .map_err(|e| format!("failed to spawn {} frontend: {e}", parsed.language.label()))?;

    // Analyze phase: get function metadata.
    let analyze_response = frontend
        .send(ProtoCommand::Analyze {
            file: file_str.clone(),
            function: Some(function_name.to_string()),
            project_root: project_root_str.clone(),
        })
        .await
        .map_err(|e| format!("analyze failed: {e}"))?;

    let functions = match &analyze_response.result {
        ResponseResult::Analyze { functions } => functions.clone(),
        ResponseResult::Error { code, message, .. } => {
            shutdown_frontend(frontend).await;
            return Err(format!("analyze error ({code:?}): {message}").into());
        }
        other => {
            shutdown_frontend(frontend).await;
            return Err(format!("unexpected analyze response: {other:?}").into());
        }
    };

    // Find the requested function.
    let func = functions
        .iter()
        .find(|f| f.name == function_name)
        .ok_or_else(|| format!("function '{function_name}' not found in {file_str}"))?
        .clone();

    log::debug!(
        "Found function '{}' ({} params, {} branches)",
        func.name,
        func.params.len(),
        func.branches.len()
    );

    // Explore phase.
    let explore_result: Result<shatter_core::explorer::ObservationOutput, shatter_core::explorer::ExploreError> =
        if use_concolic {
            let seed_inputs =
                shatter_core::boundary_dict::generate_boundary_inputs(&func.params);
            let concolic_config = shatter_core::orchestrator::ExploreConfig {
                max_iterations: max_iterations as usize,
                max_executions: (max_iterations as usize) * 5,
                plateau_threshold: 20,
                mocks: vec![],
                mock_params: vec![],
                solver_timeout_ms: None,
                timeout_explore: None,
                branch_profile: None,
                meta_config: shatter_core::strategy::MetaConfig::default(),
                loop_convergence_window: 3,
                refine_budget: None,
                shrink_budget: shatter_core::orchestrator::DEFAULT_SHRINK_BUDGET,
                mcdc: false,
            };
            match shatter_core::orchestrator::explore(
                &mut frontend,
                &func.name,
                seed_inputs,
                vec![],
                &func.params,
                &concolic_config,
                None,
                None,
            )
            .await
            {
                Ok(mut concolic_result) => {
                    concolic_result.total_lines =
                        func.end_line.saturating_sub(func.start_line) + 1;
                    Ok(concolic_result.into())
                }
                Err(shatter_core::orchestrator::ExploreError::Frontend(fe)) => {
                    Err(shatter_core::explorer::ExploreError::Frontend(fe))
                }
            }
        } else {
            let explore_config = ExploreConfig {
                file: file_str.clone(),
                max_iterations,
                seed: None,
                mocks: vec![],
                mock_params: vec![],
                setup_file: None,
                setup_level: shatter_core::protocol::SetupLevel::Session,
                value_sources: shatter_core::input_gen::resolve_value_sources(
                    &func.params,
                    &std::collections::HashMap::new(),
                    &std::collections::HashMap::new(),
                ),
                capabilities: shatter_core::orchestrator::FrontendCapabilities::default(),
                user_seeds: vec![],
                candidate_inputs: vec![],
                pool_seeds: vec![],
                project_root: project_root_str.clone(),
                loop_buckets: shatter_core::explorer::LoopBuckets::default(),
                timeout_explore: None,
                meta_config: shatter_core::strategy::MetaConfig::default(),
                shrink_budget: shatter_core::orchestrator::DEFAULT_SHRINK_BUDGET,
                isolation: shatter_core::explorer::IsolationMode::None,
                capture_side_effects: false,
                budget_surplus: None,
                claim_policy: shatter_core::scan_orchestrator::ClaimPolicy::default(),
            };
            explorer::explore_function(&mut frontend, &func, &explore_config, None).await
        };

    shutdown_frontend(frontend).await;

    let observation = explore_result.map_err(|e| format!("exploration failed: {e}"))?;

    let stage_output = ObserveStageOutput {
        observation,
        analysis: func,
        file: file_str,
    };

    if let Some(out_path) = output_path {
        pipeline::write_observe_stage(&stage_output, out_path)?;
        log::info!("Wrote observe output: {}", out_path.display());
    } else {
        println!("{}", serde_json::to_string_pretty(&stage_output)?);
    }

    Ok(())
}
