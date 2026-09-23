# Two duplicate broad-run validation gates, neither scheduled or in check/affected/CI

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | chore |
| priority | P3 |
| labels | quality-gates,cleanup,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | str-jeen.14 |
| source findings | tests-ci-16 |

<!-- body -->
## Problem

broad-run-corpus and broad-run-validation use different scripts and corpora for the same purpose; neither runs anywhere automatically.

## Current code facts / evidence

- `Taskfile.yml:722-738` broad-run-corpus → tests/scripts/broad_run_validation.py + tests/fixtures/broad-run-corpus (last touched 2026-05-13).
- `Taskfile.yml:897-917` broad-run-validation → scripts/broad_run_validation_gate.py + tests/broad-run-corpus ('Documented local check; not in CI'); broad-run-validation-tests at :919.
- No workflow references broad-run.

## Acceptance criteria

- One gate kept, the other (script + corpus) deleted.
- Survivor scheduled (nightly or weekly) or explicitly documented as manual.

## Suggested approach

Implementer's choice within the acceptance criteria above.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: S

## References

- Audit findings: tests-ci-16 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-jeen.14
