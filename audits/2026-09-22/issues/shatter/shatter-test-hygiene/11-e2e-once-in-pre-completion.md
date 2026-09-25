---
slug: e2e-once-in-pre-completion
kind: new
title: "pre-completion-e2e runs E2E twice"
priority: P3
type: task
labels: [quality-gates, taskfile, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# pre-completion-e2e runs E2E twice

## Problem

`pre-completion-e2e` runs the three shatter-core E2E concolic suites twice when caches are cold (the common case after an edit): once inside `check` via `core:test-ignored --run-ignored all`, and again in `task e2e`. The e2e subtasks are checksum-cached, so the second run is skipped only when their sources are unchanged since their last run. This issue was split out of `collapse-test-tiers` so that the cheap dedupe is not held behind that issue's blockers and maintainer tier decision. It owns the "E2E runs twice in pre-completion-e2e" item that other buckets (test-tier-docs-overstate-coverage, affected-gates-routing) point at.

## Evidence

Checked against `origin/main` 70465921 (2026-09-23).

- `Taskfile.yml:673-678` `pre-completion-e2e` runs `task: check`, then `task: e2e`, then `git status --short`.
- `check` → `check-governed` (:500) → `check-integration` (:564), which runs `core:test-ignored` (:569).
- `shatter-core/Taskfile.yml:63` (`test-ignored`, nextest path): `cargo nextest run -p shatter-core --run-ignored all -E 'not binary(bench_frontier_ranking)'`; `:65` is the `cargo test -- --include-ignored` fallback. The filter excludes only `bench_frontier_ranking`, so `e2e_concolic`, `e2e_concolic_go` and `e2e_concolic_rust` run here.
- `task e2e` (:577) → `e2e-ts`/`e2e-go`/`e2e-rust`, which run `cargo test --test e2e_concolic{,_go,_rust} -- --include-ignored` (:609, :628, :646). `task e2e` took 193 s in the audit's gate run.
- `Taskfile.yml:504-506`: nested task calls do not propagate `task --force` into the stages. `scripts/gate-wrapper.sh` records outer gate names, not individual test binaries. So neither `--force` on the outer task nor the gate-wrapper log can prove how many times a binary ran.
- Audit finding gates-07 (the E2E duplication part, verified).

## Acceptance criteria

- [ ] Each of `e2e_concolic`, `e2e_concolic_go` and `e2e_concolic_rust` is executed exactly once per `task pre-completion-e2e` run. Either exclude those binaries from `core:test-ignored` (and `core:test-ignored-fast`, if the same reasoning applies to `check-fast`) and keep them in `task e2e`, or drop `e2e` from `pre-completion-e2e`. Whichever is chosen, `task check` alone must still run the E2E suites or the CLAUDE.md tier table must say it does not.
- [ ] `task e2e` remains usable standalone (CLAUDE.md "E2E gate").
- [ ] Proof at close, with caches demonstrably cold:
  1. Invalidate the checksum-cached tasks involved by a method that is shown to work: e.g. touch a file listed in the `sources:` of `core:test-ignored`, `e2e-ts`, `e2e-go` and `e2e-rust` on a scratch commit, then paste `task --status core:test-ignored e2e-ts e2e-go e2e-rust` exiting non-zero (not up to date) before the run. (Deleting `.task/checksum` is not sufficient on its own; see str-qwua7.3.)
  2. Run `task pre-completion-e2e` capturing the full output to a file, and paste the test-runner lines showing each `e2e_concolic*` binary started once (e.g. nextest `PASS [...] shatter-core::e2e_concolic ...` lines, or cargo's `Running tests/e2e_concolic*.rs` lines, with a `grep -c` per binary).
  3. Paste the same count from a run on the pre-fix commit, showing 2 per binary (red then green).
- [ ] If `core:test-ignored`'s filter changes, a comment at the filter says where the E2E suites run instead, and `scripts/test_test_tier_wiring.py` (or a new wiring test) asserts the exclusion so it cannot silently regress.
- [ ] The CLAUDE.md Test Tiers table is updated if what `check` covers changes.
- [ ] `task affected` passes, and its `Gates selected` output is recorded.

## Out of scope

- Redesigning the tier set or cache identity (`collapse-test-tiers`).
- Documentation accuracy of the tier table beyond this change (test-tier-docs-overstate-coverage).

## Priority / type / labels

P3 · task · quality-gates, taskfile, audit · Size S

## Parent epic

Epic: Audit 2026-09-22 findings (shatter)

## Dependencies

- Blocked by: none.
- Related: `collapse-test-tiers`, str-qwua7.3 (checksum invalidation).
