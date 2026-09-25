---
slug: mixed-language-scan-deletes-artifacts
kind: new
title: "Mixed-language scan: each per-language sub-scan deletes the previous one's function artifacts and overwrites summary.json; the `--resume auto` checkpoint is written to a different directory"
priority: P1
type: bug
labels: [scan, artifacts, resume, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Mixed-language scan: each per-language sub-scan deletes the previous one's function artifacts and overwrites summary.json; the `--resume auto` checkpoint is written to a different directory

## Problem

For a directory that contains more than one language, `shatter scan` runs one sub-scan per language (str-14en). All the sub-scans write into the same `scan-results/<id>/` directory. Each sub-scan deletes `functions/` and restarts the artifact index at `00001`, so only the last language's per-function artifacts and `summary.json` survive. The report on stdout still counts every language. Resume and `--from-artifacts` read the surviving artifacts, so they disagree with the report.

Separately, when `--resume auto` creates or discovers a checkpoint, it uses `project_root/shatter-artifacts/scan-results/<first 16 hex of id>/checkpoint.json`. Every other scan output goes to `resolve_artifact_root()/scan-results/<full 64-hex id>/`, which honours `SHATTER_ARTIFACT_DIR`. One scan's state is therefore split across two directories, and the auto checkpoint ignores the configured artifact root. (A scan without `--resume` writes no checkpoint, and `--resume PATH` deliberately uses the path the user gave; neither is in question.)

## Evidence

Line numbers re-checked against `56c86168` (branch `audit-2026-09-22`):

- `shatter-core/src/scan_orchestrator.rs:594-610` `prepare_fresh_scan_artifact_root` runs `remove_dir_all(scan_root/functions)`.
- `scan_orchestrator.rs:3889` computes `scan_id` and `:3912` calls `prepare_fresh_scan_artifact_root` inside `parallel_scan_with_progress` (:3806). `shatter-cli/src/commands/scan.rs:1270` (str-14en) groups analyses by language and runs the parallel scan once per language, so the cleanup runs once per language.
- `scan_orchestrator.rs:557-565` `scan_root` = `resolve_artifact_root` + `scan-results/<full id>`. `scan_artifact_path` (:586) numbers artifacts from a per-sub-scan counter.
- `shatter-core/src/checkpoint.rs:197-204` `ScanCheckpoint::default_path` hard-codes `project_root.unwrap_or(".")/shatter-artifacts/scan-results/<scan_id[..16]>/checkpoint.json`, and `auto_discover` (:191-194) looks only there. Both are reached only from the `ResumeDirective::Auto` arm in `shatter-cli/src/commands/scan.rs:1192-1214`. With no `--resume`, `resolved_resume_path` is `None` and no checkpoint is written; `--resume off` disables it; `--resume PATH` uses `PATH` as given (:1215-1224). The test `default_path_structure` (`checkpoint.rs:503`) pins the auto layout.
- `scan_orchestrator.rs:3911-3913`: `prepare_fresh_scan_artifact_root` is skipped whenever `resume_path` is set, so under `--resume` the per-language passes do not delete each other's `functions/`, but they still restart the artifact index and each overwrites `summary.json`.
- Repro (`scan mix`: 3 TS + 2 Go files): `scan-mix.err` shows all 12 artifact writes going to the same `scan-results/896e5a62…/functions/` dir. Go wrote `00001..00004_*.go__*.json`, then TS wrote `00001..00008`. Afterwards only the 8 TS artifacts exist. `summary.json` says `total_functions: 8, failed: 0`, and the report says 12 discovered, 4 failed (`audits/2026-09-22/artifact-samples/scan-summary.json` vs `scan-mix.json`, `scan-mix.err`).

## Acceptance criteria

- [ ] One scan invocation has one artifact namespace, with and without `--resume`. Stale-artifact cleanup (fresh runs only) and scan-id computation run once, before the per-language loop. Artifact indexes are global across languages, and there is one `summary.json` and one `manifest.json` covering every language.
- [ ] Under `--resume auto`, the checkpoint is created and discovered under `scan_root()` (the same full id and configured artifact root, honouring `SHATTER_ARTIFACT_DIR`).
- [ ] Unchanged behaviour, pinned by tests: a scan without `--resume` writes no checkpoint; `--resume off` writes none; `--resume PATH` reads and writes exactly `PATH`, not a file under `scan_root()`.
- [ ] Legacy auto checkpoints: either `--resume auto` also looks at the legacy 16-hex location once, or the changelog says that checkpoints from older versions are not resumed. Pick one and test it.
- [ ] Integration test in `shatter-cli/tests/` on a TS + Go fixture dir, run with and without `SHATTER_ARTIFACT_DIR`:
  - fresh scan (no `--resume`): every function in the report has an artifact file and a summary entry; summary counts equal the report's counts; manifest, run-status, summary and `functions/` share one directory; no checkpoint file exists;
  - `--resume auto`, run twice: the checkpoint is in that same directory; the second run resumes functions from both languages; and after it the summary still covers both languages with counts equal to the report's.

  At close, show the fresh-scan case failing on current `main` and passing after the fix (test name plus before/after output in the close note).
- [ ] `task affected` passes, and its `Gates selected` output is recorded.

## Suggested approach

Hoist `compute_scan_id` over the full target set and the `prepare_fresh_scan_artifact_root` call out of the per-language pass in `scan.rs`. Pass an artifact-index offset (or a shared counter) and a shared summary accumulator into each pass. Replace `ScanCheckpoint::default_path`/`auto_discover` with helpers that take the resolved `scan_root`, so the auto checkpoint goes through the same storage resolution as the other artifacts. Leave the `ResumeDirective::Path` arm alone. The progress totals were already made global for mixed scans in str-4oa1. Reuse that plumbing.

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
