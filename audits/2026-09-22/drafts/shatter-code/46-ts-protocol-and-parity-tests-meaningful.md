# TS protocol round-trip tests are tautological and the builder-parity property test cannot detect drift

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | task |
| priority | P2 |
| labels | typescript,testing,protocol,parity,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | str-jalv, str-hicn, str-rf2v, str-4btb |
| source findings | frontend-ts-10, frontend-ts-11, tests-ci-12 |

<!-- body -->
## Problem

The TS 'round-trip' tests assert `JSON.parse(JSON.stringify(x))` equals `x` on TS-built objects and never touch protocol/schemas, fixtures, serializeReplacer or parseRequest, although ts:test lists those as Task sources. The buildSymExpr/WithFlow parity property only generates node kinds both builders already handle and compares only unknown-vs-not; the analyzer builder is untested.

## Current code facts / evidence

- `shatter-ts/src/property.test.ts:717-760` plain JSON round-trips; serializeReplacer only in BigInt tests ~1798-1850.
- No `*.test.ts` references protocol/schemas or protocol/fixtures; `shatter-ts/Taskfile.yml:41-52` lists them as sources.
- `property.test.ts:1116-1296`: `arbBinOp` omits in/instanceof; assertions are hasNonUnknownLeaf / kind==='unknown'. analyzer.ts:2280 builder not exported.
- 7 of 21 TS test files use fast-check; instrumentor/analyzer have no property-level schema checks.

## Acceptance criteria

- Responses serialized via serializeReplacer are validated against `protocol/schemas` with ajv (fast-check arbitraries).
- `protocol/fixtures` requests are driven through parseRequest + handleRequest.
- Parity test uses a fixed corpus including unsupported nodes and asserts output equality modulo documented collapse rules across all builders (analyzer included, exported or deleted per str-rf2v).
- Taskfile sources match what tests actually read.

## Suggested approach

Implementer's choice within the acceptance criteria above.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: M

## References

- Audit findings: frontend-ts-10, frontend-ts-11, tests-ci-12 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-jalv, str-hicn, str-rf2v, str-4btb
