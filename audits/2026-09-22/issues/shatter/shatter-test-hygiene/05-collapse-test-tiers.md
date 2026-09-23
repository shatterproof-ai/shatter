---
slug: collapse-test-tiers
kind: new
title: "Collapse ~20 overlapping gate tiers; stop duplicating task bodies for checksum identity; E2E runs twice in pre-completion-e2e"
priority: P3
type: task
labels: [quality-gates, taskfile, refactor, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [task-list-json-poisons-checksums, task-sources-cover-real-inputs]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Collapse ~20 overlapping gate tiers; stop duplicating task bodies for checksum identity; E2E runs twice in pre-completion-e2e

## Problem

Gate tiers have piled up, one per efficiency issue, and nobody prunes them. Agents and humans have to choose among about 20 overlapping entry points. Four pairs of Task bodies are copy-pasted only so that each copy gets its own go-task checksum identity, and a wiring test exists just to keep the copies in sync. `pre-completion-e2e` runs the shatter-core E2E suites twice: once inside `check` (via `core:test-ignored --run-ignored all`) and again in `e2e`.

This issue owns the "E2E runs twice in pre-completion-e2e" item. It was removed from test-tier-docs-overstate-coverage (docs-ui/21) so that it is filed only once.

## Evidence

Checked against `origin/main` 70465921 (2026-09-23).

- Root `Taskfile.yml` test/gate entry points: `test` (:87), `test-quick` (:95), `test-standard` (:103), `check-fast` (:188), `check` (:491), `affected` (:511), `pre-completion` (:667), `pre-completion-e2e` (:673), `e2e`/`e2e-ts`/`e2e-go`/`e2e-rust` (:577-640), `smoke` (:648), `walkthrough`/`walkthrough-cold` (:680-696), `gauntlet`/`gauntlet-cold` (:701-717), `golden-test` (:312), `parity` (:245), `conformance` (:225), `broad-run-corpus` (:722), `broad-run-validation` (:897), `drift-patrol` (:278). Most also have a `*-governed` twin.
- Duplicated bodies that exist only for cache identity:
  - `Taskfile.yml:134-139` above `workspace-test-quick`: "Keep this body in sync with workspace-test above. Its distinct checksum identity protects the reduced property/fuzz budget. Task does not fingerprint caller-provided environment variables, so using workspace-test here could let a quick result satisfy test-standard later."
  - `shatter-cli/Taskfile.yml:33` `test` / `test-fast` ("Keep this body in sync with test above").
  - `shatter-core/Taskfile.yml:35-36`, `:62-64` `test-ignored` / `test-ignored-fast`.
  - `shatter-ts/Taskfile.yml:38` / `:55` `test` / `test-fast`.
  - Parity is enforced by `scripts/test_test_tier_wiring.py`.
- E2E duplication: `Taskfile.yml:673-678` `pre-completion-e2e` runs `task: check` then `task: e2e`. `check` stage 3 (`check-integration`, :564) runs `core:test-ignored`, which on main is `cargo nextest run -p shatter-core --run-ignored all -E 'not binary(bench_frontier_ranking)'` (`shatter-core/Taskfile.yml:63`). That filter still includes `e2e_concolic.rs`, `e2e_concolic_go.rs` and `e2e_concolic_rust.rs`, which `task e2e` (193 s in the audit's gate run) then runs again.
- Related open issue str-nl1g proposes yet another tier (a seconds-level live-path tier). That pulls the other way and should be reconciled here.
- Audit findings tests-ci-13 (verified, lowered to P3: the cost is maintenance, not incorrect behaviour) and gates-07 (the E2E duplication part, verified).

## Acceptance criteria

- [ ] A proposed tier set (for example dev / affected / check / release, with what each covers) is written into this issue and agreed by the maintainer before any Taskfile change. It says what happens to each current entry point (kept, aliased or deleted) and how str-nl1g fits.
- [ ] Cache identity for fast-budget variants no longer relies on copy-pasted bodies. Before relying on it, verify how go-task names checksum state (task name vs `label:`, including templated labels) with a small experiment, and record the result here. After the change, `scripts/test_test_tier_wiring.py`'s body-parity checks are deleted or reduced to what still applies.
- [ ] The shatter-core E2E binaries run exactly once in `pre-completion-e2e`. Either drop `e2e` from `pre-completion-e2e` or exclude the `e2e_concolic*` binaries from `core:test-ignored`, keeping `task e2e` usable standalone as CLAUDE.md requires. Proof at close: a forced run (`task pre-completion-e2e --force`, or a cleared `.task/` checksum dir) whose gate-wrapper log shows each `e2e_concolic*` binary executed once.
- [ ] The CLAUDE.md Test Tiers table and `/pre-completion` skill are updated to the new tier set in the same change.
- [ ] `task check` passes after the collapse (forced execution, not a cached pass; see the gate-cache caveat in project memory), and its log is attached.

## Suggested approach

Do this after the checksum-poisoning fix (str-qwua7.3, via task-list-json-poisons-checksums) and task-sources-cover-real-inputs. Until then, cache behaviour is too unreliable to judge which tiers are redundant. Start with the E2E duplication, which is a one-line change and does not need the tier decision.

## Out of scope

- Documentation accuracy of the tier table beyond updating it to the new set (test-tier-docs-overstate-coverage, shatter-docs bucket).
- The broad-run duplicate gates (broad-run-gate-duplicates).
- CI workflow restructuring.

## Priority / type / labels

P3 · task (refactor) · quality-gates, taskfile, refactor, audit · Size M

## Parent epic

Epic: Audit 2026-09-22 findings (shatter)

## Dependencies

- Blocked by: `task-list-json-poisons-checksums` (this slug is a note on existing str-qwua7.3, so the real blocker is str-qwua7.3) and `task-sources-cover-real-inputs`.
- Related: str-nl1g (open), str-35vtk (tier epic), `test-tier-docs-overstate-coverage`, `broad-run-gate-duplicates`.
