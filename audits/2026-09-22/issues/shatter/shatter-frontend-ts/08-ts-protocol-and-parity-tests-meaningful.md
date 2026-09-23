---
slug: ts-protocol-and-parity-tests-meaningful
kind: new
title: "TS protocol round-trip tests bypass the real wire path and schemas, and the builder-parity property test cannot detect drift"
priority: P2
type: task
labels: [typescript, testing, protocol, parity, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# TS protocol round-trip tests bypass the real wire path and schemas, and the builder-parity property test cannot detect drift

## Problem

Two groups of TS tests look like protocol and parity coverage but do not exercise what they are named for.

1. **Protocol "round-trip" tests.** They assert `JSON.parse(JSON.stringify(x))` equals `x` for objects built by TS arbitraries. That is not quite a tautology: it does catch values JSON cannot carry (closed str-0z1im was exactly such a failure, on `-0`). But it never touches the real wire path (`serializeReplacer` in `sendResponse`, or `parseRequest`), the shared `protocol/schemas`, or `protocol/fixtures`. Yet `shatter-ts/Taskfile.yml` lists those directories and `shatter-core/src/protocol.rs` as test sources. Those are dead cache keys that suggest schema coverage that does not exist.
2. **Builder-parity property test.** A `buildSymExpr` / `buildSymExprWithFlow` parity describe block exists (`property.test.ts:1187`, 4 cases). *Verifier correction for finding tests-ci-12: the parity test is present, so the gap is narrower than "no parity test".* It has three limits:
   - it generates only node kinds that both builders already handle;
   - it compares only unknown vs non-unknown, not the output;
   - it does not cover the analyzer's builder or the executor's walker.

   A new node kind added to one builder, or a semantic difference between builders, passes it.

## Evidence

Verified at 56c86168 and re-checked unchanged at 793f2b0b (2026-09-23).

- `shatter-ts/src/property.test.ts:717` `describe("property: protocol message round-trips")`, whose cases through `:870` are plain JSON round-trips. The pattern repeats for SideEffect (`:871`), SymExpr (`:1002`), TypeInfo (`:1036`), BranchDecision (`:1047`) and TraceEvent (`:1059`). `serializeReplacer` (`shatter-ts/src/serialize.ts`, BigInt only) appears only in the separate BigInt tests (around `:1798-1850`).
- A grep of `shatter-ts/src/*.test.ts` finds no reference to `protocol/schemas` or `protocol/fixtures`.
- `shatter-ts/Taskfile.yml` `test` sources `:45-48` and `test-fast` sources `:62-65` list `../protocol/schemas/**/*.json`, `../protocol/fixtures/**/*.json` and `../shatter-core/src/protocol.rs`.
- The valid request fixtures are schema examples, not runnable scenarios: `protocol/fixtures/requests/valid/analyze.json` and `execute.json` target `src/example.ts::processOrder` (no such file ships with the fixtures), `execute.json` carries a `setup_context` for a non-existent setup, and `execute-with-prepare-id.json` uses a fabricated `prepare_id` (`a1b2c3d4e5f60001`). Run as-is, most would only produce `file_not_found` / unknown-prepare errors.
- `property.test.ts:1117` `arbBinOp` omits `in`/`instanceof`, which both token maps handle (`instrumentor.ts:2020-2023`). `:1097` `hasNonUnknownLeaf`, and the assertions at `:1187-1296`, check only unknown-ness.
- The analyzer builder (`analyzer.ts:2280`) is not exported and is untested. The executor loop-snapshot walker (`executor.ts:1497-1810`) is untested for parity.
- 7 of the 21 `shatter-ts/src/*.test.ts` files use fast-check. There are also semantic properties for `flattenConditions` and MC/DC masking (`property.test.ts` ~1587-1721), and SymExpr structural-validity checks. Keep those.
- str-0z1im (closed 2026-09-19, landed 4f673612): InvocationOutcome round-trip failed on `-0`. It is the precedent for the serialization guarantees below; there is no open overlap.
- Audit sources: findings frontend-ts-10, frontend-ts-11, tests-ci-12 (verifier: partially confirmed, parity block exists); `audits/2026-09-22/areas/frontend-ts.md` F10/F11.

## Acceptance criteria

- [ ] **Serialization guarantees are written down and tested on the real path.** Next to the tests, list what the wire encoding (`JSON.stringify(x, serializeReplacer)` as in `main.ts:17`) must preserve and what it deliberately normalizes. At minimum: BigInt -> `__complex_type: big_int`; `-0` (preserved or normalized to `0`, matching whatever str-0z1im decided); `NaN` / `Infinity` / `undefined` fields. A property test over the existing arbitraries serializes through `serializeReplacer`, parses, and asserts equality **modulo exactly those listed normalizations**.
- [ ] **Schema validation on the real wire path:** responses produced by the arbitraries, serialized as `sendResponse` does, are validated against `protocol/schemas` with ajv. Instrument/analyze outputs from a small fixture corpus are also validated. The test demonstrably catches a violation: a deliberately invalid response case is asserted to fail validation.
- [ ] **Valid request fixtures as executable scenarios.** A test harness:
  - materializes a temp project containing the source the fixtures reference (`src/example.ts` exporting `processOrder`, and a setup module if a fixture needs one);
  - substitutes the temp project root into `file` / `project_root` / `function` paths, and replaces fabricated ids (for example `prepare_id`) with the ids returned by earlier responses;
  - sends the fixtures in protocol order (`handshake`, `analyze`, `instrument`, `prepare`, `execute` variants, `setup`, `teardown`, `generate`, `shutdown`) through `parseRequest` + `handleRequest`;
  - asserts an **expected status per fixture** from a table in the test file. A fixture TS does not support (for example `get-invocation-plan.json`, `execute-with-runtime-value-plan.json`) has an explicit expected error code with a reason. `file_not_found`, `function_not_found` and unknown-prepare errors are never accepted as the expected result of a valid fixture.
  - A new file added to `protocol/fixtures/requests/valid/` without a table entry fails the test.
- [ ] **Builder parity by output:** replace the unknown-vs-non-unknown check with a fixed corpus that includes currently unsupported nodes (`??`, `**`, shifts, element access, `as`/`!`, template literals, `in`, `instanceof`). The test asserts **output equality modulo documented collapse rules** across all builders, the analyzer builder included (exported for tests, or deleted per ts-flow-analysis-consolidation). Each collapse rule is written down next to the test.
- [ ] `shatter-ts/Taskfile.yml` `sources:` match what the tests actually read: entries for inputs no test reads are removed, and inputs that tests read are added.
- [ ] The existing plain-JSON round-trip cases are removed, or rewritten to go through `serializeReplacer`/`parseRequest`. They are not kept alongside the new tests.
- [ ] Close-time proof: `npx jest` in `shatter-ts` runs the new suites (paste the per-suite counts), and `task affected` `Gates selected` is recorded.

## Suggested approach

Add `ajv` as a devDependency (it is not in `shatter-ts/package.json` today), load `protocol/schemas` in a jest `beforeAll`, and reuse the existing arbitraries. If TS output hits a schema defect that the protocol-schemas-reject-real-output issue (shatter-protocol-parity bucket) owns, mark that case expected-fail citing its id; do not weaken the schema here. For parity, a table-driven test over a source corpus is enough; fast-check on top is optional.

## Out of scope

- Invalid request fixtures: ts-request-validation (this bucket) drives `protocol/fixtures/requests/invalid/` and asserts `invalid_request`.
- Consolidating the builders (ts-flow-analysis-consolidation / str-rf2v).
- Adding operator support (ts-operators-collapse-to-unknown). This issue only makes the parity test able to see the gaps.
- Go/Rust schema tests.

## Related

- protocol-schemas-reject-real-output (other bucket): hand-maintained schemas already reject some real frontend output, so this suite may surface more of that.
- str-jalv (closed; builder parity properties), str-hicn (closed; WithFlow must handle every node buildSymExpr handles), str-rf2v (open), str-4btb (closed; removed trivial round-trips from parseRequest tests), str-qwua7.47 (open; Rust-only PBT), str-0z1im (closed; `-0` round-trip).

## Priority / type / size

P2 · task · size M
