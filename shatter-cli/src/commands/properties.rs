use std::path::Path;
use std::time::Duration;

use shatter_core::explorer::{self, ExploreConfig};
use shatter_core::frontend::Frontend;
use shatter_core::log_level::LogLevel;
use shatter_core::protocol::{Command as ProtoCommand, ResponseResult};
use shatter_core::scope::ScopeConfig;
use shatter_core::spec::{self, FileSpecBundle};

use crate::args::*;
use crate::helpers::*;

/// Run the properties command: explore targets and export behavioral properties as YAML.
///
/// Runs the analyze → explore pipeline on each target, then builds a
/// [`FunctionSpec`] with invariant detection enabled and serialises the
/// result to YAML.  The output is either written to `output_path` or
/// printed to stdout.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_properties(
    targets: &[String],
    format: &str,
    output_path: Option<&Path>,
    max_iterations: u32,
    _timeout: u64,
    scope_path: Option<&Path>,
    request_timeout: u64,
    exec_timeout: u64,
    build_timeout: u64,
    release: bool,
    _log_level: LogLevel,
    memory_limit: Option<u64>,
    project_dir: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Validate format — only yaml is supported for now
    if format != "yaml" {
        return Err(
            format!("unsupported format '{format}': only 'yaml' is supported").into(),
        );
    }

    let _scope_config = match scope_path {
        Some(path) => {
            let config = ScopeConfig::from_file(path)
                .map_err(|e| format!("failed to load scope config: {e}"))?;
            log::info!("Loaded scope config from {}", path.display());
            config
        }
        None => ScopeConfig::default(),
    };

    let parsed: Vec<Target> = targets
        .iter()
        .map(|t| parse_target(t))
        .collect::<Result<Vec<_>, _>>()?;
    validate_targets(&parsed)?;

    let req_timeout = Duration::from_secs(request_timeout);
    let mut file_spec_bundles: Vec<FileSpecBundle> = Vec::new();

    for target in &parsed {
        let file_str = target.file.to_string_lossy();
        let func_display = target.function.as_deref().unwrap_or("(all)");
        let project_root_str = resolve_project_root(project_dir, &target.file);

        log::info!("Exploring {file_str}:{func_display} for property export...");

        let mut config = frontend_config(
            target.language,
            req_timeout,
            LogLevel::Warn,
            exec_timeout,
            build_timeout,
            memory_limit,
            None,
            false,
            release,
        )?;
        apply_project_storage(&mut config, project_root_str.as_deref());
        let mut frontend = Frontend::spawn(&config).await.map_err(|e| {
            format!("failed to spawn {} frontend: {e}", target.language.label())
        })?;

        // Analyze phase: discover functions in the target file
        let analyze_response = frontend
            .send(ProtoCommand::Analyze {
                file: file_str.to_string(),
                function: target.function.clone(),
                project_root: project_root_str.clone(),
                execution_profile: None,
            })
            .await
            .map_err(|e| format!("analyze failed: {e}"))?;

        let functions = match &analyze_response.result {
            ResponseResult::Analyze { functions } => functions.clone(),
            ResponseResult::Error { code, message, .. } => {
                log::error!("Analyze error ({code:?}): {message}");
                shutdown_frontend(frontend).await;
                continue;
            }
            other => {
                log::error!("Unexpected analyze response: {other:?}");
                shutdown_frontend(frontend).await;
                continue;
            }
        };

        let explore_config = ExploreConfig {
            file: file_str.to_string(),
            execution_profile: None,
            max_iterations,
            seed: None,
            mocks: vec![],
            mock_params: vec![],
            setup_file: None,
            setup_level: shatter_core::protocol::SetupLevel::Function,
            value_sources: vec![],
            capabilities: shatter_core::orchestrator::FrontendCapabilities::default(),
            user_seeds: vec![],
            candidate_inputs: vec![],
            pool_seeds: vec![],
            project_root: project_root_str.clone(),
            loop_buckets: explorer::LoopBuckets::default(),
            timeout_explore: None,
            meta_config: shatter_core::strategy::MetaConfig::default(),
            shrink_budget: shatter_core::orchestrator::DEFAULT_SHRINK_BUDGET,
            isolation: shatter_core::explorer::IsolationMode::None,
            capture_side_effects: false,
            budget_surplus: None,
            claim_policy: shatter_core::scan_orchestrator::ClaimPolicy::default(),
        };

        // Explore each function and build specs enriched with invariants
        let mut file_specs = Vec::new();
        for func in &functions {
            log::info!("Exploring {}...", func.name);

            match explorer::explore_function(&mut frontend, func, &explore_config, None).await {
                Ok(result) => {
                    // Run the pipeline analyze stage to derive equivalence classes
                    let analyze_output = shatter_core::pipeline::analyze(&result, func);
                    let eq_classes = &analyze_output.eq_classes;

                    let location = Some(format!(
                        "{file_str}:{}-{}",
                        func.start_line, func.end_line
                    ));
                    let func_spec = spec::build_spec_with_invariants(
                        &result,
                        eq_classes,
                        location,
                        None,
                    );
                    file_specs.push(func_spec);
                }
                Err(e) => {
                    log::error!("Exploration error for {}: {e}", func.name);
                }
            }
        }

        shutdown_frontend(frontend).await;

        if !file_specs.is_empty() {
            file_spec_bundles.push(FileSpecBundle {
                file: file_str.to_string(),
                functions: file_specs,
            });
        }
    }

    let yaml_output = spec::format_file_spec_yaml(&file_spec_bundles)
        .map_err(|e| format!("failed to serialize spec to YAML: {e}"))?;

    match output_path {
        Some(path) => {
            std::fs::write(path, &yaml_output)
                .map_err(|e| format!("failed to write to '{}': {e}", path.display()))?;
            log::info!("Wrote properties spec to {}", path.display());
        }
        None => {
            print!("{yaml_output}");
        }
    }

    Ok(())
}
