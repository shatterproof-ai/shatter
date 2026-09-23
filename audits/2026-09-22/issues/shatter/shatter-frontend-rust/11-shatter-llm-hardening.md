---
slug: shatter-llm-hardening
kind: new
title: "LLM API keys are reachable through derived Debug (JevConfig, adapter configs, ExploreConfig.frontier_ranker)"
priority: P3
type: bug
labels: [llm, security, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# LLM API keys are reachable through derived Debug (JevConfig, adapter configs, ExploreConfig.frontier_ranker)

## Problem

`JevConfig` holds `api_key: String` and derives `Debug`. The containing types (`JevAdapter`, `ReplayDecisionOracle`, `DecisionFrontierRanker`) also derive Debug, and `DecisionFrontierRanker` ends up as `ExploreConfig.frontier_ranker`, with `ExploreConfig` deriving Debug too. The other adapters (anthropic, openai, google) keep `api_key: String` in their structs, and `shatter-core/src/config.rs` holds `api_key: Option<String>` in `#[derive(Debug)]` LLM config sections. A future `{:?}` of the explore config, an adapter, or the loaded config would print the key. No current log site is known to print it.

## Evidence

Re-verified against the audit worktree (main 16794cef + audit files):

- `shatter-llm/src/jev.rs:23` `#[derive(Debug, Clone)]` on `JevConfig`, with `pub api_key: String` at `:26`. `jev.rs:47` `#[derive(Debug)]` (JevAdapter). `shatter-llm/src/replay.rs:12` and `shatter-llm/src/decision_ranker.rs:18` `#[derive(Debug)]`.
- `shatter-llm/src/anthropic.rs:21`, `openai.rs:21`, `google.rs:19`: `api_key: String`.
- `shatter-core/src/config.rs:325, 357, 388`: `pub api_key: Option<String>` in the anthropic/openai/google adapter config sections.
- `shatter-core/src/orchestrator.rs:102` `#[derive(Debug, Clone)]` on `ExploreConfig`, with `frontier_ranker: Arc<dyn FrontierRanker>` at `:164`.
- Dependency direction: `shatter-llm/Cargo.toml:8` depends on `shatter-core`; core has `shatter-llm` only as a dev-dependency (`shatter-core/Cargo.toml:44`, itself slated for removal by str-qwua7.43). A shared secret type therefore cannot live in `shatter-llm` if `shatter-core::config` is to use it.

## Acceptance criteria

- [ ] Every struct that holds an LLM API key (Jev, anthropic, openai, google, custom/local if they hold one, and the `shatter-core/src/config.rs` sections) prints a redacted value under `Debug`. Either a redacting newtype defined in `shatter-core` (or a new dependency-free crate both can use), or manual `Debug` impls. The type must not be defined in `shatter-llm` and imported into `shatter-core` (that would reverse the production dependency).
- [ ] Serialization of config files is unchanged (keys still round-trip through serde where they did before); a round-trip test covers one config section.
- [ ] Unit tests with a sentinel key (e.g. `sk-test-SENTINEL`) assert `format!("{:?}", x)` does not contain it for each config/adapter type above, for `DecisionFrontierRanker`, and for an `ExploreConfig` holding a ranker built from a keyed config. Show at least the `JevConfig` and `ExploreConfig` tests failing on main.
- [ ] `cargo test -p shatter-llm -p shatter-core` passes.

## Out of scope

- Parser type validation (`shatter-llm-parse-validation`) and retry backoff (`shatter-llm-backoff-cap`).
- User documentation for the LLM oracle (note on str-qwua7.21, slug `qwua7-21-llm-seed-oracle-docs`).
- The core→shatter-llm dev-dependency cycle (str-qwua7.43).

## Size

S

## References

- Finding frontend-rust-16 (audit 2026-09-22). Old draft: `drafts/shatter-code/66-shatter-llm-hardening.md` (split after the Codex cross-check into this issue, `shatter-llm-parse-validation` and `shatter-llm-backoff-cap`).
- Related: str-dcgk / str-m0ta (closed; built the crate).
