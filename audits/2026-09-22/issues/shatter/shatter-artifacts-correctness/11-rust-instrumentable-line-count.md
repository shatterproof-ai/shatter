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
- `protocol/parity-matrix.yaml:854-874` (`instrumentable_line_count`) says TS tracks `instrumentableLines: Set<number>`, Go threads `instrumentableLines map[int]struct{}` (str-szcn3), and "Rust does not populate this field yet", with `rust: not_supported`.
- Core fallback: `shatter-core/src/observe.rs:137-145` (span when `None`) and `shatter-core/src/explorer.rs:1686-1687`.
- Observed: `shatter explore 01_arithmetic.rs:classify_number` (the audit's standalone fixture, `audits/2026-09-22/goals-runs/standalone/rust/01_arithmetic.rs`, function at lines 6-18) gives `4 path(s) · 54% coverage (7/13 lines)` with 3/3 branches (`audits/2026-09-22/goals-runs/rust-walk.md:9`). The verifier's re-run timed out under load average 92, so reproduce on an idle machine.
- No open tracker issue covers the Rust gap. str-j49xg is about adapter-owned executions returning empty coverage, which is a different problem.

## Acceptance criteria

- [ ] shatter-rust counts the distinct real source lines that receive a `line_hit` probe for the instrumented function. The semantics match TS/Go: deduplicate per line and exclude synthetic probe lines. Instrument and Execute responses populate `instrumentable_line_count`.
- [ ] `protocol/parity-matrix.yaml` marks `instrumentable_line_count` as `rust: supported` with a note, `shatter-rust/CLAUDE.md` is updated, and `task parity` and `task conformance` pass.
- [ ] A Rust E2E test, modelled on `e2e_go_instrumentable_line_count_matches_probed_lines` (`shatter-core/tests/e2e_concolic_go.rs:354`), is added to `shatter-core/tests/e2e_concolic_rust.rs`. A fully covered function reports 100% lines, and the reported count equals the number of probed lines. At close, show it failing on current `main` and passing after the fix, with `cargo test --test e2e_concolic_rust` output in the close note.
- [ ] The Rust leg of the cross-language coverage test from go-scan-coverage-clamp is added. If this issue lands first, create that test with TS + Rust legs.
- [ ] Unit test in `shatter-rust/src/instrument.rs` for the count on a function with blank lines, comments and a multi-line expression.
- [ ] `task affected` passes, and its `Gates selected` output is recorded.

## Suggested approach

Have the instrumentor visitor collect a `BTreeSet<u32>` of the `line` values passed to `line_hit_stmt`, excluding any synthetic or zero lines. Return the set's length with the instrumented source, and thread it into the response constructors in `protocol.rs`. Follow the Go change in str-szcn3 as the template. Read `shatter-rust/CLAUDE.md` first for the per-crate parity and invocation-model rules.

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
