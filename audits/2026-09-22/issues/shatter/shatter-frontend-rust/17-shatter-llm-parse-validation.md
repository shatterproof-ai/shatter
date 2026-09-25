---
slug: shatter-llm-parse-validation
kind: new
title: "shatter-llm response parser ignores int width/signedness and tries only the first '[' in model output; no property tests"
priority: P3
type: bug
labels: [llm, input-generation, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [int-unsigned64-clamp]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# shatter-llm response parser ignores int width/signedness and tries only the first '[' in model output; no property tests

## Problem

`shatter-llm/src/parse.rs` turns untrusted model output into seed inputs.

1. `type_matches` accepts any integer for `TypeInfo::Int { .. }` regardless of `int_width` / `int_signed` (-1 passes for `u8`, 300 passes for `u8`), so LLM seeds can carry the same out-of-range values that the Rust harness rejects at deserialization.
2. `extract_first_json_array` tries only the first `[` in the model output, so prose such as "[note] ... [{...}]" loses the real array.
3. The module has only example tests (about 10) and no property tests.

## Evidence

Re-verified against the audit worktree (main 16794cef + audit files):

- `shatter-llm/src/parse.rs:104` `TypeInfo::Int { .. } => v.is_i64() || v.is_u64(),`. `parse.rs:62` `fn extract_first_json_array`.
- The core range helper `shatter_core::types::int_range` (`shatter-core/src/types.rs:310-345`) returns `None` for 64/128-bit widths including `usize`, so reusing it unchanged would still accept negatives for `usize`/`u64`. `int-unsigned64-clamp` fixes the helper; this issue is blocked by it so the parser reuses the corrected version.

## Acceptance criteria

- [ ] Integer values are validated with the corrected core helper (after `int-unsigned64-clamp`): unsigned rejects negatives at every width including 64/128/usize, and 8/16/32-bit bounds are enforced. Unit tests: u8 -1 and 256 rejected, i8 -129 rejected, usize -1 rejected, u64 0 accepted. Show them failing on main.
- [ ] `extract_first_json_array` tries successive `[` candidates until one parses as the expected array shape. Unit test with a leading bracketed prose fragment, and one where no candidate parses.
- [ ] Proptest in `parse.rs`: for arbitrary strings and arbitrary `ParamInfo` type lists, `parse_response` never panics and returns only vectors whose values conform to the declared types (including width/sign).
- [ ] `cargo test -p shatter-llm` passes.

## Out of scope

- API-key redaction (`shatter-llm-hardening`) and retry backoff (`shatter-llm-backoff-cap`).

## Size

S

## References

- Finding frontend-rust-17 (audit 2026-09-22). Split from `shatter-llm-hardening` after the Codex cross-check.
- Related: str-qwua7.47 (PBT for core modules only).
