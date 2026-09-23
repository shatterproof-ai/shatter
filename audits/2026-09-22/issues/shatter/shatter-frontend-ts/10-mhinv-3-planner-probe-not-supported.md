---
slug: mhinv-3-planner-probe-not-supported
kind: note-to-existing
title: "Note on str-mhinv.3: TS answers planner-command probes with invalid_request 'Unknown command', not the not_supported its CLAUDE.md promises"
priority: P2
type: note
labels: [typescript, protocol, parity, audit]
parent_epic: ""
blocked_by: []
existing_id: str-mhinv.3
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-mhinv.3

**Target:** `str-mhinv.3` ("Plan field unsupported behavior", P1, OPEN).
**Action:** post one comment (`bd comments add str-mhinv.3`) that adds an acceptance item. Do not change the priority.
**Ordering:** post after `ts-request-validation` is filed and substitute its id.

## Comment text

> **Audit 2026-09-22 note** (finding frontend-ts-08, confirmed; re-checked at 56c86168).
>
> This issue pins clean unsupported behaviour for plan-bearing `execute` requests. A related gap is the **planner commands themselves**.
>
> - `shatter-ts/CLAUDE.md:155-157` says conformance tests "expect TS to return a clean 'capability not supported' response" when planner commands are probed.
> - In fact the TS dispatch allow-list is a hand-written literal (`shatter-ts/src/handlers.ts:1151`: `handshake, analyze, instrument, prepare, execute, setup, teardown, generate, shutdown`), so a probe returns:
>   ```json
>   {"id":2,"status":"error","code":"invalid_request","message":"Unknown command: get_invocation_plan"}
>   ```
> - The only `get_invocation_plan` conformance case (`protocol/conformance/conformance_cases.yaml:538`, `planner_runtime_value_go`) is `frontends: [go]`, so nothing checks the documented contract.
>
> **Acceptance item to add here:** TS (and Rust, if it has the same gap) and the docs agree on the planner-command probe response. One of two things happens:
> - (a) behaviour changes: a command that is in generated `ALL_COMMANDS` but not in `SUPPORTED_CAPABILITIES` returns `not_supported`, and truly unknown commands keep `invalid_request`; or
> - (b) the contract changes: `shatter-ts/CLAUDE.md:155-157` and `protocol/parity-matrix.yaml` document `invalid_request`.
>
> Either way, a TS (and Rust) conformance case for a `get_invocation_plan` probe pins it, and `task conformance` shows the case running.
>
> Option (a) is already an acceptance criterion of <ts-request-validation id>, which derives the dispatch set from `ALL_COMMANDS`. If that lands first, this item closes by pointing to it.
