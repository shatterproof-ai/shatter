# Protocol Governance

This document defines the mandatory process for changing the Shatter protocol contract. Every protocol change must follow these steps — no exceptions.

## Authoritative Source

`protocol/registry.yaml` is the **single source of truth** for the protocol contract. It defines all commands, response statuses, error codes, capabilities, setup levels, generator kinds, branch types, frontend metadata, and versioning policy.

All implementations (core and frontends) derive from the registry. The registry is validated against source code by `scripts/validate-protocol-registry.py`.

## What Constitutes a Protocol Change

Any modification to:

- Commands (adding, removing, renaming)
- Response statuses
- Error codes or error categories
- Capabilities (command or complex-type)
- Request or response payload fields
- TypeInfo or SymExpr schema structure
- Setup levels, generator kinds, or branch types
- Versioning or compatibility policy

## Required Update Steps

When making a protocol change, complete **every applicable step** in order:

### 1. Update the registry

Edit `protocol/registry.yaml` with the new command, status, error code, capability, or enumeration.

### 2. Update JSON schemas

Add or modify schemas in `protocol/schemas/`. Each schema follows JSON Schema Draft 2020-12. If adding a new message type, create a new `*.schema.json` file.

### 3. Update fixtures

Add or modify fixtures in `protocol/fixtures/`:

- `requests/valid/` — valid request examples for the new/changed command
- `requests/invalid/` — invalid request examples (missing fields, wrong types)
- `responses/valid/` — valid response examples
- `responses/invalid/` — invalid response examples
- `errors/` — error response examples if adding a new error code

Both valid and invalid fixtures are required — schema validation tests check that valid fixtures pass and invalid fixtures fail.

### 4. Implement in shatter-core (authoritative)

Update `shatter-core/src/protocol.rs` with the Rust types and serde annotations. This is the authoritative implementation — frontends must match it.

Add round-trip serialization tests in the same module.

### 5. Implement in each frontend

Update all three frontends to handle the new/changed protocol element:

| Frontend | File(s) |
|---|---|
| TypeScript | `shatter-ts/src/protocol.ts` |
| Go | `shatter-go/protocol/constants.go`, `shatter-go/protocol/handler.go` |
| Rust | `shatter-rust/src/protocol.rs` |

Add round-trip serialization tests in each frontend.

### 6. Update conformance cases

If the change adds a new command, alters response structure, or changes error handling, update `protocol/conformance/conformance_cases.yaml` with new test cases. Document any expected cross-frontend differences in the `known_drifts` section.

### 7. Update the protocol spec

If the change affects wire format or semantics, update `PROTOCOL.md` with the new message examples and behavioral documentation.

## Validation Checks

Run **all four** checks locally before declaring the change complete:

### Registry validation

Verifies that `registry.yaml` matches source code in all implementations.

```bash
python3 scripts/validate-protocol-registry.py
```

### Schema validation

Verifies that fixtures conform to (or correctly violate) JSON schemas.

```bash
python3 protocol/schemas/test_schema_validation.py
```

Or via Taskfile:

```bash
npx task schemas
```

### Conformance testing

Spawns all frontends (noop, TypeScript, Go, Rust) and verifies structural parity of responses.

```bash
python3 protocol/conformance/conformance_harness.py
```

Or via Taskfile:

```bash
npx task conformance
```

### Per-language tests

```bash
cargo test                          # shatter-core + shatter-rust
cd shatter-ts && npm test           # TypeScript frontend
cd shatter-go && go test ./...      # Go frontend
```

## CI Integration

The same checks run in CI via:

- `npx task schemas` — schema + fixture validation
- `npx task conformance` — cross-frontend conformance

Both tasks exit non-zero on failure and block merges.

## Frontend Parity Contract

**Output parity** — the observable wire format, response fields, error codes, and behavioral semantics — is required across all frontends for every protocol command. Frontends must produce structurally equivalent responses to the same input. Differences that are acceptable in conformance are listed under `known_drifts` in `conformance_cases.yaml`.

**Implementation details** — internal types, helper functions, data structures, and language-specific idioms — do NOT need to match across frontends.

### When to update the parity contract

Update the parity contract (in the affected frontend's `CLAUDE.md` and in `conformance_cases.yaml` if drift is expected) when:

- Adding a new command handler
- Adding, renaming, or removing response fields
- Changing error codes, error categories, or error semantics
- Changing a capability or setup level
- Any change that alters the JSON a frontend writes to stdout

Do NOT update the parity contract for internal refactors, type renames, or implementation changes that leave JSON output identical.

### How to verify parity

Run the conformance harness after any protocol-visible change:

```bash
npx task conformance
```

Unexpected drift (a mismatch not listed in `known_drifts`) is a bug. Either fix the diverging frontend or, if the difference is intentional, document it in `known_drifts` with an explanation.

## Known Drifts

Cross-frontend mismatches that are tracked but not yet resolved are documented in `protocol/conformance/conformance_cases.yaml` under the `known_drifts` key. These produce warnings (not failures) in the conformance harness. Each drift entry includes the affected frontends and a description of the mismatch.

When resolving a known drift, remove its entry from `known_drifts` and verify the conformance harness passes without warnings.

## Versioning Policy

The protocol uses semantic versioning with `major_minor_match` compatibility:

- Core and frontends must agree on **major.minor** version
- Patch differences are tolerated
- Minor version bumps may add new optional fields; frontends ignore unknown fields
- Major version bumps indicate breaking changes

The current protocol version and compatibility strategy are defined in `registry.yaml` under the `version` key.
