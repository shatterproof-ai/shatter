# str-qwua7.4 purge incomplete: a March rapid failfile is still tracked and .gitignore covers only planner/

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | chore |
| priority | P3 |
| labels | go,testing,cleanup,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | str-qwua7.4 |
| source findings | tests-ci-15, prior-23 |

<!-- body -->
## Problem

str-qwua7.4 required rapid failfiles to be removed from git and ignored repo-wide.

## Current code facts / evidence

- Tracked: `shatter-go/instrument/testdata/rapid/TestPropertyExecTimeoutAlwaysPositive/TestPropertyExecTimeoutAlwaysPositive-20260306134644-2197485.fail` (added f0a58576).
- `shatter-go/.gitignore:3` ignores only `planner/testdata/rapid/**/*.fail`.

## Acceptance criteria

- File removed from git; pattern `**/testdata/rapid/**/*.fail`.

## Suggested approach

Implementer's choice within the acceptance criteria above.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: S

## References

- Audit findings: tests-ci-15, prior-23 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-qwua7.4
