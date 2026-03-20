# Shatter Frontend Protocol

Version: `0.1.0`

This document defines the JSON protocol for communication between the Shatter core engine (Rust) and language-specific frontends (TypeScript, Go, etc.). Frontends are long-lived subprocesses that the core spawns and communicates with over stdin/stdout.

## Wire Format

Messages are **newline-delimited JSON** (NDJSON) over stdio:

- **Core → Frontend**: JSON messages written to the frontend's **stdin**, one per line.
- **Frontend → Core**: JSON messages written to the frontend's **stdout**, one per line.
- **stderr** is reserved for unstructured logging and debug output. The core captures it but does not parse it.

Each line is a complete, self-contained JSON object terminated by `\n`. Messages must not contain embedded newlines in their JSON (use `\n` escape sequences for string values containing newlines).

```
{"protocol_version":"0.1.0","id":1,"command":"handshake","capabilities":["analyze","execute"]}\n
{"protocol_version":"0.1.0","id":1,"status":"handshake","frontend_version":"0.1.0","language":"typescript","capabilities":["analyze","execute","instrument"]}\n
```

## Handshake

Every session begins with a handshake. The core sends a `handshake` request and the frontend responds with its version and capabilities. If the frontend does not support the core's protocol version, it responds with an `error` (code: `version_mismatch`).

### Request

```json
{
  "protocol_version": "0.1.0",
  "id": 1,
  "command": "handshake",
  "capabilities": ["analyze", "execute", "instrument"]
}
```

### Response

```json
{
  "protocol_version": "0.1.0",
  "id": 1,
  "status": "handshake",
  "frontend_version": "0.1.0",
  "language": "typescript",
  "capabilities": ["analyze", "execute", "instrument"]
}
```

## Commands

### `analyze` — Analyze a Function

Extract type information, branch points, and external dependencies from a function.

**Request:**

```json
{
  "protocol_version": "0.1.0",
  "id": 2,
  "command": "analyze",
  "file": "src/shipping.ts",
  "function": "calculateShipping"
}
```

The `function` field is optional. If omitted, the frontend analyzes all exported functions in the file.

**Response:**

```json
{
  "protocol_version": "0.1.0",
  "id": 2,
  "status": "analyze",
  "functions": [
    {
      "name": "calculateShipping",
      "params": [
        {
          "name": "order",
          "type": {
            "kind": "object",
            "fields": [
              ["items", { "kind": "array", "element": { "kind": "int" } }],
              ["priority", { "kind": "str" }]
            ]
          }
        }
      ],
      "branches": [
        {
          "id": 1,
          "line": 23,
          "condition_text": "order.priority === \"express\"",
          "condition": {
            "kind": "bin_op",
            "op": "eq",
            "left": { "kind": "param", "name": "order", "path": ["priority"] },
            "right": { "kind": "const", "type": "str", "value": "express" }
          },
          "branch_type": "if"
        }
      ],
      "dependencies": [
        {
          "kind": "function_call",
          "symbol": "rateService.getExpressRate",
          "source_module": "./rateService",
          "return_type": { "kind": "float" },
          "param_types": [{ "kind": "str" }],
          "call_sites": [25]
        }
      ],
      "return_type": {
        "kind": "object",
        "fields": [
          ["cost", { "kind": "float" }],
          ["method", { "kind": "str" }]
        ]
      },
      "start_line": 10,
      "end_line": 45
    }
  ]
}
```

### `instrument` — Instrument a Function

Rewrite a function's source to insert symbolic tracking at every branch point and mock external dependencies.

**Request:**

```json
{
  "protocol_version": "0.1.0",
  "id": 3,
  "command": "instrument",
  "file": "src/shipping.ts",
  "function": "calculateShipping",
  "mocks": [
    {
      "symbol": "rateService.getExpressRate",
      "return_values": [12.99],
      "should_track_calls": true,
      "default_behavior": "repeat_last"
    }
  ]
}
```

**Response:**

```json
{
  "protocol_version": "0.1.0",
  "id": 3,
  "status": "instrument",
  "instrumented": true,
  "output_file": "/tmp/instrumented_shipping.ts"
}
```

### `execute` — Execute with Specific Inputs

Run an instrumented function with specific inputs and mock configurations, collecting branch decisions, constraints, and performance metrics.

**Request:**

```json
{
  "protocol_version": "0.1.0",
  "id": 4,
  "command": "execute",
  "function": "calculateShipping",
  "inputs": [
    { "items": [1, 2, 3], "priority": "express" }
  ],
  "mocks": [
    {
      "symbol": "rateService.getExpressRate",
      "return_values": [12.99],
      "should_track_calls": true,
      "default_behavior": "repeat_last"
    }
  ],
  "capture": true
}
```

**`capture` field** (optional, default: `true`):

Controls whether the frontend instruments side-effect channels (console output, global state changes, etc.) during this execution.

- **`capture: true`** (or omitted) — full side-effect capture enabled. The `side_effects` array in the response will be populated. Adds per-call overhead for console and process interception.
- **`capture: false`** — side-effect capture disabled. The `side_effects` array will be empty (`[]`). All other response fields (`branch_path`, `lines_executed`, `return_value`, `thrown_error`, `path_constraints`) remain fully populated and correct.

The core sets `capture: false` by default during exploration to maximize throughput. Use the CLI `--capture-side-effects` flag to opt in:

```
shatter explore --capture-side-effects examples/typescript/src/
```

**Response (normal return):**

```json
{
  "protocol_version": "0.1.0",
  "id": 4,
  "status": "execute",
  "return_value": { "cost": 12.99, "method": "express" },
  "thrown_error": null,
  "branch_path": [
    {
      "branch_id": 1,
      "line": 23,
      "taken": true,
      "constraint": {
        "kind": "expr",
        "expr": {
          "kind": "bin_op",
          "op": "eq",
          "left": { "kind": "param", "name": "order", "path": ["priority"] },
          "right": { "kind": "const", "type": "str", "value": "express" }
        }
      }
    }
  ],
  "lines_executed": [10, 11, 23, 24, 30],
  "calls_to_external": [
    {
      "symbol": "rateService.getExpressRate",
      "args": ["90210"],
      "return_value": 12.99
    }
  ],
  "path_constraints": [
    {
      "kind": "expr",
      "expr": {
        "kind": "bin_op",
        "op": "eq",
        "left": { "kind": "param", "name": "order", "path": ["priority"] },
        "right": { "kind": "const", "type": "str", "value": "express" }
      }
    }
  ],
  "side_effects": [
    {
      "kind": "console_output",
      "level": "info",
      "message": "Processing express order"
    }
  ],
  "performance": {
    "wall_time_ms": 0.3,
    "cpu_time_us": 250,
    "heap_used_bytes": 1024,
    "heap_allocated_bytes": 2048
  }
}
```

**Response (thrown error):**

```json
{
  "protocol_version": "0.1.0",
  "id": 5,
  "status": "execute",
  "return_value": null,
  "thrown_error": {
    "error_type": "ValidationError",
    "message": "Invalid zip code",
    "stack": "at validateZip (shipping.ts:15)"
  },
  "branch_path": [],
  "lines_executed": [10, 15],
  "calls_to_external": [],
  "path_constraints": [],
  "side_effects": [],
  "performance": {
    "wall_time_ms": 0.1,
    "cpu_time_us": 80,
    "heap_used_bytes": 256,
    "heap_allocated_bytes": 256
  }
}
```

### `shutdown` — Graceful Shutdown

Request the frontend to cleanly shut down.

**Request:**

```json
{
  "protocol_version": "0.1.0",
  "id": 6,
  "command": "shutdown"
}
```

**Response:**

```json
{
  "protocol_version": "0.1.0",
  "id": 6,
  "status": "shutdown_ack"
}
```

The frontend should exit with code 0 after sending this response.

## Error Handling

Any command can produce an error response instead of its normal response. Error responses use `"status": "error"` and include a machine-readable error code:

```json
{
  "protocol_version": "0.1.0",
  "id": 2,
  "status": "error",
  "code": "file_not_found",
  "message": "File not found: src/missing.ts",
  "details": null
}
```

### Error Codes

| Code | Description |
|------|-------------|
| `file_not_found` | The specified file does not exist or is not readable. |
| `function_not_found` | The specified function was not found in the file. |
| `parse_error` | Syntax or parse error in the source file. |
| `instrumentation_failed` | Instrumentation could not be completed. |
| `execution_timeout` | Execution exceeded the time limit. |
| `execution_crash` | Execution crashed (segfault, OOM, etc.). |
| `version_mismatch` | Protocol version is not compatible. |
| `invalid_request` | The request is malformed or missing required fields. |
| `compilation_error` | Compilation or transpilation failed. |
| `internal_error` | An unexpected internal error occurred. |
| `not_supported` | Command not supported by this frontend. |

The `details` field is optional and can contain any JSON value with additional context (e.g., source location for parse errors, stack trace for crashes).

## Type System

Types are represented using the `TypeInfo` schema with a `kind` discriminator:

| Kind | Fields | Description |
|------|--------|-------------|
| `int` | — | Integer number |
| `float` | — | Floating-point number |
| `str` | — | String |
| `bool` | — | Boolean |
| `array` | `element: TypeInfo` | Array/list with element type |
| `object` | `fields: [[name, TypeInfo]]` | Object/struct with named fields |
| `union` | `variants: [TypeInfo]` | Union/sum type |
| `nullable` | `inner: TypeInfo` | Nullable wrapper |
| `unknown` | — | Type could not be determined |

## Symbolic Expressions

Symbolic expressions (`SymExpr`) represent constraints on function inputs. They form a tree structure with a `kind` discriminator:

| Kind | Fields | Description |
|------|--------|-------------|
| `param` | `name`, `path` | Reference to a parameter (e.g., `config.timeout`) |
| `const` | `type`, `value` | Literal constant |
| `bin_op` | `op`, `left`, `right` | Binary operation |
| `un_op` | `op`, `operand` | Unary operation |
| `call` | `name`, `receiver`, `args` | Function/method call |
| `unknown` | — | Could not be tracked symbolically |

### Binary Operators

`eq`, `ne`, `lt`, `le`, `gt`, `ge`, `add`, `sub`, `mul`, `div`, `mod`, `and`, `or`, `bitwise_and`, `bitwise_or`, `bitwise_xor`, `in`, `instance_of`

### Unary Operators

`not`, `neg`, `bitwise_not`, `type_of`

## Mock Configuration

The `MockConfig` type configures how an external dependency should be mocked:

- `symbol`: Fully qualified name (e.g., `"rateService.getExpressRate"`)
- `return_values`: Array of values to return in sequence
- `should_track_calls`: Whether to record call arguments
- `default_behavior`: What to do when `return_values` are exhausted:
  - `return_generated` — Generate a value based on the return type
  - `repeat_last` — Repeat the last return value
  - `throw_error` — Throw an error
  - `passthrough` — Call the real implementation

## Versioning and Compatibility

Protocol versions follow [semver](https://semver.org/):

- **Patch** (0.1.0 → 0.1.1): Bug fixes, no message format changes.
- **Minor** (0.1.0 → 0.2.0): New optional fields or commands added. Existing messages remain valid. Frontends should ignore unknown fields.
- **Major** (0.x → 1.0): Breaking changes to message format.

The handshake response includes the frontend's protocol version. The core checks compatibility:
- Same major version: compatible.
- Different major version: `version_mismatch` error.
- Frontend minor version < core minor version: the core avoids using newer features.

## JSON Schemas

Formal JSON Schema files are in [`protocol/schemas/`](protocol/schemas/):

- `request.schema.json` — Request envelope with all command variants
- `response.schema.json` — Response envelope with all result variants
- `mock-config.schema.json` — Mock configuration
- `function-analysis.schema.json` — Analysis result for a single function
- `branch-info.schema.json` — Branch point from static analysis
- `branch-decision.schema.json` — Branch decision from execution
- `external-dependency.schema.json` — External dependency
- `external-call.schema.json` — Observed external call
- `type-info.schema.json` — Type information
- `param-info.schema.json` — Parameter metadata
- `sym-expr.schema.json` — Symbolic expression
- `sym-constraint.schema.json` — Symbolic constraint
- `side-effect.schema.json` — Observed side effect
- `error-info.schema.json` — Error information
- `performance-metrics.schema.json` — Performance metrics

## No-Op Reference Frontend

A reference implementation is provided at [`protocol/noop-frontend.sh`](protocol/noop-frontend.sh). It responds to all commands with valid stub responses and serves as:

1. A guide for frontend authors to understand the protocol
2. A test harness for verifying the core's protocol handling
3. A template for bootstrapping new language frontends

Run it with: `bash protocol/noop-frontend.sh`
