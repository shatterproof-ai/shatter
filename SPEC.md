# Shatter Behavioral Specification

> **Living document.** Updated as functionality changes. Last updated: 2026-03-10.
>
> This spec describes what Shatter does — its observable behavior from a user's perspective. It is the authoritative reference for how each command, feature, and output format should behave. The audit process (`/audit`) compares the actual codebase against this document.

---

## 1. Overview

Shatter is an automatic exploratory testing tool that uses **concolic execution** (concrete + symbolic) to discover execution paths through functions, generate inputs that exercise each path, and produce behavioral specifications, regression snapshots, and test files.

### 1.1 Core Pipeline

```
Source code → Analyze → Explore → Cluster → Report
                ↓           ↓         ↓         ↓
          Branch points  Inputs  Equivalence  Spec / Tests / Snapshot
                         & paths   classes
```

1. **Analyze**: Parse a function to extract parameter types, branch conditions, and external dependencies.
2. **Explore**: Execute the function repeatedly with generated inputs, tracking which branches are taken. After each execution, record the execution path (branch decisions + lines covered).
3. **Cluster**: Group executions into equivalence classes by branch path. Select a canonical (simplest) example for each class. Derive preconditions and postconditions.
4. **Report**: Output results as exploration reports, behavioral specifications (markdown or JSON), behavior snapshots, or generated test files.

### 1.2 Architecture

```
┌──────────┐    JSON/NDJSON     ┌──────────────┐
│ shatter  │ ◄──── stdio ─────► │  Language     │
│   core   │                    │  Frontend     │
│  (Rust)  │                    │  (TS/Go/Rust) │
└──────────┘                    └──────────────┘
     ↑
     │
┌──────────┐
│ shatter  │
│   cli    │
│  (clap)  │
└──────────┘
```

- **Core engine** (`shatter-core`): Rust library. Orchestrates analysis, exploration, clustering, spec generation, test export, and snapshot diffing.
- **CLI** (`shatter-cli`): Thin clap wrapper. Parses arguments, delegates to core.
- **Language frontends**: Separate processes (TypeScript via Node.js, Go via compiled binary). Communicate via NDJSON over stdin/stdout. Handle language-specific parsing, instrumentation, and execution.

### 1.3 Supported Languages

> **Canonical source of truth** for language support status. Other docs link here.

| Language   | Frontend      | File extensions | Status    |
|------------|---------------|-----------------|-----------|
| TypeScript | `shatter-ts`  | `.ts`, `.tsx`   | Supported |
| Go         | `shatter-go`  | `.go`           | Supported |
| Rust       | `shatter-rust`| `.rs`           | Stub (protocol handler only) |

---

## 2. CLI Commands

### 2.1 `shatter explore`

**Purpose**: Explore one or more functions to discover branches and generate inputs that exercise each path.

**Syntax**: `shatter explore [OPTIONS] <TARGETS>...`

**Targets**: `<file>:<function>` or `<file>` (all exported functions). Language determined by file extension.

**Behavior**:
1. Spawn the appropriate language frontend subprocess.
2. Send `handshake` to verify protocol compatibility.
3. For each target function:
   a. Send `analyze` to extract parameters, types, and branches.
   b. If `--analyze-only`: print analysis and stop.
   c. Send `instrument` to prepare the function for execution tracking.
   d. Generate inputs (random + boundary values + solver-guided).
   e. Send `execute` for each input set, collecting branch decisions, return values, errors, and lines executed.
   f. Track unique execution paths. Stop when `--max-iterations` reached or `--timeout` expires.
   g. Group executions into equivalence classes by branch path.
   h. Select canonical examples, derive preconditions and postconditions.
4. Send `shutdown` to the frontend.
5. Print exploration report to stdout.

**Key options**:

| Flag | Default | Description |
|------|---------|-------------|
| `--max-iterations N` | 100 | Maximum execute calls per function |
| `--timeout SECS` | 60 | Overall timeout for the exploration |
| `--analyze-only` | false | Only analyze, skip exploration |
| `--show-clusters` | false | Display behavior clusters in output |
| `--cache-dir PATH` | `.shatter-cache/behavior-maps/` | Directory for behavior map caching |
| `--no-cache` | false | Disable caching entirely |
| `--request-timeout SECS` | 30 | Per-request timeout for frontend communication |
| `--exec-timeout SECS` | 10 | Per-execution timeout in the frontend |
| `--build-timeout SECS` | 30 | Timeout for compiling instrumented code |
| `--inputs PATH` | — | Path to candidate inputs JSON file |
| `--config PATH` | — | Path to `.shatter/config.yaml` |
| `--spec` | false | Output behavioral specification (markdown) |
| `--spec-json` | false | Output behavioral specification (JSON) |
| `--invariants` | false | Enable Daikon-style invariant detection |
| `--no-boundary-values` | false | Disable built-in boundary value seeding |
| `--scope PATH` | — | Scope configuration YAML for mocking/inclusion |

**Output formats**:
- Default: Human-readable exploration report showing paths discovered, coverage, and exemplar inputs.
- `--show-clusters`: Adds behavior cluster grouping.
- `--spec`: Markdown behavioral specification with equivalence classes, pre/postconditions, examples, provenance.
- `--spec-json`: Machine-readable JSON version of the spec (used by `spec-diff`).
- `--invariants`: Adds Daikon-style invariants (numeric comparisons, null checks, string properties, output-equals-input) at function-wide and per-class levels.

### 2.2 `shatter scan`

**Purpose**: Explore multiple functions in dependency order, using behavior maps from already-explored callees as mocks for callers.

**Syntax**: `shatter scan [OPTIONS] <TARGETS>...`

**Behavior**:
1. Analyze all target functions.
2. Build a call graph of inter-function dependencies.
3. Compute topological order (leaves first).
4. Explore functions layer by layer:
   a. For each function in the current layer, explore it using mocks derived from callee behavior maps.
   b. Store the resulting behavior map for use as a mock by callers in higher layers.
5. With `--parallelism N`: spawn N frontend worker processes and assign functions within a layer concurrently.
6. Write reports to `--output-dir` (default: `./shatter-report/`).

**Key options** (in addition to explore options):

| Flag | Default | Description |
|------|---------|-------------|
| `--parallelism N` | 0 (auto) | Number of parallel frontend subprocesses |
| `--timeout-per-fn SECS` | 30 | Per-function timeout; skip if exceeded |
| `--output-dir PATH` | `./shatter-report/` | Report output directory |
| `--report FORMAT` | `json` | Report format: `json`, `markdown`, or `both` |
| `--progress-json` | false | Emit structured JSON progress events to stdout |
| `--emit-tests FRAMEWORK` | — | Generate test files: `jest`, `vitest`, or `gotest` |
| `--emit-tests-dir PATH` | same as output-dir | Directory for emitted test files |

**Output**:
- JSON report: per-function behavior maps, coverage, branch analysis.
- Markdown report: human-readable summary of all explored functions.
- Test files: generated test code in the specified framework.

### 2.3 `shatter export-tests`

**Purpose**: Generate test files from behavior maps produced by exploration.

**Syntax**: `shatter export-tests [OPTIONS] <TARGETS>...`

**Behavior**:
1. Run exploration on the given targets (same as `explore`).
2. Convert the resulting behavior maps into test source code.
3. Write to stdout or `--output` file.

**Supported frameworks**:

| Framework | Language | Test structure |
|-----------|----------|----------------|
| `jest` | TypeScript | `describe`/`it` blocks with ES module imports |
| `vitest` | TypeScript | Same as Jest (identical output) |
| `gotest` | Go | Table-driven `t.Run()` subtests |

**Generated test structure**:
- One `describe` (Jest/Vitest) or `Test*` function (Go) per explored function.
- One `it` block / table entry per observed behavior.
- Each test calls the function with the canonical example inputs and asserts the expected return value or error.

### 2.4 `shatter run`

**Purpose**: Discover, analyze, and explore an entire directory in one shot.

**Syntax**: `shatter run [OPTIONS] <PATH>`

**Behavior**:
1. Discover all supported source files in `<PATH>` (recursive).
2. Analyze all exported functions in discovered files.
3. Build a call graph across all functions.
4. Explore functions in dependency order (leaves first).
5. Output a markdown summary report.

**Key options**:

| Flag | Default | Description |
|------|---------|-------------|
| `--output-dir PATH` | — | Write per-function reports |
| `--max-iterations N` | 50 | Iterations per function |
| `--timeout SECS` | 300 | Overall timeout |
| `--analyze-only` | false | Only discover and analyze |
| `--request-timeout SECS` | 30 | Per-request timeout |
| `--exec-timeout SECS` | 10 | Per-execution timeout |
| `--build-timeout SECS` | 30 | Build timeout |

### 2.5 `shatter diff`

**Purpose**: Compare two behavior snapshots to detect regressions.

**Syntax**: `shatter diff <SNAPSHOT> <CURRENT>`

**Behavior**:
1. Load both snapshot JSON files.
2. For each function in both snapshots, compare behaviors:
   - **Matched**: Same behavior exists in both (by exemplar input and expected output).
   - **Added**: Behavior in current but not in snapshot.
   - **Removed/Regressed**: Behavior in snapshot but not in current.
3. Exit code: `0` if all behaviors match, nonzero if regressions found.

**Options**: `--json` for machine-readable output.

### 2.6 `shatter spec-diff`

**Purpose**: Compare two behavioral specifications (from `--spec-json`) to detect changes.

**Syntax**: `shatter spec-diff <OLD> <NEW>`

**Behavior**:
1. Load both spec JSON files.
2. Compare equivalence classes by branch path:
   - **Added classes**: Present in new but not old.
   - **Removed classes**: Present in old but not new.
   - **Changed postconditions**: Same branch path, different output.
   - **Changed preconditions**: Same branch path, different input constraints.
   - **Lost properties**: Invariants that held in old but not in new.
3. Exit code: `0` if specs are equivalent, nonzero if regressions found.

**Options**: `--json` for machine-readable output.

### 2.7 `shatter init`

**Purpose**: Initialize a repository for persistent Shatter project state.

**Syntax**: `shatter init [OPTIONS]`

**Behavior**:
1. Resolve the target directory (explicit `--directory`, detected project root, or current directory).
2. Create `.shatter/` if it does not already exist.
3. Write `.shatter/config.yaml` with starter defaults if it does not already exist.
4. If the project is already initialized, report the existing `.shatter/` contents without overwriting them.

**Effect on the project tree**:
- Establishes the repo-local Shatter configuration root at `.shatter/`.
- Signals that project-local Shatter state is expected to live in this repository.
- Other commands may also create `.shatter-cache/` and `shatter-artifacts/` when using the initialized-project path.

**Options**:
- `--directory <DIR>`: Initialize that directory instead of the auto-detected project root.

### 2.8 Global Options

| Flag | Default | Description |
|------|---------|-------------|
| `--log-level LEVEL` | `info` | Log verbosity: `error`, `warn`, `info`, `debug`, `trace` |
| `-v` / `--verbose` | — | Increase verbosity (`-v` = debug, `-vv` = trace) |
| `-q` / `--quiet` | — | Decrease to warnings and errors only |
| `--perf` | false | Show per-function performance timing |

---

## 3. Core Concepts

### 3.1 Equivalence Classes

Executions that follow the **same sequence of branch decisions** (same branch IDs, same taken/not-taken) belong to the same equivalence class. Within each class:

- The **canonical example** is the simplest input (by JSON complexity).
- **Preconditions** are derived from patterns across all inputs in the class (e.g., "all inputs have param[0] > 0").
- **Postconditions** describe what the function does on this path: returns a value, throws an error, or returns void.

### 3.2 Behavior Maps

A `BehaviorMap` records a function's observed input→output mappings. Each entry (`Behavior`) captures:
- A representative input
- The corresponding output (return value or error)
- The execution path taken
- External calls made and side effects observed (via `DependencyTrace`)

Behavior maps serve two purposes:
1. **Mocking**: When scanning, callee behavior maps become mock configurations for callers.
2. **Regression detection**: Behavior maps are serialized as snapshots for `diff`.

### 3.3 Behavioral Specifications

A `FunctionSpec` is a structured description of a function's complete behavior:
- **Equivalence classes** (`SpecClass`): Each with branch path, preconditions, postcondition, side effects, concrete examples, sample count.
- **Provenance**: Whether each pre/postcondition is `proven` (by Z3 constraint solving) or `observed` (seen in all samples but not formally verified).
- **Invariants** (optional): Daikon-style properties that hold across all executions or within a class.
- **Coverage**: Iteration count, lines covered, total lines.

Output formats: human-readable markdown or machine-readable JSON.

### 3.4 Invariant Detection

When `--invariants` is enabled, Shatter runs Daikon-style invariant detection over execution records. Detected invariant kinds:

| Kind | Example | Target |
|------|---------|--------|
| `NumericComparison` | `x > 0` | Input or Output |
| `NumericConstant` | `x == 42` | Input or Output |
| `NotNull` | `x is never null` | Input or Output |
| `IsNull` | `x is always null` | Input or Output |
| `StringNonEmpty` | `s is never empty` | Input or Output |
| `StringLength` | `len(s) >= 3` | Input or Output |
| `OutputEqualsInput` | `output.id == input[0].id` | Cross |
| `AlwaysTrue` | `flag is always true` | Input or Output |
| `AlwaysFalse` | `flag is always false` | Input or Output |

Invariants are classified with confidence scores (satisfied_count / total_count) and reported at both function-wide and per-class levels.

### 3.5 Input Generation

Inputs are generated from multiple sources, in priority order:

1. **User-provided inputs**: From `.shatter/config.yaml` or `--inputs` flag.
2. **Boundary values**: Built-in dictionary of edge-case values per type (0, -1, MAX_INT, empty string, etc.). Disabled with `--no-boundary-values`.
3. **Solver-guided inputs**: Z3 constraint solving to negate path constraints and reach unexplored branches. *(Note: currently the explorer uses random generation; Z3 solving is integrated but the concolic loop is still primarily random-driven.)*
4. **Random generation**: Type-aware random values as fallback.

### 3.6 Configuration

Hierarchical `.shatter/config.yaml` files can be placed at any level of the project tree. The nearest config to each target file takes precedence.

Running `shatter init` is the explicit way to opt a repository into persistent
project-local Shatter state. That installed-project path establishes
`.shatter/config.yaml` as the repo-local configuration root. Depending on the
commands you run, Shatter may also persist repo-local cache and artifact data in
`.shatter-cache/` and `shatter-artifacts/`.

```yaml
defaults:
  max_iterations: 50
  setup_file: "./setup.ts"
  setup_mode: per_function  # or per_execution

functions:
  "src/auth.ts:validateToken":
    max_iterations: 200
    inputs: ["valid-token", "expired-token", "malformed"]

opaque_types:
  - DatabaseConnection
  - HttpClient
```

**Scope configuration** (`shatter.scope.yaml`): Controls which files/functions are included and which dependencies are mocked.

### 3.7 Caching

Behavior maps can be cached to disk (`--cache-dir`) to avoid re-exploring unchanged functions across runs. Cache files are JSON, keyed by function identity.

When using the project-initialized path, the default repo-local cache location is
under `.shatter-cache/`.

---

## 4. Frontend Protocol

Frontends are long-lived subprocesses communicating via NDJSON over stdio. See `PROTOCOL.md` for the full wire format specification.

**Commands**:

| Command | Direction | Purpose |
|---------|-----------|---------|
| `handshake` | Core → Frontend | Version and capability negotiation |
| `analyze` | Core → Frontend | Extract types, branches, dependencies |
| `instrument` | Core → Frontend | Prepare function for execution tracking |
| `execute` | Core → Frontend | Run function with given inputs, return results |
| `shutdown` | Core → Frontend | Graceful termination |

**Capabilities**: `analyze`, `execute`, `instrument`, `setup`, `teardown`, `generate`.

**Type system**: `int`, `float`, `str`, `bool`, `array`, `object`, `union`, `nullable`, `unknown`.

---

## 5. Output Formats

### 5.1 Exploration Report (default)

Human-readable summary printed to stdout:
```
Explored: classifyNumber
  Iterations: 50
  Unique paths: 4
  Lines covered: 8/10 (80%)
  New paths:
    [1] classifyNumber(-5) → "negative"
    [2] classifyNumber(0) → "zero"
    [3] classifyNumber(4) → "positive-even"
    [4] classifyNumber(7) → "positive-odd"
```

### 5.2 Behavioral Specification (Markdown)

```markdown
# Specification: `classifyNumber`

**Location:** `src/01-arithmetic.ts:1`

**Behavioral classes:** 4
**Exploration:** 50 iterations, 8/10 lines covered (80%)

---

## Class 1 — returns "negative"

**Preconditions** [observed]:
- param[0] < 0

**Postcondition** [observed]: returns "negative"

**Example** (12 execution(s) observed):
\```
classifyNumber(-5) -> "negative"
\```
```

### 5.3 Behavioral Specification (JSON)

```json
{
  "function_name": "classifyNumber",
  "location": "src/01-arithmetic.ts:1",
  "classes": [
    {
      "label": "Class 1 — returns \"negative\"",
      "branch_path": [{"branch_id": 0, "taken": true}],
      "preconditions": [{"AllNegative": {"param_index": 0}}],
      "postcondition": {"kind": "returns", "value": "negative"},
      "side_effects": [],
      "examples": [{"inputs": [-5], "return_value": "negative", "thrown_error": null}],
      "sample_count": 12,
      "precondition_provenance": "observed",
      "postcondition_provenance": "observed"
    }
  ],
  "iterations": 50,
  "lines_covered": 8,
  "total_lines": 10
}
```

### 5.4 Behavior Snapshot (JSON)

```json
{
  "version": "0.1.0",
  "functions": [
    {
      "function_id": "classifyNumber",
      "behaviors": [
        {
          "id": "b0",
          "exemplar_input": [-5],
          "expected_output": "negative"
        }
      ]
    }
  ]
}
```

### 5.5 Generated Tests

**Jest/Vitest** (TypeScript):
```typescript
import { classifyNumber } from './src/01-arithmetic';

describe('classifyNumber', () => {
  it('returns "negative" when given -5', () => {
    expect(classifyNumber(-5)).toBe("negative");
  });
});
```

**Go** (table-driven):
```go
package main

import "testing"

func TestClassifyNumber(t *testing.T) {
    tests := []struct {
        name string
        n    int
        want string
    }{
        {"returns negative", -5, "negative"},
    }
    for _, tt := range tests {
        t.Run(tt.name, func(t *testing.T) {
            got := ClassifyNumber(tt.n)
            if got != tt.want {
                t.Errorf("ClassifyNumber(%d) = %q, want %q", tt.n, got, tt.want)
            }
        })
    }
}
```

### 5.6 Scan Reports

**JSON report**: Written to `<output-dir>/report.json`. Contains per-function behavior maps, coverage, and analysis.

**Markdown report**: Written to `<output-dir>/report.md`. Human-readable summary of all explored functions with behavior tables.

---

## 6. Known Limitations

1. **Explorer is primarily random**: The concolic loop currently uses random input generation with boundary value seeding. Z3 constraint solving is integrated but the symbolic path negation loop is not yet driving exploration systematically.
2. **Provenance is always "observed"**: Since the explorer doesn't use Z3 for path constraint solving, all preconditions and postconditions are marked `observed`, never `proven`.
3. **Rust frontend is a stub**: `shatter-rust` implements the protocol handler but `analyze`, `instrument`, and `execute` return "not yet implemented".
4. **No Windows support**: Frontends assume Unix-like process spawning.
5. **No support for async functions**: The explorer does not handle async/await or promises.
6. **Limited type inference**: Complex TypeScript types (generics, conditional types, mapped types) may not be fully analyzed.
7. **Limited string theory support**: Z3 Seq theory covers 8 string operations (see `string-ops.yaml`). `split()` cannot be modeled (would require bounded unrolling). Regex support is limited to Z3's decidable fragment (`str.in_re`) — backreferences, lookahead, and named groups are unsupported. Functions using these operations fall back to random/mutation-based exploration. Planned workaround: frontend-side structural candidate generation (deferred).

---

## 7. Changelog

| Date | Change | Section |
|------|--------|---------|
| 2026-02-28 | Initial specification created | All |
