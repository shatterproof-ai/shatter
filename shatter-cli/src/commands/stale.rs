use std::path::Path;
use std::time::Duration;

use shatter_core::cache::BehaviorMapCache;
use shatter_core::frontend::Frontend;
use shatter_core::log_level::LogLevel;
use shatter_core::protocol::{Command as ProtoCommand, ResponseResult};

use crate::args::*;
use crate::helpers::*;

///
/// Returns `Ok(true)` if all functions are fresh, `Ok(false)` if any are stale or removed.
// Each argument corresponds to a CLI flag; this is only called from one callsite.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_stale(
    source: &str,
    spec_path: &Path,
    format: &str,
    request_timeout: u64,
    exec_timeout: u64,
    build_timeout: u64,
    release: bool,
    memory_limit: Option<u64>,
    log_level: LogLevel,
    project_dir: Option<&Path>,
    cache_dir: Option<&Path>,
    no_cache: bool,
) -> Result<bool, Box<dyn std::error::Error>> {
    let target = parse_target(source)?;
    let file_str = target.file.to_string_lossy();
    let project_root_str = resolve_project_root(project_dir, &target.file);

    let req_timeout = Duration::from_secs(request_timeout);
    let mut config = frontend_config(target.language, req_timeout, log_level, exec_timeout, build_timeout, memory_limit, None, false, release)?;
    apply_project_storage(&mut config, project_root_str.as_deref());
    let mut frontend = Frontend::spawn(&config).await.map_err(|e| {
        format!("failed to spawn {} frontend: {e}", target.language.label())
    })?;

    let analyze_response = frontend
        .send(ProtoCommand::Analyze {
            file: file_str.to_string(),
            function: target.function.clone(),
            project_root: project_root_str,
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

    shutdown_frontend(frontend).await;

    let bm_cache = if no_cache {
        None
    } else {
        let dir = match cache_dir {
            Some(p) => p.to_path_buf(),
            None => BehaviorMapCache::default_dir(&std::env::current_dir()?),
        };
        BehaviorMapCache::new(dir).ok()
    };
    let external_fingerprints = load_external_fingerprints(&functions, bm_cache.as_ref());

    let existing = shatter_core::spec::read_file_spec_bundle(spec_path)
        .map_err(|e| format!("failed to read spec file {}: {e}", spec_path.display()))?;

    let plan = shatter_core::spec::compute_incremental_plan(&target.file, &functions, &existing, &external_fingerprints)
        .map_err(|e| format!("failed to compute incremental plan: {e}"))?;

    let all_fresh = plan.stale.is_empty() && plan.removed.is_empty();

    if format == "json" {
        let output = serde_json::json!({
            "stale": plan.stale,
            "fresh": plan.fresh,
            "removed": plan.removed,
            "all_fresh": all_fresh,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        if !plan.stale.is_empty() {
            println!("Stale ({}):", plan.stale.len());
            for name in &plan.stale {
                println!("  {name}");
            }
        }
        if !plan.fresh.is_empty() {
            println!("Fresh ({}):", plan.fresh.len());
            for name in &plan.fresh {
                println!("  {name}");
            }
        }
        if !plan.removed.is_empty() {
            println!("Removed ({}):", plan.removed.len());
            for name in &plan.removed {
                println!("  {name}");
            }
        }
        if all_fresh {
            println!("All functions are fresh.");
        }
    }

    Ok(all_fresh)
}
