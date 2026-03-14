# str-fpgb.4: Protocol Fixture Corpus

## Context

The Shatter protocol has JSON schemas in `protocol/schemas/` and test frontends in `protocol/`, but no shared corpus of valid/invalid fixture JSON files. Schema validation tests (`str-fpgb.3`) and frontend conformance checks need a common set of fixtures covering all commands, lifecycle messages, and error cases. This task creates that corpus.

## Plan

### 1. Create fixture directory structure

```
protocol/fixtures/
  requests/
    valid/          — one file per command
    invalid/        — malformed request variants
  responses/
    valid/          — one file per response status
    invalid/        — malformed response variants
  errors/           — canonical error responses (one per ErrorCode)
```

### 2. Valid request fixtures (8 files)

All use `protocol_version: "0.1.0"` and sequential `id` values.

| File | Command | Key fields |
|------|---------|------------|
| `handshake.json` | handshake | capabilities array |
| `analyze.json` | analyze | file, function (optional), project_root |
| `instrument.json` | instrument | file, function, mocks array |
| `execute.json` | execute | function, inputs, mocks, setup_context |
| `setup.json` | setup | file, scope, level, project_root, parent_context |
| `teardown.json` | teardown | scope, level |
| `generate.json` | generate | file, name, kind, recipe |
| `shutdown.json` | shutdown | minimal (just command) |

### 3. Valid response fixtures (9 files)

| File | Status | Key content |
|------|--------|-------------|
| `handshake.json` | handshake | frontend_version, language, capabilities |
| `analyze.json` | analyze | functions array with FunctionAnalysis (params, branches with SymExpr, dependencies, return_type) |
| `instrument.json` | instrument | instrumented: true, output_file, instrumentable_line_count |
| `execute.json` | execute | Full ExecuteResult: return_value, branch_path with constraints, lines_executed, path_constraints, side_effects, performance, capture_truncation |
| `setup.json` | setup | setup_context (opaque JSON) |
| `teardown-ack.json` | teardown_ack | minimal |
| `generate.json` | generate | value, generator_id, recipe |
| `shutdown-ack.json` | shutdown_ack | minimal |
| `error.json` | error | code, message, details |

### 4. Invalid request fixtures (~8 files)

| File | What's wrong |
|------|-------------|
| `missing-protocol-version.json` | No `protocol_version` field |
| `missing-id.json` | No `id` field |
| `missing-command.json` | No `command` field |
| `unknown-command.json` | `command: "foobar"` |
| `wrong-id-type.json` | `id: "not-a-number"` |
| `missing-required-field.json` | analyze with no `file` |
| `wrong-field-type.json` | `capabilities: "not-array"` |
| `extra-fields-only.json` | Valid but with extra unknown fields (should still parse) |

### 5. Invalid response fixtures (~4 files)

| File | What's wrong |
|------|-------------|
| `missing-status.json` | No `status` field |
| `unknown-status.json` | `status: "bogus"` |
| `missing-required-field.json` | execute response missing `branch_path` |
| `wrong-error-code.json` | `code: "not_a_real_code"` |

### 6. Error response fixtures (10 files, one per ErrorCode)

Each in `errors/` named by error code: `file-not-found.json`, `function-not-found.json`, `parse-error.json`, `instrumentation-failed.json`, `execution-timeout.json`, `execution-crash.json`, `version-mismatch.json`, `invalid-request.json`, `compilation-error.json`, `internal-error.json`.

Each includes realistic `message` text and `details` where applicable (e.g., stack trace for internal_error, line number for parse_error).

### 7. Add a README

`protocol/fixtures/README.md` documenting the corpus structure, naming convention, and how to use fixtures in tests.

## Key files to reference

- `protocol/schemas/request.schema.json` — request schema definitions
- `protocol/schemas/response.schema.json` — response schema definitions
- `protocol/concolic-test-frontend.sh` — realistic JSON examples to base fixtures on
- `shatter-core/src/protocol.rs` — Rust types (source of truth for all field names and types)

## Notes

- Protocol version is `"0.1.0"` (from `PROTOCOL_VERSION` constant in protocol.rs)
- The request schema's `oneOf` is missing `setup` and `teardown` — fixtures should still include them per the Rust types
- The response schema's error codes are missing `compilation_error` — fixture should include it per the Rust enum
- Fixtures use relative paths (e.g., `"src/example.ts"`) per the no-hardcoded-absolute-paths rule
- YAML is preferred for config, but these are JSON protocol messages — JSON is the correct format

## Verification

1. Each fixture should be valid JSON: `find protocol/fixtures -name '*.json' -exec python3 -c "import json,sys; json.load(open(sys.argv[1]))" {} \;`
2. Valid fixtures should validate against their schemas (future schema test will use these)
3. Fixture count: ~8 valid requests + ~9 valid responses + ~8 invalid requests + ~4 invalid responses + ~10 error responses = ~39 fixtures total
