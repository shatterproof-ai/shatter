---
slug: protocol-codegen-all-registry-enums
kind: new
title: "protocol-codegen emits 6 of the 13 registry enums; error_category already differs between TS and the registry"
priority: P3
type: task
labels: [protocol, codegen, parity, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# protocol-codegen emits 6 of the 13 registry enums; error_category already differs between TS and the registry

Split out of capability-single-source (manifest instruction: split codegen for the 13 registry enums when that issue exceeds about 2 days; it is sized L).

## Problem

`protocol/registry.yaml` declares 13 enums under `enums:`. `scripts/protocol-codegen.py` generates only the legacy vocabulary: commands, statuses, error codes, setup levels, generator kinds and branch types. The other enums are hand-copied in each language with no check, and one has already drifted.

## Evidence (re-verified 2026-09-23 at 56c86168)

- Registry enums (`python3 -c "import yaml; print(list(yaml.safe_load(open('protocol/registry.yaml'))['enums']))"`): setup_level, generator_kind, branch_type, outcome_status, value_plan_kind, value_requirement_kind, runtime_requirement_kind, apply_policy, error_category, unsatisfied_requirement_kind, discovered_dependency_kind, trace_event_type, crypto_boundary_kind.
- `shatter-ts/src/generated/protocol-enums.ts` exports only PROTOCOL_VERSION and ALL_COMMANDS, ALL_RESPONSE_STATUSES, ALL_ERROR_CODES, ALL_SETUP_LEVELS, ALL_GENERATOR_KINDS and ALL_BRANCH_TYPES (lines 11-89). The Go output (`shatter-go/protocol/protocol_enums_gen.go`) and the Rust FE parity test (`shatter-rust/tests/codegen_parity.rs`) cover the same subset.
- Drift: the registry has `error_category: [validation, runtime, infrastructure]`, but `shatter-ts/src/protocol.ts:631-635` `ErrorCategory` adds `"unknown"`. Core carries `error_category` as a `String` (`shatter-core/src/protocol.rs`, e.g. :1821).
- Audit finding protocol-parity-19 (confirmed, P3).

## Acceptance criteria

- [ ] `protocol-codegen.py` emits every `enums:` entry for TS, Go and the Rust frontend. `protocol-codegen.py --check` (already run in `task parity`) fails on drift for all 13.
- [ ] A shatter-core test asserts that the serde spelling of every core enum that mirrors a registry enum equals the registry values.
- [ ] The error_category mismatch is resolved: either the registry gains `unknown`, or TS drops it. The choice is recorded in the registry comment.
- [ ] Proof at close: add a value to one non-legacy registry enum without regenerating, and show `task parity` (forced to execute) failing. Paste the output, then revert.

## Suggested approach

Make the enum list data-driven: iterate `enums:` instead of naming the legacy six. Replace the hand-written TS/Go/Rust definitions of the new enums with imports of the generated ones.

## Out of scope

Capability single-sourcing (capability-single-source) and field-level codegen from `field_model`.

## Dependencies

- Blocked by: none.
- Related: capability-single-source, str-2fjn (its notes record the error_category mismatch).

Size: M. Priority: P3. Type: task. Labels: protocol, codegen, parity, audit. Parent: Epic: Audit 2026-09-22 findings.
