---
slug: ts-timeout-classification
kind: new
title: "TS frontend classifies any target error whose message mentions 'timeout' as timed_out/infrastructure"
priority: P2
type: bug
labels: [typescript, error-handling, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# TS frontend classifies any target error whose message mentions 'timeout' as timed_out/infrastructure

## Problem

The TS executor picks the outcome from a substring match on the error **message**. User code that throws `new RangeError("timeout must be positive")` for `ms <= 0` is ordinary input validation. It is reported as `outcome.status: "timed_out"` with `error_category: "infrastructure"`, so reports and gates treat an expected target error as a harness fault. The same applies to any user error whose message contains "timeout" or "timed out".

## Evidence

Re-verified against the audit worktree at 56c86168 (2026-09-23).

- `shatter-ts/src/executor.ts:277-286` `classifyError`: returns `infrastructure` for `ERR_SCRIPT_EXECUTION_TIMEOUT` **or** for a message matching `/timed?\s*out/i`.
- `shatter-ts/src/executor.ts:331` `TIMEOUT_PATTERNS`, which includes the bare `"timeout"`; the loop over it is at `:356`.
- `shatter-ts/src/executor.ts:3293-3298` `isTimeoutError`: true for `ERR_SCRIPT_EXECUTION_TIMEOUT`, or for an infrastructure-classified error whose message matches `TIMEOUT_PATTERNS`. That maps to `timed_out`.
- There are only two real harness timeouts:
  - the vm sync timeout (`ERR_SCRIPT_EXECUTION_TIMEOUT`);
  - the async race at `executor.ts:1274-1281`, which rejects with a plain `new Error("async execution timed out")`. It carries no distinguishing class or code, which is why message matching was used.
- Probe from the audit: a target throwing `new RangeError("timeout must be positive")` returned `"outcome":{"status":"timed_out","short_reason":"timeout must be positive",...}` with `error_category: "infrastructure"`. The verifier confirmed the code path but did not re-run the probe.
- Analogous Rust pattern: str-qwua7.33 (classify by downcast, not substring).
- Audit sources: finding frontend-ts-05; `audits/2026-09-22/areas/frontend-ts.md` F5.

## Acceptance criteria

- [ ] Only harness-owned timeouts produce `timed_out` / `infrastructure`:
  - the vm error code `ERR_SCRIPT_EXECUTION_TIMEOUT`;
  - a dedicated error class (for example `ShatterAsyncTimeout`) thrown by the async race at `executor.ts:1274-1281`, and recognized by `instanceof` or a unique code, not by message.
- [ ] User-thrown errors are classified by type/origin and never by message substring. `TIMEOUT_PATTERNS` is removed, or restricted to errors that provably come from the harness.
- [ ] Negative test: a target throwing `new RangeError("timeout must be positive")` gets a runtime (thrown-error) outcome carrying that error, and no `timed_out`. A positive test for each real timeout path (sync vm timeout, async race) still yields `timed_out`. The negative test fails on current `main` and passes after the fix.
- [ ] If the wire-visible outcome changes for any existing fixture, update `shatter-ts/CLAUDE.md` and the parity contract, and run `task parity` + `task conformance`.
- [ ] Record `task affected` `Gates selected` at close.

## Suggested approach

Introduce the timeout error class in the race and check it first in `classifyError`/`isTimeoutError`. Leave `classifyConnectionFailure` for mocked network failures only. ts-lifecycle-and-packaging-hygiene touches the same race to add `clearTimeout`, so do both in one change if they are picked up together.

## Out of scope

- The timer leak in the same race, which is in ts-lifecycle-and-packaging-hygiene. Doing it together is fine.
- Go/Rust outcome classification.

## Priority / type / size

P2 · bug · size S
