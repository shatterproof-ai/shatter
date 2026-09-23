---
slug: planning-rules-location-and-open-decisions
kind: new
title: "Planning rules in CLAUDE.md: plan/spec location + Status banner, and check open tracker decisions before planning"
priority: P2
type: task
labels: [agents, docs, governance, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Planning rules in CLAUDE.md: plan/spec location + Status banner, and check open tracker decisions before planning

## Problem

Two planning failures trace back to rules the repo never states:

1. **Plan location.** The superpowers `writing-plans` skill saves plans to
   `docs/superpowers/plans/` "unless user preferences for plan location
   override this default". Shatter states no preference, so the duplicate
   plan tree keeps growing. On 2026-09-21 a 1,694-line plan landed there with
   no Status banner. Its epic has since closed, it still has 43 unticked
   boxes, and nothing links to it.
2. **Open decisions.** That plan told the implementer to "add shatter-llm
   under shatter-core [dev-dependencies]". This directly contradicts the open,
   decided issue str-qwua7.43, which removes the core→shatter-llm dev-dependency
   cycle. No planning step checks open tracker decisions that touch the files
   or dependency edges a plan changes.

## Evidence

Re-verified 2026-09-23 in `/home/ketan/.local/share/worktrees/shatter/audit-2026-09-22`:

- `docs/superpowers/plans/2026-09-21-jev-frontier-ranking-benchmark.md`:
  1,694 lines; `grep -c -- '- \[ \]'` -> 43; no `Status:` line; `:1037`
  "Modify: `shatter-core/Cargo.toml` — add `shatter-llm = { path = "../shatter-llm" }` under `[dev-dependencies]`".
  Its issue str-hjrnp.4 is closed.
- `shatter-core/Cargo.toml:44` `shatter-llm = { path = "../shatter-llm" }`
  (dev-dep). Two test files use it: `shatter-core/tests/bench_frontier_ranking.rs`
  and `shatter-core/tests/e2e_llm_oracle.rs`.
- `grep -nE 'superpowers|docs/plans|docs/specs' CLAUDE.md AGENTS.md` -> no
  matches.
- Both `docs/plans/` and `docs/specs/` exist, alongside `docs/superpowers/`.
- str-qwua7.44 (open) will merge `docs/superpowers/{plans,specs}` into
  `docs/{plans,specs}` and add Status banners plus an orphan check. It does not
  add the CLAUDE.md rule that stops the default path from recreating the tree.
- Audit sources: docs-12, frontend-rust-09 (process part)
  (`audits/2026-09-22/findings.json`, `audits/2026-09-22/areas/docs.md`).

## Acceptance criteria

1. CLAUDE.md states: plans go in `docs/plans/`, design specs in `docs/specs/`,
   and each starts with
   `Status: draft | approved | implemented (str-x) | superseded-by <path>`.
   It says explicitly that this overrides the superpowers default location.
2. CLAUDE.md (or AGENTS.md) gains a planning rule: before writing a plan, run
   `bd search` for each file, crate and dependency edge the plan modifies,
   and list any open issue or recorded decision it contradicts in the plan
   header (or "none found", with the searches run).
3. `docs/superpowers/plans/2026-09-21-jev-frontier-ranking-benchmark.md`
   gets `Status: implemented (str-hjrnp)`. If str-qwua7.44 has landed first,
   the banner goes on the file at its moved path.
4. A comment is appended to str-qwua7.43: add
   `shatter-core/tests/bench_frontier_ranking.rs` (introduced by the
   2026-09-21 plan) to the move-out-of-core scope, next to `e2e_llm_oracle.rs`.
5. Proof in the close reason: `grep -n 'docs/plans' CLAUDE.md` shows the rule,
   and the str-qwua7.43 comment id is cited.

## Suggested approach

Keep both rules to two or three lines each in CLAUDE.md's Code Quality or
Agent Workflow section. str-qwua7.23 is cutting AGENTS.md, so do not grow it.

## Out of scope

- Moving the existing `docs/superpowers/` tree and adding the orphan check
  (str-qwua7.44).
- Removing the core→shatter-llm dev-dependency (str-qwua7.43).
- A dotfiles-level default plan location for all repos.

## Dependencies

None. Related: str-qwua7.44, str-qwua7.43.
