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

This issue covers the single-file success case (the empty bundle). It needs no spec schema change. Two follow-ups build on it: recording attempted-but-failed functions in the bundle instead of the no-target marker (explore-spec-bundle-failed-functions, which needs a schema change), and multi-file and glob targets keeping only the first file's bundle (multi-file-spec-bundle-first-only).

## Evidence

Line numbers re-checked against `56c86168` (branch `audit-2026-09-22`):

- `shatter-cli/src/commands/explore.rs:6459-6480`: per-target bundles are pushed into `file_spec_bundles` only `if output_path.is_some()`, where `output_path` is the `--spec-out` destination.
- `explore.rs:6643-6682`: the `-o <path>` JSON branch logs `[warn] JSON output for explore writes spec bundle; use --spec-out for explicit spec output`, then writes `file_spec_bundles.first()`. The list is empty when `--spec-out` is not given, so it writes `build_no_target_spec_bundle(...)` with reason `unclassified` (the str-ni32 fallback, whose comment says it is for "analyze/preflight failed before any spec was produced").
- `finalize_explore` (`--from-artifacts`, `explore.rs:3773`) has two separate sinks and does not share the live-path bug. Its `-o *.json` branch (`explore.rs:4006-4021`) builds the bundle from `acc.file_specs` directly and writes **nothing** when `acc.file_specs` is empty (no marker, no warning). Its `--spec-out` branch (`explore.rs:4040-4074`) is the one that falls back to `build_no_target_spec_bundle` when `acc.file_specs` is empty. The criteria below pin both sinks so the fix does not regress them and so the empty `-o *.json` case stops being silent.
- Repro (audit verifier, release binary built from the audit HEAD, fresh dir): `shatter explore 01-arithmetic.ts:classifyNumber --clean -o b.json` → stderr `[batch 1/1] classifyNumber: 30 iters, 4 paths, 3/3 branches (ok)`, then `[warn] JSON output for explore writes spec bundle; use --spec-out…` and `[info] Wrote no-target spec marker (reason=unclassified) to b.json`, exit 0. `b.json` is the marker above. `--spec -o x.json` gives the same result. Transcripts: `audits/2026-09-22/cli-ux-transcripts/explore-bundle{2,3,4}.json`, `explore-o3.err`.
- The same marker was written for a Rust target whose build timed out (`rust-spec.json`, finding artifacts-02), so a failed run and a successful run are indistinguishable in the file.
- Existing test `shatter-cli/tests/explore_no_target_spec.rs` covers only the true no-target case.

## Acceptance criteria

- [ ] Live path: `explore <target> -o out.json` writes a bundle that contains every explored function with its equivalence classes, whether or not `--spec`/`--spec-out` is given. For `01-arithmetic.ts:classifyNumber` this is one function with 4 classes. The warn line goes away or states exactly what was written. `--spec-out` output is unchanged for this case.
- [ ] Live path: the no-target marker is written to `-o *.json` only when analysis found no target (the str-jeen.67 case). This issue does not change what is written when targets were attempted and all failed; that is explore-spec-bundle-failed-functions.
- [ ] `--from-artifacts` (`finalize_explore`): for the same artifacts, `-o out.json` and `--spec-out spec.json` each contain the same functions as the live path would write. When there are no specs, `-o out.json` is no longer silently skipped: it writes the same marker (or failure bundle, once explore-spec-bundle-failed-functions lands) as the `--spec-out` sink, and logs what it wrote.
- [ ] CLI test in `shatter-cli/tests/` (extend `explore_no_target_spec.rs` or add one) covers, for each sink separately (`-o x.json` alone, `--spec-out y.json` alone) and for both the live path and `--from-artifacts`: success, and a true no-target file. At close, show the live `-o x.json` success case failing on current `main` and passing after the fix (test name plus before/after output in the close note).
- [ ] SPEC §2.1's `-o` row says what a `.json` destination contains, which is the spec bundle schema (§5).
- [ ] `task affected` passes, and its `Gates selected` output is recorded.

## Suggested approach

Build the per-file bundle whenever any JSON sink is requested: `-o *.json`, `--spec-out` or `--spec-json`. Share one "collect bundles" step between the live path and `finalize_explore`. Decide "no target" from the per-function summaries (`report_summaries`), not from whether a bundle is empty. Keep the `.first()` behaviour here. Changing it to all files is multi-file-spec-bundle-first-only.

## Out of scope

- Multi-file/glob output keeping only the first file's bundle (multi-file-spec-bundle-first-only).
- Representing attempted-but-failed functions in the bundle, and any spec schema version bump (explore-spec-bundle-failed-functions).
- `-o out.json` suppressing the markdown on stdout, and `--format`/stdout purity (str-qwua7.11, explore-format-flag-ignored).
- Formal JSON Schemas for artifacts (artifact-json-schemas).

## Priority

P1: the documented JSON output of the primary command is wrong on success.

## Type

bug

## Dependencies

- Blocked by: none.
- Blocks: multi-file-spec-bundle-first-only (same writer), explore-spec-bundle-failed-functions (same writer).
- Related: str-ni32 (closed; added the fallback), str-jeen.67 (closed; no-target marker), str-jeen.21 (closed; no-target reason schema), str-zt4v (closed; `-o` multi-format).

## References

Audit 2026-09-22 findings cli-ux-01 and artifacts-02 (both verified P1). Source draft: `drafts/shatter-code/26-explore-json-output-bundles.md` (single-file half; split per report §14 item 8). The failed-function half was split out to explore-spec-bundle-failed-functions after the 2026-09-23 cross-check.
