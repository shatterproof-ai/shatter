# str-fpgb.3: Lifecycle Schema Coverage

## Context

The JSON schemas in `protocol/schemas/` define the wire format for the Shatter protocol. The `request.schema.json` and `response.schema.json` files enumerate valid message variants via `oneOf` discriminated unions. Currently, **setup and teardown messages are missing** from both schemas:

- `request.schema.json`: the `command` enum includes `"setup"` and `"teardown"`, but the `oneOf` array and `$defs` section have no `setup_request` or `teardown_request` definitions.
- `response.schema.json`: the `status` enum includes `"setup"` and `"teardown_ack"`, but the `oneOf` array and `$defs` section have no `setup_response` or `teardown_ack_response` definitions.

All three frontends (TS, Go, Rust) fully implement setup/teardown, the registry documents their fields, and property tests exercise roundtrips — but the formal JSON schemas can't validate this traffic. There are also no shared schema-level supporting types for `SetupLevel` or `SetupContextStack`.

## Plan

### 1. Add supporting schema: `setup-level.schema.json`

**File**: `protocol/schemas/setup-level.schema.json`

```json
{
  "type": "string",
  "enum": ["session", "file", "function", "execution"]
}
```

### 2. Add supporting schema: `setup-context-stack.schema.json`

**File**: `protocol/schemas/setup-context-stack.schema.json`

Defines `SetupContextStack` with a `contexts` array of `{level, context}` entries.

### 3. Add `setup_request` and `teardown_request` to `request.schema.json`

**File**: `protocol/schemas/request.schema.json`

Add to `$defs`:
- `setup_request`: command="setup", fields: file (string), scope (string), level ($ref setup-level), project_root (optional string), parent_context (optional $ref setup-context-stack). Required: command, file, scope, level.
- `teardown_request`: command="teardown", fields: scope (string), level ($ref setup-level). Required: command, scope, level.

Add both `$ref`s to the `oneOf` array.

### 4. Add `setup_response` and `teardown_ack_response` to `response.schema.json`

**File**: `protocol/schemas/response.schema.json`

Add to `$defs`:
- `setup_response`: status="setup", setup_context (any JSON value). Required: status, setup_context.
- `teardown_ack_response`: status="teardown_ack". Required: status.

Add both `$ref`s to the `oneOf` array.

### 5. Write a schema validation test (failing first)

**File**: `protocol/schemas/test_schema_validation.py` (new)

A Python script using `jsonschema` that:
- Loads `request.schema.json` and `response.schema.json`
- Validates setup/teardown fixture messages against them
- Exits non-zero if validation fails

Run this BEFORE making schema changes to confirm the test fails (bugfix workflow).

### 6. Add fixture files

**Directory**: `protocol/fixtures/`

Add JSON fixture files for lifecycle messages:
- `setup-request.json` — setup request with all fields including parent_context
- `teardown-request.json` — teardown request
- `setup-response.json` — setup response with setup_context
- `teardown-ack-response.json` — teardown_ack response

These match the wire format that current frontends emit.

### 7. Verify

- Run validation script against fixtures → passes
- Run `cargo test` in shatter-core (protocol roundtrip tests still pass)
- Run Rust clippy

## Files Modified

- `protocol/schemas/request.schema.json` (edit)
- `protocol/schemas/response.schema.json` (edit)
- `protocol/schemas/setup-level.schema.json` (new)
- `protocol/schemas/setup-context-stack.schema.json` (new)
- `protocol/fixtures/setup-request.json` (new)
- `protocol/fixtures/teardown-request.json` (new)
- `protocol/fixtures/setup-response.json` (new)
- `protocol/fixtures/teardown-ack-response.json` (new)
- `protocol/schemas/test_schema_validation.py` (new)
