---
slug: shatter-llm-backoff-cap
kind: new
title: "shatter-llm retry backoff is uncapped: `100ms << attempt` overflows at attempt 64 and Retry-After is honored without a ceiling"
priority: P3
type: bug
labels: [llm, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# shatter-llm retry backoff is uncapped: `100ms << attempt` overflows at attempt 64 and Retry-After is honored without a ceiling

## Problem

`RateLimitedOracle` retries with `Duration::from_millis(100u64 << attempt)` and no cap. The delay reaches minutes by attempt 11 and hours by attempt 16, and the shift overflows at attempt 64 (a panic in debug builds). A server-provided Retry-After is also used uncapped, so one bad header can stall an explore run indefinitely. The number of attempts is bounded only by `max_retries`, which is user configuration (`LlmConfig.max_retries`), so large values are reachable.

## Evidence

Re-verified against the audit worktree (main 16794cef + audit files):

- `shatter-llm/src/rate_limit.rs:61-62` `retry_after.unwrap_or_else(|| Duration::from_millis(100u64 << attempt))`, bounded only by `max_retries` (`:30`, `:58`).
- `shatter-llm/src/registry.rs:28-50` and `shatter-cli/src/helpers.rs:1612` pass `config.max_retries` straight through.

## Acceptance criteria

- [ ] Computed backoff is capped (e.g. `min(100ms · 2^n, 30s)` using `checked_shl` / saturating math), and Retry-After is clamped to the same ceiling. The ceiling is a named constant documented in the module docs.
- [ ] Unit tests: attempt 63 and 64 do not panic and return the cap; a Retry-After of 1 hour is clamped. Use a mock clock or assert on the computed `Duration` rather than sleeping. Show the attempt-64 test failing (panicking) on main in a debug build.
- [ ] `cargo test -p shatter-llm` passes.

## Out of scope

- API-key redaction (`shatter-llm-hardening`) and parser validation (`shatter-llm-parse-validation`).

## Size

XS

## References

- Finding frontend-rust-16 (audit 2026-09-22). Split from `shatter-llm-hardening` after the Codex cross-check.
