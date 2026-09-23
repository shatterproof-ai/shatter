# Cross-check review

- **Reviewer:** claude (DEGRADED same-runtime fallback)
- **Artifact type:** issue
- **Mode:** degraded

> **DEGRADED REVIEW.** The counterpart runtime was unavailable, so this review came from an independent agent of the *same* runtime. It shares the original author's model and blind spots; weight it accordingly.

## Findings

DEGRADED same-runtime review (Codex review failed identity validation; cross-check-run.py exit 4). Reviewer: Claude, read-only, independent of the drafting agent. Claims checked against /home/ketan/.local/share/worktrees/shatter/audit-2026-09-22 at HEAD 56c86168 and against the live bd tracker.

## Verification summary

I spot-checked the cited code facts. Unless a finding below says otherwise, they match 56c86168:
- 01: try_resume_function (explore.rs:1209-1232) compares only name, status and deep_fingerprint. The resume hit at :5241-5258 logs `[resumed]` at :5253. render.rs:143-145 labels the explorer from the current opts.is_concolic.
- 02/03: bundles are pushed only `if output_path.is_some()` (:6459-6480). The `-o *.json` branch warns and writes `.first()`, falling back to build_no_target_spec_bundle (:6643-6682). `--spec-out` writes `.first()` with the "Single-target is the primary Make use case" comment (:6703-6704). finalize_explore labels the file from `artifacts.first()` (:4041-4057).
- 04: prepare_fresh_scan_artifact_root does remove_dir_all (:594-610). It is called at :3912 after compute_scan_id at :3889. scan.rs:1270 has the str-14en per-language loop. ScanCheckpoint::default_path hard-codes `shatter-artifacts/scan-results/<id[..16]>`, and the default_path_structure test is at :503.
- 05: classify_verdict (:87-120) has no output input. revalidate.rs:143-147 and :180-186 count ExpectedDrift as confirmed. The SPEC.md:459-465 wording is as quoted.
- 07: snapshot.rs is 1,042 lines, the method line numbers match, lib.rs:78 has `pub mod snapshot`, diff.rs:3 imports snapshot, the args.rs `Diff {` variant is at ~1432, and main.rs:981-997 dispatches it.
- 10: observe.rs:128-147 has the `.max(covered)` clamp. The two pinning tests are at :869 and :881. The span override is at scan_orchestrator.rs:3129. The e2e test is at e2e_concolic_go.rs:354.
- 11: shatter-rust/src/protocol.rs sets instrumentable_line_count to None at every cited line. line_hit_stmt is at instrument.rs:230, used at :260/:289/:317. The parity-matrix entry says rust: not_supported.
- 12: cache.store keys on map.function_id (cache.rs:123), and store_with_fingerprint (:134) documents that scan should use it. Lookups at :4141-4142 and :1600-1601, and stores at :3360/:4931/:1840, match. Both scan-cache transcripts show `0 expected skipped`.
- 13: write_scan_artifact_json only warns and returns on every failure.
- 14: init.rs:82/:91/:143 use println!. maybe_implicit_init is at main.rs:53 and is called only at :318 and :648.
- Tracker IDs: all referenced str-* IDs exist in bd. str-jd0d1, str-9m9o3 and str-4ajhz are missing from the committed .beads/issues.jsonl snapshot but present in the live bd DB. str-8q1b4 shows as closed in bd, although the snapshot still says in_progress.
- Evidence files cited under audits/2026-09-22/ (artifact-samples, cli-ux-transcripts, goals-runs) exist.

## Findings

1. **MAJOR — retire-snapshot-diff (07): the repo-wide `rg` acceptance criterion cannot pass as written.** The pattern `Snapshot::|snapshot::` also matches the unrelated `SourceFileSnapshot::` in shatter-cli/src/commands/run.rs:1318 and shatter-core/src/run_manifest.rs:126, so "no remaining references outside the changelog" is unachievable. The criterion also finds references the Evidence list omits: CONTRIBUTING.md:130 (`snapshot::Snapshot` docs-smoke description), and docs/perf/inventories/rust-shatter-core.txt:2704-2722 (snapshot test inventory, not gated but will be stale). Tighten the pattern (e.g. `\bsnapshot::Snapshot\b|\bSnapshot::(read_from_file|from_behavior_map|write_to_file)|shatter diff\b(?!-)`) and add CONTRIBUTING.md and the perf inventory to the doc list.

2. **MAJOR — revalidate-return-values (05): an undecided product change is written into the acceptance criteria.** "ExpectedDrift ... fails the exit code by default" makes `shatter revalidate` exit 1 after almost any code edit that renumbers branch paths, even when outputs are unchanged. That changes the command's meaning from "outputs regressed" to "anything changed". None of the maintainer decisions D1-D6 cover it. The output-comparison fix (OutputChanged) alone closes the reported defect. Either split the ExpectedDrift exit-code policy into a decision item, or make the criterion "ExpectedDrift is reported separately from confirmed in the summary line" and leave the exit-code default to the maintainer.

3. MINOR — explore-resume-options-key (01): the Problem names `--no-cache` as a result-affecting option, and the whole-dir repro uses it, but the options-hash list in the acceptance criteria omits it. `--no-cache` controls the behavior-map cache, not the explore result, so say explicitly whether it belongs in the key. The issue is also large (hash + provenance label + per-resume info line + walkthrough + SPEC). It is coherent, but consider making the walkthrough change a checklist item that depends on the core fix.

4. MINOR — explore-o-json-empty-bundle (02): the finalize_explore evidence overstates the parallel. finalize_explore's own `-o *.json` branch (explore.rs:4006-4013) builds from `acc.file_specs` without depending on `--spec-out`, so `--from-artifacts` probably does not have the single-file empty-bundle bug. It emits the marker only when no specs exist at all. Keep the criterion as a guard, but reword the evidence so an implementer does not look for a bug that may not be there.

5. MINOR — multi-file-spec-bundle-first-only (03): the Evidence says spec-diff's loader accepts "a single FileSpecBundle or a bare FunctionSpec list". The `SpecInput` enum (diff.rs:44-70) accepts a bundle or a single `FunctionSpec` object, not a list. Correct this, because the suggested approach adds "a third variant" to that enum.

6. MINOR — go-scan-coverage-clamp (10): the title's headline numbers (15/15 for a 67-line function) come from a zolem artifact that the verifier did not re-run, and the root cause of the undersized Go denominator is unknown. The code-level defects (the clamp at observe.rs:146 and the span override at scan_orchestrator.rs:3129) are verified, so the issue stands. The criteria correctly require root-cause and repro first, so this is investigative work. Consider retitling around the verified clamp and span-override defects, with the zolem numbers as supporting evidence.

7. MINOR — behavior-map-cache-keys (12): the criterion adding a `source_file` field to BehaviorMap and a revalidate exit-2 refusal widens the issue into revalidate behaviour. Draft 05 defers to it, so the two are consistent. Still, a field on BehaviorMap is a persisted-format change: note the schema/version implication and whether the parity contract applies. BehaviorMap is core-only, so probably not.

8. MINOR — bundle-level: str-jd0d1, str-9m9o3 and str-4ajhz are absent from the committed .beads/issues.jsonl snapshot but present in bd. The filer script should resolve IDs against bd, not the JSONL. str-8q1b4 is closed in bd, which matches the drafts, but the snapshot says in_progress.

9. MINOR — mixed-language-scan-deletes-artifacts (04): prepare_fresh_scan_artifact_root is skipped when `resume_path` is set (:3911). The criteria should state the expected behaviour for a mixed-language `--resume` run too, which today would append rather than delete. Otherwise the fix could be tested only on the fresh path.

No BLOCKERs. Drafts 06, 08, 09, 11, 13 and 14 are accurate and self-contained. The reopen-notes and notes follow the file-first-then-substitute-ID ordering correctly.

## Verdict

The bundle is ready to file after small edits. The code claims are accurate at 56c86168, and each draft has checkable acceptance criteria with before/after test evidence. Top fixes:
1. Fix draft 07's `rg` criterion (false positives from SourceFileSnapshot) and add CONTRIBUTING.md and the perf inventory to its doc list.
2. Split or soften draft 05's "ExpectedDrift fails the exit code by default" into a maintainer decision. Keep the output-comparison fix as the core.
3. Correct the smaller evidence inaccuracies in 02 (finalize_explore) and 03 (SpecInput shape), and reconcile `--no-cache` in 01.
