# str-fpgb.2: Error Code Contract Parity

## Context

Error code lists have drifted across the protocol registry, JSON schema, docs, and all three frontends. The canonical source (`protocol/registry.yaml`) defines 11 error codes, but every downstream consumer is missing at least one. The existing `validate-protocol-registry.py` script doesn't catch this because it only flags codes present in source but absent from registry — not the reverse for error codes.

### Current Drift

| Location | Count | Missing |
|---|---|---|
| `protocol/registry.yaml` (canonical) | 11 | — |
| `protocol/schemas/response.schema.json` | 9 | `compilation_error`, `not_supported` |
| `PROTOCOL.md` error codes table | 9 | `compilation_error`, `not_supported` |
| `shatter-core` `ErrorCode` enum | 10 | `not_supported` |
| `shatter-ts` `ErrorCode` type | 10 | `not_supported` |
| `shatter-ts` `arbErrorCode` generator | 10 | `not_supported` |
| `shatter-go` constants | 9 | `ErrFunctionNotFound`, `ErrInstrumentationFailed` (used as string literals) |
| `shatter-rust` handler | uses `"non_executable"` | not a standard code |

## Plan

### Phase 1: Failing tests first (bugfix workflow)

**1a. Rust core — exhaustive parity test** (`shatter-core/src/protocol.rs`, test module)
- Add `error_code_parity_with_registry` test that lists all 11 expected codes and asserts each deserializes to an `ErrorCode` variant. This fails immediately because `NotSupported` doesn't exist yet.

**1b. Go — parity test** (`shatter-go/protocol/parity_test.go`)
- Add `TestAllErrorCodesDefinedAsConstants` that checks all 11 expected codes exist as constants. Fails because `ErrFunctionNotFound` and `ErrInstrumentationFailed` are missing.

**1c. TS — parity test** (`shatter-ts/src/property.test.ts`)
- Add test that asserts the `ErrorCode` type covers all 11 codes. Fails because `not_supported` is missing.

### Phase 2: Fix schema and docs

**2a. `protocol/schemas/response.schema.json`** (line 184-188)
- Add `"compilation_error"` and `"not_supported"` to the enum array.

**2b. `PROTOCOL.md`** (line 322-332)
- Add rows for `compilation_error` and `not_supported` to the Error Codes table.

### Phase 3: Fix Rust core

**3a. `shatter-core/src/protocol.rs`** (~line 458)
- Add `NotSupported` variant to `ErrorCode` enum with doc comment.

**3b. `shatter-core/src/test_arbitraries.rs`**
- Add `Just(ErrorCode::NotSupported)` to `arb_error_code()`.

### Phase 4: Fix TypeScript

**4a. `shatter-ts/src/protocol.ts`** (line 135-145)
- Add `ALL_ERROR_CODES` const array with all 11 codes.
- Derive `ErrorCode` type from it: `type ErrorCode = (typeof ALL_ERROR_CODES)[number]`.

**4b. `shatter-ts/src/property.test.ts`** (line 83-87)
- Update `arbErrorCode` to use `ALL_ERROR_CODES`.

### Phase 5: Fix Go

**5a. `shatter-go/protocol/constants.go`**
- Add `ErrFunctionNotFound` and `ErrInstrumentationFailed` constants.
- Add `AllErrorCodes` slice referencing all 11 constants.

**5b. `shatter-go/protocol/handler.go`** (lines 187, 281, 283)
- Replace string literals with constants.

**5c. `shatter-go/protocol/handler_test.go`** (lines 258, 439)
- Replace string literals with constants.

**5d. `shatter-go/protocol/property_test.go`**
- Update error code generator to use `AllErrorCodes`.

### Phase 6: Fix Rust frontend

**6a. `shatter-rust/src/handler.rs`** (line 383)
- Replace `"non_executable"` with `"not_supported"`.

**6b. `shatter-rust/src/protocol.rs`** or new `constants.rs`
- Add `ERR_*` string constants for all 11 error codes.
- Replace string literals in `handler.rs` with constants.

**6c. Parity test** in `shatter-rust`
- Test that all 11 codes are defined and round-trip correctly.

### Phase 7: Strengthen validator

**7a. `scripts/validate-protocol-registry.py`**
- Make error code missing-from-source a hard error (not just commands).
- Remove hardcoded allowlist in `extract_rust_fe` — extract dynamically from `ERR_*` constants.
- Add JSON schema enum validation against registry.

### Verification

1. `cargo test -p shatter-core` — parity test passes
2. `cd shatter-ts && npm test` — parity test passes
3. `cd shatter-go && go test ./...` — parity test passes
4. `cargo test -p shatter-rust` — parity test passes
5. `python3 scripts/validate-protocol-registry.py` — zero warnings/errors
6. `cargo clippy -p shatter-core -p shatter-rust -- -D warnings` — clean

### Critical Files

- `protocol/registry.yaml` — canonical source (read-only, already correct)
- `protocol/schemas/response.schema.json` — add 2 missing codes
- `PROTOCOL.md` — add 2 missing rows
- `shatter-core/src/protocol.rs` — add `NotSupported` variant + parity test
- `shatter-core/src/test_arbitraries.rs` — update generator
- `shatter-ts/src/protocol.ts` — `ALL_ERROR_CODES` + derive type
- `shatter-ts/src/property.test.ts` — update generator + parity test
- `shatter-go/protocol/constants.go` — add 2 constants + `AllErrorCodes`
- `shatter-go/protocol/handler.go` — replace literals
- `shatter-go/protocol/handler_test.go` — replace literals
- `shatter-rust/src/handler.rs` — fix `non_executable` → `not_supported`
- `scripts/validate-protocol-registry.py` — stricter validation
