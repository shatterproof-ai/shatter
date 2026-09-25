---
slug: protocol-schemas-reject-real-output
kind: new
title: "Published protocol JSON schemas reject real frontend output (shl/shr/bit_clear, complex constants), and no gate validates live output"
priority: P2
type: bug
labels: [protocol, schema, conformance, docs, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Published protocol JSON schemas reject real frontend output (shl/shr/bit_clear, complex constants), and no gate validates live output

## Problem

`protocol/schemas/*.schema.json` is maintained by hand and validated only against hand-written fixtures. The core types have grown past the schemas, so real frontend responses fail them. External consumers and future frontend authors who trust the published schemas get a wrong contract.

## Evidence (re-verified 2026-09-23 at 56c86168)

- The Go analyzer emits `"shl"` for `<<` (`shatter-go/protocol/analyzer.go:2440`). During the audit, a Go analyze of `if x<<2 > 8` was validated against `protocol/schemas/response.schema.json` using the schemas' own resolver. It failed with `'shl' is not one of ['eq', …, 'instance_of']`. The verifier confirmed this from code but did not re-run the live validation.
- Core `BinOpKind` has `Shl`, `Shr` and `BitClear` (`shatter-core/src/sym_expr.rs:106-110`), and `ConstValue::Complex` exists (`sym_expr.rs:74-79`). `/usr/bin/grep -cE '"shl"|"shr"|"bit_clear"|complex' protocol/schemas/sym-expr.schema.json` → `0`.
- PROTOCOL.md's Binary Operators list (`PROTOCOL.md:546-548`) ends at `instance_of`, with no shl, shr or bit_clear.
- `protocol/schemas/test_schema_validation.py` validates checked-in fixtures only. `/usr/bin/grep -c schema protocol/conformance/conformance_harness.py` → `0`, so the harness never loads a schema.
- `protocol/GOVERNANCE.md:35` ("2. Update JSON schemas") is a manual step with no check behind it.
- Audit finding protocol-parity-03 (confirmed, P2).

## Acceptance criteria

- [ ] `sym-expr.schema.json` (and any schema that embeds its enums) accepts `shl`, `shr`, `bit_clear` and complex constants, and `PROTOCOL.md`'s operator and constant lists name them.
- [ ] One of the following, recorded in the close note:
  - a shatter-core test serializes one fully populated instance of every `SymExpr`, `BinOpKind` and `ConstValue` variant and validates each against the schema;
  - or the schemas are generated from core types (`schemars`) with a `--check` mode in `task schemas`.
- [ ] The conformance harness validates every response it receives against `response.schema.json` and fails on a violation.
- [ ] GOVERNANCE step 2 links to the new check (coordinate with governance-md-omits-matrix if that rewrite lands first).
- [ ] Proof at close: the new core test, or the harness validation, fails on the pre-fix schema (paste the output) and passes after the fix. `task schemas` and `task conformance` pass when forced to execute.

## Suggested approach

Start with the core round-trip-against-schema test. It is cheap and catches future enum additions. Add harness validation next. Generating the schemas with `schemars` is the long-term option, but it may change schema layout that external readers depend on, so decide that separately.

## Out of scope

Output-artifact schemas (spec, scan report and so on; shatter-docs bucket artifact-json-schemas).

## Dependencies

- Blocked by: none.
- Related: str-2fjn (option b compares registry field lists, not live schema validity), str-fpgb.5 (closed; fixtures only), str-a4c (closed; added shl/shr/bit_clear to core but not to the schema), conformance-harness-correctness, governance-md-omits-matrix.

Size: M. Priority: P2. Type: bug. Labels: protocol, schema, conformance, docs, audit. Parent: Epic: Audit 2026-09-22 findings.
