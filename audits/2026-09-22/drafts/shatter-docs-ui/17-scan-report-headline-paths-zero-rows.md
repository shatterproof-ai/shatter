# Scan report: HTML headline shows 100% for 1 of 12 functions; absolute temp paths everywhere; zero-row sections; undefined "Interesting Inputs"

- Priority: P2
- Type: bug
- Labels: report,scan,ux
- Tracker action: new issue
- Related: str-3f27b, str-73pl. Also finding artifacts-13 (HTML "Paths Found" shows branches_covered, html_templates.rs:382/427). It is not drafted separately because its dedupe relation is "related", but it is worth fixing in the same change.
- Source findings: audit 2026-09-22 artifacts-15 (partially confirmed; narrowed)

<!-- body -->
## Problem (mixed TS+Go scan: 12 discovered, 5 attempted, 1 completed, 4 failed)
1. HTML (`shatter-core/templates/scan_report.html:13-17`): the tiles read "Functions 1" (completed only), "Paths Found 3" and an unqualified "Coverage 100%". There is no discovered or failed count. The markdown is correct: it leads with discovered/attempted/completed/failed and labels 100% as "(completed-functions subset)".
2. Absolute temp paths appear in every table, in JSON `file_path` and `qualified_id`, and in artifact names (6 in the markdown, 29 in the JSON).
3. The Source Set Summary prints six rows that are all zero.
4. "Interesting Inputs" lists 2 of 4 inputs with no stated selection rule.
5. With `-o`, the stdout `# Scan Results` table uses `/abs::fn` while the file uses Function/File columns: two differing summaries.

## Acceptance criteria
- The HTML headline leads with "N of M functions completed" and labels coverage with its basis (completed subset or all discovered).
- Reports use project-relative paths; JSON stores `project_root` once.
- Zero rows and empty sections are omitted.
- "Interesting Inputs" is either defined in the report (e.g. "one per distinct outcome") or removed.
- stdout and file summaries share one shape.
- HTML insta snapshot updated, plus a new test comparing the HTML counts against the markdown and JSON counts of the same ScanReport.

## Scope
In: rendering. Out: artifact filename scheme (ENAMETOOLONG is a separate L1 bug, cli-ux-07).
