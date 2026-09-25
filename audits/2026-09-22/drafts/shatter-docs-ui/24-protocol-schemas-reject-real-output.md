# Published protocol JSON schemas reject real frontend output (shl/shr/bit_clear, complex consts); no gate validates live output

- Priority: P2
- Type: bug
- Labels: protocol,schema,conformance,docs
- Tracker action: new issue (str-2fjn option b is related, but it compares registry field lists, not the validity of live output against the schemas)
- Related: str-2fjn, str-fpgb.5, str-a4c
- Source findings: audit 2026-09-22 protocol-parity-03 (confirmed)

<!-- body -->
## Problem
`protocol/schemas/*.schema.json` is hand-maintained and validated only against hand-written fixtures. Core types have grown past it, so real frontend responses fail the published schemas.

## Evidence / current code facts
- Go analyze of `if x<<2 > 8` emits op `shl` (`shatter-go/protocol/analyzer.go:2440-2444`). Validating that response against `protocol/schemas/response.schema.json` with the schemas' own resolver fails: `'shl' is not one of ['eq', …, 'instance_of']`.
- Core `BinOpKind` has `Shl`, `Shr` and `BitClear`, and `ConstValue::Complex` exists (`shatter-core/src/sym_expr.rs:76-79`, `:107-110`). None of them appears in `protocol/schemas/sym-expr.schema.json` or in PROTOCOL.md's operator list (`PROTOCOL.md:546-548`).
- `protocol/schemas/test_schema_validation.py` checks fixtures only, and `protocol/conformance/conformance_harness.py` never loads a schema.

## Acceptance criteria
- The schemas include `shl`, `shr`, `bit_clear` and complex constants, and PROTOCOL.md lists them.
- A core test serializes one fully populated instance of every SymExpr/BinOpKind/ConstValue variant and validates it against the schema. Alternatively, the schemas are generated from core types with `schemars` and a `--check` mode.
- The conformance harness validates every response it receives against `response.schema.json` and fails on violations.
- GOVERNANCE step 2 ("update schemas") links the new check.

## Scope
In: SymExpr schema correctness and live validation. Out: output-artifact schemas (separate issue).
