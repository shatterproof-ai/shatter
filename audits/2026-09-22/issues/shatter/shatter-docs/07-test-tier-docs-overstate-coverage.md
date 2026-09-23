---
slug: test-tier-docs-overstate-coverage
kind: new
title: "CLAUDE.md test-tier table overstates what each tier covers; check-fast's description and the CI snapshot claim are stale"
priority: P3
type: task
labels: [docs, quality-gates, taskfile, agents, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# CLAUDE.md test-tier table overstates what each tier covers; check-fast's description and the CI snapshot claim are stale

## Problem

The Test Tiers table in the root `CLAUDE.md` is the main thing agents use to pick a gate. It says what each tier is for, but not what it runs, and some of those claims overstate coverage. The `check-fast` tier describes itself as the pre-push gate, but it is not, and CLAUDE.md does not mention it. CLAUDE.md also claims CI verifies snapshots by running the full `task check`. That claim is false while CI's `task check` executes no test leaves, which is the checksum-poisoning bug.

## Evidence

Re-verified at 56c86168:

- **Standard tier.** The table (`CLAUDE.md:20-30`) labels Standard (`task test-standard`) "Before committing". In `Taskfile.yml:103-111`, `test-standard` is `deps: [frontends-built, workspace-clippy]` plus `task: workspace-test`, which is `cargo test --workspace` over core, cli and llm. It runs no TypeScript, Go or rust-frontend unit tests.
- **CI snapshot claim.** `CLAUDE.md:13` says "Regression snapshots are checked into the repo and verified in CI by `.github/workflows/ci.yml` (runs the full `task check` landing gate …)". While the task checksum-poisoning bug stands (str-qwua7.3, audit slug task-list-json-poisons-checksums), CI's `task check` reports its test leaves "up to date" and executes none of them.
- **check-fast.** `Taskfile.yml:188`ff. describes `check-fast` as "Fast quality gate (pre-push: clippy + tests + TS + Go; …)". `scripts/setup-hooks.sh:164-172` selects only `check` (`SHATTER_FULL_PUSH=1` or gate rank 2) or `affected` for pre-push, never `check-fast`. `check-fast` is absent from CLAUDE.md and appears only in the heavyweight-slot list at `AGENTS.md:540`. The body of str-35vtk.25 (open) also calls `task check-fast` the feature-ref pre-push policy.

## Acceptance criteria

- [ ] The tier table gains a "Covers" column listing, for each tier, the crates/frontends and test kinds it runs (unit, proptest, E2E, snapshot, clippy).
- [ ] `scripts/test_test_tier_wiring.py` validates that column against the Taskfile deps graph, so the table cannot drift again. Proof at close: the test fails when `workspace-test` is removed from `test-standard` (or when the table claims a frontend the tier does not run), and passes on the committed table.
- [ ] `check-fast` is resolved one of two ways:
  - It is documented in the CLAUDE.md table with an accurate description, and its `desc:` no longer says "pre-push".
  - Or it is removed, together with its `gate-wrapper.sh` wiring and the AGENTS.md:540 mention.

  Either way, str-35vtk.25's body is corrected to match what `scripts/setup-hooks.sh` actually selects.
- [ ] The CI/snapshot sentence at `CLAUDE.md:13` states what CI actually runs today. Restore the stronger claim only when ci-executed-leaf-guard has landed and a CI run URL shows test leaves executing.

## Suggested approach

Parse `Taskfile.yml` (and the included per-crate Taskfiles) in the wiring test, expand each tier's deps to leaf tasks, and map leaf names to a short coverage vocabulary. Keep the table's "Covers" cells in that vocabulary so the test can compare them.

## Out of scope

- E2E running twice in `pre-completion-e2e`. That belongs to collapse-test-tiers (report §15.1).
- Collapsing or renaming tiers (collapse-test-tiers). If that lands first, write the "Covers" column for the new tier set.
- Fixing the checksum poisoning itself (str-qwua7.3) and the CI executed-leaf guard (ci-executed-leaf-guard).

## Dependencies

- Blocked by: none. Only the CI-sentence restoration waits on ci-executed-leaf-guard.
- Related: collapse-test-tiers, ci-executed-leaf-guard, str-qwua7.2, str-qwua7.3, str-35vtk.25.

## Source

Audit 2026-09-22, findings gates-07 (partially confirmed; verifier corrected it to P3) and tests-ci-17 (confirmed, P3). Draft `shatter-docs-ui/21`, minus its e2e-duplication item. Evidence is in `audits/2026-09-22/areas/tests-ci.md`.
