---
slug: docs-smoke-coverage
kind: new
title: "Extend docs-smoke to resource-parameters, distribution, execution-adapters, PROJECT-LAYOUT and PROTOCOL.md"
priority: P3
type: task
labels: [docs, smoke, quality-gates, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Extend docs-smoke to resource-parameters, distribution, execution-adapters, PROJECT-LAYOUT and PROTOCOL.md

## Problem

`task docs-smoke` runs the examples in the docs against the built CLI to catch stale flags, removed commands and invalid JSON/YAML. It checks only the four docs it was created with. User-facing docs added since then are not checked, and PROTOCOL.md's JSON examples, which frontend authors copy, are never validated.

## Evidence

Re-verified at 56c86168:

- `scripts/docs-smoke.yaml:20-24` lists exactly `README.md`, `QUICKSTART.md`, `SPEC.md` and `docs/INDEX.md`.
- The `docs-smoke` task in `Taskfile.yml` (around `:334`) repeats the same four files in its `sources:`. Because task results are checksum-cached, a doc added to the YAML but not to `sources:` would not trigger a re-run when it changes.
- `docs/INDEX.md` lists these user-facing docs, none of which is covered:
  - `docs/PROJECT-LAYOUT.md` (`:11`)
  - `docs/distribution.md` (`:12`)
  - `docs/resource-parameters.md` (`:25`)
  - `docs/execution-adapters.md` (`:29`)
  - `docs/CI-INTEGRATION.md` (`:26`)
- The auditor ran docs-smoke with resource-parameters, distribution, execution-adapters and PROJECT-LAYOUT added, and all four passed (4 + 4 + 3 blocks checked). Adding them is free today. The verifier did not re-run this.
- `PROTOCOL.md` has 22 ```` ```json ```` fences, none of which is validated against `protocol/schemas/`.

## Acceptance criteria

- [ ] `docs/resource-parameters.md`, `docs/distribution.md`, `docs/execution-adapters.md`, `docs/PROJECT-LAYOUT.md` and `docs/CI-INTEGRATION.md` are added to `scripts/docs-smoke.yaml` and to the `docs-smoke` task's `sources:`. `python3 scripts/docs-smoke.py` passes when run directly; paste the output into the close note.
- [ ] Each PROTOCOL.md JSON example is either validated against `protocol/schemas/request.schema.json` / `response.schema.json` (selected by its `command` field), or marked illustrative with an explicit fence tag that docs-smoke recognizes and skips. Proof at close: docs-smoke fails when a required field is removed from one validated PROTOCOL.md example.
- [ ] A unit test in `scripts/test_docs_smoke.py` asserts that the docs-smoke doc list equals the docs in `docs/INDEX.md` whose Audience includes users, minus an explicit exclusion list in `docs-smoke.yaml` that gives a reason for each exclusion. The test also asserts that the `docs-smoke` task's `sources:` include every listed doc. Proof: the test fails on a fixture INDEX that lists an uncovered doc.

## Suggested approach

Add the docs first, since that is free. Then add a `schema:` mode to docs-smoke for PROTOCOL.md that loads the schemas with `jsonschema` (already used by `protocol/schemas/test_schema_validation.py`). Validating output-artifact samples in SPEC §5 is handled by spec-s5-contract-table-and-samples and can reuse this mode.

## Out of scope

- Artifact schemas and SPEC §5 samples (artifact-json-schemas, spec-s5-contract-table-and-samples).
- The CLI-surface drift gate (str-wurp).

## Dependencies

- Blocked by: none.
- Related: str-qwua7.9, str-qwua7.9.1, str-qwua7.44, spec-s5-contract-table-and-samples.

## Source

Audit 2026-09-22, finding docs-19 (confirmed, P3). Draft `shatter-docs-ui/29`. Evidence is in `audits/2026-09-22/areas/docs.md`.
