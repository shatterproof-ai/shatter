---
slug: protocol-codegen-all-registry-enums
kind: new
title: "protocol-codegen emits 3 of the 13 registry enums; the other 10 are hand-copied per language with no check, and error_category already differs in TS"
priority: P3
type: task
labels: [protocol, codegen, parity, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# protocol-codegen emits 3 of the 13 registry enums; the other 10 are hand-copied per language with no check, and error_category already differs in TS

Split out of capability-single-source (manifest instruction: split codegen for the 13 registry enums when that issue exceeds about 2 days).

## Problem

`protocol/registry.yaml` declares 13 enums under `enums:`. The TS, Go and Rust emitters in `scripts/protocol-codegen.py` render only three of them (`setup_level`, `generator_kind`, `branch_type`), plus three vocabularies that come from separate registry mappings rather than `enums:` (commands, response statuses, error codes). The other ten enums are hand-copied in each language, and nothing compares those hand-written definitions with the registry. One has already drifted.

The manifest (`protocol/generated/manifest.json`) does contain all 13 enums, so `protocol-codegen.py --check` already fails when a registry enum changes without regenerating. What it does not catch is a frontend's hand-written definition drifting from the registry. The generated Go and Rust artifacts are vocabulary arrays; the handwritten wire types (TS unions, Go constants/strings, Rust serde enums) stay separate. Emitting ten more unused arrays would therefore not close the gap.

## Evidence (re-verified 2026-09-23 at 56c86168; unchanged at 793f2b0b)

- Registry enums (`python3 -c "import yaml; print(list(yaml.safe_load(open('protocol/registry.yaml'))['enums']))"`): setup_level, generator_kind, branch_type, outcome_status, value_plan_kind, value_requirement_kind, runtime_requirement_kind, apply_policy, error_category, unsatisfied_requirement_kind, discovered_dependency_kind, trace_event_type, crypto_boundary_kind.
- `scripts/protocol-codegen.py` calls `_ts_enum_values` only for `setup_level`, `generator_kind` and `branch_type` in each emitter (TS `:236-250`, Go `:341-355`, Rust `:446-460`). Commands, statuses and error codes come from `registry['commands']`/`['error_codes']`.
- `shatter-ts/src/generated/protocol-enums.ts` exports only PROTOCOL_VERSION and ALL_COMMANDS, ALL_RESPONSE_STATUSES, ALL_ERROR_CODES, ALL_SETUP_LEVELS, ALL_GENERATOR_KINDS and ALL_BRANCH_TYPES. The Go output (`shatter-go/protocol/protocol_enums_gen.go`) and the Rust frontend output (`shatter-rust/src/generated/protocol_enums.rs:16-22`, checked by `shatter-rust/tests/codegen_parity.rs`) cover the same subset. The Rust emitter's own comment (`protocol-codegen.py:408`) says the serde enums in `shatter-rust/src/protocol.rs` own the wire shape.
- `build_manifest()` (`protocol-codegen.py:72-96`) projects every `enums:` entry; `manifest.json` lists all 13.
- Drift: the registry has `error_category: [validation, runtime, infrastructure]`, but `shatter-ts/src/protocol.ts:631-635` `ErrorCategory` adds `"unknown"`. Go carries it as `*string` (`shatter-go/protocol/types.go:570`) and core as a `String` (`shatter-core/src/protocol.rs`, e.g. :1821), so neither can drift-check it today.
- Audit finding protocol-parity-19 (confirmed, P3).

## Acceptance criteria

- [ ] `protocol-codegen.py` iterates `enums:` rather than naming the legacy three, and emits every entry for TS, Go and the Rust frontend.
- [ ] Every newly covered enum is actually tied to each language's wire definition, not emitted as an unused array. For each of the ten enums and each of TS, Go, Rust frontend and shatter-core, one of the following holds, recorded in a table in the close note:
  - the hand-written definition is replaced by (or derived from) the generated one, e.g. the TS union type becomes `(typeof ALL_X)[number]`; or
  - a test in that language asserts that the hand-written definition's wire spellings equal the generated array (for Rust/core serde enums: serialize every variant and compare the set); or
  - the language carries the field as an untyped string, and the table says so explicitly. For such fields, a conformance or unit check validates emitted values against the registry values.
- [ ] The error_category mismatch is resolved: either the registry gains `unknown`, or TS drops it. The choice is recorded in the registry comment.
- [ ] Proof at close, per language: add a value to one hand-written definition only (for example add `"bogus"` to TS `ErrorCategory`, a new variant to one Rust frontend serde enum mirroring a registry enum, a new Go constant), without touching the registry. Paste the **pre-change** gate exit 0 on that mutation (the current `--check` cannot see it), then the post-change failure. A registry-only edit does not count, because the manifest check already catches it.
- [ ] `task parity` and the per-language test gates that host the new tests (`task ts:test`, `task go:test`, `task rust-fe:test`, core tests) pass; paste the output of runs that executed (not checksum-cached). If a new test lives outside an existing task's `sources:`, add it.

## Suggested approach

Make the enum list data-driven: iterate `enums:` instead of naming the legacy three. In TS, derive the union types from the generated arrays. In Rust and Go, where hand-written types own the wire shape, prefer a generated parity test over replacing the types.

## Out of scope

Capability single-sourcing (capability-single-source) and field-level codegen from `field_model`.

## Dependencies

- Blocked by: none.
- Related: capability-single-source, str-2fjn (its notes record the error_category mismatch).

Size: M. Priority: P3. Type: task. Labels: protocol, codegen, parity, audit. Parent: Epic: Audit 2026-09-22 findings.
