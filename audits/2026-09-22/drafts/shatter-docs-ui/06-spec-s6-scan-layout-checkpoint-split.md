# SPEC §6 progress events and scan layout are stale; one scan's checkpoint and artifacts go to two different directories

- Priority: P2
- Type: bug
- Labels: docs,spec,scan,artifacts,resume
- Tracker action: new issue
- Related: str-8q1b4 (in_progress, resume report parity), str-ck05
- Source findings: audit 2026-09-22 docs-07 (confirmed). The same checkpoint split appears in L5 finding artifacts-04 (a mixed-language scan clobbers artifacts). Link that issue if it is filed.

<!-- body -->
## Problem
SPEC §6.1 (progress events) and §6.2 (artifact layout) describe an older design. The code also writes a scan's checkpoint to a different directory from the rest of its artifacts, and ignores the configured artifact root for the checkpoint.

## Current code facts
- `shatter-core/src/checkpoint.rs:197-203` `Checkpoint::default_path` hard-codes `project_root/shatter-artifacts/scan-results/<first 16 hex of scan_id>/checkpoint.json`. It is used by `shatter-cli/src/commands/scan.rs:1203`.
- `shatter-core/src/scan_orchestrator.rs:556-564` `scan_root` uses `resolve_artifact_root` (honours `SHATTER_ARTIFACT_DIR`) plus `scan-results/<full 64-hex id>/` for manifest.json, run-status.{json,tsv}, summary.json and functions/.
- The scan id comes from `compute_scan_id_for_targets`, which hashes `scan_id_v2:` plus (qualified_id, source_file). `SPEC.md:966` says "first 16 hex characters … from the sorted list of source file paths".
- `scan --progress` events carry `function` = `/abs/path/c.ts::classifyNumber` plus `qualified_id` and `display_name`. SPEC's example (`SPEC.md:900-1016`) uses a bare name and documents neither extra field.

## Acceptance criteria
- The checkpoint is written under `scan_root()` (same full id, same configured artifact root). A test asserts that every scan output (checkpoint, manifest, run-status, summary, functions/) shares one directory, including when `SHATTER_ARTIFACT_DIR` is set.
- `--resume` still finds checkpoints. Either migrate old 16-hex locations or document that older checkpoints are not resumed.
- SPEC §6.1 documents the current progress-event fields. §6.2 documents the full layout and the v2 id derivation.
- A §8 changelog row is added.

## Scope
In: checkpoint path, SPEC §6. Out: mixed-language sub-scans deleting each other's artifacts (a separate L5 bug), and resume option-keying.
