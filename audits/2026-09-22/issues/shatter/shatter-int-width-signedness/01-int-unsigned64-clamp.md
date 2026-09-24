---
slug: int-unsigned64-clamp
kind: new
title: "Unsigned 64/128-bit ints (usize, u64, u128) still get negative inputs: int_range() returns None for widths beyond i64; clamp them to [0, i64::MAX]"
priority: P2
type: bug
labels: [rust-frontend, input-generation, solver, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
parent_slug: int-width-signedness-epic
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Unsigned 64/128-bit ints (usize, u64, u128) still get negative inputs: clamp them to [0, i64::MAX]

Step 1 of epic `int-width-signedness-epic`. This is the minimal core fix, on the existing i64 data path. Full u64/i128 ranges are step 3 (`core-int-range-i128`).

## Problem

Closed str-ddxe added `int_width` / `int_signed` to `TypeInfo::Int` and in-range generation plus Z3 range assertions for sized ints. But the shared helper every consumer uses, `shatter_core::types::int_range`, deliberately returns `None` for any width whose bounds do not fit in `i64`, which includes `u64`, `u128` and `usize` (and `i128`/`isize`). Random generation, mutation, shrinking, the boundary/candidate path and the solver all treat `None` as "unconstrained full i64". So a `usize` parameter still gets negative values, including `i64::MIN`. The Rust harness rejects them at deserialization, and the report shows the rejections as `throws runtime_error` rows, i.e. as target behavior.

This is a known, documented limitation of the str-ddxe fix (its own doc comment says 64-bit ranges "stay unconstrained"), not a regression. str-ddxe's u8 E2E gate never exercised it. `usize` is the most common Rust integer parameter type, so the gap is large in practice.

## Evidence

Re-verified against the audit worktree (main 16794cef + audit files):

- `shatter-core/src/types.rs:310-345`: `TypeInfo::int_range` / `fn int_range(width, signed)` return `Some` only for 8/16/32-bit widths; the arm `// 64-bit and 128-bit ranges exceed (or fill) i64; leave unconstrained.` returns `None`.
- Consumers that fall back to full i64 on `None`: `shatter-core/src/input_gen.rs:128` (`generate_int`), `:225`, `:1955` (`mutate_int`), `:3576` (`shrink_int`), `:4020`, and `shatter-core/src/solver.rs:170-174` (Z3 range assertions).
- The analyzer maps `usize` correctly: `shatter-rust/src/analyzer.rs:791` `"usize" => Some((64, false))`, `analyzer.rs:1290` `"usize" => int_type(64, false)`.
- Reproduction (audit goals run, finding goals-15). Fixture: `standalone/rust/18_accept_language.rs` in the shatter-examples repo at snapshot `49984f4b974bf937e7e6a98e26a7bc205ddee8e2` (the checkout `scripts/examples_checkout.py` produces), function `fn parse_language_preference(part: &str, order: usize) -> Option<LanguagePreference>` at line 41. The recorded transcript (`audits/2026-09-22/goals-runs/rust-walk.md`, untracked in the audit worktree, lines 150-171) shows 16 paths, 12 of them `throws runtime_error: input 1 deserialization failed: invalid value: integer `-998`, expected usize` with values -998, -44, -1, -644, -838, -9223372036854775808, -690, -926, -301, -945, -16, -905. The exact `shatter` / `shatter-rust` build used was not recorded; an earlier verifier run reported 23 rows / 19 negative with a release build.

## Acceptance criteria

- [ ] A minimal fixture is committed with this issue's branch (not only in the external examples repo): e.g. `fn pick(order: usize) -> u8 { if order > 3 { 1 } else { 0 } }` plus `u64` and `u128` variants. Before changing code, run it with a build of main (record `git rev-parse HEAD` and the `shatter --version` / `shatter-rust` binary path used) and paste the output showing negative inputs.
- [ ] Unsigned 64- and 128-bit ints get a lower bound of 0 everywhere `int_range` is consulted: generation, mutation, shrinking, boundary/candidate seeding and the Z3 range assertion. Either extend the helper's return type (e.g. separate optional min/max) or return `(0, i64::MAX)` for unsigned widths ≥ 64, and document the choice in the helper's doc comment. Signed 64/128-bit stay full range.
- [ ] Proptest over `TypeInfo::Int { int_signed: Some(false), int_width }` for widths 8/16/32/64/128 and the `usize` mapping: every generator, mutator and shrinker entry point, and a solver model under the range assertion, produce values ≥ 0 and within width. The test must fail on main for width 64 (paste the failing output).
- [ ] E2E known-answer test in `shatter-core/tests/e2e_concolic_rust.rs` using the committed fixture asserts zero `deserialization failed` rows for the usize/u64 params. Show it failing on main and passing on the branch with `SHATTER_EXAMPLES_DIR="$(python3 scripts/examples_checkout.py --no-update)" cargo test --test e2e_concolic_rust <name> -- --include-ignored` (plain `cargo test --test e2e_concolic_rust` runs nothing; every case is `#[ignore]`d). Paste both `test result:` lines.
- [ ] `shatter-llm`'s parser width check (`shatter-llm-parse-validation`) consumes the corrected helper; post a comment on that issue when this lands.

## Suggested approach

Change `int_range` to return `(0, i64::MAX)` for unsigned widths ≥ 64 (values above `i64::MAX` cannot be represented in the current `Value::Number` i64 paths anyway), then audit the five `input_gen.rs` call sites and `solver.rs:170-174` for assumptions that `Some` implies width ≤ 32. Check the TS/Go frontends' use of the same helper (Go `uint64`) per the parity rule.

## Out of scope

- Classifying harness input-deserialization failures as tool errors instead of target throws (`rust-input-deserialize-classification`).
- Values above `i64::MAX` for `u64`/`u128` (representation change): `core-int-range-i128`.
- Go's separate `go_uint`/`go_byte` complex kinds: `go-int-width-sign-emission`.

## Size

S

## References

- Finding goals-15 (audit 2026-09-22). Old draft: `drafts/shatter-code/82-rust-usize-negative-inputs.md`.
- Related: str-ddxe (closed; introduced the helper and the 64-bit exclusion; note `rust-usize-reopen-note`), str-qwua7.14 (closed; walkthrough param-type disagreement), str-4yc9w (open, Go counterpart of the misclassification), str-cfsa (closed, earlier Go counterpart).
