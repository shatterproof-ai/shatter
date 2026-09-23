# Minor output defects: `--format text` keeps markdown, stray header with -o, --spec dropped with --spec-out, non-executable lines marked uncovered, Rust language 'any', green "complete with errors"

- Priority: P3
- Type: bug
- Labels: report,ux,cli,demo
- Tracker action: new issue
- Related: str-zt4v, str-tzbr, str-qwua7.11. Also L1 finding cli-ux-02 (`--format text|html` ignored on the explore stdout path). If an issue is filed for that, make this one depend on it.
- Source findings: audit 2026-09-22 artifacts-16 (confirmed; P3). The analyze-only parts moved to the analyze-only/help-polish issue.

<!-- body -->
## Items (each verified at HEAD unless noted)
1. `explore --format text` output is identical to markdown (`# Shatter Explore`, `**0 path(s)**`). `strip_markdown_text` in `shatter-core/src/report.rs:1918-1946` deletes every `*` and splits on `|`, which would corrupt Go `*T` types and `a | b` values. Render text from the view model instead.
2. `explore -o x.html` (no `--stdout`) leaves `# Shatter Explore` and a blank line on stdout (`shatter-cli/src/commands/explore.rs:3610`, `should_print_report`).
3. `--spec` combined with `--spec-out` prints no spec to stdout and gives no warning.
4. HTML source view (`shatter-core/src/html_templates.rs:45-85`) marks the signature and closing braces "uncovered" next to "7/7 lines". Add a non-executable line class. (Not independently re-verified.)
5. The explore failure table labels a Rust timeout as language `any` (`timed_out | any`).
6. `demo/walkthrough.sh:388` and `demo/gauntlet.sh:920` print "complete with errors" in `${GREEN}`.

## Acceptance criteria
- Each item is fixed, or split out and linked.
- Tests: `--format text` output contains no markdown syntax and preserves literal `*` and `|` in values. `-o file` without `--stdout` leaves stdout empty. The failure table carries the target's language.

## Scope
Small fixes only. The larger `--format`/`--render` unification belongs to cli-ux-02's issue.
