---
slug: markdown-drops-render-plain-info
kind: new
title: "Default markdown explore report drops information that `--render plain` shows (branches, discovery method, termination reason)"
priority: P3
type: feature
labels: [report, ux, explore, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Default markdown explore report drops information that `--render plain` shows (branches, discovery method, termination reason)

## Problem

`explore --render plain` is marked "(deprecated)" in every help page, yet it shows more than the default markdown report:

- `Branches: 3/3 (100%)`
- `[random: 3 (100%)]`, the discovery-method breakdown
- `Symbolic: 3/3 constraints (100%)`

The default markdown report shows only the path count and line coverage. Neither mode says why exploration stopped (worklist exhausted, iteration budget or timeout), although `stop_reason` is in the artifact JSON. The walkthrough-review rubric (`.claude/skills/walkthrough-review/SKILL.md`, "7. Exploration completeness" and criterion "J. Completeness signal") asks the report to say whether exploration was complete, which the missing termination reason and branch figure leave unanswered. More generally, the default renderer shows less than the deprecated one, so `--render plain` cannot be removed without losing information.

## Evidence

Re-verified against the audit worktree (HEAD 56c86168):

- `shatter-cli/src/args.rs:41` documents `plain` as "Legacy plain ANSI text output (deprecated)".
- The plain renderer's lines come from `shatter-core/src/coverage_metrics.rs:269-277` (`Branches: {covered}/{total} ...`) and `:328` (`Symbolic: {}/{} constraints ...`).
- The default markdown path is `shatter-cli/src/render.rs` (`shatter-cli/src/commands/explore.rs:3620-3638` dispatches `OutputFormat::Md` to `render::explore_fn_view`/`render_explore_fn`). It emits neither the branch nor the discovery lines, and never references `stop_reason`.
- The plain path is `shatter-core/src/explorer.rs:2950` `format_exploration_report` (explore.rs:3639-3650), which calls `coverage_metrics::format_coverage_metrics`. Besides the three lines above it conditionally prints: the MC/DC block (`coverage_metrics.rs`), a stubbed-import warning (`stubbed_modules`), float-probe results, abandoned frontiers, opaque-type suggestions with a config hint, perf lines (`--perf`), and GA stats. Some of these may already have markdown equivalents; nobody has inventoried them.
- Transcripts in `audits/2026-09-22/cli-ux-transcripts/`: `render-plain.out` has the `Branches:`, `[random: ...]` and `Symbolic:` lines, and `render-md-color.out` has 0 `Branches` lines.
- Source findings: audit 2026-09-22 cli-ux-14 (confirmed; the verifier lowered it to P3 because this is report richness, not wrong output). Area evidence: `audits/2026-09-22/areas/cli-ux.md` F14. The termination reason was also 2026-09-04 usability-ui item 12, which was never filed.

## Acceptance criteria

- [ ] For each explored function, the default markdown explore report includes:
  - branch coverage as a named secondary line (`Branch coverage: x/y (z%)`, consistent with coverage-headline-metric-unification if that lands first)
  - the discovery-method breakdown (for example `found by: random 3, Z3 0`)
  - a `Stopped: <reason>` line mapped from `stop_reason` to plain words (worklist exhausted / iteration budget reached / time limit), and `Stopped: unknown` only when `stop_reason` is absent
- [ ] Golden test on `ts/01-arithmetic.ts:classifyNumber` asserts those three lines. It fails on current main (no `Branch` or `Stopped` line in markdown).
- [ ] The mapping is an exhaustive `match` on `StopReason` (`shatter-core/src/explorer.rs:529`, set by `classify_stop_reason` at :1999), so a new variant cannot render raw, and a unit test covers each variant's wording.
- [ ] **Parity inventory, not removal.** The close reason contains a table of every fact the plain renderer can print (at least: paths/lines summary, branches, discovery breakdown, symbolic constraints, MC/DC, stubbed-import warning, float probes, abandoned frontiers, opaque suggestions, perf, GA stats) with, for each, either "in markdown" plus the test that covers it, or "not in markdown" plus a reason. This issue does **not** remove `--render plain` and does not claim markdown is complete beyond the three required lines; removal is decided in explore-format-flag-ignored using this inventory.
- [ ] `task walkthrough` passes and `/walkthrough-review` has been run. `task affected` passes, with `Gates selected` recorded.

## Suggested approach

Reuse the `coverage_metrics.rs` computations and add markdown variants next to the ANSI ones rather than recomputing. Land this before explore-format-flag-ignored (the `--render`/`--format` unification, bucket shatter-cli-flags-and-help) removes or hides `--render plain`, so that decision can use the inventory.

## Out of scope

- A per-path input-constraint column. It needs SymExpr pretty-printing; file it as a follow-up if wanted.
- Turning the entire walkthrough-review rubric into a gate.
- Removing `--render plain` (explore-format-flag-ignored).

## Related

str-zt4v, str-qwua7.15 (covered only where `--render` appears in help), explore-format-flag-ignored (cli-ux-02), str-qwua7.10 (walkthrough gate checks exit codes only).

## Priority / Type

P3, feature.
