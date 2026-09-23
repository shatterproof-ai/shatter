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

The default markdown report shows only the path count and line coverage. Neither mode says why exploration stopped (worklist exhausted, iteration budget or timeout), although `stop_reason` is in the artifact JSON. The walkthrough-review rubric (`.claude/skills/walkthrough-review/SKILL.md`, items 2, 4 and 7) treats branch coverage, discovery method and termination reason as essential for a human reader. `--render plain` therefore cannot be removed without losing information.

## Evidence

Re-verified against the audit worktree (HEAD 56c86168):

- `shatter-cli/src/args.rs:41` documents `plain` as "Legacy plain ANSI text output (deprecated)".
- The plain renderer's lines come from `shatter-core/src/coverage_metrics.rs:269-277` (`Branches: {covered}/{total} ...`) and `:328` (`Symbolic: {}/{} constraints ...`).
- The markdown path is `format_exploration_report` (`shatter-core/src/explorer.rs:2950`) and `shatter-cli/src/render.rs`. Neither emits the branch or discovery lines. `render.rs` never references `stop_reason`.
- Transcripts in `audits/2026-09-22/cli-ux-transcripts/`: `render-plain.out` has the `Branches:`, `[random: ...]` and `Symbolic:` lines, and `render-md-color.out` has 0 `Branches` lines.
- Source findings: audit 2026-09-22 cli-ux-14 (confirmed; the verifier lowered it to P3 because this is report richness, not wrong output). Area evidence: `audits/2026-09-22/areas/cli-ux.md` F14. The termination reason was also 2026-09-04 usability-ui item 12, which was never filed.

## Acceptance criteria

- [ ] For each explored function, the default markdown explore report includes:
  - branch coverage (`Branches x/y`, with the metric named)
  - the discovery-method breakdown (for example `found by: random 3, concolic 0`)
  - a `Stopped: <reason>` line mapped from `stop_reason` to plain words (worklist exhausted / iteration budget reached / timeout)
- [ ] Every fact the plain renderer shows is present in markdown, so `--render plain` can be removed without losing information. Removing it is optional here and can be part of the flag unification.
- [ ] A golden test on `ts/01-arithmetic.ts:classifyNumber` asserts the three lines above. It must fail on current main.
- [ ] `task walkthrough` passes and `/walkthrough-review` has been run. `task affected` passes, with `Gates selected` recorded.

## Suggested approach

Reuse the `coverage_metrics.rs` computations and add markdown variants next to the ANSI ones rather than recomputing. Land this together with, or right after, explore-format-flag-ignored (the `--render`/`--format` unification, bucket shatter-cli-flags-and-help), so the surviving renderer is the complete one.

## Out of scope

- A per-path input-constraint column. It needs SymExpr pretty-printing; file it as a follow-up if wanted.
- Turning the entire walkthrough-review rubric into a gate.

## Related

str-zt4v, str-qwua7.15 (covered only where `--render` appears in help), explore-format-flag-ignored (cli-ux-02), str-qwua7.10 (walkthrough gate checks exit codes only).

## Priority / Type

P3, feature.
