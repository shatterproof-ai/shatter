# Line-coverage metric inconsistent across languages: Go scan inflates to 100% with 7/18 branches; Rust reports ~54% for fully covered functions

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | bug |
| priority | P1 |
| labels | coverage,go,rust-frontend,parity,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | str-szcn3, str-uabz, str-hbky, str-j49xg |
| source findings | goals-06, prior-20 |

<!-- body -->
## Problem

The downstream ≥90% coverage goals are measured on this metric, but it is wrong in both directions. Go scan reports a denominator smaller than the function (hidden by a `.max(covered)` clamp); shatter-rust never sends `instrumentable_line_count`, so span-based denominators include non-executable lines.

## Current code facts / evidence

- `shatter-core/src/observe.rs:128-147` `reconcile_line_coverage`: `instrumentable.unwrap_or(span).max(covered)` (clamp added by str-uabz).
- Go: zolem internal/fixture scan (`audits/2026-09-22/goals-runs/zolem-fixture-default.json`): `(*Loader).Load` (loader.go:87-153) branches 7/18 but lines_covered 15 / total_lines 15; `(*fixturesYAMLSelector).Select` 2/10 branches, 5/5 lines; `(*SequenceCounters).Step` 2/6, 9/9; `(*wasmSelector).Select` lines 3 / total 0. Go instrumentable count from str-szcn3 (`shatter-go/protocol/handler.go:698-714`).
- Rust: every constructor in `shatter-rust/src/protocol.rs` (:814, :1233, :1268, :1303, :1338, :1555, :1590) sets `instrumentable_line_count: None`; `protocol/parity-matrix.yaml:854-874` marks rust not_supported. `explore 01_arithmetic.rs:classify_number` → '4 paths, 3/3 branches' but '54% (7/13 lines)'.

## Acceptance criteria

- Root cause of the undersized Go scan-path denominator found and fixed; reconcile warns/flags when covered >= instrumentable while branches are uncovered instead of silently clamping.
- shatter-rust computes instrumentable lines (syn statement spans); parity-matrix updated.
- Cross-language known-answer E2E: the same function in TS/Go/Rust with full branch coverage reports 100% lines; a half-covered one reports <100%.
- Conformance case asserts instrumentable_line_count present on instrument responses for all frontends.

## Suggested approach

Fix the Go path first (scan vs explore difference), then Rust.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: M

## References

- Audit findings: goals-06, prior-20 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-szcn3, str-uabz, str-hbky, str-j49xg
