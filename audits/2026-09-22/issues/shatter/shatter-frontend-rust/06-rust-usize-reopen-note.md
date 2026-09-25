---
slug: rust-usize-reopen-note
kind: reopen-note
title: "NOTE on closed str-ddxe: 64/128-bit unsigned ints (incl. usize) were left unbounded and still receive negative integers"
priority: P2
type: bug
labels: [rust-frontend, input-generation, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [int-unsigned64-clamp]
existing_id: str-ddxe
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# NOTE on closed str-ddxe: 64/128-bit unsigned ints (incl. usize) were left unbounded and still receive negative integers

Target: **str-ddxe** (closed). Action: post the comment below with `bd comments add str-ddxe ...`. Leave str-ddxe closed. The follow-up work is tracked in the new issue `int-unsigned64-clamp`, which must be filed first so its id can replace `<int-unsigned64-clamp id>` (that is the only reason for the blocked_by).

## Comment text

> **Audit 2026-09-22 note** (finding goals-15). The fix recorded here deliberately left 64- and 128-bit ints unconstrained: `shatter-core/src/types.rs` `int_range` returns `None` for widths whose bounds do not fit in `i64`, which includes `usize`, `u64` and `u128`, and every generator, mutator, shrinker and the Z3 range assertion treats `None` as full i64 (`input_gen.rs:128,225,1955,3576,4020`, `solver.rs:170-174`). This issue's original report was about `usize` (`score_item`, `history_hits: usize`), so for that type the acceptance ("a fn with a usize param explores non-negative inputs") is not met; the u8 E2E gate did not cover it. Observed in the 2026-09-22 audit: `parse_language_preference(part: &str, order: usize)` (shatter-examples `standalone/rust/18_accept_language.rs`, snapshot 49984f4b) produced 12 of 16 paths as `input 1 deserialization failed: invalid value: integer `-998`, expected usize` (also -44, -1, -644, -838, i64::MIN, ...). Follow-up: **<int-unsigned64-clamp id>** (lower bound 0 for unsigned ≥ 64-bit on all paths, with a committed fixture).
