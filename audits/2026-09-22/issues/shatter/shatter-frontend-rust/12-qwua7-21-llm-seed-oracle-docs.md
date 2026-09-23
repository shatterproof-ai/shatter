---
slug: qwua7-21-llm-seed-oracle-docs
kind: note-to-existing
title: "NOTE on str-qwua7.21: add an LLM seed-oracle user guide and a default-model-ID update policy"
priority: P3
type: task
labels: [docs, llm, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-qwua7.21
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# NOTE on str-qwua7.21: add an LLM seed-oracle user guide and a default-model-ID update policy

Target: **str-qwua7.21** (open user-docs epic). Action: post the comment below with `bd comments add str-qwua7.21 ...`. If the maintainer prefers a tracked child, file the "Proposed child" block as a new issue with `--parent str-qwua7.21` (P3, type task, labels docs,llm) instead of, or as well as, the comment. There is no old draft for this item; it comes from report §15.1 and finding frontend-rust-18.

## Comment text

> **Audit 2026-09-22 note** (finding frontend-rust-18). The LLM seed oracle has no user documentation, and its default model IDs are hard-coded without an update policy. Neither .21.1 (config reference, which mentions LLM config keys) nor str-qwua7.20.2 (SHATTER_* env-var table) covers this.
>
> **Current state (re-verified at 56c86168):**
> - The only user-facing mention is `SPEC.md:201`: "`explore` also accepts LLM seed-oracle overrides (`--llm`, `--llm-adapter`, `--llm-token-budget`)." `README.md` and `QUICKSTART.md` do not mention the LLM oracle or `SHATTER_ANTHROPIC_API_KEY`.
> - Adapters (anthropic/openai/google/custom/local), config keys (`llm.anthropic.api_key`, `llm.custom`, `llm.local`), `SHATTER_ANTHROPIC_API_KEY`, token budgets, and the fact that **target source code is sent to third-party APIs** are described only in `docs/superpowers` design specs.
> - Default models are hard-coded in `shatter-core/src/config.rs:338` (`claude-sonnet-4-6`), `:376` (`gpt-4o`), `:392` (`gemini-2.0-flash`). Nothing records when or how they are refreshed, and nothing warns when a provider rejects a retired model. The adapter list (anthropic/openai/google/custom/local) is documented only in a doc comment at `shatter-cli/src/helpers.rs:1598-1605` (`build_oracle_adapter`).
>
> **Proposed child (acceptance criteria):**
> - `docs/llm-oracle.md` user guide: what the oracle does and when it helps; enabling it (`--llm`, `--llm-adapter`, `--llm-token-budget`, config keys, env vars); **data egress** (what source/context is sent to which provider); cost controls (token budget, replay/cache); local/custom adapters for no-egress use. Linked from README and QUICKSTART.
> - A written default-model refresh policy (who updates the IDs in `config.rs`, when, and how the change is tested), placed next to the defaults or in the guide.
> - `shatter doctor` (or the first oracle call) reports a clear warning when the configured or default model is rejected by the provider, rather than a generic adapter error.
> - Proof at close: guide rendered and linked, and a doctor/explore run with a deliberately invalid model name showing the warning.
