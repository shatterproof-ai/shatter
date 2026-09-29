use std::path::Path;
use std::time::Duration;

use shatter_core::cache::BehaviorMapCache;
use shatter_core::frontend::Frontend;
use shatter_core::log_level::LogLevel;
use shatter_core::protocol::{Command as ProtoCommand, ResponseResult};
use shatter_core::revalidation::{RevalidationSummary, RevalidationVerdict};

use crate::args::*;
use crate::helpers::*;

/// Revalidate cached behaviors: replay recorded inputs and classify drift.
///
/// Returns `Ok(true)` if there are no regressions and no unallowed drift,
/// `Ok(false)` if any issues were found. Expected drift counts as an issue
/// unless `allow_drift` is set; output changes always do.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_revalidate(
    source: &str,
    cache_dir: Option<&Path>,
    format: &str,
    request_timeout: u64,
    exec_timeout: u64,
    build_timeout: u64,
    release: bool,
    memory_limit: Option<u64>,
    log_level: LogLevel,
    project_dir: Option<&Path>,
    allow_drift: bool,
) -> Result<bool, Box<dyn std::error::Error>> {
    reject_glob_target(source)?;
    let target = parse_target(source)?;
    let file_str = target.file.to_string_lossy();
    let project_root_str = resolve_project_root(project_dir, &target.file);

    // Initialize cache.
    let dir = match cache_dir {
        Some(p) => p.to_path_buf(),
        None => BehaviorMapCache::default_dir(&std::env::current_dir()?),
    };
    let cache =
        BehaviorMapCache::new(dir).map_err(|e| format!("failed to initialize cache: {e}"))?;

    // Spawn a frontend to analyze and replay inputs.
    let req_timeout = Duration::from_secs(request_timeout);
    let mut config = frontend_config(
        target.language,
        req_timeout,
        log_level,
        exec_timeout,
        build_timeout,
        memory_limit,
        None,
        false,
        release,
    )?;
    apply_project_storage(&mut config, project_root_str.as_deref());
    let mut frontend = Frontend::spawn(&config)
        .await
        .map_err(|e| format!("failed to spawn {} frontend: {e}", target.language.label()))?;

    // Analyze to discover functions and compute fingerprints.
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
            shutdown_frontend(frontend).await;
            return Err(format!("analyze error ({code:?}): {message}").into());
        }
        other => {
            shutdown_frontend(frontend).await;
            return Err(format!("unexpected analyze response: {other:?}").into());
        }
    };

    // Load cached behavior maps for each discovered function.
    let mut behavior_maps = Vec::new();
    for func in &functions {
        if let Ok(Some(bm)) = cache.load(&func.name) {
            behavior_maps.push(bm);
        }
    }

    if behavior_maps.is_empty() {
        shutdown_frontend(frontend).await;
        if format == "json" {
            println!(
                "{{\"reports\":[],\"summary\":{{\"confirmed\":0,\"expected_drift\":0,\"regressed\":0,\"total\":0}},\"all_confirmed\":true}}"
            );
        } else {
            println!("No cached behaviors found for {file_str}. Nothing to revalidate.");
        }
        return Ok(true);
    }

    // Compute deep fingerprints for the analyzed functions.
    let fingerprints: std::collections::HashMap<String, String> =
        shatter_core::fingerprint::compute_deep_fingerprints(
            &target.file,
            &functions,
            &std::collections::HashMap::new(),
        )
        .unwrap_or_default();

    // Instrument each function that has a cached behavior map.
    for bm in &behavior_maps {
        let func_name = bm.function_id.rsplit(':').next().unwrap_or(&bm.function_id);
        let instrument_response = frontend
            .send(ProtoCommand::Instrument {
                file: file_str.to_string(),
                function: func_name.to_string(),
                mocks: vec![],
                project_root: project_root_str.clone(),
                execution_profile: None,
            })
            .await
            .map_err(|e| format!("instrument failed for {func_name}: {e}"))?;

        if let ResponseResult::Error { code, message, .. } = &instrument_response.result {
            shutdown_frontend(frontend).await;
            return Err(format!("instrument error for {func_name} ({code:?}): {message}").into());
        }
    }

    // Revalidate each behavior map.
    let mut all_reports = Vec::new();

    for bm in &behavior_maps {
        let func_name = bm.function_id.rsplit(':').next().unwrap_or(&bm.function_id);
        let current_fp = fingerprints.get(func_name).map(|s| s.as_str());

        let reports =
            shatter_core::revalidation::revalidate_behaviors(&mut frontend, bm, current_fp)
                .await
                .map_err(|e| format!("revalidation failed for {}: {e}", bm.function_id))?;
        all_reports.extend(reports);
    }

    shutdown_frontend(frontend).await;

    let summary = RevalidationSummary::from_reports(&all_reports);
    let passed = summary.passes(allow_drift);

    // Output results.
    if format == "json" {
        let output = serde_json::json!({
            "reports": all_reports,
            "summary": summary,
            "all_confirmed": passed,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else if all_reports.is_empty() {
        println!("No behaviors to revalidate.");
    } else {
        for report in &all_reports {
            let icon = match report.verdict {
                RevalidationVerdict::Confirmed => "ok",
                RevalidationVerdict::ExpectedDrift => "drift",
                RevalidationVerdict::Flaky => "FLAKY",
                RevalidationVerdict::PotentialRegression => "REGRESSION",
                RevalidationVerdict::SeverityDowngrade => "DOWNGRADE",
                RevalidationVerdict::SeverityUpgrade => "UPGRADE",
                RevalidationVerdict::OutputChanged => "CHANGED",
            };
            println!(
                "  [{icon}] {}({}) ({})",
                report.function_name,
                format_input(&report.input_vector),
                report.verdict,
            );
        }
        println!(
            "\n{} confirmed, {} expected drift, {} regressed of {} behaviors.",
            summary.confirmed, summary.expected_drift, summary.regressed, summary.total
        );
        if !passed && summary.regressed == 0 {
            println!("Expected drift fails by default; pass --allow-drift to accept it.");
        }
    }

    Ok(passed)
}

/// Render an input vector as a comma-separated argument list.
fn format_input(inputs: &[serde_json::Value]) -> String {
    inputs
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}
