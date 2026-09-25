---
slug: llm-model-rejection-diagnostic
kind: new
title: "Reproduce and fix how a rejected/retired LLM model surfaces: Anthropic adapter reports a generic `HTTP 404`, CLI propagation unverified"
priority: P3
type: bug
labels: [llm, cli, diagnostics, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Reproduce and fix how a rejected/retired LLM model surfaces: Anthropic adapter reports a generic `HTTP 404`, CLI propagation unverified

## Problem

Default model IDs are hard-coded in `shatter-core/src/config.rs` (`claude-sonnet-4-6`, `gpt-4o`, `gemini-2.0-flash`) and will eventually be retired by the providers. The OpenAI and Google adapters already turn a 404 into a specific "model not found" error. The Anthropic adapter does not: a 404 falls through to the generic `Anthropic API HTTP {status}: {text}`. It is also unverified what the user sees when any adapter fails this way during explore: whether the error reaches the CLI output, is logged as a warning, or is swallowed while the run silently continues without the oracle.

## Evidence

Re-verified against the audit worktree (main 16794cef + audit files):

- `shatter-llm/src/openai.rs:132-135`: `NOT_FOUND` → `OpenAI model not found (404): {text}`.
- `shatter-llm/src/google.rs:116-119`: `NOT_FOUND` → `Google Gemini model not found (HTTP 404): {text}`.
- `shatter-llm/src/anthropic.rs:106-122`: handles 429 and 401 specifically; everything else, including 404, becomes `Anthropic API HTTP {status}: {text}`.
- The CLI path (`shatter-cli/src/helpers.rs:1598-1640`, `build_oracle_adapter`) was not traced to the point where oracle call errors are reported.

## Acceptance criteria

- [ ] Reproduce first, and paste the transcripts into the issue before changing code: for each of anthropic, openai and google, run `shatter explore` with the LLM oracle enabled and a deliberately invalid model name (a mock HTTP server returning each provider's real 404 body is acceptable if no keys are available; state which was used). Record exactly what the user sees on stdout/stderr and in the run's error summary.
- [ ] Every adapter maps a model-rejection response to one shared error variant (e.g. `OracleError::ModelNotFound { provider, model }`) whose message names the model ID and the config key/flag that set it.
- [ ] The CLI surfaces that error once per run as a visible warning (or a hard error if the user explicitly asked for the oracle, whichever the maintainer picks; record the choice), rather than a generic adapter error or silence. Test with a mock server for at least the Anthropic adapter, shown failing on main.
- [ ] `cargo test -p shatter-llm -p shatter-cli` passes.

## Out of scope

- The user guide and the default-model refresh policy (note on str-qwua7.21, slug `qwua7-21-llm-seed-oracle-docs`).
- Changing the default model IDs.

## Size

S

## References

- Finding frontend-rust-18 (audit 2026-09-22). Split from the str-qwua7.21 note after the Codex cross-check.
