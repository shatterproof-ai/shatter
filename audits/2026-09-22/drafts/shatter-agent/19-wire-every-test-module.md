# Meta test: every scripts/test_*.py and demo/test_*.py must be run by a gate (≈220 unwired tests today)

- Priority: P2
- Type: task
- Labels: testing,quality-gates,agents
- Tracker: shatter (bd, /home/ketan/project/shatter)
- Relation: new (partially covered by str-35vtk.35, str-35vtk.24/.25)
- Source findings: tests-ci-10, protocol-parity-05
- Parent: 01 (epic)
- Blocked by: none
- Readiness: drafted to the issue-readiness-check standard; fresh-reviewer precheck still required before filing (see INDEX.md)

<!-- body -->
## Problem
Adding a test file and wiring it into a gate are separate manual steps with
no check on the second. About 220 script unit tests — including the regression
test str-qwua7.7 just added — are never run by `task check`, `task affected`
or CI. Agents report "regression test added" and consider the work done.

## Current Code Facts
Modules referenced by no Taskfile, workflow or demo script (grep):
- `scripts/test_validate_parity.py`, `scripts/test_validate_protocol_registry.py`
  (35 tests; only mentioned as a manual step at `protocol/GOVERNANCE.md:120-121`)
- `scripts/test_docs_smoke.py`, `scripts/test_gate_event_log.py`,
  `scripts/test_gate_pressure.py`, `scripts/test_build_cache_doctor.py`,
  `scripts/test_perf_compare.py`
- `demo/test_gauntlet_check_output.py` (only a path trigger in
  `scripts/affected-gates.py:177`)
- `scripts/test_ci_workflow_structure.py` runs only in CI (str-35vtk.35 open).
- `scripts/gate-event-log.py`, `scripts/gate-pressure.py` are invoked by
  nothing. (`gate-receipt.py`'s tests do run in meta, Taskfile.yml:460.)
- The `meta` task (Taskfile.yml:~440-470) lists ~17 `scripts.test_*` modules
  explicitly.

## Acceptance Criteria
- A meta test (e.g. in `scripts/test_test_tier_wiring.py`) discovers
  `scripts/test_*.py` and `demo/test_*.py` and fails for any module not run by
  some Taskfile task reachable from `check` (allowlist with reasons permitted).
  It fails on today's tree before wiring.
- The listed modules are wired into `meta` (or deleted, with reason); they pass.
- `gate-event-log.py`/`gate-pressure.py` are either invoked by gate-wrapper or
  deleted.
- str-35vtk.35 closed as subsumed (or completed here).

## Out of Scope
Fixing test failures uncovered, beyond filing them.
