---
slug: rust-usize-reopen-note
kind: reopen-note
title: "NOTE on closed str-ddxe: usize parameters still receive negative integers"
priority: P2
type: bug
labels: [rust-frontend, input-generation, regression, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [rust-usize-negative-inputs]
existing_id: str-ddxe
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# NOTE on closed str-ddxe: usize parameters still receive negative integers

Target: **str-ddxe** (closed). Action: post the comment below with `bd comments add str-ddxe ...`. Leave str-ddxe closed. The follow-up work is tracked in the new issue `rust-usize-negative-inputs`, which must be filed first so its id can replace `<rust-usize-negative-inputs id>` (that is the only reason for the blocked_by).

## Comment text

> **Audit 2026-09-22 note** (finding goals-15). The fix recorded here is incomplete on at least one generation path. With the current release `shatter` + `shatter-rust`, `shatter explore 18_accept_language.rs:parse_language_preference` (`fn parse_language_preference(part: &str, order: usize)`) gave 23 rows, 19 of them `input 1 deserialization failed: invalid value: integer `-998`, expected usize` (also -44, -1, -644, -838, i64::MIN). The analyzer still maps `usize` to `int_type(64, false)` (`shatter-rust/src/analyzer.rs:791,1290`), so the negative values come from a downstream generator that ignores `int_signed`. The u8 E2E gate added here does not cover it. Follow-up: **<rust-usize-negative-inputs id>**. It also covers classifying harness deserialization failures as tool errors instead of target throws.
