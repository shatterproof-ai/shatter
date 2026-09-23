# Bundle: shatter-cli-flags-and-help (Audit 2026-09-22, final drafts)

- Bucket: `shatter-cli-flags-and-help`: CLI flag semantics and help text (--format, duplicate output, execution-only flags, --seed, config typos, tracker IDs in help, telemetry list, polish).
- Repo: shatter. Tracker: bd in /home/ketan/project/shatter (prefix str). Parent epic: "Epic: Audit 2026-09-22 findings".
- Entries: 8 new issues and 2 reopen-notes (comments on closed str-qwua7.15 and str-0m0vn). Nothing has been filed.
- Evidence re-verified 2026-09-23 against the `audit-2026-09-22` worktree (HEAD `56c86168`) and its `target/debug/shatter` where cheap (help output, source line numbers). Output-behavior claims cite transcripts in `audits/2026-09-22/cli-ux-transcripts/` and verifier reproductions in `findings.json`.
- Dependency edges inside this bucket: none (blocked_by is empty everywhere). The related and ordering notes are in each body.

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
| 03 | help-hides-execution-flags | new | P2 | Scope execution-only flags to executing commands: `help <cmd>` and 10 non-executing commands still show --allow-host-writes/--set (replace the str-qwua7.15 argv intercept) |
| 04 | help-flags-reopen-note | reopen-note (str-qwua7.15) | P2 | Comment on closed str-qwua7.15: execution-only flags still shown by `help <cmd>` and 10 non-executing commands |
| 05 | seed-for-explore-and-run | new | P2 | Add --seed to explore and run (str-0m0vn wired it into scan only) |
| 06 | seed-reopen-note | reopen-note (str-0m0vn) | P2 | Comment on closed str-0m0vn: symptom named explore, but --seed exists only on scan |
| 07 | unknown-config-keys-warn | new | P2 | Warn on unknown config keys and --set key typos (currently silently ignored) |
| 08 | help-tracker-ids-lint | new | P2 | Remove tracker IDs and internal status notes from --help, SPEC and README; add a lint; file the unfiled 2026-09-04 UI items |
| 09 | telemetry-known-subcommands | new | P3 | Telemetry KNOWN_SUBCOMMANDS is stale: 3 nonexistent commands listed, real ones redacted |
| 10 | cli-minor-output-and-help-polish | new | P3 | CLI polish: --analyze-only refused without a sandbox and shows no types; FunctionNotFound breakdown all zeros; false --dry-run/target help; --spec dropped with --spec-out; minor report defects |

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

str-zt4v (closed) decided that `--format` controls what goes to stdout. For `explore`, the streaming printer branches on `output_format` instead. That field is the `--render md|plain` enum (`shatter-cli/src/args.rs:37-43`), so `explore --format html` and `explore --format text` both print markdown. `--format` is honored only by the post-run replay that runs when `-o` and `--stdout` are both given. Explore has four format controls that interact silently: `--render {md,plain}`, `--format {markdown,html,text}`, `--color`, and inference from the `-o` file extension.

A second problem: `strip_markdown_text`, the text renderer, is a character filter. It deletes every `*` and backtick and splits every line containing `|`, including inside data values. Text mode would therefore corrupt Go pointer types (`*T`) and values such as `a | b`.

## Evidence

Re-verified against `audit-2026-09-22` (HEAD `56c86168`).

- Streaming printer branches on `--render`: `shatter-cli/src/commands/explore.rs:3620`, `:3854`, `:3920`, `:4783` and `:6533` (`if output_format == crate::args::OutputFormat::Md`).
- `--format` is used only by the replay: `explore.rs:4028-4029` and `:6687-6688` (`if !report_outputs.is_empty() && stdout`).
- `shatter-core/src/report.rs:1918-1946` `strip_markdown_text`: `.replace('*', "")`, `.replace('`', "")`, and `line.split('|')` on any line that contains `|`.
- Transcripts in `audits/2026-09-22/cli-ux-transcripts/`: `format-html.out` and `format-text.out` both begin `# Shatter Explore` and contain `**4 path(s)**` and markdown tables. `format-text-with-o.out` (`--format text -o r.md --stdout`) is also markdown.
- The only test of `explore --format` is a clap rejection of `json` (`shatter-cli/tests/json_stdout_contract.rs:250`).
- `explore --help` lists both `--render <MODE>` and `--format <FORMAT>`.
- `--render plain` prints lines that markdown omits (`Branches: 3/3`, `[random: 3 (100%)]`, `Symbolic: 3/3 constraints`). That gap is tracked separately as markdown-drops-render-plain-info (bucket shatter-cli-runtime-output).
- Verifier (findings.json cli-ux-02) reproduced `--format html|text` printing markdown and lowered the priority from P1 to P2: the flag is accepted and ignored, but no data is lost. The verifier did not check the `strip_markdown_text` sub-claim. It was confirmed separately under artifacts-16 by reading the code.

## Acceptance criteria

- [ ] `shatter explore <file> --format text` prints plain text with no markdown syntax (`#`, `**`, table pipes) to stdout. `--format html` prints an HTML document to stdout. `--format markdown` is unchanged.
- [ ] The same holds with `-o FILE --stdout`: the stdout format follows `--format`, not the file extension.
- [ ] Golden or snapshot tests in `shatter-cli/tests/` cover `explore --format {markdown,text,html}`. Each test is shown failing on the current code and passing after the fix (record both runs in the close comment).
- [ ] Text rendering preserves literal `*`, backtick and `|` inside data values. A test uses an outcome or type containing `*T` and `a | b` and asserts both survive in `--format text`.
- [ ] `--render` is removed, or kept as a deprecated alias that prints a warning on stderr. Before removal, the extra `--render plain` lines are either ported into the default report or explicitly handed to markdown-drops-render-plain-info (link it in the close comment).
- [ ] `task affected` passes. The close comment records its `Gates selected` line.

## Suggested approach

Use one stdout-format selector (`--format`, with `-o` extension inference only for files) that drives both the streaming printer and the replay path. Render text from the report view model rather than stripping markdown after the fact. Replace or delete `strip_markdown_text`, and check its other callers (scan uses it too) before changing its behavior. This touches the same emitter code as explore-report-printed-twice, so doing both in one branch, or one straight after the other, avoids conflicts.

## Out of scope

- Duplicate printing with `-o --stdout` and the header leak with `-q -o` (explore-report-printed-twice).
- Adding information that `--render plain` shows to the default markdown (markdown-drops-render-plain-info), except as needed to retire `--render`.
- Format-flag vocabulary across commands and help grouping (str-9ee5).
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

1. **Double print.** `explore <file> -o x.json --stdout` streams the full report to stdout while exploring, then replays it after the run. stdout therefore holds the report twice, plus extra `## <fn> / **Status:** completed / exploration completed` blocks between the two copies.
2. **Header leak.** `explore <file> -o report.html -o bundle.json -q` with no `--stdout` leaves `# Shatter Explore` and a blank line (19 bytes) on stdout. `-o x.html` without `-q` does the same. stdout should be empty.

Either one breaks piping (`shatter explore ... -o r.json --stdout | tool`) and any script that checks that stdout is empty.

## Evidence

Re-verified against `audit-2026-09-22` (HEAD `56c86168`).

- The streaming path prints if `should_print_report = opts.report_outputs_empty || opts.stdout` (`shatter-cli/src/commands/explore.rs:3610`), and the `# Shatter Explore` header is emitted before that gate is consulted (`:3607-3661`).
- The replay path prints again: `explore.rs:4028-4029` (`// Replay to stdout if report files were also written.` / `if !report_outputs.is_empty() && stdout`) and the parallel path at `:6687-6688`.
- Transcripts in `audits/2026-09-22/cli-ux-transcripts/`:
  - `explore-o2.out` (`explore 01-arithmetic.ts -o x.json --stdout`): the full streamed report ending `**Summary:** 6 path(s) across 2 function(s)`, then `## classifyNumber` / `**Status:** \`completed\`` / `exploration completed`, then the tables a second time.
  - `explore-o.out` and `explore-o3.out` (`-o` without `--stdout`): exactly `# Shatter Explore\n\n` (19 bytes).
- Verifier (findings.json cli-ux-03) reproduced both and lowered the priority from P1 to P2: the output is duplicated or cosmetic, not incorrect.
- Existing issues cover neighbouring behavior only: str-zt4v (output matrix, closed), str-6c6p (`--quiet` hid reports, closed) and str-xve (stdout/stderr mixing, closed).

## Acceptance criteria

- [ ] CLI output tests in `shatter-cli/tests/` cover every combination of `-o FILE` (absent or present), `--stdout` (absent or present) and `-q` (absent or present) for `explore` on a small TS fixture, eight cases in all. They assert:
  - no `-o`: stdout holds the report exactly once;
  - `-o` without `--stdout`: stdout is empty (0 bytes), with or without `-q`;
  - `-o` with `--stdout`: stdout holds the report exactly once (for example, the `**Summary:**` line appears once), and the file is written;
  - `-q` never suppresses the report when stdout is the sink (keep str-6c6p's behavior).
- [ ] At least the double-print and header-leak cases are shown failing on the current code and passing after the fix. Record both runs in the close comment.
- [ ] The parallel path (`explore.rs:~6687`) and the sequential path (`~4028`) behave the same. The tests exercise both, for example with a multi-function file that triggers the parallel path, or by whatever switch selects each path.
- [ ] `task affected` passes. The close comment records its `Gates selected` line.

## Suggested approach

Pick one stdout emitter per run: either stream when stdout is the sink and skip the replay, or buffer and emit once. Gate header emission on the same `should_print_report` condition as the body. Delete the replay branch, or make it the only branch. This is the same code explore-format-flag-ignored changes, so land the two together or one after the other.

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
title: "Scope execution-only flags to executing commands: `help <cmd>` and 10 non-executing commands still show --allow-host-writes/--set (replace the str-qwua7.15 argv intercept)"
priority: P2
type: bug
labels: [cli, usability, help, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Scope execution-only flags to executing commands: `help <cmd>` and 10 non-executing commands still show --allow-host-writes/--set (replace the str-qwua7.15 argv intercept)

## Problem

str-qwua7.15 ("Hide execution-only global flags on non-executing commands") was closed on 2026-09-14 with the bare reason "Closed" and only a partial fix. The execution-only flags (`--allow-host-writes`, `--set`, `--timing*`) are still `global = true` on `Cli`. The fix hides them only when raw argv looks like `<cmd> --help` for one of five hard-coded commands. It does this by guessing the subcommand from argv and rendering help from a second, mutated clap `Command`. The commit notes that the natural fix (`mut_arg(... hide)`) broke clap parsing, so the workaround was chosen instead.

Three gaps follow:

1. `shatter help <cmd>` goes through clap's own `help` subcommand and bypasses the intercept, so `help spec-diff` and `help doctor` still show the flags.
2. SPEC §2.10 names more non-executing commands than the five in the list. `--help` still shows `--allow-host-writes` for analyze, solve, specify, stale, diff, compare, list-targets, nondeterminism, workspace and build-frontend.
3. The flags still *parse* on non-executing commands: `spec-diff --allow-host-writes a b` is accepted.

The codebase already has a single source of truth, `command_executes_targets()`, but the help logic does not use it.

## Evidence

Re-verified 2026-09-23 with `target/debug/shatter` built in the `audit-2026-09-22` worktree (HEAD `56c86168`):

- `shatter help spec-diff | wc -l` → 78; line 50 is `--allow-host-writes` and line 55 is `--set <KEY=VALUE>`. `shatter spec-diff --help | wc -l` → 48, with neither flag.
- `shatter doctor --help | grep -c allow-host-writes` → 0, but `shatter help doctor | grep -c allow-host-writes` → 1.
- `<cmd> --help | grep -c allow-host-writes` → 1 for each of analyze, solve, specify, stale, diff, compare, list-targets, nondeterminism, workspace and build-frontend. `help <cmd>` gives the same result.
- Code:
  - `shatter-cli/src/main.rs:70` `maybe_print_non_executing_help(raw_args)`, called at `main.rs:95` before clap parses.
  - `shatter-cli/src/args.rs:324` `NON_EXECUTING_COMMAND_PATHS` (spec-diff, init, doctor, cache, telemetry); `args.rs:344` `help_only_command()`; `args.rs:402` `resolve_subcommand_path()`.
  - `shatter-cli/src/host_writes.rs:84-95` `command_executes_targets()`: true only for Explore, Scan, Run, Observe, Bench, Properties and Revalidate.
- SPEC §2.10 (`SPEC.md:591-596`) lists analyze, solve, specify, stale, diff, spec-diff, compare, list-targets and doctor as never executing targets.
- Transcripts: `audits/2026-09-22/cli-ux-transcripts/help/help-subcommand-spec-diff.txt` compared with `help/spec-diff.txt`.
- Findings cli-ux-05 and prior-07 (both verified at P2) and docs-22 (P3, covered by this issue).

## Acceptance criteria

- [ ] The execution-only options (`--allow-host-writes`, `--set`, `--timing`, `--timing-*`) are defined only on commands for which `command_executes_targets()` is true, for example through a flattened `ExecOptions` struct. They are no longer `global = true` on `Cli`.
- [ ] `maybe_print_non_executing_help`, `help_only_command`, `resolve_subcommand_path` and `NON_EXECUTING_COMMAND_PATHS` are deleted.
- [ ] A test walks every subcommand of `Cli::command()`, including nested ones, and for each asserts that both `shatter <cmd> --help` and `shatter help <cmd>` contain `--allow-host-writes` and `--set` if and only if the command executes targets. The same predicate drives the test and the code, or the test cross-checks `command_executes_targets()` against a table that lists every subcommand. The test fails on current HEAD (at least for `help spec-diff` and `analyze --help`) and passes after the fix; record both runs in the close comment.
- [ ] `shatter spec-diff --allow-host-writes a.json b.json` is rejected as a usage error (exit 2), and a test covers it.
- [ ] The `command_executes_targets()` list is reviewed against SPEC §2.10 and the actual behavior of every command not in either list (for example `test`, `discover-deps`, `build-frontend`, `workspace`). SPEC §2.10 is updated so the two agree.
- [ ] `SHATTER_ALLOW_HOST_WRITES` and config-file behavior are unchanged for executing commands; the existing host-write tests still pass.
- [ ] `task affected` passes, plus `task gauntlet`, since this changes flag parsing on many commands. The close comment records the gates selected.

## Suggested approach

Move the execution-only globals into a `#[command(flatten)] exec: ExecOptions` field on each executing subcommand. That is the direction of str-qwua7.20.1 (CLI argument-surface epic). Update `host_writes.rs` and the `--set` and timing plumbing to read from the subcommand, not from `Cli`. The code comment in `main.rs` about why `mut_arg(hide)` failed is no longer relevant once the flags are not global. If flattening is not practical, the fallback is to hide the args with `mut_subcommand` driven by `command_executes_targets()`. That fallback must also cover the `help` subcommand path, and it still does not fix gap 3, so say so in the close comment if it is chosen.

Note that the snapshot `shatter diff` command is being retired (retire-snapshot-diff, maintainer decision D2). If that lands first, drop `diff` from the checks. Otherwise the table covers it like any other non-executing command.

## Out of scope

- Grouping help output with `help_heading` (str-9ee5).
- Tracker IDs in help text (help-tracker-ids-lint).
- Adding `--seed` to more commands (seed-for-explore-and-run). If `ExecOptions` lands first, that issue can reuse it.

## Dependencies

- Blocked by: none.
- Related: str-qwua7.15 (closed, partial; see help-flags-reopen-note), str-qwua7.20.1 (shared options struct), str-9ee5, retire-snapshot-diff.

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
> - Verified 2026-09-23 with a `target/debug/shatter` built at HEAD `56c86168`.
>
> The structural fix is tracked in the new issue **help-hides-execution-flags** (<new id>): scope the execution-only options to executing commands (flattened `ExecOptions`, driven by `command_executes_targets()`), delete the argv intercept, and add a test over both `<cmd> --help` and `help <cmd>` for every subcommand. Tracker-ID removal from help text, which this issue left out of scope, is filed as **help-tracker-ids-lint** (<new id>).

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
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Add --seed to explore and run (str-0m0vn wired it into scan only)

## Problem

str-0m0vn's symptom was "There is no way to make `shatter scan` or `shatter explore` reproducible". It was closed after adding `--seed` to `scan` only, and its scan-only follow-ups (str-pbqyr, str-9m9o3) do not cover explore. `explore` and `run` still have no seed flag, so an explore or run result cannot be reproduced. That undermines bug reports, CI flake triage, and the D3 benchmark (concolic-vs-default-benchmark), which needs fixed seeds.

CLAUDE.md's parity rule ("When adding a new ... CLI flag ... grep for the parallel code path") was not applied to the explore/scan/run flag trio.

## Evidence

Re-verified 2026-09-23 with `target/debug/shatter` built at HEAD `56c86168` of `audit-2026-09-22`:

- `shatter scan --help | grep -cE -- '--seed( |$|<)'` → 2. The same check on `explore --help` and `run --help` → 0. Their only seed-related flags are `--seeds-dir` and `--no-seeds`, which control the cross-function seed pool, not RNG seeding.
- `shatter-cli/src/args.rs:977-979`: `--seed` (`pub(crate) seed: Option<u64>`) is defined only in the Scan args. Its doc comment cites str-0m0vn.
- Scan-only seed follow-ups already open: str-pbqyr, str-9m9o3 (scan cache ignores the seed) and str-v1tzz (report the seed and test its propagation).
- Finding cli-ux-08 (areas/cli-ux.md F8), verified at P2.

## Acceptance criteria

- [ ] `shatter explore <target> --seed N` and `shatter run --seed N` are accepted. The seed reaches the random explorer and the concolic orchestrator, both engine paths, in the same way scan's does. A test asserts propagation for each path (for example through the effective config or an injected RNG).
- [ ] A reproducibility test, modelled on the existing scan seed reproducibility test, runs `explore` twice on the same fixture with the same seed and fresh artifact directories, and asserts identical path sets and generated inputs. A run with a different seed is allowed to differ. The test is shown failing (flag rejected) before the change and passing after.
- [ ] The explore resume and artifact key includes the seed, so a resumed run with a different seed is not served stale results. Coordinate with explore-resume-options-key, which introduces the options-hash key. If that issue lands first, add the seed to its hash; if not, record the gap on it.
- [ ] Help text for `--seed` is the same on scan, explore and run, and carries no tracker IDs (see help-tracker-ids-lint).
- [ ] E2E suites pass (`cargo test --test e2e_concolic`, `e2e_concolic_go`, `e2e_concolic_rust`) because explorer and orchestrator wiring changes. `task affected` passes, and the close comment records the gates selected.

## Suggested approach

Define `--seed` once in a shared options struct flattened into scan, explore and run. That is the str-qwua7.20.1 direction; if help-hides-execution-flags adds an `ExecOptions` struct first, put it there. Thread it through the same config field scan uses so both engine paths pick it up.

## Out of scope

- The scan-specific seed bugs (str-pbqyr, str-9m9o3).
- Printing the seed in reports (str-v1tzz). Extend that issue to explore and run once this lands, rather than duplicating it here.

## Dependencies

- Blocked by: none.
- Related: str-0m0vn (closed; see seed-reopen-note), str-v1tzz, str-pbqyr, str-9m9o3, str-qwua7.20.1, explore-resume-options-key, help-hides-execution-flags, concolic-vs-default-benchmark (needs fixed seeds).

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

## Evidence

Re-verified against `audit-2026-09-22` (HEAD `56c86168`):

- `shatter-core/src/config.rs:4178-4185`, test `parse_set_overrides_unknown_field_is_ignored_by_serde`, with the comment "ShatterConfig derives Deserialize without it [deny_unknown_fields], so unknown keys are silently ignored." The test asserts only "no panic". It pins the lenient behavior.
- `shatter-core/src/config.rs:1104` `parse_set_overrides`; CLI wiring at `shatter-cli/src/helpers.rs:1574-1590` (`set_overrides` → `parse_set_overrides` → merged as the highest-priority layer).
- Go frontend contrast: `shatter-go/config/loader.go:431-444` emits `config <path>: ignoring unknown top-level key "<k>"`.
- Observed (findings.json cli-ux-09, reproduced by the verifier): `explore ... --set defaults.max_iteratons=5` exits 0 with nothing about the key on stderr (`audits/2026-09-22/cli-ux-transcripts/err-setunknown.err`). `--set foo.bar=1` exits 0. `--set defaults.max_iterations=abc` exits 2 (`err-badset.err`).
- `strsim` is already in `Cargo.lock`.

## Acceptance criteria

- [ ] An unknown key in `.shatter/config.yaml`, in `shatter.config.json`, or in a `--set KEY=VALUE` override produces one warning on stderr. The warning names the source (file path or `--set`), the full dotted key path and, when a known key is close, a "did you mean `defaults.max_iterations`?" suggestion.
- [ ] A strict mode turns those warnings into a usage error (exit 2). Pick one form (`--strict-config` flag, config key, or env var), document it in the config reference, and test it.
- [ ] `parse_set_overrides_unknown_field_is_ignored_by_serde` is replaced by tests that assert (a) the warning and suggestion for a typo in `--set`, (b) the same for a typo in a YAML config file, (c) exit 2 in strict mode, and (d) no warning for a valid config (use the repo's own example configs and `demo/` configs as a no-false-positive check). The typo tests are shown failing before the change.
- [ ] Keys that are legitimately open-ended (maps keyed by user data, if any) do not warn. List them in the close comment.
- [ ] Warnings go to stderr only, so JSON stdout contracts (`shatter-cli/tests/json_stdout_contract.rs`) still pass.
- [ ] `task affected` passes, and the close comment records the gates selected.

## Suggested approach

Wrap the deserializer at each config load site, and in `parse_set_overrides`, with `serde_ignored`, collecting the ignored paths. Match them against the known key set with `strsim` to produce suggestions. Avoid `deny_unknown_fields` as the default because it rules out warn-only mode and forward compatibility.

## Out of scope

- A generated config reference document (str-qwua7.21.1).
- Validating free-string format values for stale/revalidate (str-9ee5).
- Changes to the Go frontend loader, which already warns.

## Dependencies

- Blocked by: none.
- Related: str-qwua7.21.1, str-9ee5.

## Source

Audit 2026-09-22 finding cli-ux-09 (areas/cli-ux.md F9); old draft `drafts/shatter-code/31-unknown-config-keys-warn.md`.

---

<!-- file: 08-help-tracker-ids-lint.md -->

---
slug: help-tracker-ids-lint
kind: new
title: "Remove tracker IDs and internal status notes from --help, SPEC and README; add a lint; file the unfiled 2026-09-04 UI items"
priority: P2
type: task
labels: [cli, docs, usability, agents, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Remove tracker IDs and internal status notes from --help, SPEC and README; add a lint; file the unfiled 2026-09-04 UI items

## Problem

User-facing text cites internal tracker IDs and process status. `///` doc comments on clap args double as internal notes, so IDs such as `str-gg9v` and `str-frc.6` appear in `--help`. The 2026-09-04 audit flagged this, and a new ID (`str-0m0vn`, scan `--seed`) was added on 2026-09-05, the day after. SPEC and README carry the same pattern, including one paragraph of internal status ("Do not wait for that issue to land ...") inside the exit-code contract. Nothing lints for it.

Separately, the 2026-09-04 audit's filing step dropped several UI findings, which were never filed. str-qwua7.15 also explicitly left "tracker-ID removal from help text" out of scope, and that item was never filed either.

## Evidence

Re-verified 2026-09-23 with `target/debug/shatter` built at HEAD `56c86168` of `audit-2026-09-22`:

- Rendering `--help` for every top-level subcommand and running `grep -o 'str-[a-z0-9.]*[a-z0-9]' | sort | uniq -c` finds **13 distinct IDs**: str-gg9v (19 times), str-jeen.13 (2), str-v01r (2), str-0m0vn, str-1fwt, str-1wcl, str-d6hj, str-frc.3, str-frc.5, str-frc.6, str-izhn, str-o09e and str-p2rz. The verifier counted 11-13 depending on whether nested pages and dotted IDs are counted separately.
- Sources in `shatter-cli/src/args.rs` (`///` comments): lines 106, 165, 796, 801, 811, 818, 833, 977 (str-0m0vn), 1014, 1144, 1148, 1168, 1589, 1773, 1775, 2200, 2225, 2587, 2608. `args.rs` also has IDs in `//` comments (for example 283, 321, 3491), which are fine.
- `SPEC.md` has 10 lines containing `str-`. `SPEC.md:645-647` (§2.11): "str-qwua7.12 tracks bringing the remaining commands into line. Do not wait for that issue to land before relying on this table."
- `README.md:293`: `> **Breaking change (str-gg9v).** ...`
- `.claude/skills/rust-conventions/SKILL.md` has no rule on tracker IDs in user-facing strings.
- Unfiled 2026-09-04 usability-ui.md items: 9 (strip tracker IDs, which this issue covers), 12 (surface the termination reason), 16 (scan double report and absolute paths), 17 (`Wrote ... artifact -> <abs path>` at info level), 18 (`--format text` not stripped, now filed as explore-format-flag-ignored) and 20. `bd search` for "tracker id", "termination", "Scan Results", "absolute path", "artifact path" and "format text" returned nothing (areas/cli-ux.md F17; the verifier did not re-run these searches).
- str-qwua7.45 covers only README's "Executing Target Functions Safely" section.
- Findings cli-ux-17 (P2) and docs-18 (P3), merged here at P2 (report section 15.1).

## Acceptance criteria

- [ ] A unit test in `shatter-cli` walks `Cli::command()` recursively, renders every subcommand's long help (`render_long_help`), and fails on `/str-[a-z0-9]+/`. It fails on current HEAD and passes after the fix; record both in the close comment.
- [ ] Every tracker ID now in `///` on clap args is moved to a `//` code comment next to the arg, or dropped. No help text loses user-relevant meaning. For example, the `str-gg9v` sentences become a plain description of the host-write default.
- [ ] `SPEC.md` and `README.md` user-facing sections contain no tracker IDs outside the SPEC §8 changelog. Where timing matters, use dated notes ("since 2026-07"). A check (script or test run by an existing gate) greps these files, excluding the changelog, and fails on `str-` IDs.
- [ ] The internal-status paragraph in SPEC §2.11 (`SPEC.md:645-647`) is removed, or rewritten as a short "Known deviations" list that states each deviation, with no tracker references.
- [ ] `.claude/skills/rust-conventions/SKILL.md` gains a rule: no tracker IDs in `///` docs on clap args or in any user-facing string; put them in `//` comments.
- [ ] The unfiled 2026-09-04 UI items (12, 16, 17, 20, and any other items from that report's usability-ui section that have no issue) are each either filed as a child of the audit epic (deduplicated against str-9ee5 and this audit's CLI issues) or recorded as already covered, with the covering id. The close comment lists every item and its outcome.
- [ ] `task affected` passes, and the close comment records the gates selected.

## Suggested approach

Write the help-rendering test first to get the inventory, then rewrite each `///` line. The SPEC/README check can be a small script called from `check-static`, or a Rust test that reads the files. For the 09-04 items, read the usability/UI section of the 2026-09-04 audit report. It is not in the `audit-2026-09-22` tree, so find it from the str-qwua7 epic description or the `audit-2026-09-04` branch or tag. Search `bd` for each item before filing.

## Out of scope

- Help-heading grouping and flag vocabulary (str-9ee5).
- Hiding execution-only flags (help-hides-execution-flags).
- The README "Executing Target Functions Safely" wording already owned by str-qwua7.45. Coordinate so both do not rewrite the same lines.
- Closing str-qwua7.12 (cli-minor-output-and-help-polish).

## Dependencies

- Blocked by: none.
- Related: str-qwua7.15, str-qwua7.45, str-9ee5, str-qwua7.22 (audit filing process), help-hides-execution-flags, explore-format-flag-ignored.

## Source

Audit 2026-09-22 findings cli-ux-17 (areas/cli-ux.md F17) and docs-18 (areas/docs.md). Merges the old drafts `drafts/shatter-agent/28-help-tracker-ids-lint-and-unfiled-ui-items.md` (primary) and `drafts/shatter-docs-ui/08-tracker-ids-in-help-and-docs.md`.

---

<!-- file: 09-telemetry-known-subcommands.md -->

---
slug: telemetry-known-subcommands
kind: new
title: "Telemetry KNOWN_SUBCOMMANDS is stale: 3 nonexistent commands listed, real ones redacted"
priority: P3
type: bug
labels: [telemetry, cli, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Telemetry KNOWN_SUBCOMMANDS is stale: 3 nonexistent commands listed, real ones redacted

## Problem

A hand-maintained allow-list in core decides which subcommand names are reported in `command_run` telemetry events. Names not on the list are redacted. The list has drifted from the clap definition: it names three commands that do not exist and omits most real ones. Telemetry is on by default, so the data collected for spec-diff, doctor, list-targets, observe and others is useless.

## Evidence

Re-verified against `audit-2026-09-22` (HEAD `56c86168`):

- `shatter-core/src/telemetry.rs:53-65` `KNOWN_SUBCOMMANDS` = explore, scan, **export**, **spec**, run, analyze, init, stale, telemetry, help, **version**. `export`, `spec` and `version` are not subcommands.
- Used at `telemetry.rs:263` (`let known_subcommands: HashSet<&str> = KNOWN_SUBCOMMANDS...`). Proptests at `:1120-1139` sample from the same list, so they cannot detect drift.
- Missing real subcommands include spec-diff, diff, doctor, list-targets, observe, bench, properties, revalidate, solve, specify, compare, nondeterminism, workspace, build-frontend, discover-deps, cache and test (compare with `shatter --help`).
- `shatter telemetry status` reports enabled by default.
- Finding cli-ux-18, verified at P3.

## Acceptance criteria

- [ ] Core no longer hardcodes the subcommand list. It is derived from `Cli::command().get_subcommands()` in shatter-cli and passed into the telemetry call, which keeps the cli → core dependency direction. Alternatively, if the list must stay in core, a shatter-cli test asserts it equals the clap subcommand set.
- [ ] A test fails if a clap subcommand is added without being reportable, for example by iterating the clap subcommands and asserting each is recorded unredacted. It is shown failing on current HEAD (for example for `spec-diff`).
- [ ] Unknown or free-form argv values are still redacted, and the existing redaction proptests still pass.
- [ ] When `diff` is removed by retire-snapshot-diff, the derived list follows with no manual edit.
- [ ] `task affected` passes, and the close comment records the gates selected.

## Suggested approach

Add a `known_subcommands: &[&str]` (or owned) parameter to the telemetry entry point, filled from clap in shatter-cli.

## Out of scope

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
