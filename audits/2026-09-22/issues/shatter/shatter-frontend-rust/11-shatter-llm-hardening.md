---
slug: shatter-llm-hardening
kind: new
title: "shatter-llm hardening: Jev API key reachable via derived Debug; parser ignores int width/signedness and tries only the first '['; uncapped backoff; no PBT"
priority: P3
type: bug
labels: [llm, security, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# shatter-llm hardening: Jev API key reachable via derived Debug; parser ignores int width/signedness and tries only the first '['; uncapped backoff; no PBT

## Problem

There are latent defects in the LLM seed-oracle crate (`shatter-llm`). None has a known live trigger today, but each is one call site away from one:

1. **Secret in Debug.** `JevConfig` holds `api_key: String` and derives `Debug`. The containing types (`JevAdapter`, `ReplayDecisionOracle`, `DecisionFrontierRanker`) also derive Debug, and `DecisionFrontierRanker` ends up as `ExploreConfig.frontier_ranker`, with `ExploreConfig` deriving Debug too. A future `{:?}` of the explore config would print the key. No current log site prints it.
2. **Parser type checks.** `type_matches` accepts any integer for `TypeInfo::Int { .. }` regardless of `int_width` / `int_signed` (-1 passes for `u8`, 300 passes for `u8`). `extract_first_json_array` tries only the first `[` in the model output, so prose such as "[note] ... [{...}]" loses the real array.
3. **Backoff.** Retry backoff is `Duration::from_millis(100u64 << attempt)` with no cap. That overflows at attempt 64 (a panic in debug builds) and grows to hours well before that. A server-provided Retry-After is also used uncapped.
4. **Tests.** `parse.rs` parses untrusted model output but has only example tests (about 10) and no property tests.

## Evidence

Re-verified against the audit worktree at commit 56c86168:

- `shatter-llm/src/jev.rs:23` `#[derive(Debug, Clone)]` on `JevConfig`, with `api_key` at `:26`. `jev.rs:47` `#[derive(Debug)]` (JevAdapter). `shatter-llm/src/replay.rs:12` and `shatter-llm/src/decision_ranker.rs:18` `#[derive(Debug)]`.
- `shatter-core/src/orchestrator.rs:102` `#[derive(Debug, Clone)]` on `ExploreConfig`, with `frontier_ranker: Arc<dyn FrontierRanker>` at `:164`.
- `shatter-llm/src/parse.rs:104` `TypeInfo::Int { .. } => v.is_i64() || v.is_u64(),`. `parse.rs:62` `fn extract_first_json_array`.
- `shatter-llm/src/rate_limit.rs:61-62` `retry_after.unwrap_or_else(|| Duration::from_millis(100u64 << attempt))`, bounded only by `max_retries` (`:30`, `:58`). The verifier did not check the configured `max_retries`, so how reachable the overflow/hours case is remains unconfirmed.

## Acceptance criteria

- [ ] API keys in all LLM adapter configs (Jev, anthropic, openai, google, custom) are wrapped in a redacting newtype (e.g. `SecretString` with `Debug` printing `***`) or given a manual `Debug`. A unit test asserts `format!("{:?}", config)` does not contain the key.
- [ ] Integer values are validated against `int_width` / `int_signed` (unsigned rejects negatives, width bounds enforced). Unit tests for u8 -1 / 256 and i8 -129 are rejected.
- [ ] `extract_first_json_array` tries successive `[` candidates until one parses as the expected array shape. Unit test with a leading bracketed prose fragment.
- [ ] Backoff is capped (e.g. `min(100ms · 2^n, 30s)` using `checked_shl`/saturating math), and Retry-After is capped at the same ceiling. Unit test at attempt 63/64 does not panic and returns the cap.
- [ ] Proptest in `parse.rs`: for arbitrary strings and arbitrary `ParamInfo` type lists, `parse_response` never panics and returns only vectors whose values conform to the declared types (including width/sign).
- [ ] `cargo test -p shatter-llm` passes, with the new tests shown failing on the old code where applicable.

## Suggested approach

Add a small `secret.rs` newtype in shatter-llm, reused by `shatter-core/src/config.rs`'s LLM sections if they hold keys too. Reuse core's integer range helper (the one that str-ddxe added for in-range generation) for the width check, rather than re-deriving bounds.

## Out of scope

- User documentation for the LLM oracle and the default-model policy (note on str-qwua7.21, slug `qwua7-21-llm-seed-oracle-docs`).
- The core→shatter-llm dev-dependency cycle (str-qwua7.43).

## Size

S

## References

- Findings frontend-rust-16, frontend-rust-17 (audit 2026-09-22). Old draft: `drafts/shatter-code/66-shatter-llm-hardening.md`.
- Related: str-qwua7.47 (PBT for core modules only), str-dcgk / str-m0ta (closed; built the crate).
