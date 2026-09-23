---
slug: collapse-test-tiers
kind: new
title: "Gate tier sprawl and body duplication"
priority: P3
type: task
labels: [quality-gates, taskfile, refactor, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [task-list-json-poisons-checksums, task-sources-cover-real-inputs]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Gate tier sprawl and body duplication

## Problem

Gate tiers have piled up, one per efficiency issue, and nobody prunes them. Agents and humans have to choose among about 20 overlapping entry points. Four pairs of Task bodies are copy-pasted only so that each copy gets its own go-task checksum identity, and a wiring test exists just to keep the copies in sync.

This issue has two deliverables that share one design decision (which tiers exist determines which cache identities are needed), so they stay together; the design is agreed before any Taskfile change. The E2E double-run that an earlier draft bundled here is now `e2e-once-in-pre-completion`, which is unblocked and independent.

## Evidence

Checked against `origin/main` 70465921 (2026-09-23).

- Root `Taskfile.yml` test/gate entry points: `test` (:87), `test-quick` (:95), `test-standard` (:103), `check-fast` (:188), `check` (:491), `affected` (:511), `pre-completion` (:667), `pre-completion-e2e` (:673), `e2e`/`e2e-ts`/`e2e-go`/`e2e-rust` (:577-640), `smoke` (:648), `walkthrough`/`walkthrough-cold` (:680-696), `gauntlet`/`gauntlet-cold` (:701-717), `golden-test` (:312), `parity` (:245), `conformance` (:225), `broad-run-corpus` (:722), `broad-run-validation` (:897), `drift-patrol` (:278). Most also have a `*-governed` twin.
- Duplicated bodies that exist only for cache identity:
  - `Taskfile.yml:134-139` above `workspace-test-quick`: "Keep this body in sync with workspace-test above. Its distinct checksum identity protects the reduced property/fuzz budget. Task does not fingerprint caller-provided environment variables, so using workspace-test here could let a quick result satisfy test-standard later."
  - `shatter-cli/Taskfile.yml:33` `test` / `test-fast` ("Keep this body in sync with test above").
  - `shatter-core/Taskfile.yml:37` `test-ignored` / `:71` `test-ignored-fast` (comment at :68-70).
  - `shatter-ts/Taskfile.yml:38` / `:55` `test` / `test-fast`.
  - Parity is enforced by `scripts/test_test_tier_wiring.py`.
- `Taskfile.yml:504-506`: nested task calls do not propagate `task --force` into stages, so "forced" outer runs can still serve cached stages.
- Related open issue str-nl1g (P2, verified open 2026-09-23) proposes yet another tier (a seconds-level live-path tier). That pulls the other way and should be reconciled here.
- Audit finding tests-ci-13 (verified, lowered to P3: the cost is maintenance, not incorrect behaviour).

## Acceptance criteria

- [ ] A proposed tier set (for example dev / affected / check / release, with what each covers) is written into this issue and agreed by the maintainer (recorded as a comment) before any Taskfile change. It says what happens to each current entry point (kept, aliased or deleted) and how str-nl1g fits.
- [ ] Before relying on any alternative cache-identity mechanism, a small recorded experiment shows how go-task names checksum state (task name vs `label:`, including templated labels) and that two variants with different budgets get distinct, non-interchangeable up-to-date status: run variant A, then show `task --status` for variant B still reports not up to date.
- [ ] Cache identity for fast-budget variants no longer relies on copy-pasted bodies. `scripts/test_test_tier_wiring.py`'s body-parity checks are deleted or reduced to what still applies, and a replacement test asserts the property the duplication protected (a quick-budget result cannot satisfy the standard-budget task).
- [ ] Every entry point marked "deleted" in the agreed set is gone, and every "aliased" one delegates to its target. `task --list` output before and after is pasted.
- [ ] The CLAUDE.md Test Tiers table, `/pre-completion` skill and `task affected` selection are updated to the new set in the same change.
- [ ] `task check` passes on the final branch with each stage actually executed: force each stage directly (`task --force check-static`, `task --force check-unit`, `task --force check-integration`, per the `Taskfile.yml:504-506` note) and attach the logs.

## Suggested approach

Do this after the checksum-poisoning fix (str-qwua7.3, via task-list-json-poisons-checksums) and task-sources-cover-real-inputs. Until then, cache behaviour is too unreliable to judge which tiers are redundant.

## Out of scope

- The E2E double-run (`e2e-once-in-pre-completion`).
- Documentation accuracy of the tier table beyond updating it to the new set (test-tier-docs-overstate-coverage, shatter-docs bucket).
- The broad-run duplicate gates (broad-run-gate-duplicates).
- CI workflow restructuring.

## Priority / type / labels

P3 · task (refactor) · quality-gates, taskfile, refactor, audit · Size M

## Parent epic

Epic: Audit 2026-09-22 findings (shatter)

## Dependencies

- Blocked by: `task-list-json-poisons-checksums` (this slug is a note on existing str-qwua7.3, so the real blocker is str-qwua7.3, verified open 2026-09-23) and `task-sources-cover-real-inputs`.
- Related: str-nl1g (open), str-35vtk (tier epic), `e2e-once-in-pre-completion`, `test-tier-docs-overstate-coverage`, `broad-run-gate-duplicates`.
