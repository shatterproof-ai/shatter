---
slug: rust-usize-negative-inputs
kind: new
title: "Rust targets still receive negative integers for usize params (str-ddxe fix incomplete), and the deserialization failures are reported as target throws"
priority: P2
type: bug
labels: [rust-frontend, input-generation, regression, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Rust targets still receive negative integers for usize params (str-ddxe fix incomplete), and the deserialization failures are reported as target throws

## Problem

Closed str-ddxe added `int_width` / `int_signed` to `TypeInfo::Int`, in-range generation and Z3 range assertions for unsigned Rust ints. Its close reason: "sized int_width/int_signed on TypeInfo::Int; in-range generation + Z3 range assertions; new u8 e2e gate green". Yet exploring a function with a `usize` parameter still produces many negative inputs. The harness rejects them at deserialization, and the report shows them as `throws runtime_error` rows, i.e. as behaviors of the target. The findings are noise twice over: the inputs are invalid, and the rejections are shown as target behavior.

## Evidence

- Audit repro (finding goals-15, reproduced by the verifier with the current release `shatter` + `shatter-rust`): `shatter explore audits/2026-09-22/goals-runs/standalone/rust/18_accept_language.rs:parse_language_preference` (signature `fn parse_language_preference(part: &str, order: usize) -> Option<LanguagePreference>`, line 41) gave 23 rows. 19 of them were `throws runtime_error: input 1 deserialization failed: invalid value: integer `-998`, expected usize`, with values -998, -44, -1, -644, -838 and i64::MIN. Transcript: `audits/2026-09-22/goals-runs/rust-walk.md` §`parse_language_preference`.
- The analyzer maps `usize` correctly (re-verified at 56c86168): `shatter-rust/src/analyzer.rs:791` `"usize" => Some((64, false))`, `analyzer.rs:1290` `"usize" => int_type(64, false)`. So the negative values come from a downstream generation path that ignores `int_signed` (candidates: boundary-value seeds, literal/constant mining, mutation, solver models, seed/corpus replay in `shatter-core/src/input_gen.rs` and the orchestrator). i64::MIN in particular looks like a boundary seed.
- The walkthrough error regex does not match "deserialization failed", so no gate catches these rows (see the prior audit and str-qwua7.14).

## Acceptance criteria

- [ ] Identify which generation path(s) emit negative values for an unsigned `TypeInfo::Int`, and name them in the close note.
- [ ] Unsigned params (`u8`/`u16`/`u32`/`u64`/`u128`/`usize`) never receive negative values, or values above their width, on any path: random, boundary, mutation, literal mining, solver, seed/corpus replay. Add a proptest over `TypeInfo::Int { int_signed: Some(false), int_width }` for every generator/mutator entry point.
- [ ] Harness input-deserialization failures are classified as tool/input errors (not target `throws`) in explore/scan output, matching the direction of str-4yc9w for Go.
- [ ] E2E known-answer test (in `shatter-core/tests/e2e_concolic_rust.rs`, fixture copied into the repo's Rust examples) with a `(&str, usize)` signature asserts zero deserialization-failure rows. Show it failing before the fix and passing after.
- [ ] Post the reopen note on str-ddxe (slug `rust-usize-reopen-note`) linking this issue.

## Suggested approach

Start by diffing the `TypeInfo` emitted for `parse_language_preference` against the u8 E2E fixture str-ddxe used, to confirm `int_signed: Some(false)` reaches the core for `usize`. Then grep `shatter-core/src` for integer candidate producers that construct `Value::Number` from i64 constants without consulting `int_signed` (boundary tables, mined constants, mutation deltas, solver model extraction).

## Out of scope

- Go-side misclassification (str-4yc9w).
- Rust walkthrough param-type disagreement beyond this signature (str-qwua7.14).

## Size

S

## References

- Finding goals-15 (audit 2026-09-22). Old draft: `drafts/shatter-code/82-rust-usize-negative-inputs.md`.
- Related: str-ddxe (closed; the fix this regresses or does not fully cover), str-qwua7.14 (open), str-4yc9w (open, Go counterpart of the misclassification), str-cfsa (closed, earlier Go counterpart).
