---
slug: broad-run-gate-duplicates
kind: new
title: "Two duplicate broad-run validation gates (different scripts and corpora); neither in check, affected, CI or any schedule"
priority: P3
type: chore
labels: [quality-gates, cleanup, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Two duplicate broad-run validation gates (different scripts and corpora); neither in check, affected, CI or any schedule

## Problem

Two Task gates do the same job, validating shatter against the Kapow-derived broad-run failure-class corpus, with different driver scripts and different corpora. Neither is wired into `check`, `affected`, `ci.yml` or any scheduled workflow, so the corpus can rot unnoticed and nobody knows which one is authoritative. One of them (`broad-run-corpus`) has not been touched since 2026-05-13.

## Evidence

Checked against `origin/main` 70465921 (2026-09-23).

- `Taskfile.yml:722-738` `broad-run-corpus` ("Run the broad-run validation corpus gate (Kapow failure-class fixtures)"). Its sources are `tests/fixtures/broad-run-corpus/**/*` and `tests/scripts/broad_run_validation.py`, and it runs `python3 tests/scripts/broad_run_validation.py`. Last commit touching either input: 630e8ccb, 2026-05-13.
- `Taskfile.yml:897-917` `broad-run-validation` ("Broad-run validation corpus gate (str-jeen.14). Documented local check; not in CI."). Its sources are `tests/broad-run-corpus/**/*`, `scripts/broad_run_validation_gate.py` and four `examples/go/*` dirs, and it runs `python3 scripts/broad_run_validation_gate.py --corpus tests/broad-run-corpus/manifest.yaml -v`. `broad-run-validation-tests` at `:919` runs `python3 -m unittest scripts.test_broad_run_validation_gate`. Last commit touching the corpus or driver: a14370ef, 2026-05-02.
- `docs/validation/broad-run-corpus.md` documents only `tests/broad-run-corpus/` + `task broad-run-validation`.
- `grep -n broad Taskfile.yml .github/workflows/*.yml` finds no reference from `check`, `affected`, `ci.yml`, `drift-patrol.yml` or `perf-ci.yml`.
- Audit finding tests-ci-16 (verified, P3). str-jeen.14 (closed) created the corpus.

## Acceptance criteria

- [ ] One gate is kept (probably `broad-run-validation`, the documented one). The other gate's Task entry, driver script and corpus directory are deleted. Before deleting, any fixture that exists only in the deleted corpus is either ported to the survivor or listed in the issue as intentionally dropped.
- [ ] The survivor is either scheduled (nightly or weekly workflow, or added to drift-patrol's cadence) or explicitly documented as manual-only, with the reason, in `docs/validation/broad-run-corpus.md` and the CLAUDE.md tier table.
- [ ] If scheduled, proof at close: the URL of a green scheduled or `workflow_dispatch` run. If manual-only, proof at close: a forced local run log (`task broad-run-validation --force`) showing it executes and passes on current main.
- [ ] `broad-run-validation-tests` stays wired to whatever gate runs the Python meta tests.
- [ ] `task affected` passes, and its `Gates selected` output is recorded.

## Suggested approach

Diff the two corpora's fixture lists first. The newer corpus is `tests/broad-run-corpus` with a manifest.yaml, so it has probably absorbed the older one. If a schedule is chosen, drift-patrol's weekly workflow is the cheapest home (see `docs/DRIFT-PATROL.md`).

## Out of scope

- Adding new failure-class fixtures.
- The general tier collapse (`collapse-test-tiers`), although the deletion here reduces that list.

## Priority / type / labels

P3 · chore · quality-gates, cleanup, audit · Size S

## Parent epic

Epic: Audit 2026-09-22 findings (shatter)

## Dependencies

- Blocked by: none.
- Related: str-jeen.14 (closed), `collapse-test-tiers`.
