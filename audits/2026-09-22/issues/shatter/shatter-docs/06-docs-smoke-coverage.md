---
slug: docs-smoke-coverage
kind: new
title: "Extend docs-smoke to resource-parameters, distribution, execution-adapters, CI-INTEGRATION, PROJECT-LAYOUT and PROTOCOL.md"
priority: P3
type: task
labels: [docs, smoke, quality-gates, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Extend docs-smoke to resource-parameters, distribution, execution-adapters, CI-INTEGRATION, PROJECT-LAYOUT and PROTOCOL.md

## Problem

`task docs-smoke` runs the examples in the docs against the built CLI to catch stale flags, removed commands and invalid JSON/YAML. It checks only the four docs it was created with. User-facing and contributor docs added since then are not checked, and PROTOCOL.md's JSON examples, which frontend authors copy, are never validated.

## Evidence

Re-verified at 56c86168:

- `scripts/docs-smoke.yaml:20-24` lists exactly `README.md`, `QUICKSTART.md`, `SPEC.md` and `docs/INDEX.md`.
- The `docs-smoke` task in `Taskfile.yml` (around `:334`) repeats the same four files in its `sources:`. Because task results are checksum-cached, a doc added to the YAML but not to `sources:` would not trigger a re-run when it changes.
- `docs/INDEX.md` lists these docs, none of which is covered (Audience per INDEX: PROJECT-LAYOUT "Users and contributors", distribution "Users and CI maintainers", resource-parameters "Users and contributors", execution-adapters "Contributors and architects", CI-INTEGRATION "Contributors"):
  - `docs/PROJECT-LAYOUT.md` (`:11`)
  - `docs/distribution.md` (`:12`)
  - `docs/resource-parameters.md` (`:25`)
  - `docs/execution-adapters.md` (`:29`)
  - `docs/CI-INTEGRATION.md` (`:26`)
- The auditor ran docs-smoke with resource-parameters, distribution, execution-adapters and PROJECT-LAYOUT added, and all four passed (4 + 4 + 3 blocks checked). Adding them is free today. The verifier did not re-run this.
- `PROTOCOL.md` has 22 ```` ```json ```` fences, none of which is validated against `protocol/schemas/`.

## Acceptance criteria

- [ ] `docs/resource-parameters.md`, `docs/distribution.md`, `docs/execution-adapters.md`, `docs/PROJECT-LAYOUT.md`, `docs/CI-INTEGRATION.md` and `PROTOCOL.md` are added to `scripts/docs-smoke.yaml` and to the `docs-smoke` task's `sources:`. `python3 scripts/docs-smoke.py` passes when run directly; paste the output into the close note.
- [ ] The `docs-smoke` task's `sources:` also include `protocol/schemas/*.schema.json`, so a schema-only change invalidates the checksum cache once PROTOCOL.md examples are validated against the schemas. Proof at close: after a passing run, touch one field in a schema and show `task docs-smoke` re-executes (not "up to date").
- [ ] Each PROTOCOL.md JSON example is either validated against a schema or explicitly skipped. Selection is by an explicit fence tag, not by guessing from keys: for example ```` ```json protocol=request ```` / ```` ```json protocol=response ```` (or a specific per-message schema name), and ```` ```json illustrative ```` for examples that are deliberately partial. Note that requests carry `command` while responses carry `status` (for example `PROTOCOL.md:18-19`), so a single key-based selector cannot work. An untagged ```` ```json ```` fence in PROTOCOL.md fails docs-smoke. Proof at close: docs-smoke fails when a required field is removed from one validated request example and from one validated response example (paste both failing runs), and passes on the committed text.
- [ ] A unit test in `scripts/test_docs_smoke.py` checks coverage by rules that the committed list can satisfy:
  - every doc in `docs/INDEX.md` whose Audience cell contains "Users" (case-insensitive) is in the docs-smoke list, unless it is in an explicit `exclude:` list in `docs-smoke.yaml` with a reason (subset rule, not equality);
  - the docs-smoke list may contain additional docs (for example `docs/INDEX.md` itself, `docs/execution-adapters.md`, `docs/CI-INTEGRATION.md`, `PROTOCOL.md`) that are not user-audience docs;
  - every doc in the docs-smoke list is also in the `docs-smoke` task's `sources:`.
  Proof: the test fails on a fixture INDEX that adds an uncovered Users-audience doc, and fails on a fixture Taskfile missing one listed doc from `sources:`; it passes on the committed files.

## Suggested approach

Add the docs first, since that is free. Then add a `schema:` mode to docs-smoke for PROTOCOL.md, driven by the fence tag, that loads the schemas with `jsonschema` (already used by `protocol/schemas/test_schema_validation.py`). Validating output-artifact samples in SPEC §5 is handled by spec-s5-contract-table-and-samples and can reuse this mode.

## Out of scope

- Artifact schemas and SPEC §5 samples (artifact-json-schemas, spec-s5-contract-table-and-samples).
- The CLI-surface drift gate (str-wurp).

## Dependencies

- Blocked by: none.
- Related: str-qwua7.9, str-qwua7.9.1, str-qwua7.44, spec-s5-contract-table-and-samples.

## Source

Audit 2026-09-22, finding docs-19 (confirmed, P3). Draft `shatter-docs-ui/29`. Evidence is in `audits/2026-09-22/areas/docs.md` (on branch `audit-2026-09-22` until the audit directory lands on `main`).
