---
slug: multi-file-spec-bundle-first-only
kind: new
title: "Multi-file and glob explore write only the first file's spec bundle to -o *.json / --spec-out; a glob writes a no_targets marker"
priority: P1
type: bug
labels: [cli, explore, artifacts, spec, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [explore-o-json-empty-bundle]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Multi-file and glob explore write only the first file's spec bundle to -o *.json / --spec-out; a glob writes a no_targets marker

## Problem

`shatter explore` accepts several files or a glob. Its JSON outputs (`-o *.json`, `--spec-out`) still write one `FileSpecBundle`, the first file's, and drop the rest. A glob over a directory writes only a `no_targets` marker for the first file, while the markdown report for the same run lists every function. spec-diff, which decision D2 makes the regression tool, consumes `--spec-out` and silently sees a fraction of the explored code.

## Evidence

Line numbers re-checked against `56c86168` (branch `audit-2026-09-22`):

- `shatter-cli/src/commands/explore.rs:6648` (`-o <path>.json` branch) and `:6703` (`--spec-out` branch) both write `file_spec_bundles.first()`. The comment at :6704 says "Single-target is the primary Make use case; write the first bundle."
- `explore.rs:4041-4057` (`finalize_explore`, `--from-artifacts`) builds one bundle from all artifacts' specs but labels it with `artifacts.first()`'s file. Functions from different files are merged under one `file`.
- Repro (audit verifier): `shatter explore 01-arithmetic.ts 02-strings.ts -o out.json --spec-out spec.json` → stderr `Wrote spec bundle (2 function(s)) to spec.json`. `classifyString` (in 02-strings.ts) appears 0 times in either file, although artifacts were written for all 4 functions.
- Repro: `shatter explore '*.ts' -o ts-all-default.json` over the 26 TS examples → the file is `{"file":"01-arithmetic.ts","functions":[],"status":"no_targets","no_target_reason":"unclassified"}`, while the markdown for the same run has 52 functions (`audits/2026-09-22/goals-runs/ts-all-default.{json,md}`). Part of this is the empty-bundle bug (explore-o-json-empty-bundle). Once that is fixed, this path would still write only 01-arithmetic.ts.
- spec-diff's loader (`SpecInput`, `shatter-cli/src/commands/diff.rs:43-70`) accepts either one `FileSpecBundle` (detected by a top-level `functions` array) or one bare `FunctionSpec` object. It does not accept a list.
- spec-diff pairs functions by bare name: `diff_spec_collections` (`diff.rs:223-262`) builds `HashMap<&str, &FunctionSpec>` keyed on `function_name` only. Once a bundle can hold several files, two same-named functions in different files (for example `classify` in `a.ts` and in `b.ts`, the collision behind behavior-map-cache-keys) would overwrite each other in the map and be compared against the wrong counterpart.
- `compare` (`shatter-cli/src/commands/compare.rs:19-22`) reads only a bare `FunctionSpec` per side, so it does not read `--spec-out` bundles at all today. Bundle support for `compare` is owned by spec-json-shapes-compare (shatter-reports-and-specs bucket), not this issue.

## Acceptance criteria

- [ ] A multi-file or glob explore writes every explored file's functions to `-o *.json` and `--spec-out`, in one of two documented shapes: a multi-file envelope (a `files: [FileSpecBundle]` array, with the spec schema version bumped), or one bundle per source file plus a manifest. Record the choice and the reason in the issue before implementing, and post the chosen shape on spec-json-shapes-compare so its shared reader accepts it.
- [ ] Single-file runs keep writing the current single `FileSpecBundle` shape, so existing Make recipes keep working.
- [ ] `finalize_explore` (`--from-artifacts`) produces the same shape and never labels one file's functions with another file's path.
- [ ] Incremental re-explore (`merge_file_spec_bundles`) merges per file.
- [ ] spec-diff matches functions by (source file, function name) whenever either input carries file identity (a bundle or the envelope), using the bundle's `file` normalized to a project-relative path. Bare-`FunctionSpec` inputs keep name-only matching. Added/removed lists name the file.
- [ ] spec-diff tests (unit or CLI):
  - two multi-file inputs with a change in the second file only report that change and nothing else;
  - **same-name collision:** both inputs contain `classify` in `a.ts` and `classify` in `b.ts`; only `b.ts:classify` changes; the diff reports exactly one change, attributed to `b.ts`, and `a.ts:classify` is unchanged. This test must fail against a name-keyed implementation;
  - a single-file bundle diffed against a single-file bundle behaves exactly as today.
- [ ] CLI test: a two-file explore with `-o out.json` and `--spec-out spec.json` contains both files' functions (`classifyNumber`, `compareMagnitudes`, `classifyString` and the fourth). A glob over a fixture dir contains all files. At close, show the test failing on current `main` and passing after the fix.
- [ ] SPEC §2.1 and §5 describe the multi-file shape and spec-diff's matching rule.
- [ ] `task affected` passes, and its `Gates selected` output is recorded.

## Suggested approach

After explore-o-json-empty-bundle has unified bundle collection, replace the two `.first()` writes with a writer that takes all bundles. The envelope is the simpler change for consumers, because one file in gives one file out. Teach spec-diff's `SpecInput` loader a third variant for the envelope, flatten every input to `(file, FunctionSpec)` pairs, and key `diff_spec_collections` on the pair. If spec-json-shapes-compare's shared core reader lands first, extend that reader instead of `SpecInput`.

## Out of scope

- The single-file empty bundle (explore-o-json-empty-bundle).
- `compare` reading bundles or envelopes, and pairing functions across two multi-file inputs (spec-json-shapes-compare owns the shared spec reader and `compare --function`).
- The SPEC §5 producer/consumer table and generated JSON Schemas (spec-s5-contract-table-and-samples, artifact-json-schemas). Those issues should describe the shape chosen here.

## Priority

P1: the spec outputs that spec-diff (the regression tool, D2) consumes silently omit most of a multi-file run.

## Type

bug

## Dependencies

- Blocked by: explore-o-json-empty-bundle (shares the bundle writer).
- Related: spec-json-shapes-compare (shatter-reports-and-specs; shared spec reader and `compare`, must accept the shape chosen here), explore-spec-bundle-failed-functions, artifact-json-schemas, spec-s5-contract-table-and-samples, behavior-map-cache-keys (same bare-name collision class), str-zt4v (closed).

## References

Audit 2026-09-22 finding goals-04 (verified P1). Source draft: `drafts/shatter-code/26-explore-json-output-bundles.md` (multi-file half; split per report §14 item 8).
