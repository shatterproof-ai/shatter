use shatter_core::test_impact::{self, CoverageMap};
use shatter_core::test_runner::{self, TestTier};

/// Run tests with impact analysis, tier execution, or coverage recording.
#[allow(clippy::too_many_arguments)] // CLI dispatch — one param per flag
pub(crate) fn run_test(
    all: bool,
    record: bool,
    tier: Option<String>,
    base: &str,
    include_untracked: bool,
    dry_run: bool,
    prioritize: bool,
    budget: Option<std::time::Duration>,
    use_color: bool,
) -> Result<bool, Box<dyn std::error::Error>> {
    let project_root = std::env::current_dir()?;
    let shatter_dir = project_root.join(".shatter-cache");

    // --- Tier mode: run predefined tier commands ---
    if let Some(tier_str) = &tier {
        let tier_val: TestTier = tier_str
            .parse()
            .map_err(|e: String| -> Box<dyn std::error::Error> { e.into() })?;
        eprintln!("Running {} tier...", tier_val);
        let success = test_runner::run_tier(tier_val, &project_root)?;
        if success {
            let commit = test_runner::git_head_commit(&project_root).unwrap_or_default();
            test_impact::write_tier_marker(&shatter_dir, tier_val.as_str(), &commit)?;
            eprintln!("Tier {tier_val} passed. Marker written.");
        } else {
            eprintln!("Tier {tier_val} FAILED.");
        }
        return Ok(success);
    }

    // --- Record mode: run all tests with coverage instrumentation ---
    if record {
        eprintln!("Recording coverage...");
        let runners = test_runner::detect_runners(&project_root);
        if runners.is_empty() {
            eprintln!("No test runners detected.");
            return Ok(false);
        }

        let mut map = CoverageMap::empty();
        let mut all_success = true;

        for runner in &runners {
            eprintln!("  Running {} in {}...", runner.kind, runner.root.display());
            match test_runner::run_with_coverage(runner, &project_root) {
                Ok(coverage) => {
                    if !coverage.run_result.success {
                        eprintln!("  {} tests FAILED", runner.kind);
                        all_success = false;
                    }
                    map.update_from_coverage(&coverage.test_file_map, &project_root)?;
                }
                Err(e) => {
                    eprintln!("  {} coverage failed: {e}", runner.kind);
                    all_success = false;
                }
            }
        }

        map.save(&shatter_dir)?;
        eprintln!(
            "Coverage map saved ({} test entries).",
            map.data.entries.len()
        );
        return Ok(all_success);
    }

    // --- All mode: run all tests without filtering ---
    if all {
        eprintln!("Running all tests...");
        let runners = test_runner::detect_runners(&project_root);
        let mut all_success = true;
        for runner in &runners {
            eprintln!("  Running {} in {}...", runner.kind, runner.root.display());
            let result = test_runner::run_tests(runner, &[])?;
            if !result.success {
                eprintln!(
                    "  {} FAILED ({:.1}s)",
                    runner.kind,
                    result.duration.as_secs_f64()
                );
                all_success = false;
            } else {
                eprintln!(
                    "  {} passed ({:.1}s)",
                    runner.kind,
                    result.duration.as_secs_f64()
                );
            }
        }
        return Ok(all_success);
    }

    // --- Default: impact analysis ---
    let map = match CoverageMap::load(&shatter_dir) {
        Ok(m) => m,
        Err(test_impact::TiaError::NoCoverageMap { .. }) => {
            eprintln!("No coverage map found. Run `shatter test --record` first to build one.");
            return Ok(false);
        }
        Err(e) => return Err(e.into()),
    };

    let scm_provider = shatter_core::scm::detect_provider(&project_root)?;
    use shatter_core::scm::ScmProvider;
    let changed = if base == "HEAD" {
        scm_provider.changed_files(&project_root, include_untracked)?
    } else {
        let mut files = scm_provider.diff_files(&project_root, base)?;
        if include_untracked {
            let uncommitted = scm_provider.changed_files(&project_root, true)?;
            for f in uncommitted {
                if !files.contains(&f) {
                    files.push(f);
                }
            }
        }
        files
    };

    // Convert to relative paths
    let changed_relative: Vec<String> = changed
        .iter()
        .filter_map(|p| p.strip_prefix(&project_root).ok())
        .map(|p| p.to_string_lossy().to_string())
        .collect();

    if changed_relative.is_empty() {
        println!("No changed files detected. Nothing to test.");
        return Ok(true);
    }

    let query = map.query_affected(&changed_relative);

    if dry_run {
        let header = format!(
            "{} changed file(s), {} affected test(s):",
            query.changed_files.len(),
            query.affected_tests.len()
        );
        if use_color {
            println!("\x1b[1m{header}\x1b[0m");
        } else {
            println!("{header}");
        }
        println!();
        println!("Changed files:");
        for f in &query.changed_files {
            println!("  {f}");
        }
        println!();
        println!("Affected tests:");
        for t in &query.affected_tests {
            println!("  {t}");
        }
        if !query.unmapped_files.is_empty() {
            println!();
            println!("Unmapped files (not in coverage map):");
            for f in &query.unmapped_files {
                println!("  {f}");
            }
        }
        return Ok(true);
    }

    if query.affected_tests.is_empty() {
        if query.unmapped_files.is_empty() {
            println!("No affected tests found. All changes are in untested files.");
        } else {
            println!(
                "No affected tests found. {} file(s) not in coverage map — consider `shatter test --record`.",
                query.unmapped_files.len()
            );
        }
        return Ok(true);
    }

    // --- Prioritization: order tests by marginal coverage / time ---
    let effective_prioritize = prioritize || budget.is_some();
    let affected_tests = if effective_prioritize {
        use shatter_core::test_prioritization;

        // Build test cases from coverage map, then filter to affected tests
        let durations = std::collections::BTreeMap::new();
        let result = test_prioritization::prioritize_affected(
            &map,
            &query.affected_tests,
            &durations,
            &test_prioritization::PrioritizeConfig {
                budget: budget.unwrap_or(std::time::Duration::ZERO),
                use_recency: true,
            },
            Some(&project_root),
        )
        .map_err(|e| -> Box<dyn std::error::Error> { e.to_string().into() })?;

        let report = test_prioritization::format_prioritize_report(&result, use_color);
        print!("{report}");

        result
            .ordered
            .iter()
            .map(|r| r.test.id.clone())
            .collect::<Vec<_>>()
    } else {
        query.affected_tests.clone()
    };

    eprintln!(
        "Running {} affected test(s) for {} changed file(s)...",
        affected_tests.len(),
        query.changed_files.len()
    );

    let runners = test_runner::detect_runners(&project_root);
    let mut all_success = true;
    for runner in &runners {
        // Filter tests relevant to this runner
        let runner_prefix = match runner.kind {
            test_runner::RunnerKind::Cargo => "",
            test_runner::RunnerKind::Vitest => "shatter-ts",
            test_runner::RunnerKind::GoTest => "shatter-go",
        };

        let runner_tests: Vec<String> = if runner_prefix.is_empty() {
            affected_tests.clone()
        } else {
            affected_tests
                .iter()
                .filter(|t| t.contains(runner_prefix))
                .cloned()
                .collect()
        };

        if runner_tests.is_empty() {
            continue;
        }

        eprintln!(
            "  Running {} ({} test(s))...",
            runner.kind,
            runner_tests.len()
        );
        let result = test_runner::run_tests(runner, &runner_tests)?;
        if !result.success {
            eprintln!(
                "  {} FAILED ({:.1}s)",
                runner.kind,
                result.duration.as_secs_f64()
            );
            all_success = false;
        } else {
            eprintln!(
                "  {} passed ({:.1}s)",
                runner.kind,
                result.duration.as_secs_f64()
            );
        }
    }

    Ok(all_success)
}
