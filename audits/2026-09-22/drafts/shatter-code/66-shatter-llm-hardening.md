# shatter-llm hardening: Jev API key reachable via derived Debug; parser ignores int width and first-bracket only; uncapped backoff; no PBT

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | bug |
| priority | P3 |
| labels | llm,security,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | none |
| source findings | frontend-rust-16, frontend-rust-17 |

<!-- body -->
## Problem

Latent issues in the LLM seed-oracle crate.

## Current code facts / evidence

- `shatter-llm/src/jev.rs:23-29` `#[derive(Debug, Clone)] JevConfig { api_key }`; JevAdapter, ReplayDecisionOracle (replay.rs:12), DecisionFrontierRanker (decision_ranker.rs:18) derive Debug; `shatter-core/src/orchestrator.rs:102-164` ExploreConfig derives Debug and holds frontier_ranker. No current log site prints it.
- `shatter-llm/src/parse.rs:62-103` `TypeInfo::Int { .. } => v.is_i64() || v.is_u64()` (ignores int_width/int_signed: -1 passes for u8); `extract_first_json_array` tries only the first '['.
- `shatter-llm/src/rate_limit.rs:52-66` `Duration::from_millis(100u64 << attempt)` uncapped (overflow at 64); Retry-After uncapped.
- parse.rs: 10 example tests, no proptest.

## Acceptance criteria

- Secrets wrapped in a redacting newtype (or manual Debug) across LLM adapter configs.
- Integer range validated against width/signedness; successive '[' candidates tried.
- Backoff capped (e.g. min(2^n·100ms, 30s)); Retry-After capped.
- Proptest: parse_response never panics and returns only type-conforming vectors.

## Suggested approach

Implementer's choice within the acceptance criteria above.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: S

## References

- Audit findings: frontend-rust-16, frontend-rust-17 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: none
