---
slug: wire-every-test-module
kind: new
title: "Meta test: every scripts/test_*.py, scripts/test_*.sh and demo/test_*.py must be run by a gate reachable from `check` (8 modules, ~209 tests unwired today)"
priority: P2
type: task
labels: [testing, quality-gates, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Meta test: every script test module must be run by a gate reachable from `check`

## Problem

Adding a test file and wiring it into a gate are separate manual steps, and nothing checks the second step. Eight script test modules, about 209 test methods, are never run by `task check`, `task affected` or CI. They include the regression test that str-qwua7.7 added. Agents report "regression test added" and treat the work as done. The `meta` task lists its test modules one by one (`Taskfile.yml:449-466`) and lists its `sources:` file by file (`:396` onward), so a new module is neither run nor able to invalidate `meta`'s checksum until someone edits both lists by hand. A wiring guard that lives inside `meta` would itself be skipped as `up to date` when the only change is a new, unwired test file.

## Evidence (re-verified 2026-09-23; `Taskfile.yml` and the listed modules are identical on main `70465921` and the audit snapshot)

The following modules are referenced by no Taskfile, workflow or demo/scripts shell script. The check looked for the module basename in `Taskfile.yml`, `*/Taskfile.yml`, `.github/` and `demo/` or `scripts/` `*.sh`/`*.yml`. Test method counts come from `grep -c "def test_"`.

| Module | Tests | Note |
|---|---|---|
| `scripts/test_build_cache_doctor.py` | 57 | |
| `scripts/test_docs_smoke.py` | 54 | |
| `scripts/test_gate_pressure.py` | 30 | tests the pressure provider staged for str-35vtk.17/.19 |
| `scripts/test_gate_event_log.py` | 25 | tests the event store (str-35vtk.18, closed) that str-35vtk.25 depends on |
| `scripts/test_validate_parity.py` | 24 | includes the str-qwua7.7 regression test (4cf2165f) |
| `scripts/test_validate_protocol_registry.py` | 11 | only mentioned as a manual step at `protocol/GOVERNANCE.md:120-121` |
| `demo/test_gauntlet_check_output.py` | 6 | appears only as a path trigger at `scripts/affected-gates.py:177`; it pins a dead output format and is rewritten by `gauntlet-scan-checker-consumes-json` |
| `scripts/test_perf_compare.py` | 2 | |

- `scripts/test_ci_workflow_structure.py` runs only in CI (`.github/workflows/ci.yml:110`), never locally. str-35vtk.35 (open, P3; verified with `bd show`) covers this one file.
- `scripts/test_broad_run_validation_gate` and `scripts/test_kapow_refute_agent` run only in non-gate tasks (`Taskfile.yml:922`, `:932`).
- `scripts/gate-event-log.py` and `scripts/gate-pressure.py` are not invoked by any gate yet. That is intentional staging: str-35vtk.19 (open) owns the pressure-aware wait loop, and str-35vtk.25 (open) writes shadow events into the event store. Their tests must run; the scripts themselves stay.
- `scripts/test_gate_receipt.py` does run in `meta` (`Taskfile.yml:460`). `scripts/test_test_tier_wiring.py` also runs in `meta` (`:466`) and is the natural home for the new check.

## Acceptance criteria

- [ ] A meta test, for example in `scripts/test_test_tier_wiring.py`, discovers `scripts/test_*.py`, `scripts/test_*.sh` and `demo/test_*.py`. It fails for any module that no task reachable from `check` runs, whether through `python3 -m unittest scripts.<module>`, a direct path, or a `discover` pattern. Reachability is computed by parsing the Taskfiles as YAML (not via `task --list-all --json`; see str-qwua7.3). An allowlist with a reason per entry is permitted only for modules intentionally run by a named non-gate task (for example `test_broad_run_validation_gate`).
- [ ] The task that hosts the guard declares glob `sources:` that cover future additions: `scripts/test_*.py`, `scripts/test_*.sh`, `demo/test_*.py` (and the Taskfiles it parses). A meta test asserts those globs are present.
- [ ] Regression proof that the guard cannot be cached away: on a scratch branch, with the checksum-poisoning fix (str-qwua7.3) in place, run `task meta` until it exits 0, confirm a second run prints `Task "meta" is up to date`, then add an empty-but-valid `scripts/test_zz_unwired.py` containing one passing test and referenced by no task. `task meta` (without `--force`) must execute and fail naming that module. Record the transcript in the close reason and delete the scratch file.
- [ ] Proof on today's tree: the meta test fails, listing the 8 modules above (record the output in the close reason), and passes after wiring.
- [ ] Each listed module except `demo/test_gauntlet_check_output.py` is wired into `meta` (or a better-fitting gate reachable from `check`) and passes. Wiring failures are fixed or filed, with ids in the close reason. `scripts/test_gate_pressure.py` and `scripts/test_gate_event_log.py` are wired as tests only; this issue does not invoke, integrate or delete `gate-pressure.py` or `gate-event-log.py`.
- [ ] `demo/test_gauntlet_check_output.py`: if `gauntlet-scan-checker-consumes-json` has landed, it is already in `meta` and the guard sees it. If not, it is allowlisted with the reason "pins dead format; wired into meta by <gauntlet-scan-checker-consumes-json id>", and that issue's acceptance removes the allowlist entry.
- [ ] `protocol/GOVERNANCE.md:120-121` no longer describes the validator tests as a manual step.
- [ ] str-35vtk.35 is closed as subsumed (with a pointer to this issue), or completed here.

## Suggested approach

Replace the explicit module list in `meta` with `python3 -m unittest discover -s scripts -p 'test_*.py'` plus the same for `demo/`, and add the glob `sources:`. Keep the meta test as a guard against layouts `discover` would not reach. Alternatively, keep the explicit list and let the meta test enforce it.

## Out of scope

- Fixing test failures uncovered by wiring, beyond filing them.
- Rewriting the gauntlet checker (`gauntlet-scan-checker-consumes-json`).
- Integrating `gate-pressure.py` or `gate-event-log.py` into `gate-wrapper.sh` (str-35vtk.19, str-35vtk.25).

## Metadata

- Priority: P2. Type: task. Size: S-M.
- Labels: testing, quality-gates, audit.
- Parent epic: Epic: Audit 2026-09-22 findings.
- Blocked by: none. The cache-regression proof needs the str-qwua7.3 fix (`task-list-json-poisons-checksums`) to be meaningful; do it last.
- Related: str-35vtk.35, str-35vtk.19, str-35vtk.25, str-qwua7.7, `gauntlet-scan-checker-consumes-json`.
- Source findings: tests-ci-10 (verifier correction applied: gate-receipt.py's tests do run in meta), protocol-parity-05. Draft shatter-agent/19.
