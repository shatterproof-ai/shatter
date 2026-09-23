# Test inputs come from an unpinned external examples repo (origin/main, refreshed every 10 min)

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | task |
| priority | P2 |
| labels | testing,examples,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | str-35vtk.4, str-w5ry |
| source findings | tests-ci-09 |

<!-- body -->
## Problem

Unit, E2E, smoke, TS and walkthrough tests read SHATTER_EXAMPLES_DIR, a checkout of shatterproof-ai/examples reset to origin/main every 10 minutes. Results are not reproducible and no Task fingerprint covers the content.

## Current code facts / evidence

- `scripts/examples_checkout.py:19-20` DEFAULT_REPO_URL shatterproof-ai/examples.git, DEFAULT_BRANCH main; `:48` REFRESH_WINDOW_SECONDS=600; `:136` `reset --hard origin/main`.

## Acceptance criteria

- A tracked lock file pins the examples SHA; checkout uses that SHA; lock file is in the relevant Task sources.
- Bumping the pin is a normal PR that runs gates.

## Suggested approach

Implementer's choice within the acceptance criteria above.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: S

## References

- Audit findings: tests-ci-09 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-35vtk.4, str-w5ry
