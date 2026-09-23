---
slug: go-scan-coverage-clamp
kind: new
title: "Line coverage silently clamps the denominator up to the covered count: Go scan reports 100% lines (15/15) for a 67-line function with 7/18 branches"
priority: P1
type: bug
labels: [coverage, go, scan, parity, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Line coverage silently clamps the denominator up to the covered count: Go scan reports 100% lines (15/15) for a 67-line function with 7/18 branches

## Problem

Downstream coverage goals (for example the zolem and pickpackit ≥90% goals) are measured on `lines_covered / total_lines`. For Go under `scan`, that metric is inflated. Functions whose loop bodies and error returns never ran report 100% line coverage. The frontend sends an undersized instrumentable-line denominator, and `reconcile_line_coverage` hides it by raising the denominator to whatever was covered (`.max(covered)`, added by str-uabz). A metric that cannot show a gap makes the coverage goals unfalsifiable.

This issue covers the Go inflation, the silent clamp and a cross-language known-answer coverage test. The Rust frontend's missing `instrumentable_line_count` (the deflation side) is rust-instrumentable-line-count.

## Evidence

Line numbers re-checked against `56c86168` (branch `audit-2026-09-22`):

- `shatter-core/src/observe.rs:128-147` `reconcile_line_coverage`: `denom = instrumentable_line_count.unwrap_or(span).max(covered)`. Unit tests `reconcile_bumps_denominator_when_observed_exceeds_instrumentable` (:869) and `reconcile_invariant_holds_for_zolem_style_overcount` (:881) pin the clamp as intended behaviour.
- Callers: `reconcile_observation_coverage` (:153) from `shatter-core/src/explorer.rs:1969`, `:2640` and `shatter-core/src/pipeline_orchestrator.rs:553`. `explorer.rs:1686-1690` and `:2612-2613` derive `total_lines` from `instrumentable_line_count`, falling back to the span.
- Parallel-path divergence: the concolic scan path overwrites the denominator with the raw span, `result.total_lines = analysis.end_line.saturating_sub(analysis.start_line) + 1` (`shatter-core/src/scan_orchestrator.rs:3129`). The random and concolic scan paths therefore compute coverage differently.
- Go instrumentable count comes from `shatter-go/protocol/handler.go:698-714` (`MaterializeInstrumentedDirectory` → `resp.InstrumentableLineCount`, str-szcn3). The E2E `e2e_go_instrumentable_line_count_matches_probed_lines` (`shatter-core/tests/e2e_concolic_go.rs:354`) covers `explore`, not `scan`.
- Observed (zolem `internal/fixture` default scan, `audits/2026-09-22/goals-runs/zolem-fixture-default.json`):
  - `(*Loader).Load` (loader.go:87-153, 67 lines): branches 7/18, `lines_covered 15 / total_lines 15`. Executed lines were 88, 89, 93-95, 99-101, 104, 108-110, 138, 145 and 152, so the loop body (111-136) and every error return never ran.
  - `(*fixturesYAMLSelector).Select`: 2/10 branches, 5/5 lines.
  - `(*SequenceCounters).Step`: 2/6 branches, 9/9 lines.
  - `(*wasmSelector).Select`: `lines_covered 3 / total_lines 0`. The clamp would have produced 3/3, so at least one scan path writes `total_lines` without going through `reconcile_*`.
- The verifier did not re-run the zolem scan and relied on the reviewer's artifact. Reproduce first (see acceptance).

## Acceptance criteria

- [ ] Root cause of the undersized Go denominator under `scan` is found and stated in the issue before the fix. Likely places: the instrumentable count may be computed for the wrong function or file in the scan-path Instrument request, it may be cached across functions, or the default and concolic scan paths may take different denominators. Include a reproduction on a checked-in Go fixture shaped like `(*Loader).Load` (a method with a loop and early error returns).
- [ ] The denominator is correct for Go under both `explore` and `scan`, and under both explorer modes. `scan_orchestrator.rs:3129` no longer bypasses the instrumentable count.
- [ ] `reconcile_line_coverage` no longer silently raises the denominator. When `covered > instrumentable`, it logs a `warn` naming the function and both numbers, and falls back to the span. When `total_lines == 0` with `lines_covered > 0`, it is treated the same way. The two unit tests that pin the clamp are rewritten to assert the new behaviour.
- [ ] Cross-language known-answer coverage test (new, in `shatter-core/tests/` or the E2E suites): the same small function in TS and Go, one fully covered and one half covered, run through both `explore` and `scan`. It asserts 100% lines for the fully covered function and <100% for the half-covered one. A Rust leg is added by rust-instrumentable-line-count. If that issue lands first, it creates this test with TS + Rust legs, and this issue adds Go. At close, show the Go `scan` leg failing on current `main` and passing after the fix.
- [ ] A conformance case asserts that `instrumentable_line_count` is present on Instrument responses for every frontend that `protocol/parity-matrix.yaml` marks supported.
- [ ] `task affected` passes, and its `Gates selected` output is recorded. `cargo test --test e2e_concolic_go` passes.

## Suggested approach

Start by diffing the Instrument request/response for `(*Loader).Load` between `explore` and `scan` (log both at debug). Then trace which `total_lines` value each scan path writes into the summary. Fix the Go side, remove the clamp in the same change, and remove the span override at :3129 so every path goes through `reconcile_observation_coverage`.

## Out of scope

- Rust frontend `instrumentable_line_count` (rust-instrumentable-line-count).
- Adapter-owned executions that return empty coverage (str-j49xg).
- The branch metric counting sites instead of arms (branch-metric-counts-sites).

## Priority

P1: the coverage metric that downstream goals are measured on overstates Go coverage by a wide margin, and the clamp hides the error.

## Type

bug

## Dependencies

- Blocked by: none.
- Related: rust-instrumentable-line-count (shares the cross-language test), str-szcn3 (closed; Go instrumentable count), str-uabz (closed; added the clamp), str-hbky (closed; span denominator), str-j49xg (open epic), branch-metric-counts-sites.

## References

Audit 2026-09-22 finding goals-06 (verified P1), Go half. Source draft: `drafts/shatter-code/78-line-coverage-metric-consistency.md` (split per report §14 item 11).
