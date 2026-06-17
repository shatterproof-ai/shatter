//! View model types and helper functions for Askama HTML templates.
//!
//! This module owns the data-shaping layer between raw engine types and the
//! template rendering layer. All HTML-unsafe values are escaped here before
//! being handed to templates as pre-rendered fragments (marked `|safe`).

use std::collections::HashSet;
use std::path::Path;

use askama::Template;

use crate::explorer::{ExecutionSummary, ObservationOutput};
use crate::report::{DiscoveredInput, ScanReport};

// ---------------------------------------------------------------------------
// HTML escaping
// ---------------------------------------------------------------------------

/// HTML-escape a string for safe embedding in HTML content.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

// ---------------------------------------------------------------------------
// Source code display helpers
// ---------------------------------------------------------------------------

/// Parse a location string of the form `"path/to/file.ts:10-30"` into its
/// components. Returns `None` if the string does not match the expected format.
fn parse_location(location: &str) -> Option<(&str, u32, u32)> {
    let (path, range) = location.rsplit_once(':')?;
    let (start, end) = range.split_once('-')?;
    let start_line: u32 = start.parse().ok()?;
    let end_line: u32 = end.parse().ok()?;
    Some((path, start_line, end_line))
}

/// Build an HTML fragment of annotated source lines for embedding in a
/// `.src-block` div. Each line gets a `covered` or `uncovered` CSS class.
///
/// Returns `None` when the file cannot be read or the line range is invalid.
fn render_source_block(
    file_path: &str,
    project_root: Option<&Path>,
    start_line: u32,
    end_line: u32,
    covered: &HashSet<u32>,
) -> Option<String> {
    if start_line == 0 || end_line < start_line {
        return None;
    }

    let resolved = if let Some(root) = project_root {
        root.join(file_path)
    } else {
        std::path::PathBuf::from(file_path)
    };

    let contents = std::fs::read_to_string(&resolved).ok()?;
    let all_lines: Vec<&str> = contents.lines().collect();

    let start_idx = (start_line as usize).saturating_sub(1);
    let end_idx = (end_line as usize).min(all_lines.len());
    if start_idx >= all_lines.len() {
        return None;
    }

    let mut html = String::new();
    for (i, line_text) in all_lines[start_idx..end_idx].iter().enumerate() {
        let lineno = start_line + i as u32;
        let cls = if covered.contains(&lineno) {
            "covered"
        } else {
            "uncovered"
        };
        html.push_str(&format!(
            r#"<div class="src-line {cls}" data-line="{lineno}"><span class="src-ln {cls}">{lineno}</span><span class="src-text">{}</span></div>"#,
            html_escape(line_text)
        ));
    }
    Some(html)
}

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
    /// Comma-separated line numbers executed by this path (e.g. `"10,11,23"`).
    /// Empty string when no line data is available.
    pub lines_executed_csv: String,
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
    /// Pre-rendered source code block HTML, or `None` if source is unavailable.
    pub source_code_html: Option<String>,
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
/// `project_root` is used to resolve relative file paths in `location`.
/// Passing `None` falls back to treating `file_path` as an absolute path.
///
/// This is the Askama-backed implementation called by `report::render_explore_fn_html`.
pub fn render_explore_fn(
    result: &ObservationOutput,
    location: &str,
    project_root: Option<&Path>,
) -> String {
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
            lines_executed_csv: exec
                .lines_executed
                .iter()
                .map(|n| n.to_string())
                .collect::<Vec<_>>()
                .join(","),
        })
        .collect();

    // Build source block: parse location, aggregate covered lines, render.
    let source_code_html =
        parse_location(location).and_then(|(file_path, start_line, end_line)| {
            let covered: HashSet<u32> = result
                .new_path_executions
                .iter()
                .flat_map(|exec| exec.lines_executed.iter().copied())
                .collect();
            render_source_block(file_path, project_root, start_line, end_line, &covered)
        });

    let tmpl = ExploreFnTemplate {
        fn_name: &result.function_name,
        location,
        cov_bar_html: render_cov_bar(cov_pct),
        iterations: result.iterations,
        unique_paths: result.unique_paths,
        lines_covered: result.lines_covered,
        total_lines: result.total_lines,
        paths,
        source_code_html,
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
    /// Pre-rendered source code block HTML, or `None` if source is unavailable.
    pub source_code_html: Option<String>,
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
        lines_executed_csv: inp
            .lines_executed
            .iter()
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join(","),
    }
}

/// Render the full scan report HTML page from a `ScanReport`.
///
/// `project_root` is used to resolve relative file paths when reading source
/// files. Passing `None` treats file paths as absolute.
///
/// This is the Askama-backed implementation called by
/// `report::generate_html_scan_report`.
pub fn render_scan_report(report: &ScanReport, project_root: Option<&Path>) -> String {
    // str-jeen.46: render shows completed exploration count. Use
    // `completed_functions` (the v1 `total_functions` was renamed when
    // the schema gained explicit attempted/failed/skipped/unsupported
    // counts).
    let total_fn = report.codebase.completed_functions;
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
                    f.mocks_used.iter().map(|m| html_escape(&m.name)).collect();
                Some(escaped.join(", "))
            };

            // Aggregate all covered lines across every discovered input.
            let covered: HashSet<u32> = f
                .discovered_inputs
                .iter()
                .flat_map(|inp| inp.lines_executed.iter().copied())
                .collect();

            // Infer start_line from the minimum covered line number (the
            // function's first line is always executed when it is called).
            let source_code_html = covered.iter().copied().min().and_then(|start_line| {
                let end_line = start_line + f.total_lines.saturating_sub(1);
                render_source_block(&f.file_path, project_root, start_line, end_line, &covered)
            });

            let function_display_name = report_display_name(&f.display_name, &f.function_name);

            ScanFnView {
                fn_name: html_escape(function_display_name),
                file_path: html_escape(&f.file_path),
                paths_count: f.branches_covered,
                cov_bar_html: render_cov_bar(f.coverage_pct),
                iterations: f.iterations,
                lines_covered: f.lines_covered,
                total_lines: f.total_lines,
                discovered_inputs,
                mocks_html,
                source_code_html,
            }
        })
        .collect();

    let skipped: Vec<SkippedView> = report
        .codebase
        .skipped_functions
        .iter()
        .map(|s| {
            let function_display_name = report_display_name(&s.display_name, &s.function_name);

            SkippedView {
                fn_name: html_escape(function_display_name),
                reason: html_escape(&s.reason),
            }
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

fn report_display_name<'a>(display_name: &'a str, function_name: &'a str) -> &'a str {
    if display_name.is_empty() {
        function_name
    } else {
        display_name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::explorer::{ExecutionSummary, ObservationOutput};
    use crate::report::{
        CodebaseReport, ConstraintStats, DiscoveredInput, FunctionReport, ScanReport,
        SkippedFunctionReport,
    };
    use proptest::prelude::*;
    use serde_json::json;

    /// A token guaranteed to appear in `templates/includes/style.html`. Used to
    /// assert that the shared stylesheet is wired into a rendered page.
    const STYLE_MARKER: &str = "box-sizing: border-box";

    // -----------------------------------------------------------------------
    // html_escape
    // -----------------------------------------------------------------------

    #[test]
    fn html_escape_escapes_all_special_chars() {
        assert_eq!(
            html_escape(r#"<a href="x">b & c</a>"#),
            "&lt;a href=&quot;x&quot;&gt;b &amp; c&lt;/a&gt;"
        );
    }

    #[test]
    fn html_escape_leaves_plain_text_untouched() {
        assert_eq!(html_escape("hello world 123"), "hello world 123");
    }

    #[test]
    fn html_escape_does_not_double_escape_ampersand() {
        // '&' is replaced first, so the entities it produces for the other
        // special characters are not re-escaped.
        assert_eq!(html_escape("&lt;"), "&amp;lt;");
    }

    proptest! {
        /// Core security invariant: no raw `<` may survive escaping, because
        /// none of the entities `html_escape` emits contain `<`.
        #[test]
        fn html_escape_never_emits_raw_lt(s in any::<String>()) {
            let escaped = html_escape(&s);
            prop_assert!(!escaped.contains('<'), "raw '<' survived: {escaped:?}");
        }

        /// Every `&` in the output must begin a recognized entity — i.e. there
        /// is no unescaped ampersand that could start an injected entity.
        #[test]
        fn html_escape_amp_always_starts_entity(s in any::<String>()) {
            let escaped = html_escape(&s);
            for (idx, _) in escaped.match_indices('&') {
                let rest = &escaped[idx..];
                prop_assert!(
                    rest.starts_with("&amp;")
                        || rest.starts_with("&lt;")
                        || rest.starts_with("&gt;")
                        || rest.starts_with("&quot;"),
                    "unescaped '&' at byte {idx} in {escaped:?}"
                );
            }
        }

        /// Raw double-quotes must not survive, so escaped values are safe inside
        /// double-quoted HTML attributes.
        #[test]
        fn html_escape_never_emits_raw_quote(s in any::<String>()) {
            let escaped = html_escape(&s);
            prop_assert!(!escaped.contains('"'), "raw '\"' survived: {escaped:?}");
        }
    }

    // -----------------------------------------------------------------------
    // parse_location
    // -----------------------------------------------------------------------

    #[test]
    fn parse_location_valid_and_invalid() {
        assert_eq!(parse_location("a/b.ts:10-20"), Some(("a/b.ts", 10, 20)));
        assert_eq!(parse_location("no-colon"), None);
        assert_eq!(parse_location("file:notrange"), None);
        assert_eq!(parse_location("file:10-"), None);
        assert_eq!(parse_location("file:-20"), None);
    }

    // -----------------------------------------------------------------------
    // coverage helpers
    // -----------------------------------------------------------------------

    #[test]
    fn coverage_class_thresholds() {
        assert_eq!(coverage_class(90.0), "cov-high");
        assert_eq!(coverage_class(80.0), "cov-high");
        assert_eq!(coverage_class(79.9), "cov-mid");
        assert_eq!(coverage_class(50.0), "cov-mid");
        assert_eq!(coverage_class(49.9), "cov-low");
        assert_eq!(coverage_class(0.0), "cov-low");
    }

    proptest! {
        #[test]
        fn render_cov_bar_width_is_clamped(pct in -1000.0f64..1000.0) {
            let html = render_cov_bar(pct);
            let expected_width = pct.clamp(0.0, 100.0) as u32;
            prop_assert!((0..=100).contains(&expected_width));
            prop_assert!(
                html.contains(&format!("width:{expected_width}%")),
                "bar width not clamped: {html}"
            );
        }

        #[test]
        fn render_cov_bar_class_matches_coverage_class(pct in 0.0f64..=100.0) {
            let html = render_cov_bar(pct);
            prop_assert!(html.contains(coverage_class(pct)));
        }
    }

    // -----------------------------------------------------------------------
    // render_source_block
    // -----------------------------------------------------------------------

    #[test]
    fn render_source_block_marks_covered_and_uncovered_lines() {
        let path = std::env::temp_dir().join("shatter_html_templates_src_block_test.txt");
        std::fs::write(&path, "line one\nline two\nline three\n").unwrap();

        let covered: HashSet<u32> = [2u32].into_iter().collect();
        let html = render_source_block(path.to_str().unwrap(), None, 1, 3, &covered)
            .expect("source block should render for a readable file");

        let _ = std::fs::remove_file(&path);

        assert!(html.contains(r#"data-line="1""#));
        assert!(html.contains(r#"data-line="2""#));
        assert!(html.contains(r#"data-line="3""#));
        assert!(html.contains("src-line covered"));
        assert!(html.contains("src-line uncovered"));
        assert!(html.contains("line two"));
    }

    #[test]
    fn render_source_block_rejects_invalid_range() {
        let covered: HashSet<u32> = [1u32].into_iter().collect();
        assert!(render_source_block("whatever", None, 0, 5, &covered).is_none());
        assert!(render_source_block("whatever", None, 5, 2, &covered).is_none());
    }

    #[test]
    fn render_source_block_returns_none_for_missing_file() {
        let covered: HashSet<u32> = HashSet::new();
        assert!(render_source_block("/no/such/file.rs", None, 1, 5, &covered).is_none());
    }

    // -----------------------------------------------------------------------
    // input / outcome formatting (escaping into |safe template fields)
    // -----------------------------------------------------------------------

    fn sample_exec(thrown_error: Option<&str>) -> ExecutionSummary {
        ExecutionSummary {
            inputs: vec![json!(1), json!("x")],
            return_value: Some(json!("ok")),
            thrown_error: thrown_error.map(str::to_string),
            lines_executed: vec![10, 11],
            is_new_path: true,
            error_intent: None,
        }
    }

    #[test]
    fn format_inputs_wraps_and_escapes_values() {
        let exec = ExecutionSummary {
            inputs: vec![json!("<script>"), json!(42)],
            ..sample_exec(None)
        };
        let html = format_inputs(&exec);
        assert!(html.contains("<code>"));
        assert!(html.contains("&lt;script&gt;"));
        assert!(!html.contains("<script>"));
        assert!(html.contains("42"));
    }

    #[test]
    fn format_outcome_throws_escapes_error() {
        let exec = ExecutionSummary {
            return_value: None,
            ..sample_exec(Some("<bad>"))
        };
        let html = format_outcome(&exec);
        assert!(html.contains("outcome-throw"));
        assert!(html.contains("&lt;bad&gt;"));
        assert!(!html.contains("<bad>"));
    }

    #[test]
    fn format_outcome_returns_escapes_value() {
        let exec = ExecutionSummary {
            return_value: Some(json!("<v>")),
            thrown_error: None,
            ..sample_exec(None)
        };
        let html = format_outcome(&exec);
        assert!(html.contains("outcome-return"));
        assert!(html.contains("&lt;v&gt;"));
        assert!(!html.contains("<v>"));
    }

    #[test]
    fn format_outcome_void_when_no_return_or_error() {
        let exec = ExecutionSummary {
            return_value: None,
            thrown_error: None,
            ..sample_exec(None)
        };
        assert!(format_outcome(&exec).contains("outcome-void"));
    }

    fn sample_discovered_input(thrown_error: Option<&str>) -> DiscoveredInput {
        DiscoveredInput {
            inputs: vec![json!(7)],
            return_value: Some(json!("r")),
            thrown_error: thrown_error.map(str::to_string),
            lines_executed: vec![3, 4],
            outcome_status: None,
            outcome_reason: None,
        }
    }

    #[test]
    fn format_discovered_input_escapes_thrown_error() {
        let inp = DiscoveredInput {
            return_value: None,
            ..sample_discovered_input(Some("<x>"))
        };
        let entry = format_discovered_input(&inp);
        assert_eq!(entry.lines_executed_csv, "3,4");
        assert!(entry.outcome_html.contains("outcome-throw"));
        assert!(entry.outcome_html.contains("&lt;x&gt;"));
        assert!(!entry.outcome_html.contains("<x>"));
    }

    // -----------------------------------------------------------------------
    // report_display_name
    // -----------------------------------------------------------------------

    #[test]
    fn report_display_name_prefers_display_name() {
        assert_eq!(report_display_name("Display", "func"), "Display");
        assert_eq!(report_display_name("", "func"), "func");
    }

    // -----------------------------------------------------------------------
    // render_explore_fn / render_explore_page
    // -----------------------------------------------------------------------

    fn sample_observation(name: &str) -> ObservationOutput {
        ObservationOutput {
            function_name: name.to_string(),
            iterations: 3,
            unique_paths: 2,
            lines_covered: 5,
            total_lines: 10,
            new_path_executions: vec![sample_exec(None), sample_exec(Some("boom"))],
            ..Default::default()
        }
    }

    #[test]
    fn render_explore_fn_contains_structure_and_escapes_name() {
        let obs = sample_observation("<script>alert(1)</script>");
        let html = render_explore_fn(&obs, "src/foo.ts:10-20", None);

        // Structural elements.
        assert!(html.contains("<details>"));
        assert!(html.contains("<summary>"));
        assert!(html.contains("src/foo.ts:10-20"));
        assert!(html.contains("cov-bar"));
        assert!(html.contains("iteration(s)"));

        // Function name is rendered without `|safe`, so Askama auto-escapes it.
        // (Askama emits numeric character references, e.g. `<` -> `&#60;`.)
        assert!(!html.contains("<script>alert"));
        assert!(html.contains("&#60;script&#62;"));
    }

    #[test]
    fn render_explore_fn_shows_empty_paths_message() {
        let obs = ObservationOutput {
            function_name: "f".into(),
            total_lines: 4,
            ..Default::default()
        };
        let html = render_explore_fn(&obs, "no-location", None);
        assert!(html.contains("No new paths recorded."));
    }

    #[test]
    fn render_explore_page_includes_stylesheet_and_fragments() {
        let fragments = vec!["<p>FRAGMENT_ALPHA</p>".to_string()];
        let html = render_explore_page(&fragments, 1, 2, 5, 10);

        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("Shatter Explore Report"));
        assert!(html.contains(STYLE_MARKER), "shared stylesheet not wired in");
        // Fragments are emitted with `|safe`, so they pass through verbatim.
        assert!(html.contains("FRAGMENT_ALPHA"));
    }

    // -----------------------------------------------------------------------
    // render_scan_report
    // -----------------------------------------------------------------------

    fn sample_function_report(name: &str) -> FunctionReport {
        FunctionReport {
            function_name: name.to_string(),
            display_name: name.to_string(),
            qualified_id: format!("src/x.ts::{name}"),
            file_path: "src/x.ts".to_string(),
            source_bucket: Default::default(),
            branch_count: 2,
            branches_covered: 2,
            coverage_pct: 75.0,
            discovered_inputs: vec![sample_discovered_input(None)],
            behavior_clusters: vec![],
            constraint_stats: ConstraintStats {
                total_constraints: 0,
                solver_guided_inputs: 0,
            },
            iterations: 4,
            lines_covered: 6,
            total_lines: 8,
            mocks_used: vec![],
            refactoring_recommendations: vec![],
            completion_outcome: Default::default(),
            completion_reason: None,
        }
    }

    fn sample_scan_report(fn_name: &str, skipped_reason: Option<&str>) -> ScanReport {
        let skipped: Vec<SkippedFunctionReport> = skipped_reason
            .map(|r| {
                vec![SkippedFunctionReport {
                    function_name: "skippy".to_string(),
                    display_name: "skippy".to_string(),
                    qualified_id: String::new(),
                    reason: r.to_string(),
                    category: "expected".to_string(),
                }]
            })
            .unwrap_or_default();

        ScanReport {
            version: 6,
            functions: vec![sample_function_report(fn_name)],
            codebase: CodebaseReport {
                completed_functions: 1,
                overall_coverage: 75.0,
                skipped_functions_count: skipped.len(),
                skipped_functions: skipped,
                ..Default::default()
            },
            test_order: vec![fn_name.to_string()],
            test_order_display_names: vec![fn_name.to_string()],
            cumulative: None,
        }
    }

    #[test]
    fn render_scan_report_contains_structure_and_stylesheet() {
        let report = sample_scan_report("doStuff", Some("unsupported param"));
        let html = render_scan_report(&report, None);

        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("Shatter Scan Report"));
        assert!(html.contains(STYLE_MARKER), "shared stylesheet not wired in");
        assert!(html.contains("Function Summary"));
        assert!(html.contains("doStuff"));
        assert!(html.contains("Skipped Functions"));
        assert!(html.contains("unsupported param"));
    }

    #[test]
    fn render_scan_report_escapes_function_name() {
        // `fn_name` is rendered with `|safe`, so html_templates must pre-escape
        // it. A raw `<img>` payload must not survive into the page.
        let report = sample_scan_report("<img src=x onerror=alert(1)>", None);
        let html = render_scan_report(&report, None);
        assert!(!html.contains("<img src=x"));
        assert!(html.contains("&lt;img src=x"));
    }

    #[test]
    fn render_scan_report_omits_skipped_section_when_empty() {
        let report = sample_scan_report("f", None);
        let html = render_scan_report(&report, None);
        assert!(!html.contains("Skipped Functions"));
    }
}
