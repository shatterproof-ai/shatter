# Cross-check review

- **Reviewer:** codex (independent runtime)
- **Artifact type:** issue
- **Mode:** cross
- **Artifact SHA-256:** de3726cce90e90914a90dc593bf1de058ca17d5135d9afc78bf31b2dfe77a4c2
- **Scope:** 2026-09-22 Shatter audit issue drafts, bucket shatter-cli-flags-and-help (shatter). Repos to verify claims against (read by absolute path): shatter at /home/ketan/.local/share/worktrees/shatter/audit-2026-09-22 (audit worktree == main 16794cef + audit files), bento /home/ketan/project/bento, bugshot /home/ketan/project/bugshot, storystore /home/ketan/project/storystore, shatter-agents /home/ketan/project/shatter-agents, dotfiles /home/ketan/dotfiles. Tracker snapshots: <repo>/.beads/issues.jsonl (bd may be unusable read-only).


## Findings

Not ready to file as-is. I checked source at `56c86168` and the saved transcripts; live tracker status remains unverified.

- **MAJOR — #02’s suggested duplication assertion already passes.** `explore-o2.out` contains duplicated function reports but only one `**Summary:**` line. Assert that each function report or distinctive result row appears exactly once; counting summaries will miss the reported bug.

- **MAJOR — #02 identifies the two output paths incorrectly.** `explore.rs:4028` belongs to `finalize_explore`, reached through `--from-artifacts`; `:6687` belongs to normal exploration. Sequential/parallel testing does not establish coverage of both—require explicit artifact-finalization tests.

- **MAJOR — #03 removes functioning timing options without acknowledging the behavior change.** `main.rs:1456–1505` persists timing artifacts through common command finalization, including non-executing commands. Treating every timing option as execution-only needs an explicit compatibility decision, rather than assuming the host-write predicate defines its applicability.

- **MAJOR — #03’s fallback cannot meet its acceptance criteria.** Hiding arguments with `mut_subcommand` neither removes global definitions nor rejects `spec-diff --allow-host-writes`. Remove that fallback or define a separate, narrower issue; explaining unmet criteria in a close comment is not completion.

- **MAJOR — #05 allows closure with seed-sensitive resume behavior still broken.** It requires the seed in artifact/resume identity, then permits merely recording the gap elsewhere if the options-hash issue has not landed. Require implementation here or declare the prerequisite and prevent premature closure.

- **MAJOR — #08 combines code cleanup with an unbounded tracker-reconciliation task.** Filing items “12, 16, 17, 20, and any other items” introduces unrelated deliverables, while item 20 is not described and the source report lacks a pinned location. Split reconciliation into a bounded inventory with exact source references and deduplication outcomes, and reconcile its filing instructions with D6’s maintainer-only filing rule.

- **MAJOR — #09 misstates what telemetry loses.** `KNOWN_SUBCOMMANDS` controls `sanitized_args`, while `command_run.subcommand` is populated independently in `main.rs:1543–1555`. The command identity remains available; describe the argv-redaction defect and test that specific field rather than claiming the command’s telemetry is useless.

- **MAJOR — #10 is several independent issues disguised as polish.** Sandbox admission, analysis rendering, diagnostics, spec-output behavior, HTML coverage, demo colors, I/O exit codes, and tracker closure have separate implementations and completion conditions. Split before filing; “split if it grows” leaves scope determination to the implementing agent.

- **MAJOR — #10’s exit-code closure is unsupported by its checks.** The available record of `str-qwua7.12` requires mixed success/failure to exit 1, but `decide_exit_status_ok_partial_success_with_some_failed_targets` at `explore.rs:7075` explicitly enforces success. Because that tracker export is stale, reconcile the live criteria and this behavior before requiring closure; selected single-target exit-2 examples cannot establish completion.

- **MAJOR — #10’s HTML requirement hides a coverage-data dependency.** `render_source_block` receives a source span and covered-line set, insufficient to distinguish uncovered executable lines from non-executable lines. Identify the authoritative executable-line data and required upstream changes; signatures and brace lines can also contain executable code.

- **MAJOR — #01’s retirement of `--render` extends beyond explore.** The option is global (`args.rs:190–197`), and `main.rs` passes it to both explore and scan. Define scan’s migration and compatibility behavior, including precedence if a deprecated alias remains, before making retirement an explore acceptance criterion.

- **MINOR — #05’s reproducibility test leaves important state uncontrolled.** Fresh artifact directories do not isolate caches, seed pools, scheduling, or time budgets. The cited scan test disables caches and seed pools and constrains parallelism; specify equivalent controls alongside the proposed propagation assertions.

The top fixes are to split #08/#10 into bounded issues, repair the contradictory acceptance criteria and ineffective regression checks, and reconcile tracker closure claims against live records before filing.
