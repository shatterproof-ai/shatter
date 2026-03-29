# Askama HTML Templating Refactor — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace hand-built `String` concatenation in `report.rs` with Askama compile-time templates, producing semantically identical HTML output.

**Architecture:** Three Askama template files (explore function fragment, explore page wrapper, scan report page) backed by view-model structs with `#[derive(Template)]`. The existing public API (`render_explore_fn_html`, `wrap_explore_html`, `generate_html_scan_report`) stays unchanged — internals switch from `write!`/`push_str` to `template.render().expect("...")`. A shared CSS include avoids duplication. Snapshot tests lock down the output with whitespace-normalized comparison.

**Tech Stack:** Askama 0.15, Rust, existing shatter-core types (`ObservationOutput`, `ScanReport`)

**Whitespace strategy:** Askama templates produce different whitespace than hand-built `write!`/`push_str`. Rather than fighting Askama's whitespace with `{%- -%}` trim markers everywhere, snapshot tests compare whitespace-normalized HTML. The normalization function collapses runs of whitespace to a single space and trims lines. This is sufficient because the HTML rendering is identical in browsers — whitespace between tags is insignificant.

**Error handling:** `Template::render()` returns `askama::Result<String>`. Since template rendering only fails on I/O errors (not possible with in-memory rendering), wrapper functions use `.expect("template rendering failed")` to preserve the existing `-> String` signatures.

**Issue:** str-heiu

---

## File Structure

| Action | Path | Responsibility |
|--------|------|----------------|
| Create | `shatter-core/templates/explore_fn.html` | Per-function `<details>` fragment |
| Create | `shatter-core/templates/explore_page.html` | Full explore report page (wraps fragments) |
| Create | `shatter-core/templates/scan_report.html` | Full scan report page |
| Create | `shatter-core/templates/includes/style.html` | Shared CSS block (`<style>` tag content) |
| Create | `shatter-core/src/html_templates.rs` | Askama view-model structs + `#[derive(Template)]` |
| Modify | `shatter-core/src/report.rs:506-573` | `render_explore_fn_html` → delegate to Askama |
| Modify | `shatter-core/src/report.rs:579-624` | `wrap_explore_html` → delegate to Askama |
| Modify | `shatter-core/src/report.rs:628-761` | `generate_html_scan_report` → delegate to Askama |
| Modify | `shatter-core/src/report.rs:414-500` | Remove `html_escape`, `coverage_class`, `render_cov_bar`, `HTML_CSS` (moved to templates/view models) |
| Modify | `shatter-core/Cargo.toml` | Add `askama` dependency |
| Modify | `shatter-core/src/lib.rs` | Add `mod html_templates;` |
| Create | `shatter-core/tests/html_snapshots.rs` | Snapshot tests comparing old vs new output |

---

### Task 1: Add Askama Dependency

**Files:**
- Modify: `shatter-core/Cargo.toml`

- [ ] **Step 1: Add askama to Cargo.toml**

Add to `[dependencies]`:
```toml
askama = "0.15"
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p shatter-core`
Expected: compiles with no errors

- [ ] **Step 3: Commit**

```bash
git add shatter-core/Cargo.toml
git commit -m "deps: add askama for HTML templating (str-heiu)"
```

---

### Task 2: Capture Baseline HTML Snapshots

Before changing any rendering code, capture the exact HTML output from the current hand-built implementation. These snapshots are the source of truth for the refactor.

**Files:**
- Create: `shatter-core/tests/html_snapshots.rs`
- Create: `shatter-core/tests/snapshots/explore_fn.html`
- Create: `shatter-core/tests/snapshots/explore_page.html`
- Create: `shatter-core/tests/snapshots/scan_report.html`

- [ ] **Step 1: Write the snapshot capture test**

Create `shatter-core/tests/html_snapshots.rs` with three test functions that call the existing `render_explore_fn_html`, `wrap_explore_html`, and `generate_html_scan_report` with deterministic inputs, then compare against snapshot files. Use the same test data constructors already used in `report.rs` tests (the `ObservationOutput` and `ScanReport` builders).

Each test should:
1. Build a deterministic input (same data as the existing `render_explore_fn_html_contains_function_name` and `html_scan_report_is_valid_structure` tests)
2. Call the current function to produce HTML
3. Write the HTML to `tests/snapshots/<name>.html` if the snapshot file doesn't exist yet (first run creates the baseline)
4. If the snapshot exists, compare using a `normalize_ws(s: &str) -> String` helper that collapses runs of whitespace to a single space and trims each line. This allows Askama templates to differ in insignificant whitespace while catching structural/content regressions.

- [ ] **Step 2: Run tests to generate baseline snapshots**

Run: `cargo test -p shatter-core --test html_snapshots`
Expected: PASS — first run creates the snapshot files

- [ ] **Step 3: Verify snapshot files are non-empty and contain valid HTML**

Read each snapshot file. Confirm it starts with the expected content (`<details>` for the fragment, `<!DOCTYPE html>` for the pages).

- [ ] **Step 4: Commit**

```bash
git add shatter-core/tests/html_snapshots.rs shatter-core/tests/snapshots/
git commit -m "test: capture baseline HTML snapshots for Askama migration (str-heiu)"
```

---

### Task 3: Create View Model Module and CSS Include

Set up the Askama infrastructure: the view model module, shared CSS, and helper functions.

**Files:**
- Create: `shatter-core/templates/includes/style.html`
- Create: `shatter-core/src/html_templates.rs`
- Modify: `shatter-core/src/lib.rs`

- [ ] **Step 1: Create the shared CSS include**

Extract `HTML_CSS` from `report.rs` into `shatter-core/templates/includes/style.html`. This file contains only the CSS content (no `<style>` tags — those go in the page-level templates).

- [ ] **Step 2: Create the view model module**

Create `shatter-core/src/html_templates.rs` with:
- Helper functions: `coverage_class(pct) -> &str`, `render_cov_bar(pct) -> String`, `format_outcome(exec) -> String`, `format_inputs(exec) -> String`
- `PathEntry` struct for path table rows: `index: usize`, `inputs_html: String`, `outcome_html: String`

Note: Askama auto-escapes by default, so `html_escape` calls on template variables are no longer needed — but pre-rendered HTML fragments (`cov_bar_html`, `inputs_html`, `outcome_html`) must be marked safe in the template with `{{ value|safe }}`.

Note: `Template::render()` returns `askama::Result<String>`. Since in-memory rendering cannot fail, all public wrapper functions use `.expect("template rendering failed")` to preserve the existing `-> String` return type.

- [ ] **Step 3: Add `mod html_templates;` to lib.rs**

- [ ] **Step 4: Verify it compiles**

Run: `cargo check -p shatter-core`
Expected: compiles (module exists but has no templates yet)

- [ ] **Step 5: Commit**

```bash
git add shatter-core/templates/includes/ shatter-core/src/html_templates.rs shatter-core/src/lib.rs
git commit -m "refactor: add Askama view model module and shared CSS include (str-heiu)"
```

---

### Task 4: Port Explore Function Fragment to Askama

Port `render_explore_fn_html` to an Askama template.

**Files:**
- Create: `shatter-core/templates/explore_fn.html`
- Modify: `shatter-core/src/html_templates.rs`
- Modify: `shatter-core/src/report.rs:506-573`

- [ ] **Step 1: Create the explore_fn template**

Create `shatter-core/templates/explore_fn.html`. This template produces the `<details>` fragment. It receives:
- `fn_name: &str` (auto-escaped by Askama)
- `location: &str` (auto-escaped)
- `cov_bar_html: &str` (pre-rendered coverage bar — use `{{ cov_bar_html|safe }}`)
- `iterations: u32`
- `unique_paths: usize`
- `lines_covered: usize`
- `total_lines: u32`
- `paths: Vec<PathEntry>` where each has `index: usize`, `inputs_html: String` (use `|safe`), `outcome_html: String` (use `|safe`)
- `has_paths: bool`

Must reproduce the structure from `render_explore_fn_html` in `report.rs:519-571`.

- [ ] **Step 2: Add ExploreFnTemplate struct**

Add to `html_templates.rs`:
- `ExploreFnTemplate` struct with `#[derive(Template)]` and `#[template(path = "explore_fn.html")]`
- `pub fn render_explore_fn(result: &ObservationOutput, location: &str) -> String` that builds the view model and calls `.render().expect("template rendering failed")`

- [ ] **Step 3: Wire render_explore_fn_html to use Askama**

In `report.rs`, change `render_explore_fn_html` body to call `html_templates::render_explore_fn(result, location)`.

- [ ] **Step 4: Run snapshot test**

Run: `cargo test -p shatter-core --test html_snapshots -- snapshot_explore_fn`
Expected: PASS — whitespace-normalized output matches baseline snapshot

If it fails, diff the output against the snapshot and fix the template until they match.

- [ ] **Step 5: Run all existing report tests**

Run: `cargo test -p shatter-core -- report::`
Expected: all PASS

- [ ] **Step 6: Commit**

```bash
git add shatter-core/templates/explore_fn.html shatter-core/src/html_templates.rs shatter-core/src/report.rs
git commit -m "refactor: port render_explore_fn_html to Askama template (str-heiu)"
```

---

### Task 5: Port Explore Page Wrapper to Askama

Port `wrap_explore_html`.

**Files:**
- Create: `shatter-core/templates/explore_page.html`
- Modify: `shatter-core/src/html_templates.rs`
- Modify: `shatter-core/src/report.rs:579-624`

- [ ] **Step 1: Create the explore_page template**

This template renders the full HTML page. It receives:
- `css: &str` (the CSS content, marked safe)
- `fn_count: usize`
- `total_paths: usize`
- `cov_bar_html: &str` (pre-rendered, marked safe)
- `fragments: &[String]` (pre-rendered `<details>` blocks, each marked safe)

Must reproduce the exact structure from `wrap_explore_html` in `report.rs:596-623`.

- [ ] **Step 2: Add ExplorePageTemplate view model**

Add to `html_templates.rs`:
- `ExplorePageTemplate` struct with `#[derive(Template)]`
- `pub fn render_explore_page(fragments: &[String], fn_count: usize, total_paths: usize, total_covered: usize, total_lines: u32) -> String`

- [ ] **Step 3: Wire wrap_explore_html to use Askama**

In `report.rs`, change `wrap_explore_html` body to call `html_templates::render_explore_page(...)`.

- [ ] **Step 4: Run snapshot test**

Run: `cargo test -p shatter-core --test html_snapshots -- snapshot_explore_page`
Expected: PASS

- [ ] **Step 5: Run all report tests**

Run: `cargo test -p shatter-core -- report::`
Expected: all PASS

- [ ] **Step 6: Commit**

```bash
git add shatter-core/templates/explore_page.html shatter-core/src/html_templates.rs shatter-core/src/report.rs
git commit -m "refactor: port wrap_explore_html to Askama template (str-heiu)"
```

---

### Task 6: Port Scan Report to Askama

Port `generate_html_scan_report`. This is the largest template — it includes the summary stats, function summary table, per-function details, and skipped functions list.

**Files:**
- Create: `shatter-core/templates/scan_report.html`
- Modify: `shatter-core/src/html_templates.rs`
- Modify: `shatter-core/src/report.rs:628-761`

- [ ] **Step 1: Create the scan_report template**

Receives:
- `css: &str` (safe)
- `total_fn: usize`, `total_paths: usize`, `skipped_count: usize`
- `overall_cov_bar_html: &str` (safe)
- `functions: Vec<ScanFnView>` — each with `fn_name`, `file_path`, `paths_count`, `cov_bar_html` (safe), `iterations`, `lines_covered`, `total_lines`, `discovered_inputs: Vec<PathEntry>`, `has_inputs: bool`, `mocks_html: Option<String>` (safe)
- `skipped: Vec<SkippedView>` — each with `fn_name`, `reason`
- `has_skipped: bool`

Must reproduce the exact structure from `report.rs:628-761`.

- [ ] **Step 2: Add ScanReportTemplate and ScanFnView structs**

Add to `html_templates.rs`:
- `ScanReportTemplate`, `ScanFnView`, `SkippedView` structs
- `pub fn render_scan_report(report: &ScanReport) -> String`

- [ ] **Step 3: Wire generate_html_scan_report to use Askama**

In `report.rs`, change `generate_html_scan_report` body to call `html_templates::render_scan_report(report)`.

- [ ] **Step 4: Run snapshot test**

Run: `cargo test -p shatter-core --test html_snapshots -- snapshot_scan_report`
Expected: PASS

- [ ] **Step 5: Run all report tests**

Run: `cargo test -p shatter-core -- report::`
Expected: all PASS

- [ ] **Step 6: Commit**

```bash
git add shatter-core/templates/scan_report.html shatter-core/src/html_templates.rs shatter-core/src/report.rs
git commit -m "refactor: port generate_html_scan_report to Askama template (str-heiu)"
```

---

### Task 7: Clean Up Dead Code

Remove the now-unused hand-built HTML helpers from `report.rs`.

**Files:**
- Modify: `shatter-core/src/report.rs`

- [ ] **Step 1: Remove dead code**

Delete from `report.rs`:
- `html_escape` fn (Askama handles escaping)
- `coverage_class` fn (moved to `html_templates.rs`)
- `render_cov_bar` fn (moved to `html_templates.rs`)
- `HTML_CSS` const (moved to template include)

Keep the public function signatures — they now delegate to `html_templates`.

- [ ] **Step 2: Run full test suite**

Run: `cargo test -p shatter-core`
Expected: all PASS, no unused warnings

- [ ] **Step 3: Run clippy**

Run: `cargo clippy -p shatter-core -- -D warnings`
Expected: clean

- [ ] **Step 4: Commit**

```bash
git add shatter-core/src/report.rs
git commit -m "cleanup: remove hand-built HTML helpers replaced by Askama (str-heiu)"
```

---

### Task 8: End-to-End Validation

Verify the full CLI pipeline still works with the new templates.

**Files:** None (validation only)

- [ ] **Step 1: Run the quick test tier**

Run: `npx task test-quick`
Expected: PASS

- [ ] **Step 2: Run the standard test tier**

Run: `npx task test-standard`
Expected: PASS

- [ ] **Step 3: Run the walkthrough**

Run: `npx task walkthrough`
Expected: PASS (check ERROR SUMMARY at end)

- [ ] **Step 4: Manual spot-check**

Run an explore command with `--report-file` against a known example and open the HTML file to visually confirm it renders correctly:

```bash
cargo run -p shatter-cli -- explore examples/typescript/src/01-basic-arithmetic.ts --report-file /tmp/test-report.html
```

Open `/tmp/test-report.html` in a browser and verify it looks correct.

- [ ] **Step 5: Final commit (if any fixes needed)**

If any fixes were required during validation, commit them.
