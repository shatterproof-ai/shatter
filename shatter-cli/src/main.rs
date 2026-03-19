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
            writeln!(buf, "[{}] {}", record.level().to_string().to_lowercase(), record.args())
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
            timeout,
            scope,
            analyze_only,
            show_clusters,
            cache_dir,
            no_cache,
            request_timeout,
            exec_timeout,
            build_timeout,
            inputs,
            config_path,
            output,
            spec,
            spec_json,
            invariants,
            no_boundary_values: _,
            concolic,
            genetic: _,
            genetic_population: _,
            genetic_generations: _,
            genetic_timeout: _,
            no_adaptive,
            score_window,
            cold_start,
            strategy_floor,
            strategy_weights,
            solver_timeout,
            memory_limit,
            clean,
            dry_run,
            loop_buckets,
            timeout_explore,
            seeds_dir,
            no_seeds,
            setup_timeout,
            fail_on_setup_error: _,
            record,
            observe_output,
            replay_recorded,
            no_replay,
            refine_budget,
            mcdc,
            isolation,
        } => {
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
            let budgets = resolve_mcdc_budgets(max_iterations, timeout, solver_timeout, mcdc);

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
                timing_config.mode.is_enabled(),
                inputs.as_deref(),
                config_path.as_deref(),
                output.as_deref(),
                log_level,
                timing_config.show_text_summary(),
                &colors,
                spec || spec_json || output.is_some() || invariants,
                spec_json || output.is_some(),
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
                &meta_config,
                observe_output.as_deref(),
                replay_recorded,
                no_replay,
                refine_budget,
                mcdc,
                isolation.into(),
                cli.format,
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
            commands::analyze::run_analyze(
                &input,
                output.as_deref(),
                spec || spec_json || invariants,
                spec_json,
                invariants,
                use_color,
            )
        }
        CliCommand::Observe {
            target,
            concolic,
            max_iterations,
            timeout,
            request_timeout,
            exec_timeout,
            build_timeout,
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
            json,
            invariants,
            output,
        } => commands::specify::run_specify(
            &observation_file,
            analyze_file.as_deref(),
            json,
            invariants,
            output.as_deref(),
            use_color,
        ),
        CliCommand::Scan {
            directory,
            language,
            include,
            exclude,
            changed,
            since,
            include_untracked,
            all,
            max_depth,
            timeout_per_fn,
            timeout_total,
            parallelism,
            mock_config,
            output,
            report_format,
            emit_tests,
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
            stratum,
            genetic: _,
            genetic_population: _,
            genetic_generations: _,
            genetic_timeout: _,
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
        } => {
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
            commands::scan::run_scan(
                &directory,
                language.as_deref(),
                &include,
                &exclude,
                changed,
                since.as_deref(),
                include_untracked,
                all,
                max_depth,
                max_iterations,
                timeout_total,
                cache_dir.as_deref(),
                no_cache,
                request_timeout,
                exec_timeout,
                build_timeout,
                parallelism,
                timeout_per_fn,
                timeout_explore,
                output.as_deref(),
                &report_format,
                progress,
                emit_tests.as_deref(),
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
                cli.format,
                &seeds_dir,
                no_seeds,
                parsed_policy,
                isolation.into(),
            )
            .await
        }
        CliCommand::ExportTests {
            targets,
            framework,
            module_path,
            output,
            max_iterations,
            timeout,
            scope,
            request_timeout,
            exec_timeout,
            build_timeout,
            memory_limit,
        } => {
            commands::export::run_export_tests(
                &targets,
                &framework,
                &module_path,
                output.as_deref(),
                max_iterations,
                timeout,
                scope.as_deref(),
                request_timeout,
                exec_timeout,
                build_timeout,
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
                log_level,
                memory_limit,
                cli.project_dir.as_deref(),
                use_color,
            )
            .await
        }
        CliCommand::Diff {
            snapshot,
            current,
            json,
        } => {
            match commands::diff::run_diff(&snapshot, &current, json, use_color) {
                Ok(has_regressions) => {
                    let code = if has_regressions { 1 } else { 0 };
                    return finalize_exit_code(&subcommand_name, cmd_start.elapsed().as_millis() as u64, code, &timing_config, timing_start_unix_ms, timing_handle.as_ref());
                }
                Err(e) => Err(e),
            }
        }
        CliCommand::SpecDiff { old, new, json } => {
            match commands::diff::run_spec_diff(&old, &new, json, use_color) {
                Ok(has_regressions) => {
                    let code = if has_regressions { 1 } else { 0 };
                    return finalize_exit_code(&subcommand_name, cmd_start.elapsed().as_millis() as u64, code, &timing_config, timing_start_unix_ms, timing_handle.as_ref());
                }
                Err(e) => Err(e),
            }
        }
        CliCommand::BuildFrontend {
            language,
            config,
            output,
        } => commands::build_frontend::run_build_frontend(&language, config.as_deref(), output.as_deref())
            .map_err(|e| e.into()),
        CliCommand::DiscoverDeps {
            command,
            strace,
            working_dir,
            json,
        } => {
            if !strace {
                eprintln!("Error: --strace flag is required. Currently strace is the only supported discovery method.");
                return finalize_exit_code(&subcommand_name, cmd_start.elapsed().as_millis() as u64, 1, &timing_config, timing_start_unix_ms, timing_handle.as_ref());
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
            match commands::test::run_test(all, record, tier, &base, include_untracked, dry_run, prioritize, budget, use_color) {
                Ok(success) => {
                    let code = if success { 0 } else { 1 };
                    return finalize_exit_code(&subcommand_name, cmd_start.elapsed().as_millis() as u64, code, &timing_config, timing_start_unix_ms, timing_handle.as_ref());
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
            memory_limit,
            cache_dir,
            no_cache,
        } => {
            match commands::stale::run_stale(
                &source,
                &spec,
                &output_format,
                request_timeout,
                exec_timeout,
                build_timeout,
                memory_limit,
                log_level,
                cli.project_dir.as_deref(),
                cache_dir.as_deref(),
                no_cache,
            )
            .await
            {
                Ok(all_fresh) => {
                    let code = if all_fresh { 0 } else { 1 };
                    return finalize_exit_code(&subcommand_name, cmd_start.elapsed().as_millis() as u64, code, &timing_config, timing_start_unix_ms, timing_handle.as_ref());
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
                memory_limit,
                log_level,
                cli.project_dir.as_deref(),
            )
            .await
            {
                Ok(all_confirmed) => {
                    let code = if all_confirmed { 0 } else { 1 };
                    return finalize_exit_code(&subcommand_name, cmd_start.elapsed().as_millis() as u64, code, &timing_config, timing_start_unix_ms, timing_handle.as_ref());
                }
                Err(e) => Err(e),
            }
        }
        CliCommand::Telemetry { action } => {
            let dm = cmd_start.elapsed().as_millis() as u64;
            return match commands::telemetry::run_telemetry(&action) {
                Ok(()) => finalize_exit_code(&subcommand_name, dm, 0, &timing_config, timing_start_unix_ms, timing_handle.as_ref()),
                Err(e) => {
                    eprintln!("Error: {e}");
                    queue_command_error_event(&subcommand_name, &*e);
                    finalize_exit_code(&subcommand_name, dm, 1, &timing_config, timing_start_unix_ms, timing_handle.as_ref())
                }
            };
        }
    };

    let duration_ms = cmd_start.elapsed().as_millis() as u64;

    match result {
        Ok(()) => finalize_exit_code(&subcommand_name, duration_ms, 0, &timing_config, timing_start_unix_ms, timing_handle.as_ref()),
        Err(e) => {
            eprintln!("Error: {e}");
            queue_command_error_event(&subcommand_name, &*e);
            finalize_exit_code(&subcommand_name, duration_ms, 1, &timing_config, timing_start_unix_ms, timing_handle.as_ref())
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
    persist_timing_run(subcommand, duration_ms, exit_code, timing_config, timing_start_unix_ms, timing_handle);
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
    debug.split_whitespace()
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
    } else if msg.contains("directory") && (msg.contains("not found") || msg.contains("does not exist")) {
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
