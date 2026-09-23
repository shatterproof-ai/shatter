---
slug: mixed-language-scan-deletes-artifacts
kind: new
title: "Mixed-language scan: each per-language sub-scan deletes the previous one's function artifacts and overwrites summary.json; the checkpoint is written to a different directory"
priority: P1
type: bug
labels: [scan, artifacts, resume, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Mixed-language scan: each per-language sub-scan deletes the previous one's function artifacts and overwrites summary.json; the checkpoint is written to a different directory

## Problem

For a directory that contains more than one language, `shatter scan` runs one sub-scan per language (str-14en). All the sub-scans write into the same `scan-results/<id>/` directory. Each sub-scan deletes `functions/` and restarts the artifact index at `00001`, so only the last language's per-function artifacts and `summary.json` survive. The report on stdout still counts every language. Resume and `--from-artifacts` read the surviving artifacts, so they disagree with the report.

Separately, the scan checkpoint is written to `project_root/shatter-artifacts/scan-results/<first 16 hex of id>/checkpoint.json`. Every other scan output goes to `resolve_artifact_root()/scan-results/<full 64-hex id>/`, which honours `SHATTER_ARTIFACT_DIR`. One scan's state is therefore split across two directories, and the checkpoint ignores the configured artifact root.

## Evidence

Line numbers re-checked against `56c86168` (branch `audit-2026-09-22`):

- `shatter-core/src/scan_orchestrator.rs:594-610` `prepare_fresh_scan_artifact_root` runs `remove_dir_all(scan_root/functions)`.
- `scan_orchestrator.rs:3889` computes `scan_id` and `:3912` calls `prepare_fresh_scan_artifact_root` inside `parallel_scan_with_progress` (:3806). `shatter-cli/src/commands/scan.rs:1270` (str-14en) groups analyses by language and runs the parallel scan once per language, so the cleanup runs once per language.
- `scan_orchestrator.rs:557-565` `scan_root` = `resolve_artifact_root` + `scan-results/<full id>`. `scan_artifact_path` (:586) numbers artifacts from a per-sub-scan counter.
- `shatter-core/src/checkpoint.rs:197-204` `ScanCheckpoint::default_path` hard-codes `project_root.unwrap_or(".")/shatter-artifacts/scan-results/<scan_id[..16]>/checkpoint.json`. It is used at `shatter-cli/src/commands/scan.rs:1203`. The test `default_path_structure` (:503) pins this layout.
- Repro (`scan mix`: 3 TS + 2 Go files): `scan-mix.err` shows all 12 artifact writes going to the same `scan-results/896e5a62…/functions/` dir. Go wrote `00001..00004_*.go__*.json`, then TS wrote `00001..00008`. Afterwards only the 8 TS artifacts exist. `summary.json` says `total_functions: 8, failed: 0`, and the report says 12 discovered, 4 failed (`audits/2026-09-22/artifact-samples/scan-summary.json` vs `scan-mix.json`, `scan-mix.err`).

## Acceptance criteria

- [ ] One scan invocation has one artifact namespace. Stale-artifact cleanup and scan-id computation run once, before the per-language loop. Artifact indexes are global across languages, and there is one `summary.json` and one `manifest.json` covering every language.
- [ ] The checkpoint is written under `scan_root()` (the same full id and configured artifact root, honouring `SHATTER_ARTIFACT_DIR`).
- [ ] `--resume auto` still finds a checkpoint. Either it also looks at the legacy 16-hex location once, or the changelog says that checkpoints from older versions are not resumed. Pick one and test it.
- [ ] Integration test: scan a TS + Go fixture dir, with and without `SHATTER_ARTIFACT_DIR`. Every function in the report has an artifact file and a summary entry, summary counts equal the report's counts, and checkpoint, manifest, run-status, summary and `functions/` share one directory. At close, show the test failing on current `main` and passing after the fix.
- [ ] `task affected` passes, and its `Gates selected` output is recorded.

## Suggested approach

Hoist `compute_scan_id` over the full target set and the `prepare_fresh_scan_artifact_root` call out of the per-language pass in `scan.rs`. Pass an artifact-index offset (or a shared counter) and a shared summary accumulator into each pass. Replace `ScanCheckpoint::default_path` with a helper that takes the resolved `scan_root`, so the checkpoint goes through the same storage resolution as the other artifacts. The progress totals were already made global for mixed scans in str-4oa1. Reuse that plumbing.

## Out of scope

- SPEC §6.1/§6.2 (progress-event fields, artifact layout, v2 id derivation). That is spec-s6-layout-and-checkpoint, which is blocked by this issue and documents the layout this issue produces.
- Artifact file naming (scan-artifact-filenames-abs-path).
- Explore resume keying (explore-resume-options-key).

## Priority

P1: a mixed-language scan silently loses most of its per-function artifacts, and resume/summary disagree with the report.

## Type

bug

## Dependencies

- Blocked by: none.
- Blocks: spec-s6-layout-and-checkpoint.
- Related: str-14en (closed; introduced per-language passes), str-9f6f (closed; stale-artifact cleanup), str-4oa1 (closed; mixed-scan progress totals), str-8q1b4 (closed; resume report parity).

## References

Audit 2026-09-22 findings artifacts-04 (verified P1) and docs-07 (code half: the checkpoint location). Source draft: `drafts/shatter-code/34-mixed-language-scan-clobbers-artifacts.md`.
