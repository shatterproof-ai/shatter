---
slug: spec-s6-layout-and-checkpoint
kind: new
title: "SPEC §6.1/§6.2 describe stale progress-event fields, scan-id derivation and artifact layout"
priority: P2
type: bug
labels: [docs, spec, scan, artifacts, resume, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [mixed-language-scan-deletes-artifacts]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# SPEC §6.1/§6.2 describe stale progress-event fields, scan-id derivation and artifact layout

## Problem

SPEC §6 "Live Output and Resume" describes an older design:

- The progress-event fields in §6.1 do not match what `scan --progress` emits.
- The scan-id derivation in §6.2 is wrong.
- The artifact layout in §6.2 lists only `checkpoint.json` at a 16-hex prefix.

Anyone writing a CI consumer for `--progress`, or looking for a scan's artifacts, follows the SPEC and gets it wrong.

The code half is owned by mixed-language-scan-deletes-artifacts (P1): the checkpoint is written outside `scan_root()`, and each per-language sub-scan clobbers the others' artifacts. This issue documents the layout that exists after that fix. It is blocked by that issue so it describes one layout, not two.

## Evidence

Re-verified at 56c86168:

- **Scan-id derivation.** `SPEC.md:960-970` (§6.2) shows `shatter-artifacts/scan-results/<scan-id-prefix>/checkpoint.json`. It says the prefix is "the first 16 hex characters of a SHA-256 hash computed from the sorted list of source file paths". The code does not work that way:
  - `compute_scan_id_for_targets` in `shatter-core/src/checkpoint.rs:132` hashes `scan_id_v2:` plus (qualified_id, source_file) pairs.
  - `scan_root` (`scan_orchestrator.rs:557`ff.) resolves `resolve_artifact_root` (`shatter-core/src/harness_storage.rs:77`, which honours `SHATTER_ARTIFACT_DIR`) plus `scan-results/<full 64-hex id>/`.
  - `manifest.json`, `run-status.{json,tsv}`, `summary.json` and `functions/` are written under that `scan_root`.
- **Checkpoint location.** `ScanCheckpoint::default_path` (`shatter-core/src/checkpoint.rs:197`ff., called from `shatter-cli/src/commands/scan.rs:1203`) hard-codes `project_root/shatter-artifacts/scan-results/<first 16 hex>/checkpoint.json`. That is a second directory for the same scan. Observed directory: `scan-results/4dfd2b95…36f8/`.
- **Progress events.** `scan --progress` events carry `function` = `/abs/path/c.ts::classifyNumber` (a qualified id), plus `qualified_id` and `display_name`. SPEC's §6.1 example (`SPEC.md:900-954`) uses a bare function name and documents neither extra field. The auditor observed this by running the command; the verifier checked it at code level only. Re-run `shatter scan examples/ts --progress` to confirm before editing.

## Acceptance criteria

- [ ] §6.1 documents every field of each progress event type as emitted after mixed-language-scan-deletes-artifacts lands. `function` is the qualified id, and `qualified_id` and `display_name` are included. The example is pasted from a real `shatter scan <examples dir> --progress` run.
- [ ] §6.2 documents the full per-scan layout: `scan-results/<full id>/{checkpoint.json, manifest.json, summary.json, run-status.json, run-status.tsv, functions/}`. It also documents the v2 id derivation, the `SHATTER_ARTIFACT_DIR` override, and how a mixed-language scan is laid out.
- [ ] §6.3 resume semantics states what happens to checkpoints written at the old 16-hex location: migrated, or not resumed. This matches what mixed-language-scan-deletes-artifacts implemented.
- [ ] The §6 samples sit in tagged fences that docs-smoke checks. At minimum, the progress-event JSON lines must parse and contain the documented keys. Proof at close: docs-smoke fails when a documented key is removed from the sample. Run it directly and paste the output.
- [ ] A §8 changelog row is added and the `Last updated` header is bumped.

## Suggested approach

After the blocker lands, run a two-language scan with `--progress` and `SHATTER_ARTIFACT_DIR` set to a temp directory. Copy the real event lines and the `find` listing of the scan directory into §6, then trim.

## Out of scope

- The code changes: checkpoint under `scan_root()`, and the shared namespace for per-language sub-scans. Both belong to mixed-language-scan-deletes-artifacts.
- Keying resume by options and resume report parity (str-8q1b4).
- The §5 artifact table (spec-s5-contract-table-and-samples).

## Dependencies

- Blocked by: mixed-language-scan-deletes-artifacts.
- Related: str-8q1b4, str-ck05, spec-s5-contract-table-and-samples.

## Source

Audit 2026-09-22, finding docs-07 (confirmed, P2; docs half). Draft `shatter-docs-ui/06`. Evidence is in `audits/2026-09-22/areas/docs.md` (on branch `audit-2026-09-22` until the audit directory lands on `main`).
