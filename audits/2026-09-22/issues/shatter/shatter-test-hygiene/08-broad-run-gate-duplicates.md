---
slug: broad-run-gate-duplicates
kind: new
title: "Broad-run gates duplicated, unscheduled"
priority: P3
type: chore
labels: [quality-gates, cleanup, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Broad-run gates duplicated, unscheduled

## Problem

Two Task gates cover the same purpose, validating shatter against a Kapow-derived broad-run failure-class corpus, with different driver scripts, different corpora and different assertion sets. They are overlapping, not identical, so deleting either one without porting its assertions would silently drop regression coverage. Neither is wired into `check`, `affected`, `ci.yml` or any scheduled workflow, so the corpus can rot unnoticed and nobody knows which one is authoritative. One of them (`broad-run-corpus`) has not been touched since 2026-05-13.

## Evidence

Checked against `origin/main` 70465921 (2026-09-23).

- `Taskfile.yml:722-738` `broad-run-corpus` ("Run the broad-run validation corpus gate (Kapow failure-class fixtures)"). Its sources are `tests/fixtures/broad-run-corpus/**/*` and `tests/scripts/broad_run_validation.py`, and it runs `python3 tests/scripts/broad_run_validation.py`. Last commit touching either input: 630e8ccb, 2026-05-13.
- `Taskfile.yml:897-917` `broad-run-validation` ("Broad-run validation corpus gate (str-jeen.14). Documented local check; not in CI."). Its sources are `tests/broad-run-corpus/**/*`, `scripts/broad_run_validation_gate.py` and four `examples/go/*` dirs, and it runs `python3 scripts/broad_run_validation_gate.py --corpus tests/broad-run-corpus/manifest.yaml -v`. `broad-run-validation-tests` at `:919` runs `python3 -m unittest scripts.test_broad_run_validation_gate`. Last commit touching the corpus or driver: a14370ef, 2026-05-02.
- The two drivers check different invariants:
  - `tests/scripts/broad_run_validation.py` (docstring :2-22): (1) denominator integrity, `completed + failed + skipped + unsupported == attempted` and `attempted >= min_attempted` (`assert_denominator`, :124); (2) every referenced `file_path` exists (`assert_artifacts_exist`, :188); (3) failure-class presence via pinned stderr regex or `failed[].reason` (`assert_failure_reason_present`, :196); (4) stale-source detection: scan with a transient file, delete it, rescan, assert no artifact references the deleted path (`run_stale_source_phase`, :278).
  - `scripts/broad_run_validation_gate.py` (docstring :2-23, `assert_fixture` from :292): per-fixture min/max/range thresholds on report counts (`compare_min`/`compare_max`/`compare_range`), `run_must_succeed`, `artifact_paths_must_resolve`, `no_target_reasons` checks, and `tighten_when:` ratchet notes; its corpus has its own fixture set (e.g. `stale-source-go`, `ts-browser-globals`, `mixed-rust-frontend`) that does not match the older corpus's (`dangling-artifacts`, `no-target-categories`, `rust-unavailable`, `source-churn`, `ts`).
  Neither driver's assertions are a superset of the other's.
- `docs/validation/broad-run-corpus.md` documents only `tests/broad-run-corpus/` + `task broad-run-validation`.
- `grep -n broad Taskfile.yml .github/workflows/*.yml` finds no reference from `check`, `affected`, `ci.yml`, `drift-patrol.yml` or `perf-ci.yml`.
- Audit finding tests-ci-16 (verified, P3). str-jeen.14 (closed) created the corpus.

## Acceptance criteria

- [ ] Before any deletion, the issue contains an assertion-by-assertion comparison table: each assertion and each fixture of both drivers, mapped to where the survivor covers it (existing check, ported check, or ported fixture) or marked "dropped".
- [ ] Every "dropped" row has explicit maintainer approval recorded as an issue comment. Without approval, the assertion is ported. In particular, denominator integrity and stale-source disappearance (older driver) and threshold ratchets (newer driver) are not dropped by default.
- [ ] One gate is kept (probably `broad-run-validation`, the documented one), with the ported assertions and fixtures. Only then are the other gate's Task entry, driver script and corpus directory deleted.
- [ ] Proof that ported assertions can fail (red then green): for each ported assertion class, show the survivor failing on a scratch change that breaks it (e.g. a corrupted fixture report or a manifest threshold set impossibly high), then passing on the final branch.
- [ ] The survivor is either scheduled (nightly or weekly workflow, or added to drift-patrol's cadence) or explicitly documented as manual-only, with the reason, in `docs/validation/broad-run-corpus.md` and the CLAUDE.md tier table.
- [ ] If scheduled, proof at close: the URL of a green scheduled or `workflow_dispatch` run. If manual-only, proof at close: a forced local run log (`task broad-run-validation --force`) showing it executes and passes on current main.
- [ ] `broad-run-validation-tests` stays wired to whatever gate runs the Python meta tests.
- [ ] `task affected` passes, and its `Gates selected` output is recorded.

## Suggested approach

Build the comparison table first; do not assume the newer corpus absorbed the older one (their assertion sets differ, see Evidence). If a schedule is chosen, drift-patrol's weekly workflow is the cheapest home (see `docs/DRIFT-PATROL.md`).

## Out of scope

- Adding new failure-class fixtures.
- The general tier collapse (`collapse-test-tiers`), although the deletion here reduces that list.

## Priority / type / labels

P3 · chore · quality-gates, cleanup, audit · Size M

## Parent epic

Epic: Audit 2026-09-22 findings (shatter)

## Dependencies

- Blocked by: none.
- Related: str-jeen.14 (closed), `collapse-test-tiers`.
