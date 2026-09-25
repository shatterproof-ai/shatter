# validate-protocol-registry prints a permanent get_invocation_plan 'may be unimplemented' warning although the matrix marks it optional

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | chore |
| priority | P3 |
| labels | protocol,parity,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | str-qwua7.7 |
| source findings | prior-24 |

<!-- body -->
## Problem

A permanent warning trains readers to ignore validator output.

## Current code facts / evidence

- `python3 scripts/validate-protocol-registry.py` → "commands: 'get_invocation_plan' in registry but not found in shatter-rust (may be unimplemented)" then 'All checks passed (with informational warnings)'.
- `protocol/parity-matrix.yaml:175-181` lists get_invocation_plan `status: optional`; warning emitted near validate-protocol-registry.py:595.

## Acceptance criteria

- Validator reads parity-matrix and suppresses warnings for commands marked optional/unsupported per frontend; clean run prints no warnings.

## Suggested approach

Implementer's choice within the acceptance criteria above.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: S

## References

- Audit findings: prior-24 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-qwua7.7
