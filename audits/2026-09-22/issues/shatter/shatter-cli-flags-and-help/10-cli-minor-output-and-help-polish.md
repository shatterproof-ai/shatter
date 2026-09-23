---
slug: cli-minor-output-and-help-polish
kind: new
title: "CLI polish: --analyze-only refused without a sandbox and shows no types; FunctionNotFound breakdown all zeros; false --dry-run/target help; --spec dropped with --spec-out; minor report defects"
priority: P3
type: bug
labels: [cli, ux, report, error-handling, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# CLI polish: --analyze-only refused without a sandbox and shows no types; FunctionNotFound breakdown all zeros; false --dry-run/target help; --spec dropped with --spec-out; minor report defects

## Problem

These are small, independent CLI output and help defects. None of them is severe on its own, but together they make first use confusing. They are grouped so they can be fixed in one pass. Split any item into its own issue if it grows.

Two items that the source drafts included are **not** here, because they have their own issues in this bucket:
- `--format text` still emits markdown, and `strip_markdown_text` corrupts `*` and `|`: explore-format-flag-ignored.
- `-o FILE` without `--stdout` leaves `# Shatter Explore` on stdout: explore-report-printed-twice.

Removing the internal-status paragraph from SPEC §2.11 belongs to help-tracker-ids-lint.

## Items and evidence

Line numbers were re-verified against `audit-2026-09-22` (HEAD `56c86168`). Transcripts are in `audits/2026-09-22/cli-ux-transcripts/`.

1. **`--analyze-only` is refused without a sandbox.** `shatter explore 05-unions.ts:computeArea --analyze-only` fails with `Error: refusing to execute target functions without a sandbox.` and exits 2, although analyze-only executes nothing (goals-18, reproduced by the verifier). str-gg9v introduced the default-deny without an analyze-only exemption. The artifacts-16 verifier could not reproduce this in `err-analyze-only.*`, but that transcript was captured with `SHATTER_ALLOW_HOST_WRITES=1`. Reproduce with the variable unset and no sandbox backend configured.
2. **`--analyze-only` output is thin and ignores `--format`.** It prints only `classifyNumber  (arithmetic-v1.ts:11)\n  params: 1, branches: 3` (`err-analyze-only.out`) or `computeArea (05-unions.ts:17) params: 1, branches: 6`, with no parameter names or types and no branch conditions, although the walkthrough promises "types and conditions". The output is un-headed plain text whatever `--format` says.
3. **The FunctionNotFound summary is all zeros.** `shatter explore arithmetic-v1.ts:doesNotExist` prints `[error] Analyze error (FunctionNotFound): Function not found: doesNotExist in arithmetic-v1.ts` and then `Error: explore: all 1 attempted target(s) failed (build_failed=0, runtime_failed=0, timed_out=0); no completed functions` (`err-nofn.err`). Analyze failures have no category, and the available functions are not listed.
4. **The `--dry-run` help is false.** `shatter-cli/src/args.rs:676` says "Requires --output", but `explore ... --dry-run` without `-o` works and exits 0 (`err-dryrun.out`).
5. **The target help omits Rust.** `shatter-cli/src/args.rs:501` says "(.ts = TypeScript, .go = Go)". `.rs` is supported.
6. **`--spec` is silently dropped when combined with `--spec-out`.** No spec is printed to stdout and there is no warning (artifacts-16).
7. **The HTML source view marks non-executable lines "uncovered".** `shatter-core/src/html_templates.rs:42-77` gives every line either the `covered` or the `uncovered` class, so the signature and closing braces show as uncovered next to "7/7 lines" (artifacts-16; not independently re-verified).
8. **The explore failure table labels a Rust timeout as language `any`** (`timed_out | any` in the audit's rust-explore.md sample; artifacts-16, verified).
9. **"complete with errors" is printed in green.** `demo/walkthrough.sh:388` and `demo/gauntlet.sh:920` print it with `${BOLD}${GREEN}`.
10. **`print_stdout` exits 1 on a non-EPIPE stdout I/O error** (`shatter-cli/src/helpers.rs:337-347`). SPEC §2.11 reserves 1 for regressions and uses 2 for tool errors (cli-ux-19; not verified).
11. **str-qwua7.12 (exit codes) is effectively done.** Missing file, `.py` target, unknown function, bad `--set`, function glob, spec-diff bad JSON, missing spec and host-write refusal all exit 2 (areas/cli-ux.md section 0 and prior-audit-regress.md). It stays open only because its facts came from a stale binary.

## Acceptance criteria

- [ ] (1) `explore --analyze-only` succeeds with no sandbox and no `--allow-host-writes`/`SHATTER_ALLOW_HOST_WRITES`. The host-write gate knows about analyze-only. A CLI test runs it with the variable unset and asserts exit 0; the test fails before the fix.
- [ ] (2) `--analyze-only` output lists each parameter with its name and type, and each branch with its condition text. It is rendered in the active `--format` (markdown by default, with a heading). A snapshot test covers TS and at least one of Go or Rust.
- [ ] (3) A missing target function is reported in its own category (for example `analyze_failed=1` or `not_found=1`), and the error lists the exported functions in that file, with a "did you mean" suggestion when one is close. The command exits 2. A CLI test covers it.
- [ ] (4, 5) The `--dry-run` and target-argument help strings match behavior (`.rs = Rust` added). A test asserts `--dry-run` without `-o` exits 0, so the help cannot drift back.
- [ ] (6) `--spec` together with `--spec-out` either prints the spec to stdout as well or emits a stderr warning that stdout output was suppressed. A test covers the combination.
- [ ] (7) Non-executable lines (signature, braces, blank lines, comments) get a third CSS class and are not styled as uncovered. The existing `render_source_block_marks_covered_and_uncovered_lines` test is extended.
- [ ] (8) The failure table shows the target's real language (`rust`) for Rust failures. A test covers a Rust timeout or build failure row.
- [ ] (9) Both demo scripts print "complete with errors" in red or yellow, not green.
- [ ] (10) `print_stdout` exits 2 on a non-EPIPE write error, or the close comment explains why 1 is correct and SPEC §2.11 is updated to say so.
- [ ] (11) str-qwua7.12 is closed with a comment that lists each case and its observed exit code from a fresh build (commands plus output). The SPEC §2.11 wording is handled by help-tracker-ids-lint; link it.
- [ ] Items split out into separate issues are linked from this issue before it is closed.
- [ ] `task affected` passes. Run `task walkthrough` too, since items 2 and 9 change walkthrough output. The close comment records the gates selected.

## Suggested approach

Item 1: check `command_executes_targets()` / the analyze-only flag before the refusal in `shatter-cli/src/host_writes.rs`. Item 2: render from the analyze response through the report renderer used for `--format`. First check whether that response carries parameter types and branch condition text; if it does not, extend it (a protocol-visible change, so follow protocol/GOVERNANCE.md and the parity checklist). If explore-format-flag-ignored lands first, reuse its single format selector. Item 3: add an analyze-failure category to the failure breakdown. The function names come from the same analyze response.

## Out of scope

- `--format`/`--render` unification and `strip_markdown_text` (explore-format-flag-ignored).
- stdout emission with `-o`/`--stdout`/`-q` (explore-report-printed-twice).
- Tracker IDs and internal notes in SPEC and help (help-tracker-ids-lint).
- Exit-code classification in general (str-qwua7.33).

## Dependencies

- Blocked by: none. Item 2 is simpler after explore-format-flag-ignored but does not require it.
- Related: str-qwua7.12, str-qwua7.33, str-gg9v, str-zt4v, str-tzbr, str-qwua7.11, explore-format-flag-ignored, explore-report-printed-twice, help-tracker-ids-lint.

## Source

Audit 2026-09-22 findings artifacts-16, goals-18 and cli-ux-19 (all P3). Merges the old drafts `drafts/shatter-docs-ui/13-minor-output-defects.md` and `drafts/shatter-docs-ui/14-analyze-only-and-error-help-polish.md`.
