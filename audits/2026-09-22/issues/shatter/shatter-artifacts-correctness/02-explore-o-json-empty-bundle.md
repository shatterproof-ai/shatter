---
slug: explore-o-json-empty-bundle
kind: new
title: "`explore -o out.json` writes an empty no_targets/unclassified spec bundle after a successful run and exits 0"
priority: P1
type: bug
labels: [cli, explore, artifacts, spec, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# `explore -o out.json` writes an empty no_targets/unclassified spec bundle after a successful run and exits 0

## Problem

A successful single-function `shatter explore <file>:<fn> -o out.json` explores every path, exits 0, and writes this to `out.json`:

```json
{"version":1,"file":"01-arithmetic.ts","functions":[],"status":"no_targets","no_target_reason":"unclassified"}
```

The file claims nothing was explored. Batch tooling that reads `-o *.json` gets a false no-target result. SPEC §2.1 (SPEC.md:160) documents `-o PATH` as "Write a report; format inferred from extension (`.html`, `.md`, `.json`, `.txt`)". SPEC also says JSON on stdout is not offered, and tells users to write `-o file.json` instead (SPEC.md:162). That makes this the documented JSON path.

The cause: per-file spec bundles are collected only when the `--spec-out` path is set. The `-o *.json` branch then finds no bundle and falls back to the str-ni32 "analyze failed" no-target marker, even though analysis and exploration succeeded.

This issue covers the single-file case (the empty bundle). Multi-file and glob targets keeping only the first file's bundle is multi-file-spec-bundle-first-only, which builds on this fix.

## Evidence

Line numbers re-checked against `56c86168` (branch `audit-2026-09-22`):

- `shatter-cli/src/commands/explore.rs:6459-6480`: per-target bundles are pushed into `file_spec_bundles` only `if output_path.is_some()`, where `output_path` is the `--spec-out` destination.
- `explore.rs:6643-6682`: the `-o <path>` JSON branch logs `[warn] JSON output for explore writes spec bundle; use --spec-out for explicit spec output`, then writes `file_spec_bundles.first()`. The list is empty when `--spec-out` is not given, so it writes `build_no_target_spec_bundle(...)` with reason `unclassified` (the str-ni32 fallback, whose comment says it is for "analyze/preflight failed before any spec was produced").
- `explore.rs:4041-4075` (`finalize_explore`, `--from-artifacts`): the parallel writer emits the same marker when `acc.file_specs` is empty.
- Repro (audit verifier, release binary built from the audit HEAD, fresh dir): `shatter explore 01-arithmetic.ts:classifyNumber --clean -o b.json` → stderr `[batch 1/1] classifyNumber: 30 iters, 4 paths, 3/3 branches (ok)`, then `[warn] JSON output for explore writes spec bundle; use --spec-out…` and `[info] Wrote no-target spec marker (reason=unclassified) to b.json`, exit 0. `b.json` is the marker above. `--spec -o x.json` gives the same result. Transcripts: `audits/2026-09-22/cli-ux-transcripts/explore-bundle{2,3,4}.json`, `explore-o3.err`.
- The same marker was written for a Rust target whose build timed out (`rust-spec.json`, finding artifacts-02), so a failed run and a successful run are indistinguishable in the file.
- Existing test `shatter-cli/tests/explore_no_target_spec.rs` covers only the true no-target case.

## Acceptance criteria

- [ ] `explore <target> -o out.json` writes a bundle that contains every explored function with its equivalence classes, whether or not `--spec`/`--spec-out` is given. For `01-arithmetic.ts:classifyNumber` this is one function with 4 classes. The warn line goes away or states exactly what was written.
- [ ] The no-target marker is written only when no target was attempted (analysis found nothing, or analysis failed). Functions that were attempted but failed are recorded in the bundle as failed with a failure class, not as `no_targets/unclassified`. A failed run exits non-zero as str-ni32 requires.
- [ ] The same holds for `--from-artifacts` (`finalize_explore`).
- [ ] CLI test in `shatter-cli/tests/` (extend `explore_no_target_spec.rs` or add one) covers the three cases: success with `-o x.json` and no `--spec-out`, a true no-target file, and an attempted-but-failed function. At close, show the success case failing on current `main` and passing after the fix.
- [ ] SPEC §2.1's `-o` row says what a `.json` destination contains, which is the spec bundle schema (§5).
- [ ] `task affected` passes, and its `Gates selected` output is recorded.

## Suggested approach

Build the per-file bundle whenever any JSON sink is requested: `-o *.json`, `--spec-out` or `--spec-json`. Share one "collect bundles" step between the live path and `finalize_explore`. Decide between "no target" and "failed" from the per-function summaries (`report_summaries`), not from whether a bundle is empty. Keep the `.first()` behaviour here. Changing it to all files is the follow-up issue.

## Out of scope

- Multi-file/glob output keeping only the first file's bundle (multi-file-spec-bundle-first-only).
- `-o out.json` suppressing the markdown on stdout, and `--format`/stdout purity (str-qwua7.11, explore-format-flag-ignored).
- Formal JSON Schemas for artifacts (artifact-json-schemas).

## Priority

P1: the documented JSON output of the primary command is wrong on success.

## Type

bug

## Dependencies

- Blocked by: none.
- Blocks: multi-file-spec-bundle-first-only (same writer).
- Related: str-ni32 (closed; added the fallback), str-jeen.67 (closed; no-target marker), str-jeen.21 (closed; no-target reason schema), str-zt4v (closed; `-o` multi-format).

## References

Audit 2026-09-22 findings cli-ux-01 and artifacts-02 (both verified P1). Source draft: `drafts/shatter-code/26-explore-json-output-bundles.md` (single-file half; split per report §14 item 8).
