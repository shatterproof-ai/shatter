---
slug: protocol-md-execute-fields
kind: new
title: "PROTOCOL.md's execute section documents about half of the request and response fields"
priority: P2
type: task
labels: [protocol, docs, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# PROTOCOL.md's execute section documents about half of the request and response fields

## Problem

`PROTOCOL.md` is the published wire spec for frontend authors. Its `execute` section documents 4 of the 8 request fields and 8 of the 15 response fields that core and the registry define. Fields have been added by hand under GOVERNANCE step 8 ("update PROTOCOL.md if wire format changes"), and no check catches an omission. A new frontend written from PROTOCOL.md would not know about `plan`, `outcome`, `scope_events`, `loop_body_states` and the others.

## Evidence (re-verified 2026-09-23 at 56c86168)

- The registry `commands.execute.field_model` (`protocol/registry.yaml:161-190`) has these fields:
  - request: function, inputs, mocks, setup_context, prepare_id, capture, execution_profile, plan
  - response: return_value, thrown_error, branch_path, lines_executed, calls_to_external, path_constraints, scope_events, loop_body_states, side_effects, capture_truncation, performance, discovered_dependencies, connection_failures, outcome, runtime_crypto_boundaries
- Core carries the same fields: `Command::Execute` at `shatter-core/src/protocol.rs:410`, and `ExecuteResult` at `:1130-1186`.
- `PROTOCOL.md:209-334` (execute section) documents these fields:
  - request: function, inputs, mocks, capture
  - response: return_value, thrown_error, branch_path, lines_executed, calls_to_external, path_constraints, side_effects, performance
- These fields appear 0 times anywhere in PROTOCOL.md: `execution_profile`, `plan`, `scope_events`, `loop_body_states`, `capture_truncation`, `discovered_dependencies`, `connection_failures`, `runtime_crypto_boundaries`, `outcome`. `setup_context` and `prepare_id` appear only in the prepare and setup sections (`:168-203`, `:338-362`), not in execute.
- Audit finding protocol-parity-09 (confirmed, P2; the verifier noted the setup_context/prepare_id nuance above). Report section 15.1 lists it as a new issue with no prior draft.

## Acceptance criteria

- [ ] Every request and response field of every command in `registry.yaml` `field_model` appears in PROTOCOL.md with its type, whether it is optional, and a one-line description. Execute is the known-bad case, but the check covers all commands.
- [ ] Preferred: the per-command field tables are rendered from `field_model` into marked regions of PROTOCOL.md by a generator with a `--check` mode that runs in `task parity`. If capability-single-source (or str-qwua7.24) has landed a generator by then, extend it. Otherwise add a small renderer; do not wait. If generation is rejected, a test fails when a `field_model` field name is missing from the command's PROTOCOL.md section.
- [ ] `field_model` entries gain a `description` where one is missing, so the generated table is useful.
- [ ] The narrative JSON examples stay.
- [ ] Proof at close: add a dummy field to `field_model` without touching PROTOCOL.md, run `task parity` forced to execute, and show it failing (paste the output). Then revert.
- [ ] Cache wiring: `protocol/PROTOCOL.md` and the renderer/check script are in the `sources:` of the task that runs the check (`protocol/registry.yaml` already is). Proof: `touch protocol/PROTOCOL.md && task parity` (ordinary invocation, no `--force`) executes the check rather than printing `is up to date`; paste the output.

## Suggested approach

Render a table below each command's examples, between `<!-- generated:field_model:<command> -->` markers. Have `protocol-codegen.py` own the rendering, since it already reads the registry.

## Out of scope

Operator and constant lists (protocol-schemas-reject-real-output) and error-code tables (done in str-iqta).

## Dependencies

- Blocked by: none. If capability-single-source lands first, reuse its generator.
- Related: str-iqta (closed; completed command, error-code and type tables but not per-command fields), str-2fjn, capability-single-source, governance-md-omits-matrix.

Size: S-M. Priority: P2. Type: task. Labels: protocol, docs, audit. Parent: Epic: Audit 2026-09-22 findings.
