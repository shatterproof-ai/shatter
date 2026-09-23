# Mixed-language scan: the second language sub-scan deletes the first's function artifacts and overwrites summary.json; checkpoint lives elsewhere

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | bug |
| priority | P1 |
| labels | scan,artifacts,resume,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | str-9f6f, str-14en, str-8q1b4, str-4oa1 |
| source findings | artifacts-04, docs-07 (code half) |

<!-- body -->
## Problem

`shatter scan` of a directory with TS and Go files runs one sub-scan per language into the same scan-results dir; each sub-scan wipes `functions/` and restarts indexes, so only the last language's artifacts and summary survive. The checkpoint is written to a different dir that ignores SHATTER_ARTIFACT_DIR, so resume and reports disagree.

## Current code facts / evidence

- `shatter-core/src/scan_orchestrator.rs:593-609` `prepare_fresh_scan_artifact_root` does `remove_dir_all` on functions/, called at :3912 after `compute_scan_id` (:3889) in each per-language pass (str-14en grouping, `shatter-cli/src/commands/scan.rs:1270`).
- `scan_orchestrator.rs:556-564` `scan_root` = resolve_artifact_root + scan-results/<full 64-hex id>.
- `shatter-core/src/checkpoint.rs:196-203` `default_path` hardcodes project_root/shatter-artifacts/scan-results/<id[..16]>; used at scan.rs:1203.
- Repro: `scan mix` (3 TS + 2 Go files): Go wrote functions/00001..00004, TS wrote 00001..00008 into the same dir; afterwards only 8 TS files; summary.json total_functions 8, failed 0, while the report says 12 discovered / 4 failed (`audits/2026-09-22/artifact-samples/scan-summary.json`, `scan-mix.json`).

## Acceptance criteria

- All per-language sub-scans of one invocation share one artifact namespace with global indexes, one summary.json and one manifest; stale-artifact cleanup runs once per scan.
- Checkpoint path resolves through HarnessStorage under the same scan root and full id, honouring SHATTER_ARTIFACT_DIR.
- Two-language scan test: every reported function has an artifact and a summary entry; summary counts equal the report's.
- SPEC §6.2 updated to the real layout (or tracked in the docs drafts).

## Suggested approach

Hoist cleanup and id computation above the per-language loop; route checkpoint through HarnessStorage.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: M

## References

- Audit findings: artifacts-04, docs-07 (code half) (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-9f6f, str-14en, str-8q1b4, str-4oa1
