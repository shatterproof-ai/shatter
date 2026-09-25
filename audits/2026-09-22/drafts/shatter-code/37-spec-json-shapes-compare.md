# Three incompatible spec JSON shapes; `compare` rejects the --spec-out bundle (the only clean producer)

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | bug |
| priority | P2 |
| labels | spec,cli,artifacts,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | str-wfqh, str-nq20, str-qwua7.11 |
| source findings | artifacts-09, docs-03 (code half) |

<!-- body -->
## Problem

`explore --spec-out` writes a versioned FileSpecBundle, `--spec-json` stdout emits a bare FunctionSpec (after markdown, str-qwua7.11), and `properties` emits a YAML list of bundles. `compare` deserializes only a bare FunctionSpec, so it cannot read the --spec-out file.

## Current code facts / evidence

- `shatter-cli/src/commands/compare.rs:19-21` deserializes both inputs as FunctionSpec.
- `shatter-core/src/spec.rs:259-300` bundle types.
- `compare ts-spec-out.json go-spec.json` → `missing field function_name at line 172`, exit 2; hand-extracted bare specs → '4 of 4 shared behaviors match'.
- spec-diff on TS vs Go bundles prints 'Added functions: ClassifyNumber / Removed functions: classifyNumber' with no hint to use compare.

## Acceptance criteria

- One shared spec reader accepts bundle, bare spec and bundle list; all consumers (compare, spec-diff, stale, revalidate if applicable) use it.
- `compare --function A[=B]` selects from multi-function bundles.
- spec-diff suggests compare when function names differ only by case/language.
- Round-trip test: explore --spec-out → compare succeeds.

## Suggested approach

Add the reader in core; switch consumers.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: S-M

## References

- Audit findings: artifacts-09, docs-03 (code half) (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-wfqh, str-nq20, str-qwua7.11
