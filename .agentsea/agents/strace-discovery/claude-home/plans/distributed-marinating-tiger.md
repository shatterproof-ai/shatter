# Plan: Schema Fixture Validator + Semgrep Protocol Rules

## Context

The protocol layer has JSON Schemas (`protocol/schemas/*.schema.json`) and a comprehensive fixture corpus (`protocol/fixtures/`), but the existing validator (`protocol/schemas/test_schema_validation.py`) only covers 4 top-level fixtures. The full corpus (8 valid requests, 8 invalid requests, 9 valid responses, 4 invalid responses, 10 error fixtures) is unvalidated. Additionally, hardcoded protocol literals are scattered across handler files with no automated detection.

---

## Task 1: Schema Fixture Validator (str-fpgb.5)

### Approach

Replace the partial `test_schema_validation.py` with a comprehensive `scripts/quality/check-schemas.sh` that validates **all** fixtures against schemas, including negative validation (invalid fixtures must fail).

### Implementation

**1. Create `scripts/quality/check-schemas.sh`** — bash quality entrypoint
- Source `lib/common.sh` for shared helpers
- Require Python 3 + `jsonschema` package (skip gracefully if missing, die in `--strict-optional`)
- Invoke the Python validator, report results

**2. Rewrite `protocol/schemas/test_schema_validation.py`** to be comprehensive:
- Auto-discover all fixtures in `requests/valid/`, `responses/valid/`, `errors/` — validate against `request.schema.json` / `response.schema.json` respectively
- Auto-discover all fixtures in `requests/invalid/`, `responses/invalid/` — assert they **fail** validation
- Also validate top-level fixtures (`setup-request.json`, `teardown-request.json`, `setup-response.json`, `teardown-ack-response.json`)
- Print clear pass/fail per fixture with error details
- Exit non-zero on any unexpected result
- Accept `--ci` flag for machine-readable output

**3. Wire into `check-all.sh`** — add `check-schemas.sh` call before meta checks

**4. Wire into `check-meta.sh`** — add schema validation as a step (alternative: keep as separate script called from check-all.sh directly — cleaner since it's protocol-specific, not "meta")

### Files to modify
- `protocol/schemas/test_schema_validation.py` — rewrite to cover full corpus
- `scripts/quality/check-schemas.sh` — new quality entrypoint
- `scripts/quality/check-all.sh` — add check-schemas step

### Verification
- Run `./scripts/quality/check-schemas.sh` — all valid fixtures pass, all invalid fixtures correctly fail
- Run `./scripts/quality/check-all.sh` — schema check integrated

---

## Task 2: Semgrep Protocol Rules (str-fpgb.8)

### Approach

Create `.semgrep/shatter.yml` with rules catching hardcoded protocol literals outside approved modules. The hook in `check-meta.sh` already looks for this file — no wiring changes needed.

### Rules to implement

1. **`hardcoded-error-code`** — string literals matching known error codes (`"file_not_found"`, `"parse_error"`, etc.) outside `protocol.ts`, `constants.go`, `protocol.rs`
2. **`hardcoded-command-name`** — string literals matching command names (`"handshake"`, `"analyze"`, etc.) outside protocol definition files
3. **`hardcoded-response-status`** — status strings (`"teardown_ack"`, `"shutdown_ack"`, etc.) outside protocol files
4. **`hardcoded-protocol-version`** — literal `"0.1.0"` version strings outside canonical const definitions
5. **`hardcoded-capability-array`** — inline capability arrays outside canonical locations

### Approved modules (excluded from rules)
- `shatter-core/src/protocol.rs`
- `shatter-ts/src/protocol.ts`
- `shatter-go/protocol/constants.go`, `shatter-go/protocol/types.go`
- `shatter-rust/src/protocol.rs`
- `protocol/` directory (schemas, fixtures, registry)
- Test files (`*_test.go`, `*.test.ts`, `**/tests/**`)
- `scripts/validate-protocol-registry.py`

### Files to create/modify
- `.semgrep/shatter.yml` — new Semgrep rules file
- No changes to `check-meta.sh` needed — it already picks up `.semgrep/shatter.yml`

### Verification
- Run `semgrep --config .semgrep/shatter.yml .` — should flag known drift in handler files
- Run `./scripts/quality/check-meta.sh` — Semgrep step runs (if semgrep installed)

---

## Execution Order

1. `bd start str-fpgb.5` → implement schema validator → commit → mark complete
2. `bd start str-fpgb.8` → implement Semgrep rules → commit → mark complete
3. Message team-lead with results
