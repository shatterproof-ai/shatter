# Planning rules in CLAUDE.md: plan/spec location + status banner, and check open tracker decisions before planning

- Priority: P2
- Type: task
- Labels: agents,docs,governance
- Tracker: shatter (bd, /home/ketan/project/shatter)
- Relation: new (partially covered by str-qwua7.44; append note to str-qwua7.43)
- Source findings: docs-12, frontend-rust-09 (process part)
- Parent: 01 (epic)
- Blocked by: none
- Readiness: drafted to the issue-readiness-check standard; fresh-reviewer precheck still required before filing (see INDEX.md)

<!-- body -->
## Problem
Two planning failures traced to missing repo-level rules:
1. The superpowers writing-plans skill defaults to `docs/superpowers/plans/`
   unless the repo states a preference; shatter states none, so the duplicate
   plan tree keeps growing (a 1,694-line orphan plan landed 2026-09-21, no
   Status banner, 43 unticked boxes after its epic closed).
2. That plan instructed "add shatter-llm under shatter-core
   [dev-dependencies]", directly contradicting open decided issue
   str-qwua7.43 (remove the core→shatter-llm dev-dep cycle). No step checks
   open tracker decisions touching the files or dependency edges a plan changes.

## Current Code Facts
- `docs/superpowers/plans/2026-09-21-jev-frontier-ranking-benchmark.md`
  (line ~1037: "Modify: shatter-core/Cargo.toml — add shatter-llm ... under
  [dev-dependencies]"); str-hjrnp.4 closed.
- `shatter-core/Cargo.toml:44` has shatter-llm as dev-dep; both
  `shatter-core/tests/bench_frontier_ranking.rs` and `e2e_llm_oracle.rs` use it.
- CLAUDE.md and AGENTS.md contain no plan-location rule (no match for
  `superpowers` or `docs/plans`).
- str-qwua7.44 (open) will merge `docs/superpowers/{plans,specs}` into
  `docs/{plans,specs}` and add Status banners.

## Acceptance Criteria
- CLAUDE.md states: plans go in `docs/plans/`, design specs in `docs/specs/`,
  each starting with `Status: draft|approved|implemented (str-x)|superseded-by`.
  This overrides the superpowers default.
- CLAUDE.md (or AGENTS.md) planning rule: before writing a plan, run
  `bd search` for each file/crate/dependency edge the plan modifies and list
  any open issue or decision it contradicts in the plan header.
- The 2026-09-21 plan gets a Status banner (implemented, str-hjrnp).
- Note appended to str-qwua7.43: add `shatter-core/tests/bench_frontier_ranking.rs`
  to the move-out-of-core scope.

## Out of Scope
Moving the existing tree (str-qwua7.44).
