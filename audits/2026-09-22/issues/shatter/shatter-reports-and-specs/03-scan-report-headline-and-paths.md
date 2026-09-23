---
slug: scan-report-headline-and-paths
kind: new
title: "Scan HTML report: headline shows 100% for 1 of 12 functions and 'Paths Found' counts branches; absolute temp paths, all-zero rows, undefined 'Interesting Inputs'"
priority: P2
type: bug
labels: [report, scan, html, ux, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Scan HTML report: headline shows 100% for 1 of 12 functions and "Paths Found" counts branches; absolute temp paths, all-zero rows, undefined "Interesting Inputs"

## Problem

A mixed TS+Go scan discovered 12 functions, attempted 5, completed 1 and failed 4 (7 were not attempted because the total budget ran out). The reports disagree about what happened:

1. **HTML headline misleads.** The HTML tiles read "Functions 1" (completed only), "Paths Found 3" and an unqualified "Coverage 100%". There is no discovered, attempted or failed count. The markdown report is correct: it leads with discovered/attempted/completed/failed and labels the 100% as "(completed-functions subset)". Only the HTML misleads.
2. **HTML "Paths" is really branches.** The HTML "Paths Found" tile and per-function "Paths" column show `branches_covered`. The markdown and stdout of the same run show 4 paths; the HTML shows 3. (Finding artifacts-13, folded in here.)
3. **Absolute temp paths everywhere.** Every table, the JSON `file_path` and `qualified_id`, and the artifact names contain absolute `/tmp/...` paths: 6 in the markdown, 29 in the JSON.
4. **All-zero rows.** The Source Set Summary prints seven buckets, six of them `0 | 0`.
5. **"Interesting Inputs" has no rule.** It lists 2 of the 4 inputs (`0 -> "zero"`, `-1 -> "negative"`) with no stated selection rule.
6. **Two different summaries.** With `-o`, the stdout `# Scan Results` table uses a single `/abs/path::fn` column while the written file uses separate Function/File columns.

## Evidence

Re-checked on 2026-09-23 in the audit worktree (HEAD 56c86168):

- `shatter-core/templates/scan_report.html:13-18`: the stat row has only Functions (`total_fn`), Paths Found (`total_paths`), Coverage (`overall_cov_bar_html`) and Skipped.
- `shatter-core/src/html_templates.rs:382`: `let total_paths: usize = report.functions.iter().map(|f| f.branches_covered).sum();` and `:428`: `paths_count: f.branches_covered,`.
- Captured run (on branch `audit-2026-09-22` until the audit reports land): `audits/2026-09-22/artifact-samples/scan-mix.{md,html,json,stdout}`. `scan-mix.md:3-9` has the correct discovered/attempted/completed/failed header; `scan-mix.md:14-22` has six all-zero Source Set rows; `scan-mix.md:71-76` is the Interesting Inputs block; `scan-mix.stdout` shows the `/tmp/...::classifyNumber | 4 | 100%` table.
- Interesting Inputs selection: `shatter-core/src/report.rs:2465-2468` keeps discovered inputs that threw or that `is_boundary_value` accepts. The rule exists in code but the report never states it.
- Existing HTML insta snapshot (`shatter-core/tests/html_snapshots.rs`, `shatter-core/tests/snapshots/`) renders a synthetic report and pins the current Paths=branches output, so it does not catch this.
- Coverage gap in the audit: the HTML was read as text only, never rendered in a browser.

## Acceptance criteria

- [ ] The HTML headline leads with "N of M functions completed" and shows discovered, attempted, failed and skipped counts. Coverage is labelled with its basis (completed subset or all discovered), matching the markdown wording.
- [ ] The HTML "Paths" tile and column show path counts, not `branches_covered`. If branch coverage is also shown, it is labelled as branches.
- [ ] Reports use project-relative paths. The JSON stores `project_root` once, and `file_path`/`qualified_id` are relative to it.
- [ ] Zero rows and empty sections are omitted from markdown and HTML.
- [ ] "Interesting Inputs" either states its selection rule in the report (for example "one per distinct outcome") or is removed.
- [ ] The stdout summary and the written file summary share one table shape.
- [ ] A new test builds one `ScanReport` (from a real scan of known-answer examples, not a synthetic struct) and asserts that the HTML, markdown and JSON agree on discovered/attempted/completed/failed counts and path counts. It fails on current code; record the failing and passing runs in the close comment. The HTML insta snapshot is updated.
- [ ] The rendered HTML is reviewed visually: a screenshot (or a bugshot gallery) of the before and after report for the mixed scan is attached to the close comment.

## Suggested approach

Pass the discovered/attempted/failed counts that `report.rs` already computes for markdown into the HTML template context. Fix `total_paths`/`paths_count` to use the path count field. Relativize paths once in the report model, not in each renderer. Reproduce the mixed scan with a small TS+Go directory that includes at least one Go function that fails or times out.

## Out of scope

- The artifact filename scheme and ENAMETOOLONG (separate finding cli-ux-07).
- Why the Go functions timed out.

## Related

str-3f27b, str-73pl, str-9q1z, str-qwua7.57; golden-and-consumer-suite (cross-format counts in general). Source findings: artifacts-15 (partially confirmed; narrowed to the HTML headline by the verifier), artifacts-13 (confirmed).
