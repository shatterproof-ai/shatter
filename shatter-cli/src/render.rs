//! Askama-based Markdown rendering for CLI output.
//!
//! View models convert domain types (`ObservationOutput`, `ParallelScanResult`)
//! into structs that Askama templates render as Markdown for termimad.

use askama::Template;

use shatter_core::explorer::ObservationOutput;
use shatter_core::scan_orchestrator::{ParallelScanResult, SkipCategory};

// ── View models ──────────────────────────────────────────────────────────────

/// View model for a single function's exploration result.
#[derive(Template)]
#[template(path = "explore_fn.md")]
pub(crate) struct ExploreFnView {
    pub function_name: String,
    /// Formatted location suffix, e.g. " *(src/foo.ts:10-30)*", or empty.
    pub location: String,
    /// Pre-formatted summary, e.g. "**5 paths** · **80%** coverage (80/100 lines)".
    pub summary: String,
    pub paths: Vec<PathView>,
    /// Extra info lines, e.g. ["Mocks: dep_a, dep_b", "Explorer: concolic (Z3-backed)"].
    pub extras: Vec<String>,
}

/// One explored execution path.
pub(crate) struct PathView {
    pub index: usize,
    /// Function call with inputs, e.g. "add(1, 2)".
    pub call: String,
    /// Outcome label, e.g. "returns `3`" or "throws `Error`".
    pub outcome: String,
}

/// View model for a full parallel scan result.
#[derive(Template)]
#[template(path = "scan.md")]
pub(crate) struct ScanView {
    pub functions: Vec<FnScanView>,
    pub skipped_expected: Vec<SkippedView>,
    pub skipped_errors: Vec<SkippedView>,
    pub workers_used: usize,
    /// Pre-formatted sampling line, or empty string if no sampling was active.
    pub sampling_info: String,
    pub total_tested: usize,
}

/// Per-function summary row in a scan result.
pub(crate) struct FnScanView {
    pub function_name: String,
    pub unique_paths: usize,
    /// Pre-formatted coverage string, e.g. "80%" or "n/a".
    pub coverage: String,
}

/// A function that was skipped during a scan.
pub(crate) struct SkippedView {
    pub function_name: String,
    pub reason: String,
}

// ── Options ──────────────────────────────────────────────────────────────────

/// Options needed when building an [`ExploreFnView`].
pub(crate) struct ExploreRenderOpts<'a> {
    /// Location string, e.g. "src/foo.ts:10-30".
    pub location: Option<&'a str>,
    /// Mock symbols used (already formatted as simple names).
    pub mocks_used: &'a [String],
    pub is_concolic: bool,
}

// ── Builders ─────────────────────────────────────────────────────────────────

/// Build an [`ExploreFnView`] from an [`ObservationOutput`] and render options.
pub(crate) fn explore_fn_view(
    result: &ObservationOutput,
    opts: ExploreRenderOpts<'_>,
) -> ExploreFnView {
    let location = opts
        .location
        .map(|loc| format!(" *({loc})*"))
        .unwrap_or_default();

    let summary = if result.total_lines > 0 {
        let pct = ((result.lines_covered as f64 / result.total_lines as f64) * 100.0)
            .min(100.0)
            .round() as u32;
        format!(
            "**{} path(s)** · **{}%** coverage ({}/{} lines)",
            result.unique_paths, pct, result.lines_covered, result.total_lines,
        )
    } else {
        format!("**{} path(s)**", result.unique_paths)
    };

    let paths = result
        .new_path_executions
        .iter()
        .enumerate()
        .map(|(i, exec)| {
            let inputs = exec
                .inputs
                .iter()
                .map(value_short)
                .collect::<Vec<_>>()
                .join(", ");
            let call = format!("{}({})", result.function_name, inputs);
            let outcome = if let Some(ref err) = exec.thrown_error {
                format!("throws `{err}`")
            } else if let Some(ref val) = exec.return_value {
                format!("returns `{}`", value_short(val))
            } else {
                "void".to_string()
            };
            PathView {
                index: i + 1,
                call,
                outcome,
            }
        })
        .collect();

    let mut extras = Vec::new();
    if !opts.mocks_used.is_empty() {
        extras.push(format!("Mocks: {}", opts.mocks_used.join(", ")));
    }
    if opts.is_concolic {
        extras.push("Explorer: concolic (Z3-backed)".to_string());
    }

    ExploreFnView {
        function_name: result.function_name.clone(),
        location,
        summary,
        paths,
        extras,
    }
}

/// Build a [`ScanView`] from a [`ParallelScanResult`].
pub(crate) fn scan_view(result: &ParallelScanResult) -> ScanView {
    let functions = result
        .function_results
        .iter()
        .map(|fr| {
            let coverage = if fr.exploration.total_lines > 0 {
                let pct = ((fr.exploration.lines_covered as f64
                    / fr.exploration.total_lines as f64)
                    * 100.0)
                    .min(100.0)
                    .round() as u32;
                format!("{pct}%")
            } else {
                "n/a".to_string()
            };
            FnScanView {
                function_name: fr.function_name.clone(),
                unique_paths: fr.exploration.unique_paths,
                coverage,
            }
        })
        .collect();

    let (expected, errors): (Vec<_>, Vec<_>) = result
        .skipped
        .iter()
        .partition(|s| s.category == SkipCategory::Expected);

    let to_view = |s: &&shatter_core::scan_orchestrator::SkippedFunction| SkippedView {
        function_name: s.function_name.clone(),
        reason: s.reason.clone(),
    };

    let sampling_info = result
        .sampling
        .as_ref()
        .map(|ctx| {
            let pct = if ctx.total_functions > 0 {
                ((ctx.sampled_functions as f64 / ctx.total_functions as f64) * 100.0).round()
                    as usize
            } else {
                0
            };
            let explored = ctx.sampled_functions + ctx.closure_functions;
            format!(
                "Core sample: **{explored}/{}** functions ({pct}% sampled, {} via closure)",
                ctx.total_functions, ctx.closure_functions,
            )
        })
        .unwrap_or_default();

    ScanView {
        total_tested: result.function_results.len(),
        functions,
        skipped_expected: expected.iter().map(to_view).collect(),
        skipped_errors: errors.iter().map(to_view).collect(),
        workers_used: result.workers_used,
        sampling_info,
    }
}

// ── Rendering ────────────────────────────────────────────────────────────────

/// Render an explore function view to a Markdown string.
pub(crate) fn render_explore_fn(view: &ExploreFnView) -> String {
    view.render()
        .unwrap_or_else(|e| format!("[render error: {e}]"))
}

/// Render a scan view to a Markdown string.
pub(crate) fn render_scan(view: &ScanView) -> String {
    view.render()
        .unwrap_or_else(|e| format!("[render error: {e}]"))
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn value_short(v: &serde_json::Value) -> String {
    let s = v.to_string();
    if s.len() > 40 {
        format!("{}...", &s[..37])
    } else {
        s
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use shatter_core::explorer::{ExecutionSummary, ObservationOutput};

    fn make_observation(name: &str, paths: usize) -> ObservationOutput {
        ObservationOutput {
            function_name: name.to_string(),
            iterations: 10,
            unique_paths: paths,
            lines_covered: 8,
            total_lines: 10,
            new_path_executions: vec![ExecutionSummary {
                inputs: vec![serde_json::json!(1)],
                return_value: Some(serde_json::json!(42)),
                thrown_error: None,
                lines_executed: vec![],
                is_new_path: true,
                error_intent: None,
            }],
            raw_results: vec![],
            discoveries: vec![],
            nondeterministic_fields: vec![],
            float_probe_results: vec![],
            boundary_results: vec![],
            shrunk_witnesses: Default::default(),
            shrink_stats: Default::default(),
            mcdc_summary: None,
            abandoned_frontiers: vec![],
            opaque_suggestions: vec![],
            stubbed_modules: vec![],
        }
    }

    #[test]
    fn explore_fn_view_produces_valid_markdown() {
        let obs = make_observation("add", 3);
        let view = explore_fn_view(
            &obs,
            ExploreRenderOpts {
                location: Some("src/math.ts:1-10"),
                mocks_used: &[],
                is_concolic: false,
            },
        );
        let md = render_explore_fn(&view);
        assert!(md.contains("add"), "should contain function name");
        assert!(md.contains("src/math.ts:1-10"), "should contain location");
        assert!(md.contains("3 path(s)"), "should contain path count");
        assert!(md.contains("80%"), "should contain coverage");
    }

    #[test]
    fn explore_fn_view_no_location() {
        let obs = make_observation("fn", 1);
        let view = explore_fn_view(
            &obs,
            ExploreRenderOpts {
                location: None,
                mocks_used: &[],
                is_concolic: false,
            },
        );
        let md = render_explore_fn(&view);
        assert!(md.contains("fn"), "should contain function name");
        assert!(!md.contains("*("), "should have no location suffix");
    }

    #[test]
    fn explore_fn_view_with_mocks_and_concolic() {
        let obs = make_observation("fetchUser", 2);
        let mocks = vec!["db.query".to_string(), "cache.get".to_string()];
        let view = explore_fn_view(
            &obs,
            ExploreRenderOpts {
                location: None,
                mocks_used: &mocks,
                is_concolic: true,
            },
        );
        let md = render_explore_fn(&view);
        assert!(
            md.contains("Mocks: db.query, cache.get"),
            "should list mocks"
        );
        assert!(md.contains("concolic"), "should mention concolic explorer");
    }

    #[test]
    fn value_short_truncates_long_strings() {
        let long = serde_json::json!("this is a very long string that exceeds forty characters");
        let s = value_short(&long);
        assert!(s.len() <= 43);
        assert!(s.ends_with("..."));
    }

    #[test]
    fn value_short_keeps_short_values() {
        assert_eq!(value_short(&serde_json::json!(42)), "42");
        assert_eq!(value_short(&serde_json::json!("hi")), "\"hi\"");
        assert_eq!(value_short(&serde_json::json!(null)), "null");
    }
}
