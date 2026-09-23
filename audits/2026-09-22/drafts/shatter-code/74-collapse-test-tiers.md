# Collapse ~20 overlapping gate tiers; stop duplicating task bodies for checksum identity; e2e runs twice in pre-completion-e2e

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | refactor |
| priority | P3 |
| labels | quality-gates,taskfile,audit |
| parent | audit epic (draft 00) |
| blocked by | draft 01, draft 03 |
| related | str-nl1g, str-35vtk |
| source findings | tests-ci-13, gates-07 (e2e duplication part) |

<!-- body -->
## Problem

Gate tiers accreted per efficiency issue; four task bodies are copied only to get separate cache keys, kept in sync by a wiring test, and pre-completion-e2e runs the E2E suites twice.

## Current code facts / evidence

- Tiers: test, test-quick, test-standard, check-fast, check, affected, pre-completion(-e2e), e2e(-ts/-go/-rust), smoke, walkthrough(-cold), gauntlet(-cold), golden-test, parity, conformance, broad-run-corpus, broad-run-validation, drift-patrol.
- `Taskfile.yml:136-139`: 'Task does not fingerprint caller-provided environment variables, so using workspace-test here could let a quick result satisfy test-standard later'; same pattern in cli test/test-fast and core test-ignored/-fast.
- `Taskfile.yml:673-678` pre-completion-e2e runs check then e2e; core:test-ignored `--run-ignored all` already includes e2e_concolic*.rs.

## Acceptance criteria

- Proposed tier set (dev / affected / check / release, or similar) agreed and recorded here before implementation.
- Cache identity derived without copying bodies (verify go-task label-based checksum naming first).
- E2E runs once in pre-completion-e2e.

## Suggested approach

Needs a maintainer decision on the tier set; do after drafts 01/03.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: M

## References

- Audit findings: tests-ci-13, gates-07 (e2e duplication part) (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-nl1g, str-35vtk
