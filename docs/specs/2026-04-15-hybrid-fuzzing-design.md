# Hybrid Concolic + Coverage-Guided Fuzzing

**Issue:** str-e0uy
**Status:** Design
**Date:** 2026-04-15

## Problem

Code paths guarded by opaque transformations (hashing, encryption, compression,
serialization) produce `Unknown` symbolic constraints that Z3 cannot solve. The
concolic orchestrator hits a coverage plateau on these branches and terminates
without exploring them. This is the single largest class of unreachable branches
in real-world code.

## Solution

A phase-separated hybrid fuzzer that activates as a targeted fallback when the
concolic solver stalls on opaque constraints. The fuzzer runs in the core engine
(Rust), uses type-aware mutation with a per-function corpus, and communicates
with frontends through the existing `execute` protocol message. Frontends are
unaware they are being fuzzed.

This is not a general-purpose fuzzing mode. It is a strategy the orchestrator
activates when concolic solving cannot make progress due to opaque constraints.

## Architecture

### Component Overview

```
orchestrator.rs (existing)
  |-- normal concolic loop (unchanged)
  |-- plateau detection (existing, extended with fuzz-trigger logic)
  +-- fuzz phase entry/exit
        +-- fuzzer.rs (new)
              |-- Corpus        -- per-function set of interesting inputs
              |-- Mutator       -- type-aware input mutation
              +-- FuzzSession   -- burst execution loop with termination
```

### New Module: `shatter-core/src/fuzzer.rs`

Contains three components:

- **`Corpus`** — stores inputs that discovered unique paths
- **`Mutator`** — produces type-aware mutations of corpus entries
- **`FuzzSession`** — runs a bounded mutation-execution loop, returns results

### Modified Modules

- **`orchestrator.rs`** — fuzz phase trigger, entry/exit, state tracking
- **`coverage_metrics.rs`** — new `Fuzzed` discovery method and metrics bucket

### Unchanged

- Protocol messages — frontends see normal `execute` commands
- Frontend code — no TS/Go/Rust frontend changes
- CLI interface — no new flags (hybrid is automatic under `--concolic`)
- Explorer (random mode) — unaffected

## Data Flow

1. Orchestrator detects plateau + `Unknown` constraints on frontier branches.
2. Orchestrator creates a `FuzzSession` with: seed corpus (inputs that reached
   near target branches), target branch IDs, parameter type info, budget config.
3. `FuzzSession` runs a tight loop: mutate -> execute (via existing protocol) ->
   check coverage -> update corpus.
4. On completion, `FuzzSession` returns new paths, updated corpus, and phase
   metrics.
5. Orchestrator integrates new paths into `covered_paths`, resets plateau
   counter, resumes concolic. New path constraints may now be Z3-solvable.

## Fuzz Phase Trigger

### Entry Conditions (all must hold)

1. Coverage plateau threshold reached (N consecutive executions with no new
   path).
2. Uncovered frontier contains at least one branch where the associated
   constraint is `Unknown` (not `Unsat`).
3. Those branches are eligible for fuzzing based on the retry model (see
   below).

### Exit Back to Concolic

After the fuzz phase completes, reset the plateau counter and resume the normal
observe-solve-generate loop. If the fuzz phase discovered new paths, their
constraints feed into Z3 on the next concolic iteration.

### When No Fuzz Targets Remain

If the plateau fires but all `Unknown` branches have exhausted their retry
budget (bounded mode only), the orchestrator terminates normally via the
existing `CoveragePlateau` or `WorklistExhausted` logic.

## Retry Model

Binary exhaustion (fuzz once, give up) is too aggressive. Instead, track
attempt state per branch:

```rust
struct FuzzAttemptState {
    count: u32,
    coverage_at_last_attempt: usize,  // covered_paths.len() at time of attempt
}

// Per-orchestrator state
fuzz_attempts: HashMap<u32, FuzzAttemptState>
```

**Retry eligibility:** a branch is eligible for fuzzing when:

- `attempts < max_fuzz_attempts` (bounded mode, default 3), OR
- `covered_paths.len()` has grown since `coverage_at_last_attempt` (coverage
  context changed — the corpus and constraint landscape are different now)

In indefinite mode, `max_fuzz_attempts` is unlimited. Retries are gated only
by the coverage-growth condition, which prevents spinning on branches where
nothing has changed.

**Rationale:** each retry operates with an enriched corpus and potentially
different reachable branches from intervening concolic progress. A mutation
that was useless before may hit a newly-reachable branch after Z3 solves an
adjacent constraint. If no concolic progress occurred between fuzz phases,
retrying is genuinely redundant — the coverage-growth gate prevents this.

**Budget per phase is fixed** (not escalating). The value of retrying comes
from changed coverage context, not from more mutations with the same context.

## Fuzz Phase Termination

Three bounds per phase, whichever hits first:

| Bound | Config field | Default | Purpose |
|---|---|---|---|
| Fuzz-local plateau | `fuzz_plateau_threshold` | 50 consecutive no-new-path executions | Adaptive: stops early on easy targets |
| Execution cap | `fuzz_max_executions` | 1000 | Hard ceiling per phase |
| Wall-clock timeout | `fuzz_timeout` | 30s | Safety net for slow frontends |

### Fuzz Phase Output

Returned to the orchestrator after each phase:

```rust
struct FuzzPhaseResult {
    /// Paths discovered: (path_hash, branch_path, constraints).
    new_paths: Vec<(u64, Vec<BranchDecision>, Vec<SymConstraint>)>,
    /// Updated corpus for future phases.
    corpus: Corpus,
    /// Phase execution statistics.
    stats: FuzzPhaseStats,
}

struct FuzzPhaseStats {
    executions: u32,
    new_paths_found: u32,
    corpus_size: usize,
    termination_reason: FuzzTermination,
    target_branch_ids: Vec<u32>,
}

enum FuzzTermination {
    Plateau,
    ExecutionCap,
    Timeout,
}
```

## Corpus

### Structure

```rust
struct Corpus {
    entries: Vec<CorpusEntry>,
}

struct CorpusEntry {
    inputs: Vec<serde_json::Value>,
    coverage_hash: u64,
    branch_path: Vec<BranchDecision>,
}
```

### Scope

Per-function. Created when the orchestrator begins exploring a function,
persists across fuzz phases within that function, discarded when exploration
of the function ends. Cross-function corpus sharing is a potential future
optimization for scan mode but out of scope here.

### Seeding

At fuzz phase start, the corpus is seeded from the orchestrator's execution
history: any input whose `branch_path` includes the target branch's parent
(the last known branch before the opaque decision point) is included. These
are the inputs that got closest to the target — mutating them is most likely
to steer execution through the opaque branch. On each new-path discovery
during fuzzing, the discovering input is added to the corpus.

## Mutator

### Strategy

Type-aware random mutation. The mutator takes a corpus entry and produces one
mutated input vector. Each call picks one parameter at random and applies one
mutation. Volume compensates for lack of sophistication.

### Mutations by Type

| Type | Mutations |
|---|---|
| `bool` | Flip |
| `i32`/`i64`/`f64` | Add/subtract small delta, negate, boundary values (0, 1, -1, MIN, MAX), random in range |
| `String` | Insert/delete/replace char, splice from another corpus string, empty, single char, repeated char |
| `Vec<T>` | Add/remove/swap elements, mutate random element recursively, empty, single-element |
| `Object` | Mutate random field recursively |
| Nested compounds | Pick a random leaf and mutate it |

### What the Mutator Does Not Do

- No grammar-aware or format-aware mutation (valid JSON, email patterns, etc.).
- No crossover between corpus entries — single-parent mutation only.
- No energy scheduling (AFL-style rare-branch favoring). All corpus entries
  are equally likely to be selected as mutation parents.

These are all viable future optimizations but not needed for the targeted
opaque-path-breaking use case.

## Discovery Attribution

### New Enum Variants

```rust
// coverage_metrics.rs
pub enum DiscoveryMethod {
    Z3,
    Random,
    UserProvided,
    Drilled,
    BoundarySearch,
    McdcTarget,
    Fuzzed,          // new
}

// orchestrator.rs
pub enum InputSource {
    // ... existing variants ...
    Fuzzed = 6,      // new
}
```

`DiscoveryMethod` and `InputSource` are core-internal. They do not appear in
the protocol schema or frontend code — no cross-frontend parity impact.

### Metrics

`CoverageMetrics` gains a `fuzz_found: u32` counter. `MethodPercentages` gains
a `fuzz_pct: f64` field. The explore report's method breakdown includes
`Fuzzed` entries automatically.

Fuzz phase stats (executions, new paths, corpus size, termination reason) are
emitted in verbose/debug output per phase, not in the summary report.

## Configuration

No new CLI flags. Hybrid fuzzing activates automatically during `--concolic`
exploration when trigger conditions are met.

Fuzz parameters are configurable via `.shatter/config.yaml`:

```yaml
fuzz:
  plateau_threshold: 50
  max_executions: 1000
  timeout_seconds: 30
  max_attempts: 3  # null for unlimited (indefinite mode)
```

```rust
pub struct FuzzConfig {
    pub plateau_threshold: u32,     // default: 50
    pub max_executions: u32,        // default: 1000
    pub timeout: Duration,          // default: 30s
    pub max_attempts: Option<u32>,  // default: Some(3), None = unlimited
}
```

### Future CLI Surface (Not in This Design)

- `--no-fuzz` to disable hybrid fuzzing
- `--fuzz-budget N` shorthand for the execution cap

## Testing Strategy

### Unit Tests

- **Mutator:** proptest that mutations preserve type contracts (mutated i32
  is still a valid i32, mutated string is valid UTF-8, mutated vec has valid
  element types). Proptest that mutation actually changes the input (not a
  no-op).
- **Corpus:** add/dedup semantics, seeding from execution history, entries
  with matching coverage hashes are not duplicated.
- **FuzzSession termination:** each of the three bounds fires correctly.
  Plateau detection counts consecutive no-new-path executions accurately.

### Integration Tests

- **Fuzz trigger:** orchestrator enters fuzz phase when plateau + Unknown
  constraints are present. Does not enter fuzz phase when plateau is caused
  by `Unsat` constraints only.
- **Retry model:** orchestrator retries a branch when coverage has grown,
  does not retry when coverage is unchanged and max attempts reached.

### E2E Tests

- **Opaque path discovery:** a test function with a branch guarded by an
  opaque transformation (e.g., `if (md5(x) === "known_hash")`) that the
  concolic solver cannot crack. Verify the fuzzer discovers additional paths
  beyond what pure concolic achieves. This may require a function specifically
  designed so that random mutation has a reasonable chance of hitting the
  target (e.g., a short input space).
- **Metrics attribution:** verify `Fuzzed` appears in the method breakdown
  when the fuzzer discovers paths.

## Related Issues

- **str-f42i** — Pre-execution constraint evaluator to skip predictable paths.
  Independent optimization that benefits both the fuzzer and the existing
  concolic loop. Not a dependency for this design.

## Out of Scope

- Frontend-side mutation (batch execution without IPC per input)
- Constraint-hinted mutation (biasing toward parameters in opaque constraints)
- Cross-function corpus sharing
- Grammar-aware / format-aware mutation
- Crossover between corpus entries
- Energy scheduling
- Explicit `--fuzz` CLI flag

These are all viable follow-ups once the basic hybrid loop is validated.
