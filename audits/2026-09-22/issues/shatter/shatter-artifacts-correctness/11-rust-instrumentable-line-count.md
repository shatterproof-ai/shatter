---
slug: rust-instrumentable-line-count
kind: new
title: "Rust frontend never reports instrumentable_line_count: fully covered functions show ~54% line coverage (Go was fixed in str-szcn3)"
priority: P1
type: bug
labels: [coverage, rust-frontend, parity, protocol, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Rust frontend never reports instrumentable_line_count: fully covered functions show ~54% line coverage (Go was fixed in str-szcn3)

## Problem

The core computes line coverage as lines executed / `instrumentable_line_count`. When a frontend omits that field, the core falls back to the function's source span, which counts blank lines, comments, braces and signature lines. TypeScript and Go send the count (Go since str-szcn3). shatter-rust never does. Every Rust function therefore under-reports line coverage. `classify_number` has all four outcomes and all branches covered, yet it reports 54%. Downstream Rust coverage goals are measured on this number.

## Evidence

Line numbers re-checked against `56c86168` (branch `audit-2026-09-22`):

- `shatter-rust/src/protocol.rs:734` declares `instrumentable_line_count: Option<u32>`. Every response constructor sets it to `None`: :814, :1233, :1268, :1303, :1338, :1555 and :1590.
- The Rust instrumentor already inserts one probe per real source line: `shatter-rust/src/instrument.rs:230` `line_hit_stmt(line)` → `shatter_rust_runtime::line_hit(#line)`, used at :260, :289 and :317. The count is available but is not collected.
- The numerator is wider than the `line_hit` lines. The runtime reports `lines_executed` as the union of `line_hit` lines and the `line` of every recorded branch decision: `merge_lines_executed` (`shatter-rust-runtime/src/lib.rs:291-309`, used at :333) adds each `BranchDecision.line > 0`. Branch decisions come from `branch_hit` probes whose `line` can be a line with no `line_hit` probe, notably match-arm pattern lines (`instrument.rs:420-421`, `line_of(&arm.pat)`) and `if let` condition lines (`instrument.rs:335-348`). A denominator built from `line_hit` lines alone can therefore be smaller than the numerator, which is exactly the overcount that the core's `.max(covered)` clamp hides (go-scan-coverage-clamp).
- `instrumentable_line_count` is an Instrument-response field in the core protocol (`shatter-core/src/protocol.rs:579-589`, `Response::Instrument`). shatter-rust's response is one flat struct (`shatter-rust/src/protocol.rs:725-735`), which is why every constructor, including non-Instrument ones, spells the field out as `None`.
- `protocol/parity-matrix.yaml:854-874` (`instrumentable_line_count`) says TS tracks `instrumentableLines: Set<number>`, Go threads `instrumentableLines map[int]struct{}` (str-szcn3), and "Rust does not populate this field yet", with `rust: not_supported`.
- Core fallback: `shatter-core/src/observe.rs:137-145` (span when `None`) and `shatter-core/src/explorer.rs:1686-1687`.
- Observed: `shatter explore 01_arithmetic.rs:classify_number` (the audit's standalone fixture, `audits/2026-09-22/goals-runs/standalone/rust/01_arithmetic.rs`, function at lines 6-18) gives `4 path(s) · 54% coverage (7/13 lines)` with 3/3 branches (`audits/2026-09-22/goals-runs/rust-walk.md:9`). The verifier's re-run timed out under load average 92, so reproduce on an idle machine.
- No open tracker issue covers the Rust gap. str-j49xg is about adapter-owned executions returning empty coverage, which is a different problem.

## Acceptance criteria

- [ ] shatter-rust computes `instrumentable_line_count` as the number of distinct real source lines (line > 0) that **any** probe for the instrumented function can report as executed: every `line_hit` line and every `branch_hit` line (if/else, `if let`, match-arm pattern lines, loop heads). The invariant is "the set of lines that can appear in `lines_executed` is a subset of the counted set", so a fully covered function reports exactly 100% and never more. Only the Instrument response populates the field; no field is added to Execute responses, and the other response constructors keep `None`.
- [ ] `protocol/parity-matrix.yaml` marks `instrumentable_line_count` as `rust: supported` with a note, `shatter-rust/CLAUDE.md` is updated, and `task parity` and `task conformance` pass.
- [ ] A Rust E2E test, modelled on `e2e_go_instrumentable_line_count_matches_probed_lines` (`shatter-core/tests/e2e_concolic_go.rs:354`), is added to `shatter-core/tests/e2e_concolic_rust.rs`. A fully covered function reports 100% lines, and the reported count equals the number of probed lines. A second case uses a function with a `match` whose arm patterns sit on their own lines (a multi-line match, arms not on the same line as their bodies) and asserts, over all explored inputs, that every line in `lines_executed` is counted and full coverage is exactly 100%. At close, show both failing on current `main` and passing after the fix.
- [ ] Run with `task --force e2e-rust` (it builds the frontend, sets `SHATTER_EXAMPLES_DIR` and passes `--include-ignored`; a bare `cargo test --test e2e_concolic_rust` skips the `#[ignore]`d tests and does not count). The close note shows the `... ok` lines for the new tests from that run.
- [ ] The Rust leg of the cross-language coverage test from go-scan-coverage-clamp is added. If this issue lands first, create that test with TS + Rust legs.
- [ ] Unit tests in `shatter-rust/src/instrument.rs` for the count: a function with blank lines, comments and a multi-line expression; and a multi-line `match` whose arm-pattern lines have no `line_hit` probe (the count includes them). A property test (proptest) over generated small functions asserts that the set of probe lines emitted into the instrumented source equals the counted set.
- [ ] `task affected` passes, and its `Gates selected` output is recorded.

## Suggested approach

Have the instrumentor visitor collect one `BTreeSet<u32>` of the `line` values passed to both `line_hit_stmt` and every `branch_hit`/`branch_hit_stmt` constructor, excluding zero lines. Return the set's length with the instrumented source, and thread it into the response constructors in `protocol.rs`. Follow the Go change in str-szcn3 as the template. Read `shatter-rust/CLAUDE.md` first for the per-crate parity and invocation-model rules.

## Out of scope

- The Go scan inflation and the core clamp (go-scan-coverage-clamp).
- Cross-crate and opaque-type analysis gaps in shatter-rust.

## Priority

P1: covered by verified P1 finding goals-06, and every Rust coverage number is wrong. prior-20 alone rated it P2.

## Type

bug

## Dependencies

- Blocked by: none.
- Related: go-scan-coverage-clamp (shared cross-language test), str-szcn3 (closed; the Go fix to mirror), str-hbky (closed), str-j49xg (open; different cause).

## References

Audit 2026-09-22 findings goals-06 (verified P1, Rust half) and prior-20 (verified P2). Source draft: `drafts/shatter-code/78-line-coverage-metric-consistency.md` (split per report §14 item 11).
