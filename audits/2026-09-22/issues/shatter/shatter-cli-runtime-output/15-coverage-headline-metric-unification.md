---
slug: coverage-headline-metric-unification
kind: new
title: "explore, scan and run headline three different coverage metrics without naming them; make line coverage the named headline everywhere"
priority: P2
type: feature
labels: [report, ux, coverage, explore, scan, run, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# explore, scan and run headline three different coverage metrics without naming them

## Problem

For the same TS example files:

- `explore` headlines **line** coverage: "100% coverage (7/7 lines)".
- `scan` headlines **branch** coverage: "Overall coverage (completed-functions subset): 92.5%" (37/40 branches), without saying it is branches.
- `run` totals **lines**: "80/99 81%".

`run` also uses a different default iteration budget from `scan`: for `computeStats`, run reports 31% and scan 63%. A user comparing commands cannot tell whether the numbers disagree or measure different things. Split out of run-report-verdict-and-coverage-metrics during the cross-check.

## Evidence

- Transcripts in `audits/2026-09-22/cli-ux-transcripts/`: `ts-explore.out` line 21 (`100% coverage (7/7 lines)`), `ts-scan.out` line 28 (`Overall coverage (completed-functions subset): 92.5%`), `run.out` total row `| **Total** | **40** | **80/99** | **81%** |` and `computeStats` at `5/16 | 31%`.
- The cross-command comparison was not re-run by the audit verifier (cli-ux-13 note); the first acceptance item re-establishes it.
- Source findings: audit 2026-09-22 cli-ux-13. Area evidence: `audits/2026-09-22/areas/cli-ux.md` F13.

## Contract (decided in this draft; the maintainer can override before filing)

One contract, not two:

1. **Headline metric = line coverage, for all three commands**, printed through one shared type (`CoverageHeadline { metric, covered, total }`) that always prints the metric name: `Line coverage: 80/99 (81%)`.
2. Branch coverage may appear as a **secondary**, named line (`Branch coverage: 37/40 (92.5%)`) below the headline, never as the headline.
3. Each headline states the iteration budget it was produced with when it differs from explore's default.

Rationale: two of the three commands already headline lines, and what the branch metric counts is under question in branch-metric-counts-sites (bucket shatter-reports-and-specs). Choosing lines means this issue does not wait on that one.

## Acceptance criteria

- [ ] Re-run the three commands on the same example files (record commit and commands) and record the three headline strings in the issue before changing code.
- [ ] `explore`, `scan` and `run` markdown reports print their headline through the shared type, as `Line coverage: <covered>/<total> (<pct>%)`, and any branch figure as a separately named secondary line. SPEC (reporting section) records the contract, with a §8 changelog row.
- [ ] **Metric-identity test** (fails on current main): on one deterministic single-file, single-function TS fixture with a fixed seed, the headline **denominators** (total lines) reported by explore, scan and run are equal, and each headline string starts with `Line coverage:`. On current main scan's headline is a branch figure (denominator 40 vs 99 lines), so this fails. A test that only checks the presence of a metric name does not satisfy this criterion.
- [ ] Budget: either `run` and `scan` share the default iteration budget, or the report prints the budget next to the headline; a test asserts whichever is chosen.
- [ ] JSON output keeps both line and branch figures under distinct, named fields (no field renamed without a changelog row).
- [ ] `task walkthrough` passes and `/walkthrough-review` has been run. `task affected` passes, with `Gates selected` recorded.

## Out of scope

- What the branch metric counts (branch-metric-counts-sites).
- Validity verdict placement (run-report-verdict-and-coverage-metrics).

## Related

str-qwua7.57 (terminology unification, not metrics), str-4ad5, branch-metric-counts-sites. In this bucket: run-report-verdict-and-coverage-metrics, markdown-drops-render-plain-info (branch line in explore).

## Priority / Type

P2, feature.
