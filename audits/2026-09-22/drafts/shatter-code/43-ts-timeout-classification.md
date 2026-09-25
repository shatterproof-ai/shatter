# TS frontend classifies any target error whose message mentions 'timeout' as timed_out/infrastructure

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | bug |
| priority | P2 |
| labels | typescript,error-handling,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | none |
| source findings | frontend-ts-05 |

<!-- body -->
## Problem

Outcome classification by message substring turns `RangeError('timeout must be positive')` thrown by user code into `outcome.status: timed_out` and `error_category: infrastructure`.

## Current code facts / evidence

- `shatter-ts/src/executor.ts:281-286` `classifyError` → infrastructure on `/timed?\s*out/i`; `:331-338` TIMEOUT_PATTERNS includes bare 'timeout'; `:3293-3298` `isTimeoutError` maps to timed_out.
- Harness timeouts: vm `ERR_SCRIPT_EXECUTION_TIMEOUT`, and the async Promise.race at executor.ts:1276.

## Acceptance criteria

- Only harness-owned timeouts yield timed_out (vm error code + a dedicated error class thrown by the race).
- User errors classified by type, not message.
- Negative test: target throwing `RangeError('timeout must be positive')` → runtime outcome with the thrown error.

## Suggested approach

Implementer's choice within the acceptance criteria above.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: S

## References

- Audit findings: frontend-ts-05 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: none
