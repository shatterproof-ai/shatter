# Smoke, walkthrough, gauntlet and E2E user paths never run in CI; the only CI gauntlet path (Perf CI) has 0/13 successes

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | task |
| priority | P2 |
| labels | ci,smoke,walkthrough,gauntlet,audit |
| parent | audit epic (draft 00) |
| blocked by | draft 01 |
| related | str-qwua7.10, str-qwua7.42 |
| source findings | tests-ci-11 |

<!-- body -->
## Problem

ci.yml runs only `task check` plus shatter-llm steps. The demo and user-path gates run only at agent discretion, and perf-ci's gauntlet-auto-warm step fails every run with its reason suppressed.

## Current code facts / evidence

- `.github/workflows/ci.yml`: task check, shatter-llm clippy/test, test_ci_workflow_structure.py; no smoke/walkthrough/gauntlet.
- `gh run list --workflow perf-ci.yml` → 13 failures ('gauntlet-auto-warm failed on run 1 with exit code 1').

## Acceptance criteria

- Main/nightly CI job runs `task smoke` and a bounded walkthrough, uploading output artifacts.
- perf-ci surfaces gauntlet stderr and is fixed or disabled with a tracked reason.

## Suggested approach

Combine with str-qwua7.10 content assertions.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: S-M

## References

- Audit findings: tests-ci-11 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-qwua7.10, str-qwua7.42
