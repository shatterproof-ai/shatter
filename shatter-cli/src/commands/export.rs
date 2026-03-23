use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use shatter_core::behavior::BehaviorMap;
use shatter_core::explorer::{self, ExploreConfig};
use shatter_core::export;
use shatter_core::frontend::Frontend;
use shatter_core::log_level::LogLevel;
use shatter_core::protocol::{Command as ProtoCommand, ResponseResult};
use shatter_core::scan_orchestrator;
use shatter_core::scope::ScopeConfig;

use crate::args::*;
use crate::helpers::*;

/// Run the export-tests command: explore targets and generate test code.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_export_tests(
    targets: &[String],
    framework: &str,
    module_path: &str,
    outputs: &[std::path::PathBuf],
    stdout: bool,
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
    // Validate framework
    if framework != "jest" && framework != "vitest" && framework != "gotest" {
        return Err(format!("unsupported framework '{framework}': expected 'jest', 'vitest', or 'gotest'").into());
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
    let mut all_output = String::new();

    for target in &parsed {
        let file_str = target.file.to_string_lossy();
        let func_display = target.function.as_deref().unwrap_or("(all)");
        let project_root_str = resolve_project_root(project_dir, &target.file);

        log::info!("Exploring {file_str}:{func_display} for test export...");

        let mut config = frontend_config(target.language, req_timeout, LogLevel::Warn, exec_timeout, build_timeout, memory_limit, None, false, release)?;
        apply_project_storage(&mut config, project_root_str.as_deref());
        let mut frontend = Frontend::spawn(&config).await.map_err(|e| {
            format!("failed to spawn {} frontend: {e}", target.language.label())
        })?;

        // Analyze
        let analyze_response = frontend
            .send(ProtoCommand::Analyze {
                file: file_str.to_string(),
                function: target.function.clone(),
                project_root: project_root_str.clone(),
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

        // Explore each function and generate tests
        let explore_config = ExploreConfig {
            file: file_str.to_string(),
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

        for func in &functions {
            log::info!("Exploring {}...", func.name);

            match explorer::explore_function(&mut frontend, func, &explore_config, None).await {
                Ok(result) => {
                    let behavior_map = BehaviorMap::from_exploration_result(&func.name, &result);

                    let test_code = match framework {
                        "jest" => export::generate_jest_tests(&behavior_map, &func.name, module_path),
                        "vitest" => export::generate_vitest_tests(&behavior_map, &func.name, module_path),
                        "gotest" => export::generate_go_tests(&behavior_map, &func.name, module_path),
                        _ => unreachable!("validated above"),
                    };

                    all_output.push_str(&test_code);
                    all_output.push('\n');
                }
                Err(e) => {
                    log::error!("Exploration error for {}: {e}", func.name);
                }
            }
        }

        shutdown_frontend(frontend).await;
    }

    let has_files = !outputs.is_empty();
    for path in outputs {
        std::fs::write(path, &all_output)
            .map_err(|e| format!("failed to write to '{}': {e}", path.display()))?;
        log::info!("Wrote tests to {}", path.display());
    }
    if !has_files || stdout {
        print!("{all_output}");
    }

    Ok(())
}

/// Generate test files from scan results and write them to a directory.
///
/// Each function's behavior map produces a separate test file named after
/// the function. The framework determines the format (jest, vitest, gotest).
pub(crate) fn emit_test_files(
    result: &scan_orchestrator::ParallelScanResult,
    file_map: &HashMap<String, String>,
    framework: &str,
    output_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(output_dir)
        .map_err(|e| format!("failed to create test output directory '{}': {e}", output_dir.display()))?;

    let mut files_written = 0u32;

    for func_result in &result.function_results {
        if func_result.behavior_map.behaviors.is_empty() {
            continue;
        }

        let func_name = &func_result.function_name;
        let module_path = file_map
            .get(func_name)
            .map(|p| {
                let p = Path::new(p);
                let stem = p.with_extension("");
                format!("./{}", stem.display())
            })
            .unwrap_or_else(|| ".".to_string());

        let (test_code, file_ext) = match framework {
            "jest" => (
                export::generate_jest_tests(&func_result.behavior_map, func_name, &module_path),
                "test.ts",
            ),
            "vitest" => (
                export::generate_vitest_tests(&func_result.behavior_map, func_name, &module_path),
                "test.ts",
            ),
            "gotest" => {
                let package = file_map
                    .get(func_name)
                    .and_then(|p| Path::new(p).parent())
                    .and_then(|d| d.file_name())
                    .and_then(|n| n.to_str())
                    .unwrap_or("main");
                (
                    export::generate_go_tests(&func_result.behavior_map, func_name, package),
                    "test.go",
                )
            }
            _ => return Err(format!("unsupported framework '{framework}'").into()),
        };

        let file_name = format!("{func_name}.{file_ext}");
        let file_path = output_dir.join(&file_name);
        std::fs::write(&file_path, &test_code)
            .map_err(|e| format!("failed to write test file '{}': {e}", file_path.display()))?;
        files_written += 1;
    }

    println!("Emitted {files_written} test file(s) to {}", output_dir.display());
    Ok(())
}
