use std::process::ExitCode;

use clap::Parser;

use shatter_core::log_level::LogLevel;

mod args;
mod commands;
mod embedded_frontend;
mod embedded_go_frontend;
mod helpers;

use args::*;
use helpers::*;

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let log_level = cli.effective_log_level();

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
            commands::explore::run_explore(
                &targets,
                max_iterations,
                timeout,
                timeout_explore,
                scope.as_deref(),
                analyze_only,
                show_clusters,
                cache_dir.as_deref(),
                no_cache,
                request_timeout,
                exec_timeout,
                build_timeout,
                inputs.as_deref(),
                config_path.as_deref(),
                output.as_deref(),
                log_level,
                cli.perf,
                &colors,
                spec || spec_json || output.is_some() || invariants,
                spec_json || output.is_some(),
                invariants,
                concolic,
                solver_timeout,
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
            format,
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
                &format,
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
                &seeds_dir,
                no_seeds,
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
                    return if has_regressions {
                        ExitCode::FAILURE
                    } else {
                        ExitCode::SUCCESS
                    };
                }
                Err(e) => Err(e),
            }
        }
        CliCommand::SpecDiff { old, new, json } => {
            match commands::diff::run_spec_diff(&old, &new, json, use_color) {
                Ok(has_regressions) => {
                    return if has_regressions {
                        ExitCode::FAILURE
                    } else {
                        ExitCode::SUCCESS
                    };
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
                return ExitCode::FAILURE;
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
                    return if success {
                        ExitCode::SUCCESS
                    } else {
                        ExitCode::FAILURE
                    };
                }
                Err(e) => Err(e),
            }
        }
        CliCommand::Stale {
            source,
            spec,
            format,
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
                &format,
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
                    if all_fresh {
                        return ExitCode::SUCCESS;
                    }
                    return ExitCode::FAILURE;
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
            format,
        } => {
            match commands::revalidate::run_revalidate(
                &source,
                cache_dir.as_deref(),
                &format,
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
                    if all_confirmed {
                        return ExitCode::SUCCESS;
                    }
                    return ExitCode::FAILURE;
                }
                Err(e) => Err(e),
            }
        }
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: {e}");
            ExitCode::FAILURE
        }
    }
}
