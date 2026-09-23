---
slug: wire-every-test-module
kind: new
title: "Meta test: every scripts/test_*.py and demo/test_*.py must be run by a gate (8 modules, ~209 tests unwired today)"
priority: P2
type: task
labels: [testing, quality-gates, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Meta test: every scripts/test_*.py and demo/test_*.py must be run by a gate

## Problem

Adding a test file and wiring it into a gate are separate manual steps, and nothing checks the second step. Eight script test modules, about 209 test methods, are never run by `task check`, `task affected` or CI. They include the regression test that str-qwua7.7 added. Agents report "regression test added" and treat the work as done. The `meta` task lists its test modules one by one (`Taskfile.yml:449-466`), so every new module must be added there by hand.

## Evidence (re-verified 2026-09-23)

The following modules are referenced by no Taskfile, workflow or demo/scripts shell script. The check looked for the module basename in `Taskfile.yml`, `*/Taskfile.yml`, `.github/` and `demo/` or `scripts/` `*.sh`/`*.yml`. Test method counts come from `grep -c "def test_"`.

| Module | Tests | Note |
|---|---|---|
| `scripts/test_build_cache_doctor.py` | 57 | |
| `scripts/test_docs_smoke.py` | 54 | |
| `scripts/test_gate_pressure.py` | 30 | |
| `scripts/test_gate_event_log.py` | 25 | |
| `scripts/test_validate_parity.py` | 24 | includes the str-qwua7.7 regression test (4cf2165f) |
| `scripts/test_validate_protocol_registry.py` | 11 | only mentioned as a manual step at `protocol/GOVERNANCE.md:120-121` |
| `demo/test_gauntlet_check_output.py` | 6 | appears only as a path trigger at `scripts/affected-gates.py:177`; it also pins a dead output format (see `gauntlet-scan-checker-consumes-json`) |
| `scripts/test_perf_compare.py` | 2 | |

- `scripts/test_ci_workflow_structure.py` runs only in CI (`.github/workflows/ci.yml:110`), never locally. str-35vtk.35 (open) covers this one file.
- `scripts/test_broad_run_validation_gate` and `scripts/test_kapow_refute_agent` run only in non-gate tasks (`Taskfile.yml:922`, `:932`).
- `scripts/gate-event-log.py` and `scripts/gate-pressure.py` are invoked by nothing. By contrast, `scripts/test_gate_receipt.py` does run in `meta` (`Taskfile.yml:460`).
- `scripts/test_test_tier_wiring.py` already exists and runs in `meta` (`Taskfile.yml:466`). It is the natural home for the new check.

## Acceptance criteria

- [ ] A meta test, for example in `scripts/test_test_tier_wiring.py`, discovers `scripts/test_*.py`, `scripts/test_*.sh` and `demo/test_*.py`. It fails for any module that no Taskfile task reachable from `check` runs, whether through `python3 -m unittest scripts.<module>`, a direct path, or a discover pattern. An allowlist with a reason per entry is permitted, for example a module intentionally run only by a named non-gate task.
- [ ] Proof: the meta test fails on today's tree, listing the 8 modules above (record the output in the close reason), and passes after wiring.
- [ ] Each listed module is wired into `meta`, or into a better-fitting gate, and passes. A module can instead be deleted, with the reason in the commit. Failures uncovered by wiring are fixed or filed, with ids in the close reason. `demo/test_gauntlet_check_output.py` is wired as part of `gauntlet-scan-checker-consumes-json`. This issue only requires that it be reachable.
- [ ] `scripts/gate-event-log.py` and `scripts/gate-pressure.py` are either invoked by `gate-wrapper.sh` or the verifier, or deleted along with their tests.
- [ ] `protocol/GOVERNANCE.md:120-121` no longer describes the validator tests as a manual step.
- [ ] str-35vtk.35 is closed as subsumed, or completed here.

## Suggested approach

Replace the explicit list in `meta` with `python3 -m unittest discover -s scripts -p 'test_*.py'`, plus the same for `demo/`, and keep the meta test as a guard against future non-discoverable layouts. Alternatively, keep the explicit list and let the meta test enforce it.

## Out of scope

- Fixing test failures uncovered by wiring, beyond filing them.
- Rewriting the gauntlet checker (`gauntlet-scan-checker-consumes-json`).

## Metadata

- Priority: P2. Type: task. Size: S-M.
- Labels: testing, quality-gates, audit.
- Parent epic: Epic: Audit 2026-09-22 findings.
- Blocked by: none.
- Related: str-35vtk.35, str-35vtk.24, str-35vtk.25, str-qwua7.7.
- Source findings: tests-ci-10 (verifier correction applied: gate-receipt.py's tests do run in meta), protocol-parity-05. Draft shatter-agent/19.
