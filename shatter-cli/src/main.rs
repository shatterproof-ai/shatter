use std::io::IsTerminal;
use std::process::ExitCode;

use clap::Parser;

use shatter_core::log_level::LogLevel;
use shatter_core::telemetry;
use shatter_core::timing::{self, TimingConfig, TimingHandle, TimingRun};

mod args;
mod commands;
mod embedded_frontend;
mod embedded_go_frontend;
mod helpers;
mod render;
mod telemetry_flush;

use args::*;
use helpers::*;

/// Map clap's `ErrorKind` to a telemetry-friendly string.
fn clap_error_kind_label(kind: clap::error::ErrorKind) -> &'static str {
    use clap::error::ErrorKind;
    match kind {
        ErrorKind::UnknownArgument => "unknown_flag",
        ErrorKind::InvalidSubcommand => "unknown_subcommand",
        ErrorKind::MissingRequiredArgument => "missing_required",
        ErrorKind::ValueValidation => "invalid_value",
        ErrorKind::WrongNumberOfValues => "wrong_count",
        _ => "other",
    }
}

/// If `.shatter/` does not exist under `project_dir` (or the current directory),
/// run `init` implicitly so first-time users get the config structure automatically.
fn maybe_implicit_init(project_dir: Option<&std::path::Path>, colors: &crate::helpers::Colors) {
    let base = project_dir.unwrap_or_else(|| std::path::Path::new("."));
    let shatter_dir = base.join(".shatter");
    if !shatter_dir.exists() {
        eprintln!("No .shatter/ found — initializing project");
        if let Err(e) = commands::init::run_init(project_dir, colors) {
            eprintln!("Warning: implicit init failed: {e}");
        }
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = match Cli::try_parse_from(std::env::args_os()) {
        Ok(cli) => cli,
        Err(clap_err) => {
            if telemetry::is_enabled() {
                let raw_args: Vec<String> = std::env::args().skip(1).collect();
                let sanitized_args = telemetry::sanitize_args(&raw_args);
                let error_kind = Some(clap_error_kind_label(clap_err.kind()).to_string());

                if let Ok(event) = telemetry::new_event(
                    "bad_cli_args",
                    telemetry::EventPayload::BadCliArgs {
                        sanitized_args,
                        error_kind,
                    },
                ) {
                    let _ = telemetry::queue_event(&event);
                }
            }
            clap_err.exit();
        }
    };
    let log_level = cli.effective_log_level();
    let timing_config = match cli.timing_config() {
        Ok(config) => config,
        Err(err) => {
            eprintln!("Error: {err}");
            return ExitCode::FAILURE;
        }
    };

    // Initialize env_logger: CLI flags set the default, RUST_LOG can override.
    let log_filter = match log_level {
        LogLevel::Error => log::LevelFilter::Error,
        LogLevel::Warn => log::LevelFilter::Warn,
        LogLevel::Info => log::LevelFilter::Info,
        LogLevel::Debug => log::LevelFilter::Debug,
        LogLevel::Trace => log::LevelFilter::Trace,
    };
    env_logger::Builder::new()
        .filter_level(log_filter)
        .format(|buf, record| {
            use std::io::Write;
            writeln!(
                buf,
                "[{}] {}",
                record.level().to_string().to_lowercase(),
                record.args()
            )
        })
        .parse_default_env()
        .init();

    let use_color = cli.color.use_color();
    let colors = Colors::new(use_color);

    // Show first-run telemetry notice (once) before command dispatch.
    // Skip entirely if telemetry is disabled via env vars.
    if shatter_core::telemetry::is_enabled()
        && let Err(e) = shatter_core::telemetry::show_first_run_notice()
    {
        log::debug!("Failed to show telemetry notice: {e}");
    }

    let subcommand_name = subcommand_label(&cli.command);
    let cmd_start = std::time::Instant::now();
    let timing_start_unix_ms = timing::unix_timestamp_ms_now();
    let timing_handle = if timing_config.mode.is_enabled() {
        let handle = TimingHandle::default();
        if let Err(err) = handle.install_global() {
            eprintln!("Warning: failed to install tracing timing collector: {err}");
        }
        Some(handle)
    } else {
        None
    };

    let result = match cli.command {
        CliCommand::Explore {
            targets,
            max_iterations,
            per_function_timeout,
            scope,
            analyze_only,
            show_clusters,
            cache_dir,
            no_cache,
            request_timeout,
            exec_timeout,
            build_timeout,
            release,
            inputs,
            config_path,
            spec_out,
            spec,
            spec_json,
            invariants,
            no_boundary_values: _,
            concolic,
            genetic,
            genetic_population,
            genetic_generations,
            genetic_timeout,
            no_adaptive,
            score_window,
            cold_start,
            strategy_floor,
            strategy_weights,
            planner,
            solver_timeout,
            memory_limit,
            clean,
            dry_run,
            loop_buckets,
            timeout_explore,
            time_limit,
            coverage_threshold,
            max_executions,
            seeds_dir,
            no_seeds,
            setup_timeout,
            fail_on_setup_error: _,
            record,
            observe_output,
            persist_stages,
            replay_recorded,
            no_replay,
            refine_budget,
            shrink_budget,
            no_shrink,
            mcdc,
            isolation,
            capture_side_effects,
            report_outputs,
            stdout,
            format,
            workers,
            observer_pool,
            candidate_queue_capacity,
            from_artifacts,
            parallelism_min,
            parallelism_max,
            require_rust,
        } => {
            maybe_implicit_init(cli.project_dir.as_deref(), &colors);
            let shrink_budget = if no_shrink { 0 } else { shrink_budget };
            let parallelism_bounds = match crate::helpers::ParallelismBounds::from_overrides(
                parallelism_min,
                parallelism_max,
            ) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("Error: {e}");
                    return ExitCode::FAILURE;
                }
            };
            // Set SHATTER_SETUP_TIMEOUT env var for frontends if --setup-timeout provided.
            if let Some(secs) = setup_timeout {
                // Safety: CLI is single-threaded at this point (before spawning frontends).
                unsafe {
                    std::env::set_var(
                        shatter_core::setup_manager::SETUP_TIMEOUT_ENV_VAR,
                        secs.to_string(),
                    );
                }
            }
            // Build MetaConfig from CLI flags, starting from defaults.
            let meta_config = match build_meta_config(
                no_adaptive,
                score_window,
                cold_start,
                strategy_floor,
                strategy_weights.as_deref(),
            ) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("Error: {e}");
                    return ExitCode::FAILURE;
                }
            };

            // Apply MC/DC budget multipliers for parameters not explicitly provided.
            // User-provided values always win; multipliers only expand the defaults.
            let budgets =
                resolve_mcdc_budgets(max_iterations, per_function_timeout, solver_timeout, mcdc);

            // str-frc.6: resolve concurrency knobs with CLI > project-config >
            // built-in default precedence. The project config is rooted at
            // --project-dir when set, otherwise the current working directory.
            let project_cfg_root = cli
                .project_dir
                .as_deref()
                .map(std::path::Path::new)
                .map(std::path::Path::to_path_buf)
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| ".".into()));
            let project_cfg = shatter_core::config::load_project_config(&project_cfg_root)
                .unwrap_or_else(|e| {
                    log::warn!("Failed to load project config: {e}");
                    None
                });
            let effective_observer_pool = crate::helpers::resolve_observer_pool(
                observer_pool,
                project_cfg.as_ref().and_then(|c| c.observer_pool),
            );
            let effective_candidate_queue_capacity =
                crate::helpers::resolve_candidate_queue_capacity(
                    candidate_queue_capacity,
                    project_cfg
                        .as_ref()
                        .and_then(|c| c.candidate_queue_capacity),
                );

            commands::explore::run_explore(
                &targets,
                budgets.max_iterations,
                budgets.timeout,
                timeout_explore,
                scope.as_deref(),
                analyze_only,
                show_clusters,
                cache_dir.as_deref(),
                no_cache,
                request_timeout,
                exec_timeout,
                build_timeout,
                release,
                timing_config.mode.is_enabled(),
                inputs.as_deref(),
                config_path.as_deref(),
                spec_out.as_deref(),
                log_level,
                timing_config.show_text_summary(),
                &colors,
                spec || spec_json || spec_out.is_some() || invariants,
                spec_json || spec_out.is_some(),
                invariants,
                concolic,
                budgets.solver_timeout,
                memory_limit,
                clean,
                dry_run,
                cli.project_dir.as_deref(),
                &loop_buckets,
                use_color,
                &seeds_dir,
                no_seeds,
                record,
                &cli.set_overrides,
                &meta_config,
                observe_output.as_deref(),
                persist_stages.as_deref(),
                replay_recorded,
                no_replay,
                refine_budget,
                shrink_budget,
                mcdc,
                isolation.into(),
                capture_side_effects,
                cli.render,
                &report_outputs,
                stdout,
                format,
                workers,
                genetic,
                genetic_population,
                genetic_generations,
                genetic_timeout,
                from_artifacts.as_deref(),
                time_limit,
                coverage_threshold,
                max_executions,
                planner.as_deref(),
                parallelism_bounds,
                require_rust,
                effective_observer_pool,
                effective_candidate_queue_capacity,
            )
            .await
        }
        CliCommand::Analyze {
            input,
            output,
            spec,
            spec_json,
            invariants,
        } => {
            maybe_implicit_init(cli.project_dir.as_deref(), &colors);
            commands::analyze::run_analyze(
                &input,
                output.as_deref(),
                spec || spec_json || invariants,
                spec_json,
                invariants,
                use_color,
            )
        }
        CliCommand::Solve {
            input,
            output,
            solver_timeout,
        } => commands::solve::run_solve(&input, output.as_deref(), solver_timeout),
        CliCommand::Observe {
            target,
            concolic,
            max_iterations,
            timeout,
            request_timeout,
            exec_timeout,
            build_timeout,
            release,
            output,
            memory_limit,
        } => {
            commands::observe::run_observe(
                &target,
                concolic,
                max_iterations,
                timeout,
                request_timeout,
                exec_timeout,
                build_timeout,
                release,
                output.as_deref(),
                log_level,
                memory_limit,
                cli.project_dir.as_deref(),
            )
            .await
        }
        CliCommand::Specify {
            observation_file,
            analyze_file,
            solve_file,
            json,
            yaml,
            invariants,
            output,
        } => commands::specify::run_specify(commands::specify::SpecifyOptions {
            observation_path: &observation_file,
            analyze_path: analyze_file.as_deref(),
            solve_path: solve_file.as_deref(),
            as_json: json,
            as_yaml: yaml,
            detect_invariants: invariants,
            output_path: output.as_deref(),
            use_color,
        }),
        CliCommand::Scan {
            directory,
            language,
            include,
            exclude,
            changed,
            since,
            until,
            include_untracked,
            all,
            max_depth,
            timeout_per_fn,
            timeout_total,
            parallelism,
            mock_config,
            outputs,
            stdout,
            format,
            dry_run,
            resume,
            progress,
            core_sample,
            seed,
            batch,
            max_iterations,
            cache_dir,
            no_cache,
            request_timeout,
            exec_timeout,
            build_timeout,
            release,
            stratum,
            genetic,
            genetic_population,
            genetic_generations,
            genetic_timeout,
            no_adaptive: _,
            score_window: _,
            cold_start: _,
            strategy_floor: _,
            strategy_weights: _,
            solver_timeout: _,
            memory_limit,
            loop_buckets: _,
            timeout_explore,
            seeds_dir,
            no_seeds,
            setup_timeout,
            fail_on_setup_error: _,
            scheduler_policy,
            isolation,
            capture_side_effects,
            workers_per_fn,
            parallelism_min,
            parallelism_max,
            require_rust,
            fail_on_failures,
        } => {
            // str-1wcl: clean external-audit runs (`-o <external> --no-cache
            // --no-seeds`) must not write `.shatter/` into the audited
            // project. Skip the implicit init in that case; the scan
            // command itself reaches the same default paths it would have
            // initialized.
            let scan_external_audit_mode = !outputs.is_empty() && no_cache && no_seeds;
            if !scan_external_audit_mode {
                maybe_implicit_init(cli.project_dir.as_deref(), &colors);
            }
            let parsed_policy: shatter_core::scheduler_policy::SchedulerPolicy =
                match scheduler_policy.parse() {
                    Ok(p) => p,
                    Err(e) => {
                        eprintln!("Error: invalid --scheduler-policy: {e}");
                        return ExitCode::FAILURE;
                    }
                };

            // Set SHATTER_SETUP_TIMEOUT env var for frontends if --setup-timeout provided.
            if let Some(secs) = setup_timeout {
                // Safety: CLI is single-threaded at this point (before spawning frontends).
                unsafe {
                    std::env::set_var(
                        shatter_core::setup_manager::SETUP_TIMEOUT_ENV_VAR,
                        secs.to_string(),
                    );
                }
            }
            // Resolve hierarchical .shatter/config.yaml defaults for scan budgets.
            let scan_dir = std::path::Path::new(&directory);
            let yaml_defaults = {
                let configs = shatter_core::config::discover_configs(scan_dir).unwrap_or_default();
                let merged = shatter_core::config::merge_configs(&configs);
                merged.defaults
            };
            let yaml_genetic = yaml_defaults.genetic.clone().unwrap_or_default();
            let genetic_config = if genetic {
                shatter_core::config::GeneticConfig {
                    enabled: true,
                    population_size: genetic_population.unwrap_or(yaml_genetic.population_size),
                    max_generations: genetic_generations.unwrap_or(yaml_genetic.max_generations),
                    timeout_secs: genetic_timeout.unwrap_or(yaml_genetic.timeout_secs),
                    ..yaml_genetic
                }
            } else {
                yaml_genetic
            };

            // Load project-level config (shatter.config.json) for scan defaults.
            let project_cfg =
                shatter_core::config::load_project_config(std::path::Path::new(&directory))
                    .unwrap_or_else(|e| {
                        log::warn!("Failed to load project config: {e}");
                        None
                    });

            // Resolve CLI options: CLI flag > YAML config > built-in default.
            let effective_max_iterations = max_iterations
                .or(yaml_defaults.max_iterations)
                .unwrap_or(shatter_core::config::DEFAULT_SCAN_MAX_ITERATIONS);
            let effective_timeout_total = timeout_total
                .or_else(|| project_cfg.as_ref().and_then(|c| c.timeout_total))
                .unwrap_or(shatter_core::config::DEFAULT_SCAN_TIMEOUT_TOTAL);
            let effective_timeout_per_fn = timeout_per_fn
                .or(yaml_defaults.timeout)
                .unwrap_or(shatter_core::config::DEFAULT_SCAN_TIMEOUT_PER_FN);
            let effective_exec_timeout = exec_timeout
                .or_else(|| project_cfg.as_ref().and_then(|c| c.exec_timeout))
                .unwrap_or(shatter_core::config::DEFAULT_SCAN_EXEC_TIMEOUT);
            let effective_parallelism = parallelism
                .or_else(|| project_cfg.as_ref().and_then(|c| c.parallelism))
                .unwrap_or(shatter_core::config::DEFAULT_SCAN_PARALLELISM);
            // Resolve parallelism bound overrides (str-v01r): CLI flag wins
            // over config; either side may be unset and falls back to the
            // built-in default in `ParallelismBounds::from_overrides`.
            let effective_parallelism_min =
                parallelism_min.or_else(|| project_cfg.as_ref().and_then(|c| c.parallelism_min));
            let effective_parallelism_max =
                parallelism_max.or_else(|| project_cfg.as_ref().and_then(|c| c.parallelism_max));
            let parallelism_bounds = match crate::helpers::ParallelismBounds::from_overrides(
                effective_parallelism_min,
                effective_parallelism_max,
            ) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("Error: {e}");
                    return ExitCode::FAILURE;
                }
            };
            // For Vec/bool fields: CLI non-empty/true overrides config.
            let effective_include = if include.is_empty() {
                project_cfg
                    .as_ref()
                    .map(|c| c.include.clone())
                    .unwrap_or_default()
            } else {
                include
            };
            let effective_exclude = if exclude.is_empty() {
                project_cfg
                    .as_ref()
                    .map(|c| c.exclude.clone())
                    .unwrap_or_default()
            } else {
                exclude
            };
            let effective_outputs = if outputs.is_empty() {
                project_cfg
                    .as_ref()
                    .and_then(|c| c.output.as_ref())
                    .map(|o| o.paths.clone())
                    .unwrap_or_default()
            } else {
                outputs
            };
            let effective_stdout = stdout
                || project_cfg
                    .as_ref()
                    .and_then(|c| c.output.as_ref())
                    .and_then(|o| o.stdout)
                    .unwrap_or(false);
            let effective_capture = capture_side_effects
                || project_cfg
                    .as_ref()
                    .and_then(|c| c.capture_side_effects)
                    .unwrap_or(false);
            let effective_no_cache = no_cache
                || project_cfg
                    .as_ref()
                    .and_then(|c| c.no_cache)
                    .unwrap_or(false);

            commands::scan::run_scan(
                &directory,
                language.as_deref(),
                &effective_include,
                &effective_exclude,
                changed,
                since.as_deref(),
                until.as_deref(),
                include_untracked,
                all,
                max_depth,
                effective_max_iterations,
                effective_timeout_total,
                cache_dir.as_deref(),
                effective_no_cache,
                request_timeout,
                effective_exec_timeout,
                build_timeout,
                release,
                effective_parallelism,
                effective_timeout_per_fn,
                timeout_explore,
                &effective_outputs,
                effective_stdout,
                format,
                progress,
                dry_run,
                resume.as_deref(),
                mock_config.as_deref(),
                core_sample.as_deref(),
                seed,
                batch.as_deref(),
                stratum.as_deref(),
                log_level,
                memory_limit,
                cli.project_dir.as_deref(),
                use_color,
                cli.render,
                &seeds_dir,
                no_seeds,
                parsed_policy,
                isolation.into(),
                effective_capture,
                workers_per_fn,
                &genetic_config,
                parallelism_bounds,
                require_rust,
                shatter_core::scan_orchestrator::ScanFailurePolicy::from_cli_flag(
                    fail_on_failures,
                ),
            )
            .await
        }
        CliCommand::Properties {
            targets,
            output,
            output_format,
            max_iterations,
            timeout,
            scope,
            request_timeout,
            exec_timeout,
            build_timeout,
            release,
            memory_limit,
        } => {
            commands::properties::run_properties(
                &targets,
                &output_format,
                output.as_deref(),
                max_iterations,
                timeout,
                scope.as_deref(),
                request_timeout,
                exec_timeout,
                build_timeout,
                release,
                log_level,
                memory_limit,
                cli.project_dir.as_deref(),
            )
            .await
        }
        CliCommand::Run {
            path,
            output_dir,
            max_iterations,
            timeout,
            analyze_only,
            request_timeout,
            exec_timeout,
            build_timeout,
            coverage_budget_gates,
            release,
            solver_timeout: _,
            memory_limit,
        } => {
            commands::run::run_run(
                &path,
                output_dir.as_deref(),
                max_iterations,
                timeout,
                analyze_only,
                request_timeout,
                exec_timeout,
                build_timeout,
                release,
                log_level,
                memory_limit,
                cli.project_dir.as_deref(),
                use_color,
                commands::run::CoverageBudgetGateOverrides {
                    min_source_representation_percent: coverage_budget_gates
                        .min_source_representation_percent,
                    max_failed_span_percent: coverage_budget_gates.max_failed_span_percent,
                    max_unsupported_span_percent: coverage_budget_gates
                        .max_unsupported_span_percent,
                    fail_on_stale_source_set: coverage_budget_gates.fail_on_stale_source_set,
                    fail_on_missing_artifacts: coverage_budget_gates.fail_on_missing_artifacts,
                    fail_on_low_report_validity: coverage_budget_gates.fail_on_low_report_validity,
                },
            )
            .await
        }
        CliCommand::Diff {
            snapshot,
            current,
            json,
        } => match commands::diff::run_diff(&snapshot, &current, json, use_color) {
            Ok(has_regressions) => {
                let code = if has_regressions { 1 } else { 0 };
                return finalize_exit_code(
                    &subcommand_name,
                    cmd_start.elapsed().as_millis() as u64,
                    code,
                    &timing_config,
                    timing_start_unix_ms,
                    timing_handle.as_ref(),
                );
            }
            Err(e) => Err(e),
        },
        CliCommand::SpecDiff { old, new, json } => {
            match commands::diff::run_spec_diff(&old, &new, json, use_color) {
                Ok(has_regressions) => {
                    let code = if has_regressions { 1 } else { 0 };
                    return finalize_exit_code(
                        &subcommand_name,
                        cmd_start.elapsed().as_millis() as u64,
                        code,
                        &timing_config,
                        timing_start_unix_ms,
                        timing_handle.as_ref(),
                    );
                }
                Err(e) => Err(e),
            }
        }
        CliCommand::Compare {
            spec_a,
            spec_b,
            json,
        } => match commands::compare::run_compare(&spec_a, &spec_b, json, use_color) {
            Ok(has_divergences) => {
                let code = if has_divergences { 1 } else { 0 };
                return finalize_exit_code(
                    &subcommand_name,
                    cmd_start.elapsed().as_millis() as u64,
                    code,
                    &timing_config,
                    timing_start_unix_ms,
                    timing_handle.as_ref(),
                );
            }
            Err(e) => Err(e),
        },
        CliCommand::BuildFrontend {
            language,
            config,
            output,
        } => commands::build_frontend::run_build_frontend(
            &language,
            config.as_deref(),
            output.as_deref(),
        )
        .map_err(|e| e.into()),
        CliCommand::DiscoverDeps {
            command,
            strace,
            working_dir,
            json,
        } => {
            if !strace {
                eprintln!(
                    "Error: --strace flag is required. Currently strace is the only supported discovery method."
                );
                return finalize_exit_code(
                    &subcommand_name,
                    cmd_start.elapsed().as_millis() as u64,
                    1,
                    &timing_config,
                    timing_start_unix_ms,
                    timing_handle.as_ref(),
                );
            }
            match shatter_core::strace_discovery::discover_network_deps(
                &command,
                working_dir.as_deref(),
            ) {
                Ok(report) => {
                    if json {
                        match serde_json::to_string_pretty(&report) {
                            Ok(s) => println!("{s}"),
                            Err(e) => {
                                eprintln!("Error serializing report: {e}");
                                return ExitCode::FAILURE;
                            }
                        }
                    } else {
                        print!("{}", shatter_core::strace_discovery::format_report(&report));
                    }
                    Ok(())
                }
                Err(e) => Err(e.to_string().into()),
            }
        }
        CliCommand::Test {
            all,
            record,
            tier,
            base,
            include_untracked,
            dry_run,
            prioritize,
            budget,
        } => {
            match commands::test::run_test(
                all,
                record,
                tier,
                &base,
                include_untracked,
                dry_run,
                prioritize,
                budget,
                use_color,
            ) {
                Ok(success) => {
                    let code = if success { 0 } else { 1 };
                    return finalize_exit_code(
                        &subcommand_name,
                        cmd_start.elapsed().as_millis() as u64,
                        code,
                        &timing_config,
                        timing_start_unix_ms,
                        timing_handle.as_ref(),
                    );
                }
                Err(e) => Err(e),
            }
        }
        CliCommand::Stale {
            source,
            spec,
            output_format,
            request_timeout,
            exec_timeout,
            build_timeout,
            release,
            memory_limit,
            cache_dir,
            no_cache,
            strict,
        } => {
            match commands::stale::run_stale(
                &source,
                &spec,
                &output_format,
                request_timeout,
                exec_timeout,
                build_timeout,
                release,
                memory_limit,
                log_level,
                cli.project_dir.as_deref(),
                cache_dir.as_deref(),
                no_cache,
                strict,
            )
            .await
            {
                Ok(all_fresh) => {
                    let code = if all_fresh { 0 } else { 1 };
                    return finalize_exit_code(
                        &subcommand_name,
                        cmd_start.elapsed().as_millis() as u64,
                        code,
                        &timing_config,
                        timing_start_unix_ms,
                        timing_handle.as_ref(),
                    );
                }
                Err(e) => Err(e),
            }
        }
        CliCommand::Revalidate {
            source,
            cache_dir,
            request_timeout,
            exec_timeout,
            build_timeout,
            release,
            memory_limit,
            output_format,
        } => {
            match commands::revalidate::run_revalidate(
                &source,
                cache_dir.as_deref(),
                &output_format,
                request_timeout,
                exec_timeout,
                build_timeout,
                release,
                memory_limit,
                log_level,
                cli.project_dir.as_deref(),
            )
            .await
            {
                Ok(all_confirmed) => {
                    let code = if all_confirmed { 0 } else { 1 };
                    return finalize_exit_code(
                        &subcommand_name,
                        cmd_start.elapsed().as_millis() as u64,
                        code,
                        &timing_config,
                        timing_start_unix_ms,
                        timing_handle.as_ref(),
                    );
                }
                Err(e) => Err(e),
            }
        }
        CliCommand::Init { directory } => commands::init::run_init(directory.as_deref(), &colors),
        CliCommand::Telemetry { action } => {
            let dm = cmd_start.elapsed().as_millis() as u64;
            return match commands::telemetry::run_telemetry(&action) {
                Ok(()) => finalize_exit_code(
                    &subcommand_name,
                    dm,
                    0,
                    &timing_config,
                    timing_start_unix_ms,
                    timing_handle.as_ref(),
                ),
                Err(e) => {
                    eprintln!("Error: {e}");
                    queue_command_error_event(&subcommand_name, &*e);
                    finalize_exit_code(
                        &subcommand_name,
                        dm,
                        1,
                        &timing_config,
                        timing_start_unix_ms,
                        timing_handle.as_ref(),
                    )
                }
            };
        }
        CliCommand::Bench {
            manifest,
            tier,
            repeats,
            warmups,
            max_iterations,
            output,
            request_timeout,
            exec_timeout,
            build_timeout,
        } => {
            commands::bench::run_bench(
                &manifest,
                &tier,
                repeats,
                warmups,
                max_iterations,
                output.as_deref(),
                request_timeout,
                exec_timeout,
                build_timeout,
                log_level,
                cli.project_dir.as_deref(),
            )
            .await
        }
        CliCommand::Cache { action } => {
            let dm = cmd_start.elapsed().as_millis() as u64;
            let result = std::env::current_dir()
                .map_err(|e| -> Box<dyn std::error::Error> { e.into() })
                .and_then(|project_root| {
                    commands::cache::run_cache_clear_from_action(&action, &project_root)
                });
            return match result {
                Ok(()) => finalize_exit_code(
                    &subcommand_name,
                    dm,
                    0,
                    &timing_config,
                    timing_start_unix_ms,
                    timing_handle.as_ref(),
                ),
                Err(e) => {
                    eprintln!("Error: {e}");
                    queue_command_error_event(&subcommand_name, &*e);
                    finalize_exit_code(
                        &subcommand_name,
                        dm,
                        1,
                        &timing_config,
                        timing_start_unix_ms,
                        timing_handle.as_ref(),
                    )
                }
            };
        }
        CliCommand::Nondeterminism { action } => {
            use args::NondeterminismAction;
            // Detect whether stdin is a terminal; if not, use non-interactive mode.
            let non_interactive = !std::io::stdin().is_terminal();
            let dm = cmd_start.elapsed().as_millis() as u64;
            let result = match action {
                NondeterminismAction::Review { cache_dir } => commands::nondeterminism::run_review(
                    cli.project_dir.as_deref(),
                    &colors,
                    cache_dir.as_deref(),
                    non_interactive,
                ),
            };
            return match result {
                Ok(()) => finalize_exit_code(
                    &subcommand_name,
                    dm,
                    0,
                    &timing_config,
                    timing_start_unix_ms,
                    timing_handle.as_ref(),
                ),
                Err(e) => {
                    eprintln!("Error: {e}");
                    queue_command_error_event(&subcommand_name, &*e);
                    finalize_exit_code(
                        &subcommand_name,
                        dm,
                        1,
                        &timing_config,
                        timing_start_unix_ms,
                        timing_handle.as_ref(),
                    )
                }
            };
        }
        CliCommand::Workspace { action } => {
            let dm = cmd_start.elapsed().as_millis() as u64;
            let result = commands::workspace::run_workspace(&action);
            return match result {
                Ok(()) => finalize_exit_code(
                    &subcommand_name,
                    dm,
                    0,
                    &timing_config,
                    timing_start_unix_ms,
                    timing_handle.as_ref(),
                ),
                Err(e) => {
                    eprintln!("Error: {e}");
                    queue_command_error_event(&subcommand_name, &*e);
                    finalize_exit_code(
                        &subcommand_name,
                        dm,
                        1,
                        &timing_config,
                        timing_start_unix_ms,
                        timing_handle.as_ref(),
                    )
                }
            };
        }
    };

    let duration_ms = cmd_start.elapsed().as_millis() as u64;

    match result {
        Ok(()) => finalize_exit_code(
            &subcommand_name,
            duration_ms,
            0,
            &timing_config,
            timing_start_unix_ms,
            timing_handle.as_ref(),
        ),
        Err(e) => {
            eprintln!("Error: {e}");
            queue_command_error_event(&subcommand_name, &*e);
            finalize_exit_code(
                &subcommand_name,
                duration_ms,
                1,
                &timing_config,
                timing_start_unix_ms,
                timing_handle.as_ref(),
            )
        }
    }
}

fn finalize_exit_code(
    subcommand: &str,
    duration_ms: u64,
    exit_code: i32,
    timing_config: &TimingConfig,
    timing_start_unix_ms: u128,
    timing_handle: Option<&TimingHandle>,
) -> ExitCode {
    let _finalize_span = tracing::info_span!("cli.finalize_command").entered();
    queue_command_run_event(subcommand, duration_ms, exit_code);
    // Fire-and-forget: flush the local telemetry queue to PostHog.
    // Errors are silently swallowed — telemetry must never affect CLI exit behavior.
    if telemetry::is_enabled() {
        telemetry_flush::flush_queue();
    }
    persist_timing_run(
        subcommand,
        duration_ms,
        exit_code,
        timing_config,
        timing_start_unix_ms,
        timing_handle,
    );
    if exit_code == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn persist_timing_run(
    subcommand: &str,
    duration_ms: u64,
    exit_code: i32,
    timing_config: &TimingConfig,
    timing_start_unix_ms: u128,
    timing_handle: Option<&TimingHandle>,
) {
    let Some(output) = timing_config.output.as_ref() else {
        return;
    };

    let phases = timing_handle.map_or_else(Vec::new, TimingHandle::snapshot);
    let run = TimingRun::from_phase_summaries(
        subcommand.to_string(),
        timing_config,
        timing_start_unix_ms,
        duration_ms,
        exit_code,
        phases,
    );
    if let Err(err) = run.persist(output) {
        eprintln!("Warning: failed to write timing output: {err}");
    }
}

/// Extract a stable label from the command variant for telemetry.
fn subcommand_label(cmd: &CliCommand) -> String {
    // Use Debug formatting and take the first word (variant name).
    let debug = format!("{cmd:?}");
    debug
        .split_whitespace()
        .next()
        .unwrap_or("unknown")
        .to_lowercase()
}

/// Categorize an error for telemetry without leaking paths or messages.
fn categorize_error(err: &dyn std::fmt::Display) -> &'static str {
    let msg = err.to_string().to_lowercase();
    if msg.contains("no such file") || msg.contains("not found") && !msg.contains("directory") {
        "file_not_found"
    } else if msg.contains("permission denied") {
        "permission_denied"
    } else if msg.contains("directory")
        && (msg.contains("not found") || msg.contains("does not exist"))
    {
        "dir_not_found"
    } else if msg.contains("timed out") || msg.contains("timeout") {
        "timeout"
    } else if msg.contains("spawn") || msg.contains("frontend") {
        "frontend_spawn"
    } else if msg.contains("config") && msg.contains("parse") {
        "config_parse"
    } else {
        "other"
    }
}

/// Queue a command_run telemetry event (best-effort).
fn queue_command_run_event(subcommand: &str, duration_ms: u64, exit_code: i32) {
    if !telemetry::is_enabled() {
        return;
    }
    let raw_args: Vec<String> = std::env::args().skip(1).collect();
    let sanitized_args = telemetry::sanitize_args(&raw_args);
    if let Ok(event) = telemetry::new_event(
        "command_run",
        telemetry::EventPayload::CommandRun {
            subcommand: subcommand.to_string(),
            sanitized_args,
            duration_ms,
            exit_code,
        },
    ) {
        let _ = telemetry::queue_event(&event);
    }
}

/// Queue a command_error telemetry event (best-effort).
fn queue_command_error_event(subcommand: &str, err: &dyn std::error::Error) {
    if !telemetry::is_enabled() {
        return;
    }
    if let Ok(event) = telemetry::new_event(
        "command_error",
        telemetry::EventPayload::CommandError {
            subcommand: subcommand.to_string(),
            error_category: categorize_error(err).to_string(),
        },
    ) {
        let _ = telemetry::queue_event(&event);
    }
}
