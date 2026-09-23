# Bundle: shatter-cli-flags-and-help (Audit 2026-09-22, revised after Codex cross-check)

- Bucket: `shatter-cli-flags-and-help`: CLI flag semantics and help text (--format, duplicate output, --allow-host-writes/--set scoping, --seed, config typos, tracker IDs in help, telemetry argv redaction, small CLI output defects).
- Repo: shatter. Tracker: bd in /home/ketan/project/shatter (prefix str). Parent epic: "Epic: Audit 2026-09-22 findings".
- Entries: 16 new issues, 2 reopen-notes (comments on closed str-qwua7.15 and str-0m0vn) and 1 note-to-existing (comment on open str-qwua7.12). Nothing has been filed.
- Revision: applied the Codex cross-check (`issues/crosscheck/shatter-cli-flags-and-help.codex.md`); see `REVISION.md`. cli-minor-output-and-help-polish was split into 8 drafts (11-18) and help-tracker-ids-lint's reconciliation half became unfiled-0904-ui-items-reconcile (19).
- Evidence re-verified 2026-09-23 against the `audit-2026-09-22` worktree (source at `56c86168`) and its `target/debug/shatter` where cheap. Output-behavior claims cite transcripts in `audits/2026-09-22/cli-ux-transcripts/` and verifier reproductions in `findings.json`. Tracker state for str-qwua7.12, str-v1tzz, str-qwua7.33, str-9ee5 checked with `bd show`.
- Dependency edges: seed-for-explore-and-run is blocked by explore-resume-options-key (bucket shatter-artifacts-correctness). No other blocked_by edges; ordering notes are in each body.

## Maintainer decisions (2026-09-23), which override the report and the old drafts

- **D1 Releases:** keep Windows (x86_64-pc-windows-msvc) and aarch64-unknown-linux-gnu in the release matrix, and fix them (Z3 header/static link on Windows; openssl-sys under cross for aarch64). Release work closes only with a green release-run URL.
- **D2 shatter diff:** retire the snapshot-diff command and the unused Snapshot writer path; spec-diff is the regression tool. Update SPEC/README/QUICKSTART. The `diff` name becomes free, and str-81xiw decides whether to use it. Correct the shatter-agents plugin's `shatter diff --staged` docs. *(Touches this bucket: help-hides-execution-flags and telemetry-known-subcommands note that `diff` drops out when retire-snapshot-diff lands.)*
- **D3 Concolic positioning:** measure first. P1 default-vs-concolic benchmark (fixed seeds, fresh artifacts, examples corpus plus one downstream project, reported per release); P1 fix for concolic early termination; a follow-up decision issue, blocked by both, re-decides positioning. No doc softening now. *(seed-for-explore-and-run supports the fixed-seed benchmark.)*
- **D4 Beads hook stall:** retire the JSONL import in shatter and move tracker sync to a Dolt remote. The first step verifies whether importing the stale JSONL has clobbered newer DB state. AGENTS.md drops `bd sync`, str-qwua7.28 is superseded, and bento's beads-issue-flow gets matching guidance. No BEADS_HOOK_TIMEOUT fix and no hook-bypass guidance.
- **D5 Git identity:** the leaked [user] section is already removed. Drafts cover a .mailmap for test@example.com, a git-state check (local identity override, example.com email, core.bare=true, hooksPath override), and a .git/config snapshot in test_git_fixture_isolation.py.
- **D6 Filing:** after reconciliation and the Codex cross-check, the maintainer runs one filer script. Agents file nothing.

## Contents

| # | Slug | Kind | Priority | Title |
|---|---|---|---|---|
| 01 | explore-format-flag-ignored | new | P2 | explore --format text\|html has no effect on stdout; --render and --format overlap |
| 02 | explore-report-printed-twice | new | P2 | explore -o FILE --stdout prints the report twice; -o FILE without --stdout leaks '# Shatter Explore' to stdout |
| 03 | help-hides-execution-flags | new | P2 | Scope --allow-host-writes and --set to the commands that use them: `help <cmd>` and 10 non-executing commands still show them (replace the str-qwua7.15 argv intercept) |
| 04 | help-flags-reopen-note | reopen-note (str-qwua7.15) | P2 | Comment on closed str-qwua7.15: execution-only flags still shown by `help <cmd>` and 10 non-executing commands |
| 05 | seed-for-explore-and-run | new | P2 | Add --seed to explore and run (str-0m0vn wired it into scan only) |
| 06 | seed-reopen-note | reopen-note (str-0m0vn) | P2 | Comment on closed str-0m0vn: symptom named explore, but --seed exists only on scan |
| 07 | unknown-config-keys-warn | new | P2 | Warn on unknown config keys and --set key typos (currently silently ignored) |
| 08 | help-tracker-ids-lint | new | P2 | Remove tracker IDs and internal status notes from --help, SPEC and README; add a lint |
| 09 | telemetry-known-subcommands | new | P3 | Telemetry KNOWN_SUBCOMMANDS is stale: sanitized_args redacts most real subcommand tokens and lists 3 nonexistent ones |
| 10 | cli-minor-output-and-help-polish | new | P3 | Help text is wrong: --dry-run says 'Requires --output' (it does not); target help omits .rs = Rust |
| 11 | analyze-only-sandbox-refusal | new | P3 | explore/run --analyze-only is refused by the host-write gate although it executes nothing |
| 12 | analyze-only-output-detail | new | P3 | --analyze-only output shows only counts: no parameter names/types, no branch conditions, ignores --format |
| 13 | explore-function-not-found-diagnostics | new | P3 | explore file:missingFn reports an all-zero failure breakdown and does not list available functions |
| 14 | spec-flag-dropped-with-spec-out | new | P3 | explore --spec is silently ignored when --spec-out is also given |
| 15 | html-source-non-executable-lines | new | P3 | HTML report source view marks non-executable lines (signature, braces, blanks) as uncovered |
| 16 | failure-table-language-any | new | P3 | explore failure-impact table shows Rust (and other unclassified) failures only under language `any` |
| 17 | demo-complete-with-errors-green | new | P3 | walkthrough.sh and gauntlet.sh print 'complete with errors' in green |
| 18 | exit-codes-qwua7-12-note | note-to-existing (str-qwua7.12) | P1 | Note on str-qwua7.12: most single-target exit-2 classes now hold, but multi-target partial failure exits 0 (contrary to its AC) and print_stdout exits 1 on I/O errors |
| 19 | unfiled-0904-ui-items-reconcile | new | P3 | Reconcile the four unfiled 2026-09-04 usability items (12, 16, 17, 20) against the tracker and hand drafts to the maintainer |

---

<!-- file: 01-explore-format-flag-ignored.md -->

---
slug: explore-format-flag-ignored
kind: new
title: "explore --format text|html has no effect on stdout; --render and --format overlap"
priority: P2
type: bug
labels: [cli, explore, report, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# explore --format text|html has no effect on stdout; --render and --format overlap

## Problem

str-zt4v (closed) decided that `--format` controls what goes to stdout. For `explore`, the streaming printer branches on `output_format` instead. That field is the `--render md|plain` enum (`shatter-cli/src/args.rs:37-43`), so `explore --format html` and `explore --format text` both print markdown. On stdout, `--format` is honored only by the post-run replay that runs when `-o` and `--stdout` are both given. Explore has four format controls that interact silently: `--render {md,plain}`, `--format {markdown,html,text}`, `--color`, and inference from the `-o` file extension.

A second problem: `strip_markdown_text`, the text renderer, is a character filter. It deletes every `*` and backtick and splits every line containing `|`, including inside data values. Text mode therefore corrupts Go pointer types (`*T`) and values such as `a | b`. This already affects `-o FILE.txt` output: `StdoutFormat::Text` also drives the `-o` text-file writers (`explore.rs:3992-3994`, `:6631`), which call `strip_markdown_text`.

`--render` is a **global** flag (`args.rs:190-197`, `global = true`) that `main.rs` passes to both explore (`main.rs:448`) and scan (`main.rs:893`). This issue changes explore only. Retiring `--render` across commands belongs to the flag-vocabulary work in str-9ee5.

## Evidence

Re-verified against `audit-2026-09-22` (source at `56c86168`).

- Streaming printer branches on `--render`: `shatter-cli/src/commands/explore.rs:3620`, `:3854`, `:3920`, `:4783` and `:6533` (`if output_format == crate::args::OutputFormat::Md`).
- `--format` on stdout is used only by the replays: `explore.rs:4028-4029` (in `finalize_explore`, reached via `--from-artifacts`) and `:6687-6688` (in `run_explore`, the live path), both `if !report_outputs.is_empty() && stdout`. It also selects the `-o` text writer (`:3992-3994`, `:6631`).
- `shatter-core/src/report.rs:1918-1946` `strip_markdown_text`: `.replace('*', "")`, `.replace('`', "")`, and `line.split('|')` on any line that contains `|`.
- Transcripts in `audits/2026-09-22/cli-ux-transcripts/`: `format-html.out` begins `# Shatter Explore` and contains `**4 path(s)**` and markdown tables; `format-text.out` (a `markup` fixture) begins `# Shatter Explore` and contains `**0 path(s)**`. `format-text-with-o.out` (`--format text -o r.md --stdout`) is also markdown with `**4 path(s)**`.
- The only test of `explore --format` is a clap rejection of `json` (`shatter-cli/tests/json_stdout_contract.rs:250`).
- `explore --help` and `scan --help` both list `--render <MODE>` and `--format <FORMAT>`.
- `--render plain` prints lines that markdown omits (`Branches: 3/3`, `[random: 3 (100%)]`, `Symbolic: 3/3 constraints`). That gap is tracked separately as markdown-drops-render-plain-info (bucket shatter-cli-runtime-output).
- Verifier (findings.json cli-ux-02) reproduced `--format html|text` printing markdown and lowered the priority from P1 to P2. The `strip_markdown_text` sub-claim was confirmed separately under artifacts-16 by reading the code.

## Acceptance criteria

- [ ] `shatter explore <file> --format text` prints plain text with no markdown syntax (`#` headings, `**`, table pipe rows) to stdout. `--format html` prints an HTML document (starts with `<!DOCTYPE html>` or `<html`) to stdout. `--format markdown` output is byte-identical to today's default.
- [ ] The same holds with `-o FILE --stdout`, on both the live path and the `--from-artifacts` path: the stdout format follows `--format`, not the file extension.
- [ ] Precedence is defined and tested for explore: when `--format` and `--render` are both given, `--format` wins; an explicit `--render` on `explore` prints a one-line deprecation warning on stderr naming `--format`. With neither given, explore output is unchanged.
- [ ] scan's behavior is unchanged: a test (or the existing scan output tests, named in the close comment) shows `scan --render plain` and `scan --render md` output identical before and after this change. Global retirement of `--render`, including scan's migration, is left to str-9ee5; the close comment adds a note there.
- [ ] Golden or snapshot tests in `shatter-cli/tests/` cover `explore --format {markdown,text,html}` without `-o`, and `--format text -o r.md --stdout`. The text and html tests fail on the current code and pass after the fix; the close comment records both runs (test names plus the failing assertion text).
- [ ] Text rendering preserves literal `*`, backtick and `|` inside data values. A test uses an outcome or type containing `*T` and `a | b` and asserts both appear verbatim in `--format text` stdout and in a `-o r.txt` file. This test fails on current code.
- [ ] `task affected` passes. The close comment records its `Gates selected` line.

## Suggested approach

Use one stdout-format selector for explore (`--format`, with `-o` extension inference only for files) that drives both the streaming printer and the replay paths. Render text from the report view model rather than stripping markdown after the fact. Replace or delete `strip_markdown_text`, and check its other callers (scan uses it too) before changing its behavior; if scan keeps calling it, fix the corruption there as well and cover it with the same `*T` / `a | b` test. This touches the same emitter code as explore-report-printed-twice, so do both in one branch, or one straight after the other.

## Out of scope

- Removing `--render` globally or changing scan's format flags (str-9ee5).
- Duplicate printing with `-o --stdout` and the header leak with `-q -o` (explore-report-printed-twice).
- Adding information that `--render plain` shows to the default markdown (markdown-drops-render-plain-info).
- JSON bundle contents written by `-o out.json` (explore-o-json-empty-bundle).

## Dependencies

- Blocked by: none.
- Related: explore-report-printed-twice (same code), markdown-drops-render-plain-info, str-zt4v, str-mpwp, str-9ee5.

## Source

Audit 2026-09-22 finding cli-ux-02 (areas/cli-ux.md F2); old draft `drafts/shatter-code/27-explore-format-flag-ignored.md`. Also 2026-09-04 usability-ui item 18, which was never filed.

---

<!-- file: 02-explore-report-printed-twice.md -->

---
slug: explore-report-printed-twice
kind: new
title: "explore -o FILE --stdout prints the report twice; -o FILE without --stdout leaks '# Shatter Explore' to stdout"
priority: P2
type: bug
labels: [cli, explore, report, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# explore -o FILE --stdout prints the report twice; -o FILE without --stdout leaks '# Shatter Explore' to stdout

## Problem

The str-zt4v output contract is: with no `-o`, the report goes to stdout. With `-o FILE`, it goes to the file and stdout stays empty, unless `--stdout` is also given, in which case it goes to both exactly once. Explore breaks this in two ways:

1. **Double print.** `explore <file> -o x.json --stdout` streams the full report to stdout while exploring, then replays it after the run. stdout therefore holds every per-function report twice, plus extra `## <fn> / **Status:** completed / exploration completed` blocks between the copies.
2. **Header leak.** `explore <file> -o report.html -o bundle.json -q` with no `--stdout` leaves `# Shatter Explore` and a blank line (19 bytes) on stdout. `-o x.html` without `-q` does the same. stdout should be empty.

Either one breaks piping (`shatter explore ... -o r.json --stdout | tool`) and any script that checks that stdout is empty.

Explore has two replay sites, and they are reached by different paths, not by sequential vs parallel scheduling:

- `explore.rs:4028-4029` is in `finalize_explore` (defined at `:3773`), reached only through `--from-artifacts` (`run_explore` returns into it at `:4244`).
- `explore.rs:6687-6688` is in `run_explore` (`:4116`), the live exploration path. This is the one that produced `explore-o2.out`.

## Evidence

Re-verified against `audit-2026-09-22` (source at `56c86168`).

- The streaming path prints if `should_print_report = opts.report_outputs_empty || opts.stdout` (`shatter-cli/src/commands/explore.rs:3610`), and the `# Shatter Explore` header is emitted before that gate is consulted (`:3607-3661`).
- Replays: `explore.rs:4028-4029` (`// Replay to stdout if report files were also written.`) and `:6687-6688` (`// If files were written and --stdout was also requested, replay to stdout.`).
- Transcripts in `audits/2026-09-22/cli-ux-transcripts/`:
  - `explore-o2.out` (`explore 01-arithmetic.ts -o x.json --stdout`): `## \`classifyNumber\` *(01-arithmetic.ts:10-21)*` appears twice (lines 3 and 33), as do the table rows (`| 1 | \`classifyNumber(0)\` | returns \`"zero"\` |`) and the `compareMagnitudes` heading. `**Summary:**` appears only **once**: the replayed copy has no summary line, so counting summaries does not detect the bug.
  - `explore-o.out` and `explore-o3.out` (`-o` without `--stdout`): exactly `# Shatter Explore\n\n` (19 bytes).
- Verifier (findings.json cli-ux-03) reproduced both and lowered the priority from P1 to P2: the output is duplicated or cosmetic, not incorrect.
- Existing issues cover neighbouring behavior only: str-zt4v (output matrix, closed), str-6c6p (`--quiet` hid reports, closed) and str-xve (stdout/stderr mixing, closed).

## Acceptance criteria

- [ ] CLI output tests in `shatter-cli/tests/` cover every combination of `-o FILE` (absent or present), `--stdout` (absent or present) and `-q` (absent or present) for a live `explore` on a small TS fixture with at least two exported functions (the transcript used `01-arithmetic.ts` from the examples snapshot; `demo/fixtures/arithmetic-v1.ts` has only one function, so add a two-function fixture under `shatter-cli/tests/` if none exists), eight cases. They assert:
  - no `-o`: every per-function heading line (`## \`<fn>\``) and every result-table row occurs **exactly once** on stdout;
  - `-o` without `--stdout`: stdout is empty (0 bytes), with or without `-q`;
  - `-o` with `--stdout`: the same exactly-once predicate holds per heading and per table row, no `**Status:**`/`exploration completed` replay blocks appear, and the file is written;
  - `-q` never suppresses the report when stdout is the sink (keep str-6c6p's behavior).
- [ ] The exactly-once predicate is shown to be discriminating: run against current HEAD, the `-o --stdout` case fails because `## \`classifyNumber\`` occurs twice, and the `-o` without `--stdout` case fails on the 19-byte header. Both pass after the fix. The close comment records both runs (test names and failure messages).
- [ ] The `--from-artifacts` path (`finalize_explore`) is tested separately: first run explore with an artifact dir, then `explore --from-artifacts <dir>` with each of `-o r.md --stdout`, `-o r.md` and no `-o`, asserting the same exactly-once and empty-stdout predicates. If that path is already correct today, the close comment says so and the tests still land as regression guards.
- [ ] `task affected` passes. The close comment records its `Gates selected` line.

## Suggested approach

Pick one stdout emitter per run: either stream when stdout is the sink and skip the replay, or buffer and emit once. Gate header emission on the same `should_print_report` condition as the body. Delete the replay branches or make them the only emitter, and apply the same rule in `finalize_explore`. This is the same code explore-format-flag-ignored changes, so land the two together or one after the other.

## Out of scope

- Which format stdout uses (explore-format-flag-ignored).
- What the JSON bundle written by `-o x.json` contains (explore-o-json-empty-bundle).
- scan and run stdout behavior, unless the same helper is shared (then note it in the close comment).

## Dependencies

- Blocked by: none.
- Related: explore-format-flag-ignored (same emitter code), explore-o-json-empty-bundle, str-zt4v, str-6c6p, str-xve.

## Source

Audit 2026-09-22 finding cli-ux-03 (areas/cli-ux.md F3). No prior draft (report section 15.1).

---

<!-- file: 03-help-hides-execution-flags.md -->

---
slug: help-hides-execution-flags
kind: new
title: "Scope --allow-host-writes and --set to the commands that use them: `help <cmd>` and 10 non-executing commands still show them (replace the str-qwua7.15 argv intercept)"
priority: P2
type: bug
labels: [cli, usability, help, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Scope --allow-host-writes and --set to the commands that use them: `help <cmd>` and 10 non-executing commands still show them (replace the str-qwua7.15 argv intercept)

## Problem

str-qwua7.15 ("Hide execution-only global flags on non-executing commands") was closed on 2026-09-14 with the bare reason "Closed" and only a partial fix. `--allow-host-writes` and `--set` are still `global = true` on `Cli`. The fix hides them only when raw argv looks like `<cmd> --help` for one of five hard-coded commands, by guessing the subcommand from argv and rendering help from a second, mutated clap `Command`. The commit notes that the natural fix (`mut_arg(... hide)`) broke clap parsing, so the workaround was chosen instead.

Three gaps follow:

1. `shatter help <cmd>` goes through clap's own `help` subcommand and bypasses the intercept, so `help spec-diff` and `help doctor` still show the flags.
2. SPEC §2.10 names more non-executing commands than the five in the list. `--help` still shows `--allow-host-writes` for analyze, solve, specify, stale, diff, compare, list-targets, nondeterminism, workspace and build-frontend.
3. The flags still *parse* on non-executing commands: `spec-diff --allow-host-writes a b` is accepted and silently ignored.

The two flags have different real scopes, and this issue treats them separately:

- `--allow-host-writes` matters exactly where `command_executes_targets()` (`shatter-cli/src/host_writes.rs:84-95`) is true: Explore, Scan, Run, Observe, Bench, Properties, Revalidate.
- `--set` is consumed **only by explore** today: `cli.set_overrides` is read once in `main.rs` (`:437`, the explore dispatch) and flows into `resolve_function_config_with_inputs` (`explore.rs:4832-4838`) and `resolve_llm_config` (`explore.rs:4314`). Scan, run and the other executing commands accept it and ignore it. Moving `--set` onto every executing command would advertise a flag they do not honor.

The `--timing`, `--timing-format`, `--timing-output` and `--timing-output-dir` flags are **not** execution-only and stay global: the timing collector is installed for every command and `persist_timing_run` (defined at `main.rs:1483`, called from the common finalization at `:1456-1463`) writes a timing artifact for any subcommand, executing or not. The `// ... the four timing* flags above are ...` comment at `args.rs:285` and the `timing*` entries in the hide list (`args.rs:312-315`) are wrong and are corrected by this issue.

## Evidence

Re-verified 2026-09-23 with `target/debug/shatter` built in the `audit-2026-09-22` worktree (source at `56c86168`):

- `shatter help spec-diff | wc -l` → 78; line 50 is `--allow-host-writes` and line 55 is `--set <KEY=VALUE>`. `shatter spec-diff --help | wc -l` → 48, with neither flag.
- `shatter doctor --help | grep -c allow-host-writes` → 0, but `shatter help doctor | grep -c allow-host-writes` → 1.
- `<cmd> --help | grep -c allow-host-writes` → 1 for each of analyze, solve, specify, stale, diff, compare, list-targets, nondeterminism, workspace and build-frontend. `help <cmd>` gives the same result.
- Code:
  - `shatter-cli/src/main.rs:70` `maybe_print_non_executing_help(raw_args)`, called at `main.rs:95` before clap parses.
  - `shatter-cli/src/args.rs:324` `NON_EXECUTING_COMMAND_PATHS` (spec-diff, init, doctor, cache, telemetry); `args.rs:344` `help_only_command()`; `args.rs:402` `resolve_subcommand_path()`; hidden-arg list `args.rs:310-315`.
  - `shatter-cli/src/host_writes.rs:84-95` `command_executes_targets()`.
  - `set_overrides` consumers: `main.rs:437` only (grep `set_overrides` across `shatter-cli/src`; `run.rs:3371` is a test name, not a consumer).
  - Timing: `main.rs:1456-1463` calls `persist_timing_run` for every subcommand.
- SPEC §2.10 (`SPEC.md:591-596`) lists analyze, solve, specify, stale, diff, spec-diff, compare, list-targets and doctor as never executing targets.
- Transcripts: `audits/2026-09-22/cli-ux-transcripts/help/help-subcommand-spec-diff.txt` compared with `help/spec-diff.txt`.
- Findings cli-ux-05 and prior-07 (both verified at P2) and docs-22 (P3, covered by this issue).

## Acceptance criteria

- [ ] `--allow-host-writes` is no longer `global = true` on `Cli`. It is defined only on the subcommands for which `command_executes_targets()` is true (for example through a flattened `ExecOptions` struct).
- [ ] `--set` is no longer global. It is defined only on subcommands that apply it. With today's code that is `explore` only. Wiring `--set` into scan/run is a separate decision and out of scope; if the implementer wires it into another command in this change, that command gets a test proving an override changes its effective config.
- [ ] `--timing*` flags remain global and keep working on every command; a test runs a non-executing command (for example `shatter list-targets <dir> --timing summary --timing-output <file>`) and asserts the timing file is written, both before and after the change. The `args.rs:285` comment and the hide-list entries for `timing*` are removed.
- [ ] `maybe_print_non_executing_help`, `help_only_command`, `resolve_subcommand_path` and `NON_EXECUTING_COMMAND_PATHS` are deleted. The `mut_subcommand`/hide-only approach is not an acceptable resolution, because it neither removes the definitions nor rejects the flags (gap 3).
- [ ] A test walks every subcommand of `Cli::command()`, including nested ones, and for each asserts: `shatter <cmd> --help` and `shatter help <cmd>` contain `--allow-host-writes` iff `command_executes_targets()` is true for it, and contain `--set` iff it is in an explicit `SET_CONSUMERS` table (initially `[explore]`). The subcommand list is enumerated from clap, so a new subcommand without a table entry fails the test. The test fails on current HEAD (at least for `help spec-diff` and `analyze --help`) and passes after the fix; the close comment records both runs.
- [ ] `shatter spec-diff --allow-host-writes a.json b.json` and `shatter scan <dir> --set defaults.max_iterations=5` are rejected as usage errors (exit 2, clap `unexpected argument`), each covered by a test.
- [ ] Behavior change is documented: the SPEC §8 changelog (`SPEC.md:1173`; the repo has no CHANGELOG file) notes that `--set` on commands other than explore, and `--allow-host-writes` on non-executing commands, are now rejected instead of silently ignored, and that the pre-subcommand position (`shatter --allow-host-writes explore ...`) is no longer accepted if that is the result. The walkthrough, gauntlet, `demo/` scripts and docs are grepped for such uses and fixed.
- [ ] The `command_executes_targets()` list is reviewed against SPEC §2.10 and the actual behavior of every command not in either list (for example `test`, `discover-deps`, `build-frontend`, `workspace`). SPEC §2.10 is updated so the two agree; the close comment lists each command and its classification.
- [ ] `SHATTER_ALLOW_HOST_WRITES` and config-file behavior are unchanged for executing commands; the existing host-write tests still pass.
- [ ] `task affected` passes, plus `task gauntlet` and `task walkthrough`, since this changes flag parsing on many commands. The close comment records the gates selected.

## Suggested approach

Move `--allow-host-writes` into a `#[command(flatten)] exec: ExecOptions` field on each executing subcommand (the direction of str-qwua7.20.1), and move `--set` onto `explore`'s args. Update `host_writes.rs` and the `--set` plumbing to read from the subcommand, not from `Cli`. The code comment in `main.rs` about why `mut_arg(hide)` failed is no longer relevant once the flags are not global.

The snapshot `shatter diff` command is being retired (retire-snapshot-diff, maintainer decision D2). If that lands first, `diff` drops out of the table automatically because the test enumerates clap.

## Out of scope

- Grouping help output with `help_heading` (str-9ee5).
- Tracker IDs in help text (help-tracker-ids-lint).
- Adding `--seed` to more commands (seed-for-explore-and-run). If `ExecOptions` lands first, that issue can reuse it.
- Making scan/run honor `--set` (a feature decision, not filed here).

## Dependencies

- Blocked by: none.
- Related: str-qwua7.15 (closed, partial; see help-flags-reopen-note), str-qwua7.20.1 (shared options struct), str-9ee5, retire-snapshot-diff, unknown-config-keys-warn (also touches `--set`).

## Source

Audit 2026-09-22 findings cli-ux-05 (areas/cli-ux.md F5), prior-07 (areas/prior-audit-regress.md) and docs-22. Merges the old drafts `drafts/shatter-code/28-help-only-argv-intercept.md` (the design fix) and `drafts/shatter-docs-ui/07-help-leaks-execution-flags.md` (the symptom).

---

<!-- file: 04-help-flags-reopen-note.md -->

---
slug: help-flags-reopen-note
kind: reopen-note
title: "Comment on closed str-qwua7.15: execution-only flags still shown by `help <cmd>` and 10 non-executing commands"
priority: P2
type: comment
labels: [cli, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-qwua7.15
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Comment on str-qwua7.15 (closed; do not reopen)

Target: `str-qwua7.15`. Action: add a comment only (`bd comments add str-qwua7.15 ...`). Leave the issue closed.

## Comment text

> Audit 2026-09-22 follow-up (findings cli-ux-05, prior-07, docs-22): this issue was closed on 2026-09-14 with the reason "Closed", but the fix is partial.
>
> - `shatter spec-diff --help` no longer shows `--allow-host-writes`, but `shatter help spec-diff` still does (line 50), along with `--set` (line 55): 78 lines against 48. `shatter help doctor` also shows it. The fix intercepts raw argv only for `-h/--help` (`shatter-cli/src/main.rs:70` `maybe_print_non_executing_help`), and clap's `help <cmd>` path bypasses it.
> - `NON_EXECUTING_COMMAND_PATHS` (`shatter-cli/src/args.rs:324`) lists five commands. analyze, solve, specify, stale, diff, compare, list-targets, nondeterminism, workspace and build-frontend still show `--allow-host-writes` in `--help`, and the flags still parse on non-executing commands.
> - The `timing*` flags were grouped with the execution-only flags, but timing artifacts are persisted for every command (`main.rs:1456-1463`), so they are not execution-only.
> - Verified 2026-09-23 with a `target/debug/shatter` built at `56c86168`.
>
> The structural fix is tracked in the new issue **help-hides-execution-flags** (<new id>): define `--allow-host-writes` only on commands where `command_executes_targets()` is true, define `--set` only on the commands that apply it (explore today), keep `timing*` global, delete the argv intercept, and add a test over both `<cmd> --help` and `help <cmd>` for every subcommand. Tracker-ID removal from help text, which this issue left out of scope, is filed as **help-tracker-ids-lint** (<new id>).

The filer substitutes `<new id>` with the ids assigned to help-hides-execution-flags and help-tracker-ids-lint.

---

<!-- file: 05-seed-for-explore-and-run.md -->

---
slug: seed-for-explore-and-run
kind: new
title: "Add --seed to explore and run (str-0m0vn wired it into scan only)"
priority: P2
type: feature
labels: [cli, seeds, reproducibility, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [explore-resume-options-key]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Add --seed to explore and run (str-0m0vn wired it into scan only)

## Problem

str-0m0vn's symptom was "There is no way to make `shatter scan` or `shatter explore` reproducible". It was closed after adding `--seed` to `scan` only, and its scan-only follow-ups (str-pbqyr, str-9m9o3) do not cover explore. `explore` and `run` still have no seed flag, so an explore or run result cannot be reproduced. That undermines bug reports, CI flake triage, and the D3 benchmark (concolic-vs-default-benchmark), which needs fixed seeds.

Explore resumes by default from its artifact directory, and its resume key does not include exploration options (explore-resume-options-key, bucket shatter-artifacts-correctness). If `--seed` were added before that key exists, a run with a different seed could silently be served a previous seed's results. This issue therefore depends on explore-resume-options-key and must not close with seed-sensitive resume broken.

CLAUDE.md's parity rule ("When adding a new ... CLI flag ... grep for the parallel code path") was not applied to the explore/scan/run flag trio.

## Evidence

Re-verified 2026-09-23 with `target/debug/shatter` built at `56c86168` of `audit-2026-09-22`:

- `shatter scan --help | grep -cE -- '--seed( |$|<)'` → 2. The same check on `explore --help` and `run --help` → 0. Their only seed-related flags are `--seeds-dir` and `--no-seeds`, which control the cross-function seed pool, not RNG seeding.
- `shatter-cli/src/args.rs:977-979`: `--seed` (`pub(crate) seed: Option<u64>`) is defined only in the Scan args. Its doc comment cites str-0m0vn.
- The scan reproducibility test `shatter-cli/tests/scan_seed_reproducibility.rs` controls the other nondeterminism sources: it passes `--parallelism` (`:116`), `--timeout-total` (`:118`), `--no-cache` (`:120`) and `--no-seeds` (`:121`) alongside `--seed` (`:125`).
- Scan-only seed follow-ups already open: str-pbqyr, str-9m9o3 (scan cache ignores the seed) and str-v1tzz (open, P2: report the effective seed and make seeded exploration testable).
- Finding cli-ux-08 (areas/cli-ux.md F8), verified at P2.

## Acceptance criteria

- [ ] `shatter explore <target> --seed N` and `shatter run --seed N` are accepted. The seed reaches the random explorer and the concolic orchestrator (`--concolic`), both engine paths, in the same way scan's does. A test per path asserts propagation (for example through the effective `ScanConfig`/explore config or an injected RNG), for both explore and run.
- [ ] Reproducibility tests, modelled on `scan_seed_reproducibility.rs`, exist for **explore** and for **run**. Each runs the command twice on the same fixture with the same seed, fresh artifact directories, and the same nondeterminism controls the scan test uses (caches disabled, seed pool disabled via `--no-seeds`, fixed parallelism of 1, a bounded iteration budget rather than a wall-clock-only budget). They assert identical path sets and generated inputs. A run with a different seed is allowed to differ. Each test is shown failing (flag rejected) before the change and passing after; the close comment records both runs.
- [ ] Resume is seed-sensitive: a test runs explore with `--seed 1` into an artifact dir, then re-runs with `--seed 2` against the same dir without `--clean`, and asserts the second run is not served the first run's cached results (it re-explores, or reports that the options changed). This test uses the options-hash key from explore-resume-options-key (the blocker) with the seed included in that hash.
- [ ] Help text for `--seed` is the same on scan, explore and run, and carries no tracker IDs (see help-tracker-ids-lint).
- [ ] E2E suites pass (`cargo test --test e2e_concolic`, `e2e_concolic_go`, `e2e_concolic_rust`) because explorer and orchestrator wiring changes. `task affected` passes, and the close comment records the gates selected.

## Suggested approach

Define `--seed` once in a shared options struct flattened into scan, explore and run. That is the str-qwua7.20.1 direction; if help-hides-execution-flags adds an `ExecOptions` struct first, put it there. Thread it through the same config field scan uses so both engine paths pick it up, and add it to the options hash introduced by explore-resume-options-key.

## Out of scope

- The scan-specific seed bugs (str-pbqyr, str-9m9o3).
- Printing the seed in reports (str-v1tzz). Extend that issue to explore and run once this lands, rather than duplicating it here.

## Dependencies

- Blocked by: explore-resume-options-key (bucket shatter-artifacts-correctness), which introduces the options-hash resume key the seed must be part of.
- Related: str-0m0vn (closed; see seed-reopen-note), str-v1tzz, str-pbqyr, str-9m9o3, str-qwua7.20.1, help-hides-execution-flags, concolic-vs-default-benchmark (needs fixed seeds).

## Source

Audit 2026-09-22 finding cli-ux-08; old draft `drafts/shatter-code/30-seed-for-explore-and-run.md`.

---

<!-- file: 06-seed-reopen-note.md -->

---
slug: seed-reopen-note
kind: reopen-note
title: "Comment on closed str-0m0vn: symptom named explore, but --seed exists only on scan"
priority: P2
type: comment
labels: [cli, seeds, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-0m0vn
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Comment on str-0m0vn (closed; do not reopen)

Target: `str-0m0vn`. Action: add a comment only. Leave the issue closed.

## Comment text

> Audit 2026-09-22 follow-up (finding cli-ux-08): this issue's symptom was "There is no way to make `shatter scan` or `shatter explore` reproducible", but the fix (merge aafc8ba7) added `--seed` to `scan` only (`shatter-cli/src/args.rs:977-979`). As of HEAD `56c86168` (verified 2026-09-23), `shatter explore --help` and `shatter run --help` have no `--seed`. The follow-ups filed at close (str-pbqyr, str-9m9o3) are scan-only.
>
> explore and run are now tracked in **seed-for-explore-and-run** (<new id>): a shared `--seed` on scan/explore/run reaching both engine paths, an explore reproducibility test, and the seed included in the resume key.

The filer substitutes `<new id>` with the id assigned to seed-for-explore-and-run.

---

<!-- file: 07-unknown-config-keys-warn.md -->

---
slug: unknown-config-keys-warn
kind: new
title: "Warn on unknown config keys and --set key typos (currently silently ignored)"
priority: P2
type: feature
labels: [config, cli, usability, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Warn on unknown config keys and --set key typos (currently silently ignored)

## Problem

Shatter type-checks config values but not config keys. A typo such as `--set defaults.max_iteratons=5`, or a misspelled key in `.shatter/config.yaml` or `shatter.config.json`, is silently dropped. The run continues with the default value, exits 0, and prints no warning. The user believes the setting took effect. The Go frontend's config loader already warns on unknown top-level keys, so the core is inconsistent with it as well.

A related silent drop: `--set` is a global flag, but only `explore` applies it (`main.rs:437` is the only reader of `cli.set_overrides`). `scan --set ...` and `run --set ...` accept the flag and ignore it entirely, so even a correctly spelled key has no effect there. help-hides-execution-flags removes `--set` from those commands; until it lands, this issue makes the drop visible.

## Evidence

Re-verified against `audit-2026-09-22` (source at `56c86168`):

- `shatter-core/src/config.rs:4178-4185`, test `parse_set_overrides_unknown_field_is_ignored_by_serde`, with the comment "ShatterConfig derives Deserialize without it [deny_unknown_fields], so unknown keys are silently ignored." The test asserts only "no panic". It pins the lenient behavior.
- `shatter-core/src/config.rs:1104` `parse_set_overrides`.
- Config load sites that consume `--set`:
  - the main per-function path: `shatter-core/src/config.rs:1597` `resolve_function_config_with_inputs(..., set_overrides)`, called from `shatter-cli/src/commands/explore.rs:4832-4838`;
  - the LLM path: `shatter-cli/src/helpers.rs:1571` `resolve_llm_config` (`:1584-1585` calls `parse_set_overrides`), called from `explore.rs:4314`.
- `--set` is read only in the explore dispatch (`shatter-cli/src/main.rs:437`); no scan or run code reads `set_overrides`.
- Go frontend contrast: `shatter-go/config/loader.go:431-444` emits `config <path>: ignoring unknown top-level key "<k>"`.
- Observed (findings.json cli-ux-09, reproduced by the verifier): `explore ... --set defaults.max_iteratons=5` exits 0 with nothing about the key on stderr (`audits/2026-09-22/cli-ux-transcripts/err-setunknown.err`). `--set foo.bar=1` exits 0. `--set defaults.max_iterations=abc` exits 2 (`err-badset.err`).
- `strsim` is already in `Cargo.lock`.

## Acceptance criteria

- [ ] An unknown key in `.shatter/config.yaml`, in `shatter.config.json`, or in a `--set KEY=VALUE` override produces exactly one warning on stderr per key per run, at every load site listed above (per-function config and LLM config). The warning names the source (file path or `--set`), the full dotted key path and, when a known key is close, a "did you mean `defaults.max_iterations`?" suggestion.
- [ ] A strict mode turns those warnings into a usage error (exit 2). Pick one form (`--strict-config` flag, config key, or env var), document it in the config reference, and test it.
- [ ] `parse_set_overrides_unknown_field_is_ignored_by_serde` is replaced by tests that assert (a) the warning and suggestion for a typo in `--set` on `explore`, (b) the same for a typo in a YAML config file, (c) exit 2 in strict mode, and (d) no warning for a valid config (use the repo's own example configs and `demo/` configs as a no-false-positive check). Tests (a) and (b) fail before the change; the close comment records both runs.
- [ ] `--set` on a command that does not apply it is not silent: either it is rejected by clap (if help-hides-execution-flags has landed) or the command prints `warning: --set is ignored by <cmd>` on stderr. A CLI test covers `scan <dir> --set defaults.max_iterations=5` and asserts whichever of the two applies at close time.
- [ ] Keys that are legitimately open-ended (maps keyed by user data, if any) do not warn. List them in the close comment.
- [ ] Warnings go to stderr only, so JSON stdout contracts (`shatter-cli/tests/json_stdout_contract.rs`) still pass.
- [ ] `task affected` passes, and the close comment records the gates selected.

## Suggested approach

Wrap the deserializer at each config load site, and in `parse_set_overrides`, with `serde_ignored`, collecting the ignored paths. Match them against the known key set with `strsim` to produce suggestions. Deduplicate across load sites (the same `--set` pairs are parsed per function and for the LLM config). Avoid `deny_unknown_fields` as the default because it rules out warn-only mode and forward compatibility.

## Out of scope

- A generated config reference document (str-qwua7.21.1).
- Validating free-string format values for stale/revalidate (str-9ee5).
- Changes to the Go frontend loader, which already warns.
- Making scan/run apply `--set` (see help-hides-execution-flags).

## Dependencies

- Blocked by: none.
- Related: str-qwua7.21.1, str-9ee5, help-hides-execution-flags (scopes `--set` to explore).

## Source

Audit 2026-09-22 finding cli-ux-09 (areas/cli-ux.md F9); old draft `drafts/shatter-code/31-unknown-config-keys-warn.md`.

---

<!-- file: 08-help-tracker-ids-lint.md -->

---
slug: help-tracker-ids-lint
kind: new
title: "Remove tracker IDs and internal status notes from --help, SPEC and README; add a lint"
priority: P2
type: task
labels: [cli, docs, usability, agents, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Remove tracker IDs and internal status notes from --help, SPEC and README; add a lint

## Problem

User-facing text cites internal tracker IDs and process status. `///` doc comments on clap args double as internal notes, so IDs such as `str-gg9v` and `str-frc.6` appear in `--help`. The 2026-09-04 audit flagged this, and a new ID (`str-0m0vn`, scan `--seed`) was added on 2026-09-05, the day after. SPEC and README carry the same pattern, including one paragraph of internal status ("Do not wait for that issue to land ...") inside the exit-code contract. Nothing lints for it. str-qwua7.15 explicitly left "tracker-ID removal from help text" out of scope, and it was never filed.

The other unfiled 2026-09-04 UI items are handled separately by unfiled-0904-ui-items-reconcile.

## Evidence

Re-verified 2026-09-23 with `target/debug/shatter` built at `56c86168` of `audit-2026-09-22`:

- Rendering `--help` for every top-level subcommand and running `grep -o 'str-[a-z0-9.]*[a-z0-9]' | sort | uniq -c` finds **13 distinct IDs**: str-gg9v (19 times), str-jeen.13 (2), str-v01r (2), str-0m0vn, str-1fwt, str-1wcl, str-d6hj, str-frc.3, str-frc.5, str-frc.6, str-izhn, str-o09e and str-p2rz. The verifier counted 11-13 depending on whether nested pages and dotted IDs are counted separately.
- Sources in `shatter-cli/src/args.rs` (`///` comments): lines 106, 165, 796, 801, 811, 818, 833, 977 (str-0m0vn), 1014, 1144, 1148, 1168, 1589, 1773, 1775, 2200, 2225, 2587, 2608. `args.rs` also has IDs in `//` comments (for example 283, 321, 3491), which are fine.
- `SPEC.md` has 10 lines containing `str-`. `SPEC.md:645-647` (§2.11): "str-qwua7.12 tracks bringing the remaining commands into line. Do not wait for that issue to land before relying on this table." SPEC §8 changelog starts at `SPEC.md:1173`.
- `README.md:293`: `> **Breaking change (str-gg9v).** ...`
- `.claude/skills/rust-conventions/SKILL.md` has no rule on tracker IDs in user-facing strings.
- str-qwua7.45 covers only README's "Executing Target Functions Safely" section.
- Findings cli-ux-17 (P2) and docs-18 (P3), merged here at P2 (report section 15.1).

## Acceptance criteria

- [ ] A unit test in `shatter-cli` walks `Cli::command()` recursively, renders every subcommand's long help (`render_long_help`), and fails on `/str-[a-z0-9]+/`. It fails on current HEAD and passes after the fix; the close comment records both runs (with the failing ID list from the first run).
- [ ] Every tracker ID now in `///` on clap args is moved to a `//` code comment next to the arg, or dropped. No help text loses user-relevant meaning. For example, the `str-gg9v` sentences become a plain description of the host-write default.
- [ ] `SPEC.md` and `README.md` user-facing sections contain no tracker IDs outside the SPEC §8 changelog. Where timing matters, use dated notes ("since 2026-07"). A check run by an existing gate (a script called from `check-static`, or a Rust test that reads the files) greps these files, excluding the changelog, and fails on `str-` IDs. It is shown failing on current HEAD.
- [ ] The internal-status paragraph in SPEC §2.11 (`SPEC.md:645-647`) is removed, or rewritten as a short "Known deviations" list that states each deviation, with no tracker references.
- [ ] `.claude/skills/rust-conventions/SKILL.md` gains a rule: no tracker IDs in `///` docs on clap args or in any user-facing string; put them in `//` comments.
- [ ] `task affected` passes, and the close comment records the gates selected.

## Suggested approach

Write the help-rendering test first to get the inventory, then rewrite each `///` line.

## Out of scope

- Reconciling the unfiled 2026-09-04 UI items (unfiled-0904-ui-items-reconcile).
- Help-heading grouping and flag vocabulary (str-9ee5).
- Hiding execution-only flags (help-hides-execution-flags).
- The README "Executing Target Functions Safely" wording already owned by str-qwua7.45. Coordinate so both do not rewrite the same lines.
- The status of str-qwua7.12 itself (exit-codes-qwua7-12-note).

## Dependencies

- Blocked by: none.
- Related: str-qwua7.15, str-qwua7.45, str-9ee5, help-hides-execution-flags, unfiled-0904-ui-items-reconcile.

## Source

Audit 2026-09-22 findings cli-ux-17 (areas/cli-ux.md F17) and docs-18 (areas/docs.md). Merges the old drafts `drafts/shatter-agent/28-help-tracker-ids-lint-and-unfiled-ui-items.md` (primary; its reconciliation half is now unfiled-0904-ui-items-reconcile) and `drafts/shatter-docs-ui/08-tracker-ids-in-help-and-docs.md`.

---

<!-- file: 09-telemetry-known-subcommands.md -->

---
slug: telemetry-known-subcommands
kind: new
title: "Telemetry KNOWN_SUBCOMMANDS is stale: sanitized_args redacts most real subcommand tokens and lists 3 nonexistent ones"
priority: P3
type: bug
labels: [telemetry, cli, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Telemetry KNOWN_SUBCOMMANDS is stale: sanitized_args redacts most real subcommand tokens and lists 3 nonexistent ones

## Problem

`telemetry::sanitize_args` uses a hand-maintained allow-list in core, `KNOWN_SUBCOMMANDS`, to decide which bare argv tokens are kept verbatim; any other non-flag token is redacted. The list has drifted from the clap definition: it names three commands that do not exist and omits most real ones. So for `spec-diff`, `doctor`, `list-targets`, `observe` and others, the subcommand token (and nested subcommand tokens such as `cache clear`) is redacted in the `sanitized_args` field of `command_run` events and in `bad_cli_args` events.

The command's identity itself is **not** lost in `command_run`: its `subcommand` field is filled independently from the parsed clap variant (`subcommand_label`, passed straight into `EventPayload::CommandRun` in `queue_command_run_event`, `shatter-cli/src/main.rs:1543-1555`). The defect is limited to argv redaction: `sanitized_args` for those commands, and `bad_cli_args` events (`main.rs:96-110`), where no parsed subcommand exists and `sanitized_args` is the only record of what was typed.

## Evidence

Re-verified against `audit-2026-09-22` (source at `56c86168`):

- `shatter-core/src/telemetry.rs:53-65` `KNOWN_SUBCOMMANDS` = explore, scan, **export**, **spec**, run, analyze, init, stale, telemetry, help, **version**. `export`, `spec` and `version` are not subcommands.
- Used by `sanitize_args` at `telemetry.rs:263` (`let known_subcommands: HashSet<&str> = KNOWN_SUBCOMMANDS...`; "Rule 2: Preserve known subcommands"). Proptests at `:1120-1139` sample from the same list, so they cannot detect drift.
- Callers: `queue_command_run_event` (`main.rs:1543`, `sanitized_args` field) and the clap-error path in `main.rs:96-110` (`bad_cli_args`).
- Missing real subcommands include spec-diff, diff, doctor, list-targets, observe, bench, properties, revalidate, solve, specify, compare, nondeterminism, workspace, build-frontend, discover-deps, cache and test (compare with `shatter --help`).
- `shatter telemetry status` reports enabled by default.
- Finding cli-ux-18, verified at P3.

## Acceptance criteria

- [ ] Core no longer hardcodes the subcommand list. It is derived from `Cli::command()` (all subcommand names, including nested ones such as `cache clear`) in shatter-cli and passed into `sanitize_args`, keeping the cli → core dependency direction. Alternatively, if the list must stay in core, a shatter-cli test asserts it equals the clap subcommand name set.
- [ ] A shatter-cli test iterates every clap subcommand name and asserts `sanitize_args(&[name, "/some/path"])` keeps `name` verbatim in the returned `sanitized_args` and still redacts the path. It fails on current HEAD for `spec-diff`; the close comment records the failing and passing runs.
- [ ] A test asserts that a misspelled subcommand in a `bad_cli_args` event (for example `spec-dif`) is redacted, so free-form user text is never kept just because it resembles a command.
- [ ] Unknown or free-form argv values are still redacted, and the existing redaction proptests still pass.
- [ ] `export`, `spec` and `version` are no longer preserved unless they become real subcommands.
- [ ] When `diff` is removed by retire-snapshot-diff, the derived list follows with no manual edit.
- [ ] `task affected` passes, and the close comment records the gates selected.

## Suggested approach

Add a `known_subcommands: &[&str]` (or owned) parameter to `sanitize_args`, filled from clap in shatter-cli.

## Out of scope

- The `command_run.subcommand` field, which is already correct.
- Other telemetry fields and the deferred telemetry epic (str-rlbq).

## Dependencies

- Blocked by: none.
- Related: str-rlbq (deferred telemetry epic), str-jhks (closed; added `command_run`), retire-snapshot-diff.

## Source

Audit 2026-09-22 finding cli-ux-18 (areas/cli-ux.md F18); old draft `drafts/shatter-code/33-telemetry-known-subcommands.md`.

---

<!-- file: 10-cli-minor-output-and-help-polish.md -->

---
slug: cli-minor-output-and-help-polish
kind: new
title: "Help text is wrong: --dry-run says 'Requires --output' (it does not); target help omits .rs = Rust"
priority: P3
type: bug
labels: [cli, help, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Help text is wrong: --dry-run says 'Requires --output' (it does not); target help omits .rs = Rust

## Problem

Two explore help strings contradict actual behavior:

1. **`--dry-run`** help says "Requires --output", but `explore ... --dry-run` without `-o` works and exits 0.
2. **The target argument** help says "(.ts = TypeScript, .go = Go)". `.rs` (Rust) targets are supported.

This draft previously bundled ten other CLI output defects. They have separate implementations and completion conditions, so they are now separate drafts in this bucket: analyze-only-sandbox-refusal, analyze-only-output-detail, explore-function-not-found-diagnostics, spec-flag-dropped-with-spec-out, html-source-non-executable-lines, failure-table-language-any, demo-complete-with-errors-green, and exit-codes-qwua7-12-note (the `print_stdout` exit code and the str-qwua7.12 status).

## Evidence

Re-verified against `audit-2026-09-22` (source at `56c86168`). Transcripts are in `audits/2026-09-22/cli-ux-transcripts/`.

- `shatter-cli/src/args.rs:676`: `--dry-run` doc says "Requires --output". `err-dryrun.out`: `explore ... --dry-run` with no `-o` exits 0.
- `shatter-cli/src/args.rs:501`: target help "(.ts = TypeScript, .go = Go)".
- Finding artifacts-16 (P3).

## Acceptance criteria

- [ ] The `--dry-run` help string describes actual behavior (no "Requires --output"), and a CLI test asserts `explore <fixture> --dry-run` without `-o` exits 0, so the help cannot drift back unnoticed.
- [ ] The target-argument help lists `.rs = Rust` alongside TS and Go. A test asserts `explore --help` contains `.rs`.
- [ ] Other target-help strings that list languages (scan, run) are grepped and fixed the same way; the close comment lists what was checked.
- [ ] `task affected` passes. The close comment records the gates selected.

## Suggested approach

Edit the `///` doc comments on the two args. If help-tracker-ids-lint lands first, its help-rendering test can host these assertions.

## Out of scope

- All other CLI output defects listed above (their own drafts).
- Help grouping and flag vocabulary (str-9ee5).

## Dependencies

- Blocked by: none.
- Related: help-tracker-ids-lint, str-9ee5.

## Source

Audit 2026-09-22 finding artifacts-16 (items 4-5 of the merged old drafts `drafts/shatter-docs-ui/13-minor-output-defects.md` and `drafts/shatter-docs-ui/14-analyze-only-and-error-help-polish.md`).

---

<!-- file: 11-analyze-only-sandbox-refusal.md -->

---
slug: analyze-only-sandbox-refusal
kind: new
title: "explore/run --analyze-only is refused by the host-write gate although it executes nothing"
priority: P3
type: bug
labels: [cli, sandbox, usability, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# explore/run --analyze-only is refused by the host-write gate although it executes nothing

## Problem

`shatter explore <file>:<fn> --analyze-only` only analyzes the function (it prints parameter and branch counts) and never executes a target. It is still refused with `Error: refusing to execute target functions without a sandbox.` (exit 2) when no sandbox backend is configured and `--allow-host-writes` / `SHATTER_ALLOW_HOST_WRITES` is not set. str-gg9v introduced the default-deny gate without an analyze-only exemption, so the cheapest first-use command demands a sandbox decision.

## Evidence

Re-verified against `audit-2026-09-22` (source at `56c86168`):

- `shatter-cli/src/main.rs:192` calls `host_writes::setup(&cli.command, cli.allow_host_writes)` before dispatch; on `Err` it prints the refusal and exits.
- `shatter-cli/src/host_writes.rs:84-95` `command_executes_targets()` matches `CliCommand::Explore(_)` and `CliCommand::Run { .. }` regardless of their `analyze_only` fields (`args.rs:546` for explore, `args.rs:1389` for run).
- findings.json goals-18, verifier verdict "confirmed": `explore 05-unions.ts:computeArea --analyze-only` prints the sandbox refusal and exits 2; with `--allow-host-writes` it prints `computeArea (05-unions.ts:17) params: 1, branches: 6`.
- The artifacts-16 verifier did not see the refusal in `audits/2026-09-22/artifact-samples/analyze-only-ts.err` or `cli-ux-transcripts/err-analyze-only.*`; those were captured with host writes allowed. Reproduce with `SHATTER_ALLOW_HOST_WRITES` and `SHATTER_SANDBOX_BACKEND` unset.

## Acceptance criteria

- [ ] A CLI test runs `shatter explore <ts-fixture>:<fn> --analyze-only` with `SHATTER_ALLOW_HOST_WRITES` and the sandbox-backend variable removed from the environment and no `--allow-host-writes`, and asserts exit 0 and the analysis line on stdout. The test fails on current HEAD with the refusal message; the close comment records both runs.
- [ ] The same test exists for `shatter run --analyze-only` (or the close comment shows `run --analyze-only` executes targets and must stay gated, with the code path cited).
- [ ] The exemption is decided from the parsed command, not from argv: `command_executes_targets()` (or its replacement from help-hides-execution-flags) returns false for explore/run when `analyze_only` is set, and a unit test covers both values of the flag.
- [ ] Without `--analyze-only`, explore still refuses as before; the existing host-write refusal tests pass unchanged.
- [ ] `task affected` passes. The close comment records the gates selected.

## Out of scope

- The content of the analyze-only output (analyze-only-output-detail).
- Scoping `--allow-host-writes` to executing commands (help-hides-execution-flags).

## Dependencies

- Blocked by: none.
- Related: str-gg9v (closed; introduced the gate), help-hides-execution-flags (same predicate), analyze-only-output-detail.

## Source

Audit 2026-09-22 finding goals-18 (P3), refusal half. Split from cli-minor-output-and-help-polish item 1.

---

<!-- file: 12-analyze-only-output-detail.md -->

---
slug: analyze-only-output-detail
kind: new
title: "--analyze-only output shows only counts: no parameter names/types, no branch conditions, ignores --format"
priority: P3
type: feature
labels: [cli, explore, report, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# --analyze-only output shows only counts: no parameter names/types, no branch conditions, ignores --format

## Problem

`explore --analyze-only` prints one line per function, for example `classifyNumber  (arithmetic-v1.ts:11)` / `  params: 1, branches: 3`, or `computeArea (05-unions.ts:17) params: 1, branches: 6`. It shows no parameter names or types and no branch conditions, although the walkthrough promises "types and conditions". Output is un-headed plain text whatever `--format` says. The missing parameter type hid the string-typed discriminant behind goals-07.

## Evidence

- `audits/2026-09-22/cli-ux-transcripts/err-analyze-only.out` and `audits/2026-09-22/artifact-samples/analyze-only-ts.out`.
- findings.json goals-18 (recommendation: "show parameter types in analyze-only output") and artifacts-16.

## Acceptance criteria

- [ ] Diagnosis recorded first, in a comment on this issue before implementation: whether the analyze response already carries parameter names/types and branch condition text for TS, Go and Rust (cite the protocol type and a sample response per frontend). If any frontend lacks them, the implementation follows `protocol/GOVERNANCE.md`, updates `protocol/parity-matrix.yaml` and runs `task parity` and `task conformance`.
- [ ] `--analyze-only` output lists each parameter with its name and type, and each branch with its condition text, rendered in the active `--format` (markdown by default, with a heading; text and html per explore-format-flag-ignored if it has landed).
- [ ] Snapshot tests cover TS and at least one of Go or Rust; they fail on current HEAD (no types in output) and pass after.
- [ ] `task affected` and `task walkthrough` pass (walkthrough output changes). The close comment records the gates selected.

## Out of scope

- The sandbox refusal (analyze-only-sandbox-refusal).

## Dependencies

- Blocked by: none. Simpler after explore-format-flag-ignored.
- Related: explore-format-flag-ignored, analyze-only-sandbox-refusal.

## Source

Audit 2026-09-22 findings goals-18 (output half) and artifacts-16. Split from cli-minor-output-and-help-polish item 2.

---

<!-- file: 13-explore-function-not-found-diagnostics.md -->

---
slug: explore-function-not-found-diagnostics
kind: new
title: "explore file:missingFn reports an all-zero failure breakdown and does not list available functions"
priority: P3
type: bug
labels: [cli, explore, error-handling, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# explore file:missingFn reports an all-zero failure breakdown and does not list available functions

## Problem

`shatter explore arithmetic-v1.ts:doesNotExist` prints `[error] Analyze error (FunctionNotFound): Function not found: doesNotExist in arithmetic-v1.ts` and then `Error: explore: all 1 attempted target(s) failed (build_failed=0, runtime_failed=0, timed_out=0); no completed functions`. The breakdown has no category for analyze failures, so every counter is 0, and the error does not list the functions that do exist.

## Evidence

- `audits/2026-09-22/cli-ux-transcripts/err-nofn.err`.
- `shatter-cli/src/commands/explore.rs` `ExploreFailure::AllAttemptedTargetsFailed { attempted_targets, build_failed, runtime_failed, timed_out }` (returned by `decide_explore_exit_status`, `explore.rs:744`): no analyze/not-found bucket.
- Finding artifacts-16 (P3).

## Acceptance criteria

- [ ] A missing target function is counted in its own category (for example `analyze_failed=1` or `not_found=1`) in the failure summary; the summary never reports `N attempted target(s) failed` with all counters 0. A unit test on `decide_explore_exit_status` covers it.
- [ ] The error lists the exported functions in that file and adds a "did you mean `<name>`?" suggestion when one is close (edit distance). A CLI test on a TS fixture asserts both; it fails on current HEAD.
- [ ] Exit code stays 2.
- [ ] `task affected` passes. The close comment records the gates selected.

## Dependencies

- Blocked by: none.
- Related: str-qwua7.12 (exit codes), str-qwua7.33.

## Source

Audit 2026-09-22 finding artifacts-16. Split from cli-minor-output-and-help-polish item 3.

---

<!-- file: 14-spec-flag-dropped-with-spec-out.md -->

---
slug: spec-flag-dropped-with-spec-out
kind: new
title: "explore --spec is silently ignored when --spec-out is also given"
priority: P3
type: bug
labels: [cli, explore, spec, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# explore --spec is silently ignored when --spec-out is also given

## Problem

`explore ... --spec --spec-out spec.json` writes the spec file but prints no spec to stdout and no warning. The user asked for both.

## Evidence

- `shatter-cli/src/main.rs:418-424` passes `spec_out` and folds `--spec`, `--spec-json` and `--spec-out` into combined booleans (`spec || spec_json || spec_out.is_some() || invariants`, `spec_json || spec_out.is_some()`), so the stdout request is not distinguished once `--spec-out` is set.
- Finding artifacts-16 (P3); not independently reproduced in this revision.

## Acceptance criteria

- [ ] Decide and document one behavior: `--spec` with `--spec-out` prints the spec to stdout as well as writing the file, or clap rejects the combination with a usage error (exit 2). Silent dropping is not allowed.
- [ ] A CLI test covers the combination and fails on current HEAD (stdout lacks the spec and stderr has no message).
- [ ] The `--spec` / `--spec-out` help text states the chosen behavior.
- [ ] `task affected` passes. The close comment records the gates selected.

## Dependencies

- Blocked by: none.
- Related: explore-report-printed-twice (stdout emission rules).

## Source

Audit 2026-09-22 finding artifacts-16. Split from cli-minor-output-and-help-polish item 6.

---

<!-- file: 15-html-source-non-executable-lines.md -->

---
slug: html-source-non-executable-lines
kind: new
title: "HTML report source view marks non-executable lines (signature, braces, blanks) as uncovered"
priority: P3
type: bug
labels: [report, html, coverage, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# HTML report source view marks non-executable lines (signature, braces, blanks) as uncovered

## Problem

The HTML source view gives every line in a function's span either the `covered` or the `uncovered` class. Signature lines, closing braces, blank lines and comments therefore show as uncovered next to a header saying "7/7 lines".

The renderer cannot fix this on its own. `render_source_block` (`shatter-core/src/html_templates.rs:45-51`) receives only a file path, a start/end line span and a `HashSet<u32>` of covered lines. Coverage results carry an executable-line **count** (`total_lines`, for example `shatter-core/src/pipeline.rs:947`), not the **set** of executable lines. Guessing from text (braces, blank lines) is wrong: a signature line can hold default-argument code and a brace line can hold a statement.

## Evidence

- `shatter-core/src/html_templates.rs:45-51` signature; callers at `:258` and `:420`; test `render_source_block_marks_covered_and_uncovered_lines` at `:623`.
- `total_lines` is a count in `pipeline.rs:947` and the report structs.
- Finding artifacts-16 (P3; the HTML claim was not independently re-verified by the audit verifier).

## Acceptance criteria

- [ ] Diagnosis recorded first, in a comment before implementation: for TS, Go and Rust, where the executable-line set that produces `total_lines` is computed (frontend instrumentation or core), and whether it is available at report time. If it is not persisted, the fix persists it (and, if that crosses the protocol, follows `protocol/GOVERNANCE.md` and the parity checklist).
- [ ] `render_source_block` takes the executable-line set and emits three classes: covered, uncovered (executable, not hit), and neutral (not executable). No text-pattern heuristics decide executability.
- [ ] `render_source_block_marks_covered_and_uncovered_lines` is extended with a neutral line; a new test renders a real explored TS function and asserts that when the header says N/N lines, no line has the `uncovered` class. That test fails on current HEAD.
- [ ] `task affected` passes. The close comment records the gates selected.

## Dependencies

- Blocked by: none.
- Related: none known.

## Source

Audit 2026-09-22 finding artifacts-16. Split from cli-minor-output-and-help-polish item 7.

---

<!-- file: 16-failure-table-language-any.md -->

---
slug: failure-table-language-any
kind: new
title: "explore failure-impact table shows Rust (and other unclassified) failures only under language `any`"
priority: P3
type: bug
labels: [cli, explore, report, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# explore failure-impact table shows Rust (and other unclassified) failures only under language `any`

## Problem

The "Failure impact" table in the explore report has a `Lang` column. For a Rust timeout, the only row is `| \`timed_out\` | \`any\` | ...`, which reads as "any language" rather than "Rust". The `any` row is a deliberate cross-language outcome rollup; language-specific rows exist only for Go build failures and TS build/runtime failures. When no language row exists (all Rust failures, all timeouts, Go runtime failures), the table never names the language.

## Evidence

- `audits/2026-09-22/artifact-samples/rust-explore.md:14`: `| \`timed_out\` | \`any\` | 1 | 1 | 13 | 13 | 100.0% |`.
- `shatter-cli/src/commands/explore.rs:3244-3277`: per-language rows only for `(BuildFailed, go)` and `(BuildFailed|RuntimeFailed, ts)`; every failure also adds an `("any", outcome)` rollup row.
- Finding artifacts-16 (verified).

## Acceptance criteria

- [ ] Every failure contributes to a row labelled with its target's real language (`rust`, `go`, `ts`) for its outcome category, in addition to or instead of the rollup. The rollup row, if kept, is labelled unambiguously (for example `all`) and documented.
- [ ] A unit test on the failure-impact builder feeds a Rust timed-out summary and asserts a `rust` / `timed_out` row; it fails on current HEAD. The existing `any`-row test (`explore.rs:~11280`) is updated to the new label.
- [ ] `task affected` passes. The close comment records the gates selected.

## Dependencies

- Blocked by: none.

## Source

Audit 2026-09-22 finding artifacts-16. Split from cli-minor-output-and-help-polish item 8 (evidence corrected: `any` is a rollup row, not a mislabel of the Rust row).

---

<!-- file: 17-demo-complete-with-errors-green.md -->

---
slug: demo-complete-with-errors-green
kind: new
title: "walkthrough.sh and gauntlet.sh print 'complete with errors' in green"
priority: P3
type: bug
labels: [demo, ux, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# walkthrough.sh and gauntlet.sh print 'complete with errors' in green

## Problem

When the walkthrough or gauntlet finishes with errors, the final line is printed in bold green, the same color as success, so a failing run looks like a pass at a glance.

## Evidence

- `demo/walkthrough.sh:388`: `echo "${BOLD}${GREEN}Walkthrough complete with errors.${RESET}"`.
- `demo/gauntlet.sh:920`: `echo "${BOLD}${GREEN}Gauntlet complete with errors.${RESET}"`.
- Finding artifacts-16 (verified by reading the scripts).

## Acceptance criteria

- [ ] Both lines use red (or yellow) instead of green; `grep -n 'GREEN}.*with errors' demo/*.sh` returns nothing.
- [ ] The scripts' exit status on errors is unchanged (the gauntlet still exits 1).
- [ ] `task walkthrough` and `task gauntlet` run; the close comment records their final lines.

## Dependencies

- Blocked by: none.

## Source

Audit 2026-09-22 finding artifacts-16. Split from cli-minor-output-and-help-polish item 9.

---

<!-- file: 18-exit-codes-qwua7-12-note.md -->

---
slug: exit-codes-qwua7-12-note
kind: note-to-existing
title: "Note on str-qwua7.12: most single-target exit-2 classes now hold, but multi-target partial failure exits 0 (contrary to its AC) and print_stdout exits 1 on I/O errors"
priority: P1
type: note
labels: [cli, exit-codes, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-qwua7.12
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-qwua7.12 (open, P1; do not close)

Target: `str-qwua7.12` ("Implement SPEC §2.11 exit codes: 2 for tool/usage errors, 1 only for fired gates"), verified open at P1 with `bd show` on 2026-09-23. Action: add a comment only. Do not close it: its acceptance checks are not met.

Earlier drafts proposed closing str-qwua7.12 because the single-target cases now exit 2. The live issue also requires "exit 1 if at least one target succeeded and at least one failed (partial failure is a fired-gate class)", and current code explicitly enforces exit 0 for that case, so closure is not supported.

## Comment text

> Audit 2026-09-22 status (findings cli-ux-19, areas/cli-ux.md section 0, prior-audit-regress.md), checked against source at `56c86168`:
>
> - **Now exit 2 (single target):** missing file, `.py` target, unknown function, bad `--set` value, function glob, spec-diff bad JSON, missing spec, host-write refusal. `shatter-cli/src/main.rs` `error_exit_code` (`:1475`) maps any non-`GateFailure` error to 2. These came from audit transcripts, not a fresh run; re-run them before relying on them.
> - **Not met — multi-target partial failure:** this issue's acceptance checks require exit 1 when at least one target succeeded and at least one failed. `shatter-cli/src/commands/explore.rs` `decide_explore_exit_status` (`explore.rs:744`) returns `Ok` for that case, and the test `decide_exit_status_ok_partial_success_with_some_failed_targets` (`explore.rs:7075`) pins exit 0 ("Partial-success policy"). Either the acceptance check or that policy must change; the two currently contradict. scan and run need the same check.
> - **Not met — stdout I/O error:** `shatter-cli/src/helpers.rs:337-347` `print_stdout` calls `std::process::exit(1)` on a non-EPIPE write error. Under SPEC §2.11 that is a tool error (2). Add a test (for example writing to a closed or full fd, `/dev/full` on Linux) asserting exit 2.
> - The SPEC §2.11 paragraph that cites this issue ("Do not wait for that issue to land ...", `SPEC.md:645-647`) is being removed by **help-tracker-ids-lint** (<new id>).
> - Closing evidence should be a table of every class in the acceptance checks with the command run on a fresh build, its observed exit code, and the test that pins it, including both multi-target cases.

The filer substitutes `<new id>` with the id assigned to help-tracker-ids-lint.

## Source

Audit 2026-09-22 findings cli-ux-19 and the str-qwua7.12 status in areas/cli-ux.md section 0. Split from cli-minor-output-and-help-polish items 10 and 11.

---

<!-- file: 19-unfiled-0904-ui-items-reconcile.md -->

---
slug: unfiled-0904-ui-items-reconcile
kind: new
title: "Reconcile the four unfiled 2026-09-04 usability items (12, 16, 17, 20) against the tracker and hand drafts to the maintainer"
priority: P3
type: task
labels: [audit, tracker, usability]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Reconcile the four unfiled 2026-09-04 usability items (12, 16, 17, 20) against the tracker and hand drafts to the maintainer

## Problem

The 2026-09-04 audit's filing step (str-qwua7 epic) filed only the P1 and selected P2 items. Its usability/UI section had items 9, 12, 16, 17, 18 and 20 with no tracker issue. This audit covers two of them directly: item 9 (tracker IDs in help) is help-tracker-ids-lint, and item 18 (`--format text` not stripped) is explore-format-flag-ignored. Four remain.

This issue is bounded to those four items. It does not file issues itself: under the audit filing rule (maintainer decision D6 of 2026-09-23), the output is a reconciliation table plus issue drafts that the maintainer files.

## Items

As described in `audits/2026-09-22/areas/cli-ux.md` F17 (lines ~235-247):

| 09-04 item | Description available now |
|---|---|
| 12 | Surface the termination reason (why exploration of a function stopped) in the report |
| 16 | scan prints a double report and absolute paths |
| 17 | `Wrote ... artifact -> <abs path>` is printed at info level |
| 20 | Not described in any file in the `audit-2026-09-22` tree |

## Source location (known gap)

The 2026-09-04 report is referenced by the str-qwua7 epic as "audits/2026-09-04.md on branch audit-2026-09-04, with per-area reviewer reports ... under audits/2026-09-04/". On 2026-09-23 no local branch, tag or `origin` ref named `audit-2026-09-04` exists (`git for-each-ref`, `git ls-remote origin 'audit-2026-09-04*'`), and `git log --all -- 'audits/2026-09-04/usability-ui.md'` finds nothing. Item 20's text may therefore be unrecoverable.

## Acceptance criteria

- [ ] The source is searched in this order, and the close comment records each result: `git log --all` for `audits/2026-09-04*` in shatter; `git fsck --lost-found` / unreachable commits containing `usability-ui.md`; other worktrees under `~/.local/share/worktrees/shatter/`; asking the maintainer. If found, cite the exact commit and path of `usability-ui.md`.
- [ ] For each of items 12, 16, 17 and 20, the close comment records exactly one outcome: **covered by <existing id>** (with the `bd show` title and why it covers the item), **covered by a 2026-09-22 draft slug**, **new draft** (path of a draft file in the audit drafts area, written in the repo's issue-draft format, for the maintainer to file), or **unrecoverable** (item 20 only, if the source is not found).
- [ ] Each item is checked against at least str-9ee5, the open children of str-qwua7, and this audit's drafts in the shatter-cli-flags-and-help and shatter-cli-runtime-output buckets, using targeted `bd search` terms listed in the close comment.
- [ ] No tracker issue is created by the implementing agent; drafts are handed to the maintainer.

## Out of scope

- Items 9 and 18 (help-tracker-ids-lint, explore-format-flag-ignored).
- Changing the audit filing process (str-qwua7.22).

## Dependencies

- Blocked by: none.
- Related: str-qwua7, str-qwua7.22, str-9ee5, help-tracker-ids-lint, explore-format-flag-ignored.

## Source

Audit 2026-09-22 finding cli-ux-17 (areas/cli-ux.md F17). Split out of help-tracker-ids-lint.
