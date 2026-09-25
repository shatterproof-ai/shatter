---
slug: artifact-json-schemas
kind: new
title: "Publish JSON Schemas for Shatter's output artifacts, generated from the serde types and checked for drift"
priority: P2
type: task
labels: [docs, schema, artifacts, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Publish JSON Schemas for Shatter's output artifacts, generated from the serde types and checked for drift

## Problem

No output artifact that Shatter writes has a published schema: not the spec bundles, the scan report JSON, the explore per-function artifacts, the scan `summary.json`/`manifest.json`/`run-status.json`, or the checkpoint. `protocol/GOVERNANCE.md` covers only frontend protocol messages. As a result:

- SPEC §5 samples are written by hand and have drifted from real output (spec-s5-contract-table-and-samples).
- Consumer commands each hard-code their own guess at the shape. `compare` rejects the `--spec-out` bundle (spec-json-shapes-compare).
- Plugin skills cite paths and shapes that Shatter never writes. For example, shatter-agents `interpret-shatter-spec` SKILL.md:27 expects `shatter-artifacts/<name>.spec.json`.

## Evidence

Re-verified at 56c86168:

- `protocol/schemas/` holds 22 `*.schema.json` files, all frontend-protocol types (request, response, sym-expr, type-info and so on), plus `test_schema_validation.py`. There are no output-artifact schemas.
- No crate depends on `schemars` (`grep -n schemars Cargo.toml */Cargo.toml` finds 0 matches).
- These are the serde types for the artifacts:
  - `shatter-core/src/spec.rs:259` `FileSpecBundle` (`{version, file, functions[]}`, written by `explore --spec-out`)
  - `shatter-core/src/spec.rs:304` `FunctionSpec`
  - `shatter-core/src/report.rs:1108` `ScanReport`
  - `shatter-core/src/scan_orchestrator.rs:774` `ScanSummary` (`summary.json`)
  - `shatter-core/src/status_export.rs:258` `RunStatus` (`run-status.json`)
  - `shatter-core/src/checkpoint.rs:29` `ScanCheckpoint`
  - `shatter-core/src/behavior.rs:214` `BehaviorMap`
  - the scan manifest writer in `scan_orchestrator.rs`
  - the explore per-function artifact written under `explore-results/` (`shatter-cli/src/commands/explore.rs:1067`)
- The real scan JSON keys include `discovered_inputs`, `behavior_clusters`, `constraint_stats` and `completion_outcome`, but SPEC §5.6 says the JSON holds "per-function behavior maps, coverage, and analysis". Samples are in `audits/2026-09-22/artifact-samples/`.

## Acceptance criteria

- [ ] JSON Schemas (draft 2020-12) are generated from the Rust serde types into `protocol/schemas/artifacts/`, not written by hand. `schemars` derive, or an equivalent, lives behind a feature or in a small generator binary or test. The set covers at least:
  - FileSpecBundle
  - FunctionSpec
  - the scan report
  - scan summary, manifest and run-status
  - the scan checkpoint
  - the explore per-function artifact
  - the staged-pipeline JSON outputs: Stage 1 observation JSON (`observe` / `explore` observation output), Stage 2 analysis JSON (`analyze --output`), and the `solve` output
  - BehaviorMap, if it is still a user-facing artifact after retire-snapshot-diff; otherwise record why it is omitted
- [ ] `protocol/schemas/artifacts/README.md` holds an inventory with one line per JSON artifact that spec-s5-contract-table-and-samples will list: either its schema file, or "no schema" with a reason (for example, no stable serde type yet). Non-JSON outputs (Markdown, HTML, text, YAML specs) are listed as "not applicable". spec-s5-contract-table-and-samples uses this inventory as its schema column, so the two issues cannot disagree.
- [ ] Each schema has `$id`, `title` and a `version` or `const` field that matches the artifact's own version field where one exists (for example `FileSpecBundle.version`).
- [ ] A `--check` mode regenerates into a temporary directory and fails on any difference. It is wired into `task parity` or `task check-static`, and its inputs are listed in that task's `sources:` so checksum caching cannot skip it after a type change.
  - Proof at close: a forced run (`--check` invoked directly, output pasted into the close note) that fails after a deliberate one-field change to `ScanSummary` and passes once the schema is regenerated.
- [ ] A test validates real producer output against the schemas. It runs `explore --spec-out`, a small `scan`, and the staged commands on `examples/` fixtures, then validates each written artifact that has a schema. Because it needs built frontends, it runs in `task e2e` (not `check-static`); name the task step in the close note and list the schema directory and the test file in that task's `sources:`. Proof at close: the validator fails on a hand-edited, non-conforming artifact (paste the failing run) and passes on real output (paste the direct, uncached run).
- [ ] `protocol/GOVERNANCE.md` (or a short `protocol/schemas/artifacts/README.md`) states that output-artifact shape changes require regenerating the schemas and updating the relevant SPEC §5 row.

## Suggested approach

1. Add `schemars` derives next to the existing `Serialize`/`Deserialize` derives on the types above, feature-gated so release builds do not pay for them.
2. Add a generator (a `#[test]` with an `UPDATE_SCHEMAS=1` mode, or a `shatter-core` example binary) plus `--check`.
3. Reuse the validation pattern in `protocol/schemas/test_schema_validation.py` for the producer-output test.

## Out of scope

- The SPEC §5 producer/consumer table and the rewritten samples. That is spec-s5-contract-table-and-samples, which this issue blocks.
- Making `compare` read bundles, and a shared spec reader (spec-json-shapes-compare).
- The `shatter diff` / Snapshot path, which is being retired (retire-snapshot-diff, D2). Do not generate a Snapshot schema.
- Frontend protocol schema cross-validation (str-2fjn, str-ndb.4).
- Pointing shatter-agents skills at the schemas. That is a follow-up in the shatter-agents epic once the schemas exist.

## Dependencies

- Blocked by: none.
- Blocks: spec-s5-contract-table-and-samples.
- Related: retire-snapshot-diff (D2: no Snapshot schema), spec-json-shapes-compare, str-qwua7.9, str-qwua7.21.1, str-wurp, str-2fjn.

## Source

Audit 2026-09-22, finding artifacts-10 (confirmed, P2). The schema half of draft `shatter-docs-ui/05`. Evidence is in `audits/2026-09-22/areas/artifacts.md` (on branch `audit-2026-09-22` until the audit directory lands on `main`).
