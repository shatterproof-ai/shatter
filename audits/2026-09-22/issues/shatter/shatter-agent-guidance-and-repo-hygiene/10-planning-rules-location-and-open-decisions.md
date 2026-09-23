---
slug: planning-rules-location-and-open-decisions
kind: new
title: "Planning rules in CLAUDE.md: plans/specs go in docs/plans and docs/specs (overriding the superpowers default), and check open tracker decisions before planning"
priority: P2
type: task
labels: [agents, docs, governance, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Planning rules in CLAUDE.md: plans/specs go in docs/plans and docs/specs (overriding the superpowers default), and check open tracker decisions before planning

## Problem

Two planning failures trace back to rules the repo never states:

1. **Plan location.** The superpowers `writing-plans` skill saves plans to
   `docs/superpowers/plans/` "unless user preferences for plan location
   override this default". Shatter states no preference, so the duplicate
   plan tree keeps growing. On 2026-09-21 a 1,694-line plan landed there with
   no Status banner. Its epic has since closed, it still has 43 unticked
   boxes, and nothing links to it. str-qwua7.44 will merge the existing trees
   and define the Status-banner convention, but it does not add the rule that
   stops the default path from recreating the tree.
2. **Open decisions.** The core -> shatter-llm dev-dependency already existed
   (added 2026-05-25 in 4db63be3 for `e2e_llm_oracle.rs`), and str-qwua7.43
   (open, decided) removes that edge. The 2026-09-21 plan nevertheless told
   the implementer to rely on it for a **second** consumer,
   `bench_frontier_ranking.rs` (plan `:1037`: "add `shatter-llm` ... under
   `[dev-dependencies]`"), deepening an edge a decided issue is removing. No
   planning step checks open tracker decisions that touch the files or
   dependency edges a plan changes.

## Ownership (one owner per deliverable)

- **This issue:** the two CLAUDE.md rules (location override; open-decision
  check).
- **str-qwua7.44:** the Status-banner convention and its value set, moving
  `docs/superpowers/{plans,specs}`, banners on existing files (including the
  2026-09-21 plan), and the orphan check. This issue references that
  convention and does not define its own. The companion comment below adds
  the 2026-09-21 plan to .44's banner list.
- **str-qwua7.43:** moving `bench_frontier_ranking.rs` out of core and
  dropping the edge; already covered by the audit note
  `qwua7-43-bench-dev-dep-cycle` (bucket shatter-concolic-and-engine-design).

## Evidence

Re-verified 2026-09-23 in `/home/ketan/.local/share/worktrees/shatter/audit-2026-09-22`:

- `docs/superpowers/plans/2026-09-21-jev-frontier-ranking-benchmark.md`:
  1,694 lines; `grep -c -- '- \[ \]'` -> 43; no `Status:` line; `:1037`
  "Modify: `shatter-core/Cargo.toml` — add `shatter-llm = { path = "../shatter-llm" }` under `[dev-dependencies]`".
  Its issue str-hjrnp.4 is closed.
- `shatter-core/Cargo.toml:44` `shatter-llm = { path = "../shatter-llm" }`
  (dev-dep, first added in 4db63be3, 2026-05-25). Two test files use it:
  `shatter-core/tests/e2e_llm_oracle.rs` and
  `shatter-core/tests/bench_frontier_ranking.rs`.
- `grep -nE 'superpowers|docs/plans|docs/specs' CLAUDE.md AGENTS.md` -> no
  matches.
- Both `docs/plans/` and `docs/specs/` exist, alongside `docs/superpowers/`.
- `bd show str-qwua7.44` (open): acceptance requires "docs carry
  `Status: current | draft | approved | implemented (str-xxxx) | deferred |
  superseded-by <path>`; every existing orphan gets one" and merging
  `docs/superpowers/{plans,specs}` into `docs/{plans,specs}`. It has no
  CLAUDE.md location rule.
- Audit sources: docs-12, frontend-rust-09 (process part)
  (`audits/2026-09-22/findings.json`, `audits/2026-09-22/areas/docs.md`).

## Acceptance criteria

1. CLAUDE.md states, in two or three lines: new plans go in `docs/plans/`,
   design specs in `docs/specs/`; this overrides the superpowers default
   location; each new plan/spec starts with a `Status:` line using the
   convention defined by str-qwua7.44 (link the issue, or the doc that .44
   lands, rather than restating the value list).
2. CLAUDE.md (or AGENTS.md, if str-qwua7.23's byte budget allows) gains a
   planning rule: before writing a plan, run `bd search` for each file,
   crate and dependency edge the plan modifies, and list in the plan header
   any open issue or recorded decision it contradicts (or "none found",
   with the searches run).
3. **Rule-effect proof in the close reason:** in a scratch session, invoke
   the superpowers `writing-plans` skill for a throwaway plan and record the
   path it writes to (`docs/plans/...`, not `docs/superpowers/plans/...`) and
   that the plan header contains the open-decisions line. Delete the
   throwaway plan afterwards.
4. `grep -nE 'docs/plans|superpowers' CLAUDE.md` shows the rule (output in
   the close reason); `task affected` passes, with `Gates selected` recorded.

## Suggested approach

Put both rules in CLAUDE.md's Agent Workflow section. str-qwua7.23 is cutting
AGENTS.md, so do not grow it.

## Out of scope

- The Status-banner convention, moving the existing `docs/superpowers/` tree,
  banners on existing plans, and the orphan check (str-qwua7.44).
- Removing the core->shatter-llm dev-dependency (str-qwua7.43, via
  `qwua7-43-bench-dev-dep-cycle`).
- A dotfiles-level default plan location for all repos.

## Dependencies

None. Related: str-qwua7.44, str-qwua7.43, str-qwua7.23.

## Comment for `str-qwua7.44`

> Audit 2026-09-22 (finding docs-12): please include
> `docs/superpowers/plans/2026-09-21-jev-frontier-ranking-benchmark.md`
> (1,694 lines, 43 unticked boxes, no `Status:` line; its issue str-hjrnp.4
> is closed) in this issue's banner pass, as
> `Status: implemented (str-hjrnp)`, at its moved path. The CLAUDE.md rule
> that stops the superpowers default from recreating `docs/superpowers/` is
> filed separately as <planning-rules-location-and-open-decisions>; it
> references this issue's banner convention instead of defining one.
