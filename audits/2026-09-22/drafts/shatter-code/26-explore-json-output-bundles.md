# `explore -o out.json` / `--spec-out` write an empty no_targets bundle after a successful run, and keep only the first file's bundle for multi-file/glob targets

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | bug |
| priority | P1 |
| labels | cli,explore,artifacts,spec,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | str-ni32, str-jeen.67, str-zt4v, str-jeen.21 |
| source findings | cli-ux-01, artifacts-02, goals-04 |

<!-- body -->
## Problem

The documented JSON outputs of `explore` lose data. A successful single-function explore with `-o x.json` writes `{functions:[],status:no_targets,no_target_reason:unclassified}` and exits 0; multi-file or glob explores write only the first file's bundle (or a no_targets marker).

## Current code facts / evidence

- `shatter-cli/src/commands/explore.rs:6643-6665` / `:4042-4061`: bundles are built only on the --spec-json/--spec-out path, so the str-ni32 'analyze failed' no-target fallback runs on success.
- `explore.rs:6648`, `:6703`: `file_spec_bundles.first()`.
- Repro: `explore 01-arithmetic.ts:classifyNumber --clean -o b.json` → stderr `[warn] JSON output for explore writes spec bundle; use --spec-out` and `Wrote no-target spec marker (reason=unclassified)`; file has functions:[].
- Repro: `explore 01-arithmetic.ts 02-strings.ts -o out.json --spec-out spec.json` → both files lack classifyString; `explore '*.ts' -o all.json` (26 files) writes a no_targets marker for 01-arithmetic.ts while markdown shows 52 functions.
- SPEC §2.1 (SPEC.md:160) says -o writes 'a report; format inferred from extension'.

## Acceptance criteria

- `-o *.json` writes a real report/bundle containing every explored function (or is rejected with a message pointing at --spec-out).
- Multi-file output uses a multi-file envelope, array, or one file per source plus manifest (spec schema version bumped if the shape changes).
- The no-target marker is written only when no target was attempted; attempted-but-failed functions are recorded as failed with a class.
- CLI tests: explore of classifyNumber with -o x.json yields 1 function with 4 classes; two-file explore includes both files.

## Suggested approach

Build bundles unconditionally when any JSON sink is requested; replace `.first()` with all bundles.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: M

## References

- Audit findings: cli-ux-01, artifacts-02, goals-04 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-ni32, str-jeen.67, str-zt4v, str-jeen.21
