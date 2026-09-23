# TS parseRequest validates only the envelope: missing fields crash as internal_error; validCommands hand-maintained

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | bug |
| priority | P2 |
| labels | typescript,protocol,validation,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | str-mhinv.3, str-qwua7.7 |
| source findings | frontend-ts-07 |

<!-- body -->
## Problem

A request with a valid envelope but missing fields reaches handlers and fails with `internal_error: Cannot read properties of undefined` or `file_not_found: undefined`.

## Current code facts / evidence

- `shatter-ts/src/handlers.ts:1113-1159` checks id, protocol_version, command, then `return { request: parsed as Request }`.
- `handlers.ts:1151` `validCommands` literal duplicates generated `ALL_COMMANDS`.
- Probe: `{command:'execute'}` → internal_error 'Unhandled error: Cannot read properties of undefined (reading 'includes')'; `{command:'analyze'}` → file_not_found 'File not found: undefined'.

## Acceptance criteria

- Per-command field validators return `invalid_request` naming the bad field (ideally generated from `protocol/registry.yaml` field_model).
- Dispatch set derives from ALL_COMMANDS; commands in ALL_COMMANDS but not in SUPPORTED_CAPABILITIES return `not_supported` (matches shatter-ts/CLAUDE.md:155-157), unknown ones `invalid_request`.
- Conformance cases for malformed execute/analyze on TS (and ideally all frontends).

## Suggested approach

Implementer's choice within the acceptance criteria above.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: S-M

## References

- Audit findings: frontend-ts-07 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-mhinv.3, str-qwua7.7
