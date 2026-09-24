---
slug: rust-input-deserialize-classification
kind: new
title: "Rust harness input-deserialization failures are reported as target `throws runtime_error` instead of tool/input errors"
priority: P2
type: bug
labels: [rust-frontend, reporting, audit]
parent_epic: "Epic: integer width and signedness end-to-end (protocol → core ranges → every frontend)"
parent_slug: int-width-signedness-epic
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Rust harness input-deserialization failures are reported as target `throws runtime_error` instead of tool/input errors

## Problem

When the Rust harness cannot deserialize an input into the parameter's type (`input N deserialization failed: invalid value: integer `-998`, expected usize`), the target function never runs. Explore/scan output nevertheless records the row as `throws runtime_error: ...`, which reads as a behavior of the target. Any input the generator gets wrong (see `int-unsigned64-clamp`, but also any future type mismatch) becomes a fake finding, and the walkthrough error regex does not match "deserialization failed", so no gate notices.

## Evidence

- Audit transcript `audits/2026-09-22/goals-runs/rust-walk.md` lines 159-171 (untracked in the audit worktree): 12 of 16 `parse_language_preference` rows are `throws runtime_error: input 1 deserialization failed: ...`.
- Go has the same class of problem tracked as str-4yc9w (open); str-cfsa (closed) was an earlier Go counterpart.

## Acceptance criteria

- [ ] Locate where the Rust harness produces the `input N deserialization failed` error (generated harness code in `shatter-rust/src/executor.rs` / `shatter-rust-runtime`) and give it a distinct `thrown_error.error_type` (e.g. `input_error`) or a distinct execute-result outcome, consistent with whatever str-4yc9w chooses for Go. Name the sites in the close note.
- [ ] The core and report layers treat that outcome as a tool/input error: it is not counted as a target behavior/finding in explore and scan output, and it is counted in the run's error summary.
- [ ] Test: a harness-level unit test feeding a negative integer to a `usize` param asserts the new classification, and a report-level test asserts the row is not rendered as `throws`. Show them failing on main.
- [ ] If the classification is protocol-visible (new `error_type` value), update `protocol/parity-matrix.yaml` and `shatter-rust/CLAUDE.md`, and run `task parity` + `task conformance`.

## Out of scope

- Fixing the generator bug that currently produces most of these rows (`int-unsigned64-clamp`).
- The Go-side change (str-4yc9w), except for agreeing on the shared classification.

## Size

S

## References

- Split from `int-unsigned64-clamp` after the Codex cross-check of the 2026-09-22 audit (finding goals-15).
- Related: str-4yc9w (open, Go), str-cfsa (closed, Go).
