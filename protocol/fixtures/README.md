# Protocol Fixture Corpus

Shared JSON fixtures for schema validation and frontend conformance testing.

## Structure

```
fixtures/
  requests/
    valid/        8 files — one per command
    invalid/      8 files — malformed requests (missing fields, wrong types, unknown commands)
  responses/
    valid/        9 files — one per response status (including error)
    invalid/      4 files — malformed responses
  errors/        10 files — one per ErrorCode with realistic messages and details
```

## Naming Convention

- Valid fixtures: `{command}.json` (e.g., `handshake.json`, `execute.json`)
- Invalid fixtures: `{what-is-wrong}.json` (e.g., `missing-id.json`, `wrong-field-type.json`)
- Error fixtures: `{error-code-kebab}.json` (e.g., `file-not-found.json`, `execution-timeout.json`)

## Usage

All fixtures use protocol version `0.1.0` and sequential `id` values.

- **Schema validation tests**: validate `valid/` fixtures pass, `invalid/` fixtures fail
- **Frontend conformance tests**: parse `valid/` fixtures and verify field presence/types
- **Error handling tests**: verify frontends can produce each error code in `errors/`
- **Fuzz seed corpus**: use valid fixtures as seeds for mutation-based fuzzing

## Coverage

### Commands (requests/valid/)

handshake, analyze, instrument, execute, setup, teardown, generate, shutdown

### Response statuses (responses/valid/)

handshake, analyze, instrument, execute, setup, teardown_ack, generate, shutdown_ack, error

### Error codes (errors/)

file_not_found, function_not_found, parse_error, instrumentation_failed,
execution_timeout, execution_crash, version_mismatch, invalid_request,
compilation_error, internal_error

### Malformed request classes (requests/invalid/)

missing-protocol-version, missing-id, missing-command, unknown-command,
wrong-id-type, missing-required-field, wrong-field-type, negative-id

### Malformed response classes (responses/invalid/)

missing-status, unknown-status, missing-required-field, wrong-error-code
