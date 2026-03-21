//! View model types and helper functions for Askama HTML templates.
//!
//! This module owns the data-shaping layer between raw engine types and the
//! template rendering layer. All HTML-unsafe values are escaped here before
//! being handed to templates as pre-rendered fragments (marked `|safe`).

use askama::Template;

use crate::explorer::{ExecutionSummary, ObservationOutput};
use crate::report::{DiscoveredInput, ScanReport};

// ---------------------------------------------------------------------------
// Re-export html_escape from report so we don't duplicate the logic.
// ---------------------------------------------------------------------------

use crate::report::html_escape;

// ---------------------------------------------------------------------------
// Coverage helpers
// ---------------------------------------------------------------------------

/// Return a CSS class name for a coverage percentage.
pub(crate) fn coverage_class(pct: f64) -> &'static str {
    if pct >= 80.0 {
        "cov-high"
    } else if pct >= 50.0 {
        "cov-mid"
    } else {
        "cov-low"
    }
}

/// Render a coverage bar widget as an HTML string.
pub(crate) fn render_cov_bar(pct: f64) -> String {
    let cls = coverage_class(pct);
    let width = pct.clamp(0.0, 100.0) as u32;
    format!(
        r#"<span class="cov-bar-wrap {cls}"><span class="cov-bar"><span class="cov-fill" style="width:{width}%"></span></span><span class="pct">{pct:.0}%</span></span>"#
    )
}

// ---------------------------------------------------------------------------
// Per-path view model
// ---------------------------------------------------------------------------

/// View model for a single execution path row in the explore function template.
pub(crate) struct PathEntry {
    /// 1-based row index.
    pub index: usize,
    /// Pre-rendered comma-separated `<code>` inputs (HTML-safe).
    pub inputs_html: String,
    /// Pre-rendered outcome span (HTML-safe).
    pub outcome_html: String,
}

/// Build the inputs HTML for a single `ExecutionSummary`.
pub(crate) fn format_inputs(exec: &ExecutionSummary) -> String {
    exec.inputs
        .iter()
        .map(|v| format!("<code>{}</code>", html_escape(&v.to_string())))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Build the outcome HTML for a single `ExecutionSummary`.
pub(crate) fn format_outcome(exec: &ExecutionSummary) -> String {
    if let Some(ref err) = exec.thrown_error {
        format!(
            r#"<span class="outcome-throw">throws</span> <code>{}</code>"#,
            html_escape(err)
        )
    } else if let Some(ref val) = exec.return_value {
        format!(
            r#"<span class="outcome-return">returns</span> <code>{}</code>"#,
            html_escape(&val.to_string())
        )
    } else {
        r#"<span class="outcome-void">void</span>"#.to_string()
    }
}

// ---------------------------------------------------------------------------
// Askama template
// ---------------------------------------------------------------------------

/// Askama template for a single explored function `<details>` fragment.
#[derive(Template)]
#[template(path = "explore_fn.html")]
pub(crate) struct ExploreFnTemplate<'a> {
    pub fn_name: &'a str,
    pub location: &'a str,
    pub cov_bar_html: String,
    pub iterations: u32,
    pub unique_paths: usize,
    pub lines_covered: usize,
    pub total_lines: u32,
    pub paths: Vec<PathEntry>,
}

// ---------------------------------------------------------------------------
// Explore page template
// ---------------------------------------------------------------------------

/// Askama template for the full explore report HTML page.
#[derive(Template)]
#[template(path = "explore_page.html")]
pub(crate) struct ExplorePageTemplate<'a> {
    pub fn_count: usize,
    pub total_paths: usize,
    /// Pre-rendered coverage bar HTML (HTML-safe).
    pub cov_bar_html: String,
    /// Pre-rendered `<details>` fragments (HTML-safe).
    pub fragments: &'a [String],
}

/// Render a full explore report HTML page from fragments and summary stats.
///
/// This is the Askama-backed implementation called by `report::wrap_explore_html`.
pub fn render_explore_page(
    fragments: &[String],
    fn_count: usize,
    total_paths: usize,
    total_covered: usize,
    total_lines: u32,
) -> String {
    let cov_pct = if total_lines > 0 {
        (total_covered as f64 / total_lines as f64 * 100.0).min(100.0)
    } else {
        0.0
    };

    let tmpl = ExplorePageTemplate {
        fn_count,
        total_paths,
        cov_bar_html: render_cov_bar(cov_pct),
        fragments,
    };

    tmpl.render().expect("ExplorePageTemplate rendering failed")
}

/// Render the HTML `<details>` fragment for a single explored function.
///
/// This is the Askama-backed implementation called by `report::render_explore_fn_html`.
pub fn render_explore_fn(result: &ObservationOutput, location: &str) -> String {
    let cov_pct = if result.total_lines > 0 {
        (result.lines_covered as f64 / result.total_lines as f64 * 100.0).min(100.0)
    } else {
        0.0
    };

    let paths: Vec<PathEntry> = result
        .new_path_executions
        .iter()
        .enumerate()
        .map(|(i, exec)| PathEntry {
            index: i + 1,
            inputs_html: format_inputs(exec),
            outcome_html: format_outcome(exec),
        })
        .collect();

    let tmpl = ExploreFnTemplate {
        fn_name: &result.function_name,
        location,
        cov_bar_html: render_cov_bar(cov_pct),
        iterations: result.iterations,
        unique_paths: result.unique_paths,
        lines_covered: result.lines_covered,
        total_lines: result.total_lines,
        paths,
    };

    tmpl.render().expect("ExploreFnTemplate rendering failed")
}

// ---------------------------------------------------------------------------
// Scan report view models
// ---------------------------------------------------------------------------

/// View model for a single function row and detail block in the scan report.
pub(crate) struct ScanFnView {
    /// HTML-escaped function name (rendered with `|safe` in the template).
    pub fn_name: String,
    /// HTML-escaped file path (rendered with `|safe` in the template).
    pub file_path: String,
    /// Number of unique paths discovered.
    pub paths_count: usize,
    /// Pre-rendered coverage bar HTML (HTML-safe).
    pub cov_bar_html: String,
    /// Number of exploration iterations.
    pub iterations: u32,
    /// Number of source lines covered.
    pub lines_covered: usize,
    /// Total source lines in the function.
    pub total_lines: u32,
    /// Per-path rows for the inputs table.
    pub discovered_inputs: Vec<PathEntry>,
    /// Pre-rendered mocks line (`Mocks: name1, name2`), or `None` if no mocks.
    pub mocks_html: Option<String>,
}

/// View model for a skipped function entry.
pub(crate) struct SkippedView {
    /// HTML-escaped function name (rendered with `|safe` in the template).
    pub fn_name: String,
    /// HTML-escaped skip reason (rendered with `|safe` in the template).
    pub reason: String,
}

// ---------------------------------------------------------------------------
// Scan report template
// ---------------------------------------------------------------------------

/// Askama template for the full scan report HTML page.
#[derive(Template)]
#[template(path = "scan_report.html")]
pub(crate) struct ScanReportTemplate {
    pub total_fn: usize,
    pub total_paths: usize,
    pub skipped_count: usize,
    /// Pre-rendered overall coverage bar HTML (HTML-safe).
    pub overall_cov_bar_html: String,
    pub functions: Vec<ScanFnView>,
    pub skipped: Vec<SkippedView>,
}

/// Build a `PathEntry` from a `DiscoveredInput`.
fn format_discovered_input(inp: &DiscoveredInput) -> PathEntry {
    // Re-use the same HTML helpers so output is identical to the old code.
    PathEntry {
        index: 0, // caller sets the 1-based index
        inputs_html: inp
            .inputs
            .iter()
            .map(|v| format!("<code>{}</code>", html_escape(&v.to_string())))
            .collect::<Vec<_>>()
            .join(", "),
        outcome_html: if let Some(ref err) = inp.thrown_error {
            format!(
                r#"<span class="outcome-throw">throws</span> <code>{}</code>"#,
                html_escape(err)
            )
        } else if let Some(ref val) = inp.return_value {
            format!(
                r#"<span class="outcome-return">returns</span> <code>{}</code>"#,
                html_escape(&val.to_string())
            )
        } else {
            r#"<span class="outcome-void">void</span>"#.to_string()
        },
    }
}

/// Render the full scan report HTML page from a `ScanReport`.
///
/// This is the Askama-backed implementation called by
/// `report::generate_html_scan_report`.
pub fn render_scan_report(report: &ScanReport) -> String {
    let total_fn = report.codebase.total_functions;
    let total_paths: usize = report.functions.iter().map(|f| f.branches_covered).sum();
    let skipped_count = report.codebase.skipped_functions.len();
    let overall_cov_bar_html = render_cov_bar(report.codebase.overall_coverage);

    let functions: Vec<ScanFnView> = report
        .functions
        .iter()
        .map(|f| {
            let discovered_inputs: Vec<PathEntry> = f
                .discovered_inputs
                .iter()
                .enumerate()
                .map(|(i, inp)| {
                    let mut entry = format_discovered_input(inp);
                    entry.index = i + 1;
                    entry
                })
                .collect();

            let mocks_html = if f.mocks_used.is_empty() {
                None
            } else {
                let escaped: Vec<String> =
                    f.mocks_used.iter().map(|m| html_escape(m)).collect();
                Some(escaped.join(", "))
            };

            ScanFnView {
                fn_name: html_escape(&f.function_name),
                file_path: html_escape(&f.file_path),
                paths_count: f.branches_covered,
                cov_bar_html: render_cov_bar(f.coverage_pct),
                iterations: f.iterations,
                lines_covered: f.lines_covered,
                total_lines: f.total_lines,
                discovered_inputs,
                mocks_html,
            }
        })
        .collect();

    let skipped: Vec<SkippedView> = report
        .codebase
        .skipped_functions
        .iter()
        .map(|s| SkippedView {
            fn_name: html_escape(&s.function_name),
            reason: html_escape(&s.reason),
        })
        .collect();

    let tmpl = ScanReportTemplate {
        total_fn,
        total_paths,
        skipped_count,
        overall_cov_bar_html,
        functions,
        skipped,
    };

    tmpl.render().expect("ScanReportTemplate rendering failed")
}
