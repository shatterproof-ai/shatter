---
slug: ts-request-validation
kind: new
title: "TS parseRequest validates only the envelope: missing fields crash as internal_error; validCommands hand-maintained"
priority: P2
type: bug
labels: [typescript, protocol, validation, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# TS parseRequest validates only the envelope: missing fields crash as internal_error; validCommands hand-maintained

## Problem

`parseRequest` checks `id`, `protocol_version` and `command`, then casts the rest (`parsed as Request`). A request with a valid envelope but missing or ill-typed fields reaches the handlers and fails deep inside them, as `internal_error: Cannot read properties of undefined` or `file_not_found: undefined`. It should be rejected as `invalid_request` naming the bad field.

The command allow-list is also a hand-written literal that duplicates the generated `ALL_COMMANDS`. Because of that, a known-but-unsupported command (for example `get_invocation_plan`) is answered as `invalid_request "Unknown command"` instead of `not_supported`.

## Evidence

Verified at 56c86168 and re-checked unchanged at 793f2b0b (2026-09-23).

- `shatter-ts/src/handlers.ts:1113` `export function parseRequest(...)`, which checks the envelope, then returns `{ request: parsed as Request }` (through `:1159`).
- `shatter-ts/src/handlers.ts:1151`:
  ```ts
  const validCommands = ["handshake", "analyze", "instrument", "prepare", "execute", "setup", "teardown", "generate", "shutdown"];
  ```
  followed at `:1154` by `errorResponse(id, "invalid_request", `Unknown command: ...`)`.
- Generated `ALL_COMMANDS` is at `shatter-ts/src/generated/protocol-enums.ts:14-25` (`analyze, execute, generate, get_invocation_plan, handshake, instrument, prepare, setup, shutdown, teardown`), re-exported via `protocol.ts:18`.
- `SUPPORTED_CAPABILITIES` (`shatter-ts/src/handlers.ts:56-68`) is a **capability** list, not a command list: it contains `analyze, execute, instrument, prepare, setup, teardown, generate` plus `complex_type:*` entries, and it does **not** contain the control commands `handshake` or `shutdown`. It cannot be used directly as the dispatch set.
- Audit probes over stdio against `node dist/main.js` (full envelopes; the audit notes abbreviated them):
  - `{"protocol_version":"0.1.0","id":1,"command":"execute"}` -> `internal_error "Unhandled error: Cannot read properties of undefined (reading 'includes')"`;
  - `{"protocol_version":"0.1.0","id":2,"command":"analyze"}` -> `file_not_found "File not found: undefined"`.

  The verifier confirmed the code path, not the live probe.
- `protocol/fixtures/requests/invalid/` holds nine invalid requests (`missing-required-field.json` is the analyze probe above; also `prepare-missing-function.json`, `wrong-field-type.json`, `negative-id.json`, `unknown-command.json`, ...). No TS test drives them through `parseRequest` + `handleRequest`.
- `shatter-ts/CLAUDE.md:155-157` says conformance expects TS to "return a clean 'capability not supported' response" for planner commands. The only `get_invocation_plan` case (`protocol/conformance/conformance_cases.yaml:538`, `planner_runtime_value_go`) is `frontends: [go]`, so nothing checks that claim.
- Audit sources: finding frontend-ts-07 (and frontend-ts-08 for the `not_supported` half); `audits/2026-09-22/areas/frontend-ts.md` F7/F8.

## Acceptance criteria

- [ ] There is a per-command field validator for every command TS dispatches. A missing or ill-typed required field yields `invalid_request`, with a message naming the command and the field (for example `execute: missing required field 'inputs'`). Ideally the validators are generated from `protocol/registry.yaml` `field_model` through the existing codegen, so they cannot drift.
- [ ] Dispatch classification, with the literal at `handlers.ts:1151` removed:
  - `handshake` and `shutdown` are **control commands** and are always dispatched, whatever the capability list says;
  - the supported command set is an explicit constant (for example `SUPPORTED_COMMANDS`) of command names, separate from `SUPPORTED_CAPABILITIES`; a test asserts it is a subset of `ALL_COMMANDS` and that every non-`complex_type:` capability is in it;
  - a command in `ALL_COMMANDS` but not supported returns `not_supported` (matching `shatter-ts/CLAUDE.md:155-157`);
  - a command not in `ALL_COMMANDS` returns `invalid_request`.
- [ ] A unit test pins the classification for every value of `ALL_COMMANDS` plus one unknown command; `handshake` and `shutdown` must reach their handlers and return `handshake` / `shutdown_ack`.
- [ ] Every file in `protocol/fixtures/requests/invalid/` driven through `parseRequest` + `handleRequest` yields `invalid_request` (the test iterates the directory, so a new invalid fixture is covered automatically). This test fails on current `main` for at least `missing-required-field.json` and `prepare-missing-function.json`.
- [ ] Conformance cases for TS (and for the other frontends where cheap), with full envelopes:
  - malformed `execute` (no fields);
  - malformed `analyze` (no `file`);
  - a planner-command probe (`get_invocation_plan`) expecting `not_supported`;
  - `handshake` then `shutdown` still succeed.

  Each new negative case fails on current `main` and passes after the fix; paste the `task conformance` output (case names and pass counts) in the close note.
- [ ] If the wire behaviour changes, update `shatter-ts/CLAUDE.md` and `protocol/parity-matrix.yaml`, then run `task parity`.
- [ ] Record `task affected` `Gates selected` at close.

## Suggested approach

Generate the validators if codegen already walks `field_model`. Otherwise hand-write a small `validateRequest(command, obj)` table keyed by `ALL_COMMANDS`, plus a test asserting that every command has an entry.

## Out of scope

- Implementing planner commands in TS (str-mhinv.2 / str-mhinv.3).
- Go/Rust request validation beyond the shared conformance cases.
- Valid-fixture round trips (ts-protocol-and-parity-tests-meaningful).

## Related

- str-mhinv.3 (open): the mhinv-3-planner-probe-not-supported note in this bucket points here as the implementation home for the `not_supported` item.
- str-qwua7.7 (closed; registry extractor sources); str-4btb (closed; parseRequest test consolidation).

## Priority / type / size

P2 · bug · size S-M
