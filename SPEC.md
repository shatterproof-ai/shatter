# Shatter Behavioral Specification

> **Living document.** Updated as functionality changes. Last updated: 2026-07-03.
>
> This spec describes what Shatter does — its observable behavior from a user's perspective. It is the authoritative reference for how each command, feature, and output format should behave. The audit process (`/audit`) compares the actual codebase against this document.
>
> **Flag-level source of truth.** Command flag tables below are kept in sync with `shatter-cli/src/args.rs`. When a flag list here and `shatter <command> --help` disagree, `--help` (i.e. `args.rs`) is authoritative — file an issue to reconcile this document. Any CLI-visible change (new command, new/renamed/removed flag, changed default, changed output shape) should add a row to the [changelog](#8-changelog).

---

## 1. Overview

Shatter is an automatic exploratory testing tool that uses **concolic execution** (concrete + symbolic) to discover execution paths through functions, generate inputs that exercise each path, and produce behavioral specifications and regression snapshots.

### 1.1 Core Pipeline

```
Source code → Analyze → Explore → Cluster → Report
                ↓           ↓         ↓         ↓
          Branch points  Inputs  Equivalence  Spec / Snapshot
                         & paths   classes
```

1. **Analyze**: Parse a function to extract parameter types, branch conditions, and external dependencies.
2. **Explore**: Execute the function repeatedly with generated inputs, tracking which branches are taken. After each execution, record the execution path (branch decisions + lines covered).
3. **Cluster**: Group executions into equivalence classes by branch path. Select a canonical (simplest) example for each class. Derive preconditions and postconditions.
4. **Report**: Output results as exploration reports, behavioral specifications (markdown, JSON, or YAML), or behavior snapshots.

The pipeline is also exposed as discrete **stages** that pass JSON artifacts between them: `observe` (execute and record), `analyze` (cluster offline), `solve` (Z3 branch negation offline), and `specify` (assemble a spec). This lets each stage run and be cached independently. See §2.3.

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

- **Core engine** (`shatter-core`): Rust library. Orchestrates analysis, exploration, clustering, spec generation, and snapshot diffing.
- **CLI** (`shatter-cli`): Thin clap wrapper. Parses arguments, delegates to core.
- **Language frontends**: Separate processes (TypeScript via Node.js, Go via compiled binary, Rust via compiled binary). Communicate via NDJSON over stdin/stdout. Handle language-specific parsing, instrumentation, and execution.

### 1.3 Supported Languages

> **Canonical source of truth** for language support status. Other docs link here.

| Language   | Frontend      | File extensions | Status    |
|------------|---------------|-----------------|-----------|
| TypeScript | `shatter-ts`  | `.ts`, `.tsx`   | Supported |
| Go         | `shatter-go`  | `.go`           | Supported |
| Rust       | `shatter-rust`| `.rs`           | Supported |

Rust support includes analysis, instrumentation, harness-backed execution,
setup/teardown, generator dispatch, and `prepare` caching. Remaining Rust gaps
are tracked as parity limitations, not absence of frontend support.

---

## 2. CLI Commands

Shatter's CLI (`shatter <command>`) exposes the commands below. This section
documents every command in `CliCommand` (`shatter-cli/src/args.rs`). Commands
group into four families:

| Command | Family | Purpose |
|---------|--------|---------|
| `explore` | Exploration | Explore functions to discover branches and generate inputs (§2.1). |
| `scan` | Exploration | Explore a directory in dependency order, using callee behavior maps as mocks (§2.2). |
| `run` | Exploration | Discover, analyze, and explore an entire repository in one shot (§2.4). |
| `properties` | Exploration | Discover invariants and export a YAML property spec (§2.5). |
| `observe` | Staged pipeline | Stage 1: execute a function and write raw observation JSON (§2.3). |
| `analyze` | Staged pipeline | Stage 2: cluster observation JSON offline into equivalence classes (§2.3). |
| `solve` | Staged pipeline | Stage 3: Z3-solve uncovered branches from observation JSON offline (§2.3). |
| `specify` | Staged pipeline | Stage 4: assemble a `FunctionSpec` from stage artifacts (§2.3). |
| `diff` | Regression | Compare two behavior snapshots (§2.6). |
| `spec-diff` | Regression | Compare two behavioral specs by branch path (§2.6). |
| `compare` | Regression | Compare two specs across languages by input/output only (§2.6). |
| `stale` | Regression | Report which functions are stale relative to a spec file (§2.7). |
| `revalidate` | Regression | Re-execute cached behaviors to detect drift (§2.7). |
| `init` | Project | Initialize `.shatter/` project state (§2.8). |
| `list-targets` | Project | List/classify files a scan would select (§2.9). |
| `cache` | Project | Manage the on-disk cache (§2.9). |
| `workspace` | Project | Garbage-collect the frontend artifact workspace (§2.9). |
| `telemetry` | Project | Manage anonymous usage telemetry (§2.9). |
| `doctor` | Project | Diagnose the local install and embedded-frontend staleness (§2.9). |
| `build-frontend` | Tooling | Build a custom frontend binary with native generators (§2.9). |
| `discover-deps` | Tooling | Discover network dependencies via strace (Linux-only) (§2.9). |
| `nondeterminism` | Tooling | Review and classify suspected-nondeterministic fields (§2.9). |
| `bench` | Tooling | Run timing benchmarks against a manifest (§2.9). |
| `test` | Tooling | Run tests with git-change impact analysis (§2.9). |

Every command accepts the [global options](#210-global-options) in §2.10.

### 2.1 `shatter explore`

**Purpose**: Explore one or more functions to discover branches and generate inputs that exercise each path.

**Syntax**: `shatter explore [OPTIONS] <TARGETS>...`

**Targets**: One or more positional targets, each either `<file>:<function>` or `<file>` (all exported functions). Quoted glob patterns over file paths are accepted (e.g. `'src/**/*.ts'`) and expanded against the filesystem; matches are filtered to supported source extensions, deduped, and sorted. Language is determined by file extension. Globs in the `<function>` portion or in `<file>:<function>` form are rejected with an actionable error.

**Behavior**:
1. Spawn the appropriate language frontend subprocess.
2. Send `handshake` to verify protocol compatibility.
3. For each target function:
   a. Send `analyze` to extract parameters, types, and branches.
   b. If `--analyze-only`: print analysis and stop.
   c. Send `instrument` (and `prepare` where supported) to prepare the function for execution tracking.
   d. Generate inputs (random + boundary values + solver-guided).
   e. Send `execute` for each input set, collecting branch decisions, return values, errors, and lines executed.
   f. Track unique execution paths. Stop when the per-function iteration cap (`--max-iterations`) or a wall-clock limit (`--per-function-timeout` / `--timeout-explore` / `--time-limit`) is reached.
   g. Group executions into equivalence classes by branch path.
   h. Select canonical examples, derive preconditions and postconditions.
4. Send `shutdown` to the frontend.
5. Write the exploration report to stdout and/or files (`-o`).

**Key options** (run `shatter explore --help` for the exhaustive list — `explore` has ~60 flags):

*Exploration control*

| Flag | Default | Description |
|------|---------|-------------|
| `--max-iterations N` | 100 | Max execute calls per function. `0` = unbounded (run until a timeout or interrupt). |
| `--per-function-timeout SECS` | — | Per-function default exploration timeout (applied when no per-function config timeout is set). |
| `--timeout-explore SECS` | — | Per-function wall-clock cap; whichever of it and `--max-iterations` triggers first stops that function. |
| `--time-limit SECS` | — | Whole-run wall-clock cap; stops launching new functions once reached. |
| `--max-executions COUNT` | — | Global execute-call budget shared across the whole run. |
| `--coverage-threshold PERCENT` | — | Stop once aggregate branch coverage reaches this percent (checked after each function). |
| `--analyze-only` | false | Only analyze, skip exploration. |
| `--no-boundary-values` | false | Disable built-in boundary value seeding. |
| `--inputs PATH` | — | Candidate inputs JSON file (overrides config inputs). |
| `--scope PATH` | — | Scope YAML (`shatter.scope.yaml`) controlling mocking policy and call-graph inclusion. Does not select file targets — use positional `<TARGETS>` for that. |
| `--config PATH` | — | Path to a `.shatter/config.yaml` (bypasses hierarchical discovery). |

*Explorer strategy*

| Flag | Default | Description |
|------|---------|-------------|
| `--concolic` | false | Use the concolic (Z3-backed) explorer instead of the random explorer. |
| `--genetic` | false | Enable the genetic-algorithm explorer (tune with `--genetic-population`, `--genetic-generations`, `--genetic-timeout`). |
| `--invariants` | false | Enable Daikon-style invariant detection. |
| `--mcdc` | false | Enable MC/DC analysis (decomposes compound boolean decisions; raises iteration/execution budgets). |
| `--no-adaptive` | false | Disable adaptive strategy scoring (use round-robin). Tune with `--score-window`, `--cold-start`, `--strategy-floor`, `--strategy-weights`. |
| `--planner NAME` | — | Select a frontend invocation planner (e.g. `go`) via `get_invocation_plan` so method targets dispatch through a real constructor. |
| `--solver-timeout SECS` | none | Z3 solver timeout per query. |

*Output & spec*

| Flag | Default | Description |
|------|---------|-------------|
| `-o, --output PATH` | stdout | Write a report; format inferred from extension (`.html`, `.md`, `.json`, `.txt`). Repeatable to write several formats. |
| `--stdout` | — | Also write to stdout (default when no `-o` given). |
| `--format FORMAT` | `markdown` | stdout format: `markdown`, `html`, or `text`. (`json` on stdout is not offered — write `-o file.json` instead.) |
| `--spec` | false | Emit a behavioral specification (markdown by default, JSON with `--spec-json`). |
| `--spec-json` | false | Emit the spec as JSON instead of markdown. |
| `--spec-out PATH` | — | Write per-file spec JSON to a file (implies `--spec-json`). |
| `--show-clusters` | false | Display behavior clusters in output. |

*Timeouts, caching, isolation, parallelism*

| Flag | Default | Description |
|------|---------|-------------|
| `--request-timeout SECS` | 30 | Per-request frontend-communication timeout. |
| `--exec-timeout SECS` | 10 | Per-execution timeout in the frontend. |
| `--build-timeout SECS` | 30 | Timeout for compiling instrumented code. |
| `--release` | false | Compile harnesses in release mode (env: `SHATTER_HARNESS_RELEASE`). |
| `--memory-limit MB` | — | Frontend memory cap (TS: `--max-old-space-size`; Go: `GOMEMLIMIT`). |
| `--cache-dir PATH` | `.shatter-cache/behavior-maps/` | Behavior-map cache dir (env: `SHATTER_CACHE_DIR`). |
| `--no-cache` | false | Disable behavior-map caching. |
| `--clean` | false | Force full re-exploration; discard prior explore artifacts for the targets. |
| `--isolation MODE` | `none` | Execution isolation: `none`, `function`, or `serial`. |
| `--capture-side-effects` | false | Record console/file/network/env/global side effects per execution. |
| `-w, --workers N` (alias `--jobs`) | 0 (auto) | Across-function parallel workers. Bounded by `--parallelism-min` / `--parallelism-max`. |
| `--observer-pool N` | 1 | Observer subprocesses the random explorer fans candidates across *within* one function. Tune the intake queue with `--candidate-queue-capacity`. |

*Artifacts, seeds, refinement, and recording*

| Flag | Default | Description |
|------|---------|-------------|
| `--observe-output DIR` | — | Write raw Stage-1 observation JSON (one file per function) for offline `shatter analyze`. |
| `--persist-stages DIR` | — | Persist canonical `observe/analyze/solve/specify` JSON per function for reuse. |
| `--from-artifacts PATH` | — | Finalize a previous run from saved per-function artifacts (skip exploration). |
| `--dry-run` | false | Print stale/fresh/removed functions and exit without exploring (requires `--output`). |
| `--record` | false | Record external dependency I/O as seed fixtures under `shatter-artifacts/recorded-mocks/`. |
| `--replay-recorded` / `--no-replay` | — | Replay (or suppress) recorded mock fixtures as seed mock configs. |
| `--seeds-dir DIR` / `--no-seeds` | `.shatter/seeds` | Cross-function seed pool location, or disable it (env: `SHATTER_SEEDS_DIR`). |
| `--refine-budget N` / `--shrink-budget N` / `--no-shrink` | 20 / 20 | Per-boundary refinement and per-witness shrink budgets (`0` disables). |
| `--loop-buckets SPEC` | `0,1,2,5` | Loop-iteration bucket boundaries for path hashing (`none` disables). |
| `--setup-timeout SECS` / `--fail-on-setup-error` | — | Override setup/teardown timeouts (env: `SHATTER_SETUP_TIMEOUT`); optionally abort on setup failure. |
| `--require-rust` | false | Treat an unavailable Rust frontend as a hard failure instead of skipping Rust targets. |

`explore` also accepts LLM seed-oracle overrides (`--llm`, `--llm-adapter`, `--llm-token-budget`).

**Output formats**:
- Default: Human-readable exploration report showing paths discovered, coverage, and exemplar inputs.
- `--show-clusters`: Adds behavior cluster grouping.
- `--spec`: Markdown behavioral specification with equivalence classes, pre/postconditions, examples, provenance.
- `--spec-json`: Machine-readable JSON version of the spec (used by `spec-diff`).
- `--invariants`: Adds Daikon-style invariants (numeric comparisons, null checks, string properties, output-equals-input) at function-wide and per-class levels.

### 2.2 `shatter scan`

**Purpose**: Explore multiple functions in dependency order, using behavior maps from already-explored callees as mocks for callers.

**Syntax**: `shatter scan [OPTIONS] <DIRECTORY>`

**Scope**: The single positional `<DIRECTORY>` argument is the root that `scan` walks for source files. Use `--include` / `--exclude` (repeatable glob patterns, e.g. `--include '**/*.ts' --exclude '**/vendor/**'`) to narrow the discovered file set, `--language` to restrict to one frontend, and `--max-depth` to bound traversal. Git-scoped selection is available via `--changed` (uncommitted files) or `--since <ref>` (files changed since a ref, with optional `--until`/`--include-untracked`). By default only exported functions are scanned; `--all` includes non-exported functions. `scan` does not accept positional file targets or glob targets — use `explore` (or `properties`) when you want to select specific files.

**Behavior**:
1. Analyze all target functions.
2. Build a call graph of inter-function dependencies.
3. Compute topological order (leaves first).
4. Explore functions layer by layer:
   a. For each function in the current layer, explore it using mocks derived from callee behavior maps.
   b. Store the resulting behavior map for use as a mock by callers in higher layers.
5. With `--parallelism N`: spawn N frontend worker processes and assign functions within a layer concurrently.
6. Write reports to stdout (default) and/or the files named by repeatable `-o/--output`.

**Key options** (in addition to explorer strategy/timeout flags shared with `explore`):

| Flag | Default | Description |
|------|---------|-------------|
| `--include GLOB` / `--exclude GLOB` | — | Repeatable file-selection globs. |
| `--language LANG` | auto | Restrict to `typescript`, `go`, or `rust`. |
| `--max-depth N` | — | Maximum directory traversal depth. |
| `--changed` | false | Scan only files with uncommitted changes. |
| `--since REF` / `--until REF` / `--include-untracked` | — | Scan files changed in a git range (`--until` and `--include-untracked` refine it). |
| `--all` | false | Scan non-exported functions too. |
| `--concolic` | false | Use the concolic (Z3-backed) explorer. |
| `--parallelism N` | 0 (auto) | Parallel frontend subprocesses. Bounded by `--parallelism-min` / `--parallelism-max`. |
| `--workers-per-fn N` | 1 | Workers assigned per function in shared-pool mode (`--isolation none`); splits the iteration budget across seeds. |
| `--scheduler-policy POLICY` | `layer-parallel` | `layer-parallel` (functions in a layer run concurrently) or `serial`. |
| `--timeout-per-fn SECS` | 30 | Per-function timeout; skip the function if exceeded. |
| `--timeout-total SECS` | 300 | Total scan timeout. |
| `--max-iterations N` | 100 | Iterations per function (`0` = unbounded). |
| `--mock-config PATH` | — | Mock configuration YAML file. |
| `-o, --output PATH` | stdout | Write a report; format inferred from extension (`.html`, `.md`, `.json`, `.txt`). Repeatable. |
| `--stdout` | — | Also write to stdout (default when no `-o`). |
| `--format FORMAT` | `markdown` | stdout format: `markdown`, `json`, `html`, or `text`. |
| `--progress` | false | Emit NDJSON progress events to **stderr** during the scan (see §6.1). |
| `--dry-run` | false | Show what would be scanned without executing. |
| `--resume VALUE` | — | Resume from a checkpoint: `auto` to discover it, an explicit path, or `off` to disable (see §6.3). |
| `--core-sample SPEC` | — | Explore only a representative sample (`"50%"` or `"20"`); pair with `--seed` and progressive `--batch`. |
| `--stratum RANGE` | — | Explore only specific call-graph layers (e.g. `"0"`, `"0..3"`, `"-2..-0"`). |
| `--no-cache` | false | Disable every on-disk cache the scan reads or writes (behavior-map, fingerprint, stored-inputs). |
| `--fail-on-failures[=PERCENT]` | — | Exit nonzero on attempted-function failures; with `=PERCENT`, only when the failure rate exceeds that threshold. Omitted → partial-failure scans still exit 0. |

**Output**:
- Markdown report (default): human-readable summary of all explored functions.
- JSON report (`-o report.json` or `--format json`): per-function behavior maps, coverage, branch analysis.

### 2.3 Staged pipeline: `observe` → `analyze` → `solve` → `specify`

The exploration pipeline is also exposed as four discrete commands that exchange
JSON artifacts, so each stage can run, be cached, and be inspected independently.
`explore --persist-stages` writes the same artifacts inline.

#### `shatter observe <TARGET>`

Stage 1. Execute `<file>:<function>` with generated inputs and write an
`ObserveStageOutput` JSON. Requires a live frontend and (optionally) a solver.

| Flag | Default | Description |
|------|---------|-------------|
| `--concolic` | false | Use concolic (Z3-backed) exploration instead of random. |
| `--max-iterations N` | 100 | Max iterations. |
| `--timeout SECS` | 60 | Total timeout. |
| `--request-timeout SECS` | 30 | Per-request timeout. |
| `--exec-timeout SECS` | 30 | Per-execution timeout. |
| `--build-timeout SECS` | 60 | Build timeout. |
| `--release` | false | Compile harnesses in release mode (env: `SHATTER_HARNESS_RELEASE`). |
| `-o, --output FILE` | stdout | Write the observation JSON here. |
| `--memory-limit MB` | — | Frontend memory cap. |

#### `shatter analyze <INPUT>`

Stage 2. Cluster an observation JSON into equivalence classes, a behavior map,
and coverage metrics. Pure offline computation — no frontend or solver.

| Flag | Default | Description |
|------|---------|-------------|
| `-o, --output FILE` | — | Write the Stage-2 analysis JSON (for downstream stages). |
| `--spec` | false | Also emit a markdown behavioral specification. |
| `--spec-json` | false | Emit the spec as JSON instead of markdown. |
| `--invariants` | false | Enable Daikon-style invariant detection. |

#### `shatter solve <INPUT>`

Stage 3. Read an observation JSON and use Z3 to find inputs that trigger
uncovered branch directions. Pure offline computation — no frontend.

| Flag | Default | Description |
|------|---------|-------------|
| `-o, --output FILE` | — | Write the Stage-3 solve JSON. |
| `--solver-timeout MS` | 5000 | Z3 timeout per branch, in milliseconds. |

#### `shatter specify <OBSERVATION_FILE>`

Stage 4. Assemble a `FunctionSpec` from an observation file, optionally enriched
by the analyze and solve artifacts.

| Flag | Default | Description |
|------|---------|-------------|
| `--analyze-file FILE` | — | Stage-2 artifact; if omitted, the analyze step runs inline. |
| `--solve-file FILE` | — | Stage-3 artifact; enriches the spec with Z3-**proven** provenance and coverage completeness. |
| `--json` | false | Output the spec as JSON (conflicts with `--yaml`). |
| `--yaml` | false | Output the spec as YAML with human-friendly `property:` descriptions (needs `--invariants` to populate them; conflicts with `--json`). |
| `--invariants` | false | Detect and include function-wide invariants. |
| `-o, --output FILE` | stdout | Write the spec here. |

### 2.4 `shatter run`

**Purpose**: Discover, analyze, and explore an entire directory in one shot.

**Syntax**: `shatter run [OPTIONS] <PATH>`

**Scope**: `run` honors the same `shatter.config.json` `include`, `exclude`,
`language`, and `max_depth` settings as `scan`, plus `.gitignore` and
`.shatterignore`. Unlike `scan`, `run` exposes no CLI scope-filter flags — the
project config is the sole source of scope.

**Behavior**:
1. Discover all supported source files in `<PATH>` (recursive).
2. Analyze all exported functions in discovered files.
3. Build a call graph across all functions.
4. Explore functions in dependency order (leaves first).
5. Output a markdown summary report.

**Key options**:

| Flag | Default | Description |
|------|---------|-------------|
| `--output-dir PATH` | — | Write per-function reports to this directory. |
| `--max-iterations N` | 50 | Iterations per function. |
| `--timeout SECS` | 300 | Overall timeout. |
| `--analyze-only` | false | Only discover and analyze. |
| `--request-timeout SECS` | 30 | Per-request timeout. |
| `--exec-timeout SECS` | 10 | Per-execution timeout. |
| `--build-timeout SECS` | 30 | Build timeout. |
| `--release` | false | Compile harnesses in release mode. |
| `--solver-timeout SECS` | none | Z3 solver timeout per query. |
| `--memory-limit MB` | — | Frontend memory cap. |

`run` also accepts a coverage-budget gate group (`--min-source-representation-percent`,
`--max-failed-span-percent`, `--max-unsupported-span-percent`,
`--fail-on-stale-source-set`, `--fail-on-missing-artifacts`,
`--fail-on-low-report-validity`) for CI thresholds.

### 2.5 `shatter properties`

**Purpose**: Discover behavioral properties and invariants and export them as a YAML spec.

**Syntax**: `shatter properties [OPTIONS] <TARGETS>...`

Runs analysis and exploration on the given targets (files, `<file>:<function>`,
or quoted globs — same target grammar as `explore`) to discover invariants, then
emits the behavioral spec enriched with property descriptions.

| Flag | Default | Description |
|------|---------|-------------|
| `-o, --output PATH` | stdout | Write output to a file. |
| `--output-format FORMAT` | `yaml` | Output format (currently only `yaml`). |
| `--max-iterations N` | 100 | Iterations per function. |
| `--timeout SECS` | 60 | Overall timeout. |
| `--scope PATH` | — | Scope configuration YAML. |
| `--request-timeout SECS` | 30 | Per-request timeout. |
| `--exec-timeout SECS` | 10 | Per-execution timeout. |
| `--build-timeout SECS` | 30 | Build timeout. |
| `--release` | false | Compile harnesses in release mode. |
| `--memory-limit MB` | — | Frontend memory cap. |

### 2.6 Spec and snapshot comparison: `diff`, `spec-diff`, `compare`

#### `shatter diff <SNAPSHOT> <CURRENT>`

Compare two behavior snapshots to detect regressions. For each function present
in both snapshots, behaviors are classified **matched** (same exemplar input and
output), **added** (in current only), or **removed/regressed** (in snapshot
only). Exit `0` when all behaviors match, nonzero on regressions. `--json` for
machine-readable output.

#### `shatter spec-diff <OLD> <NEW>`

Compare two behavioral specs (from `--spec-json`) by branch path:

- **Added / Removed classes**: present in only one spec.
- **Changed postconditions**: same branch path, different output.
- **Changed preconditions**: same branch path, different input constraints.
- **Lost properties**: invariants that held in old but not new.

Exit `0` when specs are equivalent, nonzero on regressions. `--json` for machine-readable output.

#### `shatter compare <SPEC_A> <SPEC_B>`

Compare two spec JSON files **across languages** by input/output behavior only,
ignoring branch paths (which are language-specific). Same inputs should produce
same outputs. Exit `0` when all shared behaviors match, nonzero on divergence.
`--json` for machine-readable output. Typical use: verify a TypeScript and a Go
implementation of the same function agree.

### 2.7 Freshness: `stale`, `revalidate`

#### `shatter stale <SOURCE> <SPEC>`

Report which functions in a source file are stale relative to a spec JSON file.
Functions classify as **fresh** (tracked, fingerprint matches), **stale**
(tracked, fingerprint differs), **removed** (in spec, gone from source), or
**untracked** (in source, never tracked). Exit `0` when no *tracked* function is
stale or removed; `1` otherwise. `--strict` also fails on untracked functions
(full-file coverage mode). `<SOURCE>` must be a concrete file path — wildcards
are rejected.

| Flag | Default | Description |
|------|---------|-------------|
| `--output-format FORMAT` | `text` | `text` or `json`. |
| `--request-timeout` / `--exec-timeout` / `--build-timeout` | 30 / 10 / 30 | Frontend timeouts. |
| `--release` | false | Compile harnesses in release mode. |
| `--cache-dir PATH` / `--no-cache` | — | Cross-file dependency fingerprint cache (or disable it). |
| `--strict` | false | Treat untracked functions as failures. |
| `--memory-limit MB` | — | Frontend memory cap. |

#### `shatter revalidate <SOURCE>`

Re-execute cached behaviors for a source file: load behavior maps from the cache,
replay each recorded input through a fresh frontend, and compare observed against
cached behavior. Exit `0` = no regressions, `1` = issues found. `<SOURCE>` must
be a concrete file path.

| Flag | Default | Description |
|------|---------|-------------|
| `--cache-dir PATH` | `.shatter-cache/behavior-maps/` | Behavior-map cache (env: `SHATTER_CACHE_DIR`). |
| `--request-timeout` / `--exec-timeout` / `--build-timeout` | 30 / 10 / 30 | Frontend timeouts. |
| `--release` | false | Compile harnesses in release mode. |
| `--memory-limit MB` | — | Frontend memory cap. |
| `--output-format FORMAT` | `text` | `text` or `json`. |

### 2.8 `shatter init`

**Purpose**: Initialize a repository for persistent Shatter project state.

**Syntax**: `shatter init [OPTIONS]`

**Behavior**:
1. Resolve the target directory (explicit `--directory`, detected project root, or current directory).
2. Create `.shatter/` if it does not already exist.
3. Write `.shatter/config.yaml` with starter defaults if it does not already exist.
4. If the project is already initialized, report the existing `.shatter/` contents without overwriting them (idempotent).

**Effect on the project tree**:
- Establishes the repo-local Shatter configuration root at `.shatter/`.
- Signals that project-local Shatter state is expected to live in this repository.
- Other commands may also create `.shatter-cache/` and `shatter-artifacts/` when using the initialized-project path.

**Options**:
- `-d, --directory <DIR>`: Initialize that directory instead of the auto-detected project root.

### 2.9 Project, tooling, and diagnostic commands

These commands support project state, diagnostics, and specialized workflows.

#### `shatter list-targets [DIRECTORY]`

Walk a directory (default `.`) and classify every file as selected, excluded,
unsupported, or candidate-outside-policy, emitting a manifest with stable config
and source-set hashes for CI change tracking. Flags: `--include`/`--exclude`
(repeatable globs), `--language`, `--format text|json|markdown` (default `text`),
`-o/--output PATH`.

#### `shatter cache <ACTION>`

Manage the on-disk cache. `cache clear` clears both the analysis cache
(`.shatter-cache/analysis/`) and results cache (`.shatter-cache/behavior-maps/`);
`--analysis` or `--results` narrows to one.

#### `shatter workspace <ACTION>`

Manage the Go frontend artifact workspace. `workspace gc` prunes old runs and
caps disk use: `--dry-run`, `--keep N` (default 20), `--max-age-days N`
(default 14), `--max-runs-size SIZE` (default `5GiB`), `--max-cache-size SIZE`
(default `5GiB`).

#### `shatter telemetry <ACTION>`

Manage anonymous usage telemetry: `status`, `off`, `on`, `reset-id`.

#### `shatter nondeterminism <ACTION>`

Review and classify suspected-nondeterministic fields. `nondeterminism review`
interactively steps through candidates from the most recent scan; responses
confirm/reject/skip and are persisted to `.shatter/config.yaml` under
`nondeterminism.confirmed` / `nondeterminism.rejected`. Flag: `--cache-dir`
(env: `SHATTER_CACHE_DIR`).

#### `shatter doctor`

Diagnose the local install. Reports embedded frontend hashes and, in a source
checkout, whether the embedded Go frontend is stale relative to `shatter-go/`
sources. Exits nonzero on a detected stale embed.

#### `shatter build-frontend <LANGUAGE>`

Build a custom frontend binary (`go` or `rust`) that embeds user-provided native
generators read from `.shatter/config.yaml`, writing it to `.shatter-cache/bin/`.
Flags: `--config PATH` (`.shatter/` dir, auto-discovered if omitted),
`-o/--output PATH`.

#### `shatter discover-deps <COMMAND>...`

Discover external network dependencies by running a command under strace
(Linux-only diagnostic), capturing network syscalls, and reporting discovered
endpoints. Flags: `--strace`, `--working-dir PATH`, `--json`.

#### `shatter bench`

Run timing benchmarks against a manifest of canonical scenarios, exploring each
function multiple times to produce a structured JSON timing bundle. Flags:
`--manifest PATH` (default `benchmarks/sample-manifest.json`), `--tier`
(`smoke`/`standard`/`full`, default `smoke`), `--repeats N` (default 5),
`--warmups N` (default 1), `--max-iterations N` (default 20), `-o/--output PATH`,
plus `--request-timeout`/`--exec-timeout`/`--build-timeout`.

#### `shatter test`

Run tests with impact analysis: using a coverage map, run only the tests affected
by git-detected changes. Flags: `--all` (bypass impact analysis), `--record`
(refresh the coverage map), `--tier NAME`, `--base REF` (default `HEAD`),
`--include-untracked`, `--dry-run`, `--prioritize` (order by marginal coverage
per unit time), `--budget DURATION` (e.g. `10s`, `2m`; implies `--prioritize`).
This is a test *runner*, not a test *exporter*.

### 2.10 Global Options

Accepted by every command (clap `global = true`):

| Flag | Default | Description |
|------|---------|-------------|
| `--log-level LEVEL` | `info` | Log verbosity: `error`, `warn`, `info`, `debug`, `trace`. |
| `-v` / `--verbose` | — | Increase verbosity (`-v` = debug, `-vv` = trace). |
| `-q` / `--quiet` | — | Decrease to warnings and errors only. |
| `--timing MODE` | `off` | Timing output mode: `off`, `summary`, `detailed`. |
| `--timing-format FORMAT` | `text` | Timing format: `text`, `json`, `both`. |
| `--timing-output PATH` | — | Write one timing artifact JSON to this path (conflicts with `--timing-output-dir`). |
| `--timing-output-dir DIR` | — | Write timing artifact JSON files into this directory. |
| `--project-dir DIR` | auto | Override the auto-detected project root. |
| `--set KEY=VALUE` | — | Override config values by dotted path (repeatable), e.g. `--set defaults.max_iterations=200`. Precedence: above `.shatter/config.yaml`, below dedicated flags. |
| `--color WHEN` | `auto` | Terminal colors: `always`, `auto`, `never` (respects `NO_COLOR`). |
| `--render MODE` | `md` | Terminal rendering: `md` (termimad) or `plain` (legacy ANSI). |

`shatter --version` prints the package version plus build-time embedded frontend
hashes (Go source/binary, TS bundle) so a stale binary is self-describing.

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
- **Provenance**: Whether each pre/postcondition is `proven` (every branch step in the class's path was solved by Z3) or `observed` (seen in all samples but not formally verified). See §3.5 and §2.3 (`specify --solve-file`).
- **Invariants** (optional): Daikon-style properties that hold across all executions or within a class.
- **Coverage**: Iteration count, lines covered, total lines.

Output formats: human-readable markdown, machine-readable JSON, or property-oriented YAML (`specify --yaml`).

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

Inputs are generated from multiple sources, blended by an adaptive strategy
scheduler (disable with `--no-adaptive`):

1. **User-provided inputs**: From `.shatter/config.yaml` or `--inputs`.
2. **Boundary values**: Built-in dictionary of edge-case values per type (0, -1, MAX_INT, empty string, etc.). Disabled with `--no-boundary-values`.
3. **Solver-guided inputs**: Z3 constraint solving negates path constraints to reach unexplored branches. Available in the default explorer and driven end-to-end by the concolic explorer (`--concolic`) and the `solve` stage. Branches reached this way carry `proven` provenance in the spec.
4. **Random generation**: Type-aware random values as fallback.

Alternative explorers can replace the scheduler: `--concolic` (Z3-driven path
negation) and `--genetic` (evolutionary search). `--mcdc` targets
condition-independence witnesses for compound decisions.

### 3.6 Configuration

Hierarchical `.shatter/config.yaml` files can be placed at any level of the project tree. The nearest config to each target file takes precedence. Individual keys can be overridden per-invocation with the global `--set KEY=VALUE` flag.

Running `shatter init` is the explicit way to opt a repository into persistent
project-local Shatter state. That installed-project path establishes
`.shatter/config.yaml` as the repo-local configuration root. Depending on the
commands you run, Shatter may also persist repo-local cache and artifact data in
`.shatter-cache/` and `shatter-artifacts/`.

```yaml
defaults:
  max_iterations: 50
  setup: "./setup.ts"
  setup_level: function  # or execution

functions:
  "src/auth.ts:validateToken":
    max_iterations: 200
    inputs: ["valid-token", "expired-token", "malformed"]

opaque_types:
  - DatabaseConnection
  - HttpClient
```

Functions that need live resource parameters such as database clients, LSP
clients, HTTP framework contexts, file handles, or async runtimes require
additional setup, generators, an execution adapter, or an intentional
`opaque_types` declaration. See
[`docs/resource-parameters.md`](docs/resource-parameters.md) for the user-facing
decision guide.

**Scope configuration** (`shatter.scope.yaml`): Controls which files/functions are included and which dependencies are mocked.

### 3.7 Caching

Behavior maps can be cached to disk (`--cache-dir`) to avoid re-exploring unchanged functions across runs. Cache files are JSON, keyed by function identity.

When using the project-initialized path, the default repo-local cache location is
under `.shatter-cache/`. The `shatter cache clear` command manages it.

---

## 4. Frontend Protocol

Frontends are long-lived subprocesses communicating via NDJSON over stdio. See `PROTOCOL.md` for the full wire format and `protocol/registry.yaml` for the authoritative message registry.

**Commands** (Core → Frontend):

| Command | Purpose |
|---------|---------|
| `handshake` | Version and capability negotiation. |
| `analyze` | Extract types, branches, dependencies. |
| `instrument` | Prepare a function for execution tracking. |
| `prepare` | Compile/cache an instrumented harness, returning a `prepare_id` reused across executions. |
| `execute` | Run a function with given inputs; return branch decisions, output, and lines executed. Carries an optional invocation `plan`. |
| `setup` / `teardown` | Establish and tear down state at a `setup_level` (session/file/function/execution). |
| `generate` | Produce values for a requested type name or parameter. |
| `get_invocation_plan` | Return an `InvocationPlan` (e.g. how to construct a receiver for a method target). |
| `shutdown` | Graceful termination. |

**Capabilities**: Frontends advertise supported capabilities in the handshake
response. `command:*` capabilities gate whether core may send a command
(`analyze`, `execute`, `instrument`, `prepare`, `setup`, `teardown`, `generate`,
`get_invocation_plan`). `complex_type:*` capabilities declare which semantic
generators the frontend supports.

**Type system**:
- **Base kinds**: `int`, `float`, `str`, `bool`, `array`, `object`, `union`, `nullable`, `unknown`.
- **Complex types**: 30+ semantic generators advertised per-frontend, including `date`, `date_time`, `duration`, `reg_exp`, `big_int`, `big_decimal`, `option`, `result`, `url`, `uuid`, `email`, `money`, `sem_ver`, `ip_address`, `path`, `buffer`, `closure`, `iterator`, and more.
- **Opaque types**: Types the analyzer cannot synthesize (e.g. `DatabaseConnection`) — declared in `opaque_types` and supplied via setup/generators, or their consumers are skipped.

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

### 5.4 Behavioral Specification (YAML)

`shatter specify --yaml` (and `shatter properties`) emit a property-oriented YAML
spec, rendering detected invariants as human-friendly `property:` descriptions
(requires `--invariants` to populate them). Used for readable, review-oriented
specs and for the `properties` command's output.

### 5.5 Behavior Snapshot (JSON)

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

Consumed by `shatter diff` for regression detection.

### 5.6 Scan Reports

Scan and run reports are written to the destinations named by repeatable
`-o/--output` (format inferred from extension: `.md`, `.json`, `.html`, `.txt`)
and/or stdout (`--stdout`, with `--format`).

- **Markdown/HTML/text**: human-readable summary of all explored functions with behavior tables.
- **JSON**: per-function behavior maps, coverage, and analysis.

---

## 6. Live Output and Resume

### 6.1 Live Progress Output

During `shatter scan`, the CLI can emit structured progress events to stderr
so users and downstream tooling can track which function is being explored in
real time.

**Enabling progress output**: Pass `--progress` to `shatter scan`.

When enabled, one NDJSON event is emitted to **stderr** for each function as
its status changes. Without `--progress`, the CLI prints a summary after the
scan completes instead.

**Progress event format** (one JSON object per line on stderr):

<!-- docs-smoke: skip reason="NDJSON stream — one JSON object per line, not a single JSON document" -->
```json
{"type":"progress","status":"started","function":"calculateShipping","current":1,"total":12,"elapsed_ms":42}
{"type":"progress","status":"completed","function":"calculateShipping","current":1,"total":12,"elapsed_ms":1503}
{"type":"progress","status":"started","function":"validateOrder","current":2,"total":12,"elapsed_ms":1510}
{"type":"progress","status":"skipped","function":"fetchUser","current":3,"total":12,"elapsed_ms":1520}
{"type":"progress","status":"failed","function":"processPayment","current":4,"total":12,"elapsed_ms":2100}
```

**Fields**:

| Field | Type | Description |
|-------|------|-------------|
| `type` | string | Always `"progress"` |
| `status` | string | One of `started`, `completed`, `skipped`, `failed` |
| `function` | string | Fully qualified function name |
| `current` | number | 1-based index of this function in the scan order |
| `total` | number | Total number of functions in the scan |
| `elapsed_ms` | number | Milliseconds elapsed since the scan started |

**Status lifecycle**: Each function transitions through exactly one of these
sequences:

- `started` → `completed` (normal exploration)
- `started` → `failed` (exploration error)
- `skipped` (no `started` event — function was skipped before exploration began)

**Skip reasons**: Functions are skipped for several reasons, categorized as
either benign (expected) or error:

- **Benign**: opaque types detected, cache hit (behavior map already cached),
  checkpoint resume (already completed in a previous run)
- **Error**: analysis failure, instrumentation failure, timeout exceeded

Without `--progress`, the CLI logs function progress at the `info` log level:

```
[1/12] calculateShipping (1.5s elapsed)
[2/12] validateOrder (3.2s elapsed)
```

### 6.2 Partial Artifact Layout

During a scan, Shatter writes partial results incrementally so that
intermediate state survives interruptions.

**Checkpoint file** (resume state):

```
shatter-artifacts/scan-results/<scan-id-prefix>/checkpoint.json
```

The `<scan-id-prefix>` is the first 16 hex characters of a SHA-256 hash
computed from the sorted list of source file paths in the scan. The checkpoint
is a lightweight JSON index — the actual behavior map data lives in the
behavior map cache.

**Checkpoint structure**:

```json
{
  "version": "1",
  "scan_id": "a1b2c3d4e5f6...",
  "completed": {
    "calculateShipping": "deep_fp_abc123...",
    "validateOrder": "deep_fp_def456..."
  },
  "layer_index": 2,
  "timestamp": "1712678400",
  "config_hash": "cfg_hash_789..."
}
```

| Field | Description |
|-------|-------------|
| `version` | Format version (currently `"1"`) |
| `scan_id` | Stable hash of the scan's input file set |
| `completed` | Map of function name → deep fingerprint for finished functions |
| `layer_index` | Index of the last fully completed dependency layer |
| `timestamp` | Unix timestamp of last save |
| `config_hash` | Hash of scan config (iterations, timeouts, parallelism) for drift detection |

**Behavior map cache** (per-function results):

```
.shatter-cache/behavior-maps/<function-name>.json
```

Each file contains the complete behavior map for one function: observed
input→output mappings, execution paths, and side effects. These are written
after each function completes exploration, so a partial scan leaves behind
all results gathered before the interruption.

**Write order**: The checkpoint is saved atomically (temp file + rename) after
each dependency layer completes. Within a layer, individual behavior maps are
written to the cache as each function finishes. This means:

1. Functions in completed layers are fully persisted.
2. Functions in the interrupted layer may or may not have cached behavior maps
   depending on how far the layer progressed.
3. The checkpoint only records functions whose behavior maps are confirmed
   cached.

### 6.3 Resume Semantics

`--resume` allows an interrupted scan to continue from where it left off,
skipping functions that were already explored. Pass `auto` to discover the
checkpoint from the standard artifact directory, an explicit checkpoint path, or
`off` (also `no`/`none`/`false`/`disabled`) to disable resume.

**Usage**:

```bash
# Auto-discover checkpoint from the standard artifact directory
shatter scan --resume auto src/

# Explicit checkpoint path
shatter scan --resume /path/to/checkpoint.json src/
```

**How resume works**:

1. Load the checkpoint file (JSON).
2. **Hard compatibility check**: Compare the checkpoint's `scan_id` against the
   current scan's file set. If they differ (files were added or removed), the
   checkpoint is discarded and the scan starts fresh.
3. **Soft config drift check**: Compare the `config_hash`. If scan configuration
   changed (different iteration counts, timeouts, parallelism), a warning is
   logged but the checkpoint is still used — completed functions are reused,
   and pending functions use the new configuration.
4. For each function in the scan order, check three conditions:
   - The function appears in the checkpoint's `completed` map
   - The stored deep fingerprint matches the current source code fingerprint
   - The behavior map still exists in the cache
5. If all three hold, skip the function (status: `skipped`, reason: "resumed
   from checkpoint"). If any condition fails, re-explore the function.

**What "deep fingerprint" means**: A fingerprint computed from the function's
source code (line range extracted from the file). If the source changes between
runs, the fingerprint changes and the function is re-explored even if the
checkpoint lists it as completed.

**What is preserved on resume**:

- All behavior maps from previously completed functions
- The scan dependency order (recalculated, but deterministic for the same file set)
- Mock configurations derived from completed callee behavior maps

**What is re-explored on resume**:

- Functions whose source code changed since the checkpoint was written
- Functions whose cached behavior maps were deleted
- Functions that were in progress when the scan was interrupted (partially
  completed layers)
- All functions in layers above the last completed layer

**Limitations**:

1. Resume requires the same set of source files. Adding or removing files from
   the scan scope invalidates the checkpoint.
2. Config drift is a soft warning only — changing `--max-iterations` between
   runs means resumed functions used the old iteration count while new functions
   use the new count.
3. The checkpoint does not store the order of function completion within a layer.
   On resume, functions within a layer may be explored in a different order.
4. Resume does not work across different `--parallelism` settings in a way that
   preserves deterministic behavior — the results are still correct, but the
   exploration order within layers may differ.

### 6.4 Examples

**Live output with `--progress`**:

```bash
$ shatter scan --progress src/
{"type":"progress","status":"started","function":"add","current":1,"total":4,"elapsed_ms":15}
{"type":"progress","status":"completed","function":"add","current":1,"total":4,"elapsed_ms":312}
{"type":"progress","status":"started","function":"multiply","current":2,"total":4,"elapsed_ms":315}
{"type":"progress","status":"completed","function":"multiply","current":2,"total":4,"elapsed_ms":890}
{"type":"progress","status":"started","function":"calculateTotal","current":3,"total":4,"elapsed_ms":893}
{"type":"progress","status":"completed","function":"calculateTotal","current":3,"total":4,"elapsed_ms":2105}
{"type":"progress","status":"skipped","function":"formatOutput","current":4,"total":4,"elapsed_ms":2108}

Scan complete: 3 completed, 0 failed, 0 unsupported, 1 skipped (4 worker(s))

-- add --
  Iterations: 100
  Unique paths: 1
  ...
```

**Final output without `--progress`** (default):

```bash
$ shatter scan src/
[1/4] add (0.3s elapsed)
[2/4] multiply (0.9s elapsed)
[3/4] calculateTotal (2.1s elapsed)
[4/4] formatOutput (2.1s elapsed)

Scan complete: 3 completed, 0 failed, 0 unsupported, 1 skipped (4 worker(s))

-- add --
  Iterations: 100
  Unique paths: 1
  ...
```

**Interrupted and resumed scan**:

```bash
# First run — interrupted after 2 functions
$ shatter scan --resume auto src/
[1/4] add (0.3s elapsed)
[2/4] multiply (0.9s elapsed)
^C  # interrupted

# Checkpoint saved at shatter-artifacts/scan-results/<id>/checkpoint.json
# Behavior maps for add and multiply cached in .shatter-cache/behavior-maps/

# Second run — resumes from checkpoint
$ shatter scan --resume auto src/
# add: skipped (resumed from checkpoint)
# multiply: skipped (resumed from checkpoint)
[3/4] calculateTotal (0.5s elapsed)
[4/4] formatOutput (1.2s elapsed)

Scan complete: 2 completed, 0 failed, 0 unsupported, 2 skipped (4 worker(s))
```

**Resume after source change**:

```bash
# Edit multiply.ts, then resume
$ shatter scan --resume auto src/
# add: skipped (resumed from checkpoint, fingerprint matches)
# multiply: re-explored (source fingerprint changed)
[2/4] multiply (0.6s elapsed)
[3/4] calculateTotal (1.1s elapsed)
[4/4] formatOutput (1.8s elapsed)
```

---

## 7. Known Limitations

1. **Rust parity gaps remain**: `shatter-rust` is supported, but some advanced analysis and execution-planning surfaces still lag TypeScript/Go. Known gaps include ITE-producing data-flow analysis, some planner-surface capabilities, and environment preflight emission. See `protocol/parity-matrix.yaml`.
2. **No Windows support**: Frontends assume Unix-like process spawning.
3. **Limited type inference**: Complex TypeScript types (generics, conditional types, mapped types) may not be fully analyzed. Cross-crate/external Rust structs the single-file analyzer cannot resolve become opaque and their consumers may be skipped.
4. **Limited string theory support**: Z3 Seq theory covers 8 string operations (see `string-ops.yaml`). `split()` cannot be modeled (would require bounded unrolling). Regex support is limited to Z3's decidable fragment (`str.in_re`) — backreferences, lookahead, and named groups are unsupported. Functions using these operations fall back to random/mutation-based exploration. Planned workaround: frontend-side structural candidate generation (deferred).
5. **Async support is adapter-bound**: The TypeScript executor awaits returned promises, and the Rust frontend runs async functions via Tokio (including Axum handler adapters). Async code paths that need a live runtime resource still require setup, generators, or an execution adapter — plain async/await over pure values is handled, but arbitrary async I/O is not automatically synthesized.

> Historical note: earlier revisions of this spec listed "explorer is primarily
> random" and "provenance is always observed" as limitations. Both are obsolete:
> the concolic explorer (`--concolic`) and `solve` stage drive Z3 path negation,
> and Z3-solved branches carry `proven` provenance (`shatter-core/src/spec.rs`).

---

## 8. Changelog

| Date | Change | Section |
|------|--------|---------|
| 2026-07-03 | SPEC overhaul: documented all 24 CLI commands (added observe/analyze/solve/specify/properties/compare/build-frontend/discover-deps/stale/revalidate/test/telemetry/cache/workspace/nondeterminism/bench/list-targets/doctor); corrected explore/scan/run/global flag tables against `args.rs` (removed nonexistent `--timeout`, `--output-dir`/`--report`/`--progress-json`, `--perf`; documented `--concolic`, staged pipeline, `-o`/`--stdout`/`--format`, progress-on-stderr); removed the deleted test-export output; refreshed the protocol summary and type system; rewrote stale limitations (random-only explorer, always-observed provenance, no-async). | 2, 3, 4, 5, 7 |
| 2026-04-09 | Added live output, partial artifacts, and resume documentation | 6 |
| 2026-02-28 | Initial specification created | All |
