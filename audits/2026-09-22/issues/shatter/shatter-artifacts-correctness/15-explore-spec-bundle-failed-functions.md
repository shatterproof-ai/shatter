---
slug: explore-spec-bundle-failed-functions
kind: new
title: "Spec bundles cannot say a function was attempted and failed: a failed explore writes the same no_targets/unclassified marker as an empty file"
priority: P2
type: bug
labels: [explore, artifacts, spec, schema, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [explore-o-json-empty-bundle]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Spec bundles cannot say a function was attempted and failed: a failed explore writes the same no_targets/unclassified marker as an empty file

## Problem

When `shatter explore` attempts a function and exploration fails (analysis or preflight failure, harness build failure or build timeout, harness crash, exploration timeout), the JSON spec outputs (`-o *.json`, `--spec-out`) write the str-ni32/str-jeen.67 no-target marker:

```json
{"version":1,"file":"…","functions":[],"status":"no_targets","no_target_reason":"unclassified"}
```

That marker means "this file has nothing to explore". Batch tooling and spec-diff (the regression tool, decision D2) therefore cannot tell "nothing to explore" from "exploration broke". The process exit code does distinguish them (`decide_explore_exit_status`, str-960w/str-ni32), but the file on disk does not, and the file is what CI keeps.

The spec bundle schema has no way to express a failure. `FileSpecBundleStatus` (`shatter-core/src/spec.rs:227-232`) has only `Ok` and `NoTargets`, and `FunctionSpec` (`spec.rs:304-328`) has no execution-status field. This issue defines that representation, versions it and updates consumers.

This was split out of explore-o-json-empty-bundle, which fixes the success case without a schema change.

## Evidence

Line numbers re-checked against `56c86168` (branch `audit-2026-09-22`):

- `shatter-core/src/spec.rs:227-232` `FileSpecBundleStatus { Ok, NoTargets }`. `spec.rs:251` `SPEC_SCHEMA_VERSION = 1`, with a bump policy (`spec.rs:234-250`) that requires a bump for any added field.
- `shatter-core/src/spec.rs:304-328` `FunctionSpec`: `function_name`, `location`, `classes`, `iterations`, `lines_covered`, `total_lines`, `invariants`, `fingerprint`, `nondeterministic_fields`. No status.
- `shatter-cli/src/commands/explore.rs:744-780` `decide_explore_exit_status` already classifies per-target outcomes from `ExploreSummary` buckets: `completed`, `build_failed`, `runtime_failed`, `timed_out`, and a `parser_failure…` status for analyze/preflight failure.
- `explore.rs:6643-6682` (live `-o *.json`) and `explore.rs:4040-4074` (`finalize_explore` `--spec-out`) fall back to `build_no_target_spec_bundle` whenever there are no specs, whatever the reason.
- Observed: a Rust target whose harness build timed out wrote the marker (`audits/2026-09-22/artifact-samples/rust-spec.json`, audit finding artifacts-02), identical to a true no-target file.

## Acceptance criteria

- [ ] Before implementing, the issue records the chosen representation. The default proposal, which the implementer may change only with a written reason in the issue:
  - `FileSpecBundle` gains `failed_functions: Vec<FailedFunction>` (serialized only when non-empty), where `FailedFunction { function_name, location, failure_class, message }`.
  - `failure_class` is a closed snake_case enum: `analyze_failed`, `build_failed`, `build_timed_out`, `harness_crashed`, `exploration_timed_out`. These are *exploration* failures. A target function that throws or panics on some inputs is **not** a failure: those remain ordinary `SpecClass` entries in `functions`.
  - `FileSpecBundleStatus` gains `Failed`, used when at least one function was attempted and none produced a spec. A bundle with some successes and some failures has status `Ok` (or absent) and a non-empty `failed_functions`.
  - `SPEC_SCHEMA_VERSION` is bumped, with the rationale comment the bump policy requires.
- [ ] `no_targets` is written only when analysis succeeded and found no target. Every attempted-but-failed function appears in `failed_functions` with its class, on both the live path and `--from-artifacts`, for both `-o *.json` and `--spec-out`.
- [ ] Exit codes are unchanged (`decide_explore_exit_status` stays the source of truth), and a unit test pins that the bundle status and the exit decision agree for each summary shape (all ok, mixed, all failed, no targets, analyze failed).
- [ ] spec-diff: a function that is a spec in the old bundle and a `failed_functions` entry in the new one is reported as `failed` (not `removed`) and counts as a regression for the exit code. A function failed on both sides is reported, not diffed. Reading a v1 bundle still works. Unit tests cover both.
- [ ] CLI test in `shatter-cli/tests/`: an attempted-but-failed function (use a fixture that deterministically fails, for example a Go file whose package does not build) writes `status: failed` with a `failed_functions` entry of the right class, and a true no-target file still writes `no_targets`. At close, show the failed case writing `no_targets/unclassified` on current `main` and the new shape after the fix.
- [ ] SPEC §5 documents `failed_functions`, the failure classes, the new status value and the version bump. SPEC §8 changelog has a row.
- [ ] `task affected` passes, and its `Gates selected` output is recorded.

## Suggested approach

Build `failed_functions` from the same per-function summaries `decide_explore_exit_status` reads, in the shared bundle-collection step that explore-o-json-empty-bundle introduces. Map summary buckets to failure classes in one function with a unit test per bucket.

## Out of scope

- The success case writing an empty bundle (explore-o-json-empty-bundle).
- Multi-file bundles (multi-file-spec-bundle-first-only). If that lands first, `failed_functions` lives on each per-file bundle.
- Generated JSON Schemas (artifact-json-schemas) and the SPEC §5 producer/consumer table (spec-s5-contract-table-and-samples), which should describe the shape chosen here.

## Priority

P2: once explore-o-json-empty-bundle lands, a successful run is distinguishable and the exit code already flags failures; the remaining defect is that the persisted artifact mislabels failures as "nothing to explore".

## Type

bug

## Dependencies

- Blocked by: explore-o-json-empty-bundle (shared bundle writer).
- Related: multi-file-spec-bundle-first-only, str-ni32 (closed), str-jeen.67 (closed), str-jeen.21 (closed), str-960w (closed), artifact-json-schemas, spec-s5-contract-table-and-samples.

## References

Audit 2026-09-22 finding artifacts-02 (failure half). Split from explore-o-json-empty-bundle after the 2026-09-23 Codex cross-check (finding "02: failed-function output requires an undefined schema change").
