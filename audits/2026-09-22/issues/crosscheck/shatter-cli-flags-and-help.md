# Cross-check review

- **Reviewer:** claude (DEGRADED same-runtime fallback)
- **Artifact type:** issue
- **Mode:** degraded

> **DEGRADED REVIEW.** The counterpart runtime was unavailable, so this review came from an independent agent of the *same* runtime. It shares the original author's model and blind spots; weight it accordingly.

## Findings

# Degraded same-runtime review: shatter-cli-flags-and-help bundle

The Codex cross-check failed identity validation (exit 4), so Claude did this review itself, read-only. The claims were checked against the `audit-2026-09-22` worktree (HEAD 56c86168) and its `target/debug/shatter`.

## Verified claims (spot checks)
- 01: the explore.rs lines 3620/3854/3920/4783/6533 branch on `OutputFormat::Md`. The replay is at 4028-4029 and 6687-6688. `strip_markdown_text` is at report.rs:1918 and matches the draft's description. The `--render` enum is at args.rs:37-43.
- 02: `should_print_report` is at explore.rs:3610. `explore-o.out` and `explore-o3.out` are 19 bytes each. `explore-o2.out` shows the duplicated per-function blocks.
- 03: `help spec-diff` gives 78 lines and `spec-diff --help` gives 48. analyze, solve, list-targets and workspace `--help` each show `--allow-host-writes`. `spec-diff --allow-host-writes a.json b.json` parses and fails only at file read. `command_executes_targets` is at host_writes.rs:84-95. The intercept is at main.rs:70/95.
- 05: `--seed` counts are scan 2, explore 0, run 0. The flag is at args.rs:977-979.
- 07: test config.rs:4179 exists as described. `parse_set_overrides` is at config.rs:1104. The Go loader warning is at loader.go:444. `strsim` is in Cargo.lock.
- 08: the 13 distinct IDs match exactly (str-gg9v x19). SPEC.md has 10 `str-` lines. SPEC.md:645-647 and README.md:293 match.
- 09: the `KNOWN_SUBCOMMANDS` contents at telemetry.rs:53-65 match the draft.
- 10: args.rs:501 (no .rs), args.rs:676 ("Requires --output") and helpers.rs:337-347 (exit 1) match the draft.

## Findings

### MAJOR: telemetry-known-subcommands overstates the impact. The `command_run` subcommand field is not redacted.
`queue_command_run_event` (shatter-cli/src/main.rs, around line 1540) passes `subcommand: subcommand.to_string()` straight into `EventPayload::CommandRun`. Only the `sanitized_args` token list goes through `KNOWN_SUBCOMMANDS`. "The data collected for spec-diff, doctor, ... is useless" is therefore false. Only the argv echo, and `bad_cli_args` events (main.rs:102), lose the subcommand token. Restate the problem and reword the AC "asserting each is recorded unredacted" so it targets `sanitize_args`. Consider P3 → P4.

### MAJOR: help-hides-execution-flags treats `--set` and `--timing*` as execution-only, which the code does not support.
- `--set` is consumed only by explore. `cli.set_overrides` appears once in main.rs (line 437, the explore dispatch), and no other command module reads it: run.rs only has a test name. Moving `--set` into an `ExecOptions` flattened onto scan, run, observe, bench, properties and revalidate would advertise a flag those commands silently ignore.
- The timing collector is installed for every command (main.rs:119/176). The draft does not justify why `--timing` should disappear from analyze and solve.
The AC should require an audit of which executing commands actually honor `--set` (and fix or reject it where they do not), and should justify or drop the timing scoping.

### MAJOR: unknown-config-keys-warn has the same `--set` blind spot and a misleading wiring citation.
The cited "CLI wiring" at helpers.rs:1574-1590 is the LLM-only config resolver (`resolve_llm_config`). The main path is `resolve_function_config_with_inputs` (config.rs:~1598, called from explore.rs:4832). Because `--set` reaches only explore, a warn-on-typo fix must cover (or reject) `--set` on scan and run, or the typo stays silent there. Correct the citation and add scan/run `--set` behavior to the AC or the out-of-scope list.

### MAJOR: explore-report-printed-twice has an example assertion that does not discriminate.
The AC offers "the `**Summary:**` line appears once" as the exactly-once check. In `explore-o2.out`, `**Summary:**` already appears once: the replayed copy carries no Summary. A test built on that example passes on current code. Specify a discriminating predicate, such as each `## \`<fn>\`` heading or each table row appearing once, or stdout byte-equal to the `-o` markdown file.

### MINOR: explore-format-flag-ignored's evidence is slightly off.
"`--format` is used only by the replay" is imprecise. `StdoutFormat::Text` also drives the `-o FILE` writers (explore.rs:3992-3994, 6631), which call `strip_markdown_text`, so text-file output is already affected by the corruption bug. Also, `format-text.out` contains `**0 path(s)**` (a `markup` fixture), not `**4 path(s)**`. Only `format-html.out` and `format-text-with-o.out` show 4.

### MINOR: seed-for-explore-and-run's reproducibility AC covers explore only.
`run` gets the flag but no reproducibility test. Add a same-seed run test, or state why the explore test is enough.

### MINOR: cli-minor-output-and-help-polish is a 10-item grab bag.
It is acceptable as P3 polish, but it mixes a behavior change (item 1, the host-write gate) and a protocol-visible change (item 2) with cosmetic fixes. Item 1's reproduction is contested, and item 2 may need a protocol change under GOVERNANCE. Consider splitting items 1-2 out now rather than "if it grows".

### MINOR: help-tracker-ids-lint bundles filing work with code work.
The AC requires filing the unfiled 2026-09-04 items. That conflicts with D6 ("agents file nothing") unless it means producing drafts for the maintainer's filer. Clarify.

## Verdict
Mostly accurate, well-evidenced drafts: line numbers, counts and transcripts largely check out. Before filing, fix four things:
1. Correct telemetry-known-subcommands' impact claim.
2. Resolve the `--set`/`--timing` scoping in help-hides-execution-flags and unknown-config-keys-warn: `--set` reaches only explore today.
3. Give explore-report-printed-twice a discriminating exactly-once assertion.

The remaining drafts are ready to file as-is.
