# MC/DC Coverage Support — Design Spec

**Date**: 2026-03-17
**Status**: Draft
**Exploration**: `docs/explorations/mcdc-coverage.md`

## Overview

Add Modified Condition/Decision Coverage (MC/DC) to Shatter via a `--mcdc` CLI flag. When enabled, frontends decompose compound boolean decisions into individual conditions and report per-condition truth values. The core tracks an MC/DC coverage table, generates targeted Z3 queries for missing independence pairs, and reports MC/DC percentages alongside branch coverage. The flag also considerably increases time and operation budgets to allow the solver to find the harder condition-independence witnesses.

## Approach: Hybrid (Approach 3 from exploration doc)

Frontends decompose compound decisions at instrumentation time and report per-condition outcomes in the protocol. The core maintains an MC/DC truth table per decision, identifies missing independence pairs, and generates targeted Z3 queries that hold other conditions constant while flipping the target condition.

## Terminology

- **Decision**: A compound boolean expression at a branch point (e.g., `a > 0 && b < 10`).
- **Condition**: An atomic boolean sub-expression within a decision (e.g., `a > 0`).
- **Independence pair**: Two test inputs where exactly one condition flips and the decision outcome flips. This demonstrates the condition independently affects the decision.
- **Masking MC/DC**: A condition is "masked" when short-circuit evaluation prevents it from being observed. Masked conditions are excluded from independence requirements. This is the standard for short-circuit languages (C, Java, TypeScript, Go).
- **Unique-cause masking MC/DC**: The specific MC/DC variant we implement. For a condition to have an independence pair, all other *non-masked* conditions must hold the same values between the two test inputs. This is stricter than general masking MC/DC (which only requires conditions that could mask the target to hold constant) but simpler to implement and verify. DO-178C accepts both variants.

---

## 1. CLI Changes

### `--mcdc` flag

Add to `shatter-cli/src/args.rs` Explore variant:

```rust
/// Enable MC/DC (Modified Condition/Decision Coverage) analysis.
/// Decomposes compound boolean decisions into individual conditions
/// and targets condition-independence witnesses. Implies increased
/// iteration/execution/plateau budgets.
#[arg(long)]
mcdc: bool,
```

### Budget multipliers

When `--mcdc` is set and no explicit override is given, the CLI applies these multipliers in `main.rs` (in the explore command handler) when constructing `ExploreConfig`:

| Parameter | Default | With `--mcdc` | Rationale |
|---|---|---|---|
| `max_iterations` | 100 | 500 (5x) | N conditions need N+1 tests per decision; typical functions have multiple compound decisions |
| `max_executions` | max_iterations × 5 | max_iterations × 5 (= 2500) | Proportional scaling |
| `plateau_threshold` | 20 | 60 (3x) | MC/DC targets are harder to satisfy — need more attempts before declaring plateau |
| `solver_timeout_ms` | user-provided or None | user-provided or 10_000ms | MC/DC queries are more constrained; give Z3 more time per query |
| `timeout` (wall-clock) | 60s | 300s (5x) | Overall budget increase |
| `timeout_explore` | user-provided | user-provided or None | Per-function; let user control this explicitly |

If the user explicitly provides `--max-iterations`, `--timeout`, or `--solver-timeout`, those values override the MC/DC defaults. The multipliers apply only to unspecified values.

The `mcdc: bool` flag is threaded through `ExploreConfig` to the orchestrator so it can activate MC/DC-specific solver logic. Add to `ExploreConfig` in `orchestrator.rs`:

```rust
/// Enable MC/DC coverage analysis. When true, the orchestrator tracks
/// per-condition independence and generates targeted Z3 queries for
/// missing MC/DC pairs.
pub mcdc: bool,
```

Default: `false`. Set to `true` when `--mcdc` is passed.

### Parallel parity

`--mcdc` must work with both `--concolic` (orchestrator path) and the default random explorer path. For random exploration, `--mcdc` only affects reporting (post-hoc analysis of observed traces). For concolic, it also drives targeted Z3 queries. The CLI must pass the flag to both code paths.

---

## 2. Protocol Changes

### `BranchDecision` gains optional `conditions` field

**Schema change** (`protocol/schemas/branch-decision.schema.json`):

```json
{
  "type": "object",
  "required": ["branch_id", "line", "taken", "constraint"],
  "properties": {
    "branch_id": { "type": "integer", "minimum": 0 },
    "line": { "type": "integer", "minimum": 0 },
    "taken": { "type": "boolean" },
    "constraint": { "$ref": "sym-constraint.schema.json" },
    "conditions": {
      "type": "array",
      "items": { "$ref": "condition-outcome.schema.json" },
      "description": "Per-condition outcomes for MC/DC analysis. Present only when MC/DC mode is enabled and the decision is compound."
    }
  }
}
```

### New `condition-outcome.schema.json`

```json
{
  "type": "object",
  "required": ["condition_index", "value", "constraint"],
  "properties": {
    "condition_index": {
      "type": "integer",
      "minimum": 0,
      "description": "Index of this condition within the parent decision's condition list (left-to-right in source order)."
    },
    "value": {
      "type": ["boolean", "null"],
      "description": "The concrete truth value of this condition. Null if masked by short-circuit evaluation."
    },
    "masked": {
      "type": "boolean",
      "default": false,
      "description": "True if this condition was not evaluated due to short-circuit semantics."
    },
    "constraint": {
      "$ref": "sym-constraint.schema.json",
      "description": "The symbolic constraint for this individual condition. For masked conditions, use Unknown { hint: 'masked by short-circuit' } since the expression was never evaluated."
    }
  }
}
```

### Rust types

The `BranchDecision` type lives in `shatter-core/src/execution_record.rs` (the canonical Rust definition). It is also defined independently in each frontend's protocol types (`shatter-ts/src/protocol.ts`, `shatter-go/protocol/types.go`, `shatter-rust/src/protocol.rs`). All four must be updated to include the optional `conditions` field.

In `shatter-core/src/execution_record.rs`:

```rust
/// Outcome of an individual condition within a compound decision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConditionOutcome {
    /// Index within the parent decision's condition list (source order).
    pub condition_index: u32,
    /// Concrete truth value. None if masked by short-circuit.
    pub value: Option<bool>,
    /// Whether short-circuit evaluation prevented observation.
    #[serde(default)]
    pub masked: bool,
    /// Symbolic constraint for this individual condition.
    pub constraint: SymConstraint,
}
```

Add to `BranchDecision`:

```rust
pub struct BranchDecision {
    pub branch_id: u32,
    pub line: u32,
    pub taken: bool,
    pub constraint: SymConstraint,
    /// Per-condition outcomes for MC/DC. Present only in MC/DC mode
    /// for compound decisions (those with && or ||).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conditions: Option<Vec<ConditionOutcome>>,
}
```

### Protocol governance

This is an additive, backward-compatible change:
- `conditions` is optional and defaults to absent
- Existing frontends that don't emit it continue to work
- The core gracefully handles `None` (no MC/DC analysis for that decision)

Update `protocol/registry.yaml` version and add `condition-outcome` to the schema list. Add valid + invalid fixtures per `protocol/GOVERNANCE.md`.

**Path hash invariant**: The `conditions` field must NOT affect path hash computation. Two executions with the same `branch_path` (same branch_id + taken sequence) but different per-condition values are the same path from the orchestrator's perspective. MC/DC analysis happens on top of path deduplication, not within it.

---

## 3. Frontend Changes

### 3.1 TypeScript (`shatter-ts/src/instrumentor.ts`)

#### Condition decomposition

Modify `wrapBranchCondition` to detect compound `&&`/`||` at the top level of the decision expression. When MC/DC mode is enabled (communicated via env var `SHATTER_MCDC=1`):

1. **Flatten** the `&&`/`||` tree into a list of leaf conditions (atomic comparisons, calls, identifiers -- anything that is not `&&`/`||`).
2. **Emit per-condition recording calls** that respect short-circuit order.
3. **Reconstruct** the original boolean expression using the recorded condition variables.

**Before (current)**:
```typescript
// if (a > 0 && b < 10)
if (__shatter_branch(0, 5, !!(a > 0 && b < 10), symExprFull)) { ... }
```

**After (MC/DC mode)**:
```typescript
// if (a > 0 && b < 10)
// Conditions are passed as thunks to preserve short-circuit semantics.
// Masked conditions get value: null, masked: true.
const __mcdc0 = __shatter_mcdc_record(0, [symExprA, symExprB], "and",
  [() => !!(a > 0), () => !!(b < 10)],
);
if (__shatter_branch_mcdc(0, 5, __mcdc0.decision, symExprFull, __mcdc0.conditions)) { ... }
```

Key details:
- `__shatter_mcdc_record(branchId, symExprs[], operator, conditionThunks[])` accepts conditions as **thunks** (zero-arg functions) and runs them left-to-right, respecting short-circuit semantics. For `&&`, it stops after the first `false`; remaining conditions are recorded with `value: null, masked: true`. For `||`, it stops after the first `true`.
- Thunks ensure masked conditions are **never executed**. This preserves the original program's semantics -- side-effectful conditions are only called if the language would have called them.
- `__shatter_branch_mcdc` records both the decision and per-condition outcomes.
- For simple (non-compound) decisions, the existing `__shatter_branch` is used unchanged.

#### Short-circuit masking (preserves program semantics)

For `A && B && C`:
- Run A's thunk. If false: B and C are masked (`value: null, masked: true`). Decision is false.
- If A is true, run B's thunk. If false: C is masked. Decision is false.
- If both true, run C's thunk. Decision is C's value.
- **No execution of masked conditions.** This is critical for correctness with side-effectful conditions.

For `A || B || C`:
- Run A's thunk. If true: B and C are masked. Decision is true.
- If A is false, run B's thunk. If true: C is masked. Decision is true.
- If both false, run C's thunk. Decision is C's value.

#### Mixed operators: `(A && B) || C`

**V1 limitation**: MC/DC decomposition is limited to **pure `&&` chains and pure `||` chains only**. Mixed `&&`/`||` trees are treated as single decisions (branch coverage only) with a diagnostic warning: "MC/DC: skipping mixed-operator decision at line N (not yet supported)."

This avoids the tree-encoding complexity and covers the vast majority of real-world compound decisions. Mixed operator support can be added in a follow-up by extending the `ConditionOutcome` schema with an optional operator tree structure.

#### buildSymExpr changes

`buildSymExpr` already recurses into `&&`/`||` and builds `BinOp { op: And/Or, left, right }`. For MC/DC, add a parallel function `flattenConditions(expr)` that extracts the leaf conditions from the `&&`/`||` tree along with their SymExpr representations. This reuses the existing `buildSymExpr` recursion.

#### MC/DC mode detection

The frontend reads `SHATTER_MCDC=1` from the environment (set by the CLI before spawning the frontend). This follows the existing pattern of `SHATTER_EXEC_TIMEOUT` and `SHATTER_LOG_LEVEL`.

### 3.2 Go (`shatter-go`)

Go's branch tracking writes to a JSON recording file read by the executor. The instrumented harness needs the same decomposition:

1. For compound conditions in `if`/`for`/`switch`, generate per-condition variables
2. Record `conditions` array in the JSON recording alongside `branch_path`
3. Apply the same masking logic (Go has short-circuit `&&`/`||`)

Go's instrumentor is in `instrument/` — the condition decomposition is mechanical (same AST walk pattern as TypeScript). The Go frontend reads `SHATTER_MCDC` env var.

### 3.3 Rust (`shatter-rust`)

Execute is unimplemented in the Rust frontend. Add the `ConditionOutcome` type to `src/protocol.rs` for forward compatibility, but no instrumentation changes are needed until execute is implemented.

---

## 4. Core Changes

### 4.1 MC/DC Truth Table (`shatter-core/src/mcdc.rs` — new file)

```rust
/// MC/DC coverage tracking for a single compound decision.
#[derive(Debug, Clone)]
pub struct DecisionMcdc {
    /// Branch ID of the parent decision.
    pub branch_id: u32,
    /// Number of leaf conditions in this decision.
    pub num_conditions: usize,
    /// Observed truth rows: (condition_values, decision_outcome).
    /// condition_values[i] is Some(bool) if observed, None if masked.
    pub observations: Vec<McdcObservation>,
    /// Which conditions have independence pairs satisfied.
    pub independent: Vec<bool>,
}

#[derive(Debug, Clone)]
pub struct McdcObservation {
    pub condition_values: Vec<Option<bool>>,
    pub decision_outcome: bool,
}

/// MC/DC state for an entire function.
#[derive(Debug, Clone)]
pub struct McdcTable {
    /// Per-decision MC/DC tracking. Key is branch_id.
    pub decisions: HashMap<u32, DecisionMcdc>,
}
```

**Independence check**: For each condition `i` in a decision, search the observations for a pair where:
1. Condition `i` has opposite values in the two observations
2. All other non-masked conditions `j != i` have the same values
3. The decision outcome is different

This is the **unique-cause masking MC/DC** criterion (the standard for short-circuit languages, per DO-178C).

### 4.2 Targeted solver queries (`shatter-core/src/orchestrator.rs`)

After each new-path observation in the concolic loop, when `config.mcdc` is true:

1. **Update MC/DC table**: For each `BranchDecision` with `conditions: Some(...)`, add the observation row to `McdcTable`.
2. **Identify gaps**: For each decision, find conditions lacking independence pairs.
3. **Generate targeted queries**: For each unsatisfied condition `i`:
   - Take the most recent observation where condition `i` was not masked
   - Build a Z3 query: assert prefix constraints + hold all other conditions at their observed values + negate condition `i`
   - If SAT, check whether the decision outcome would flip (this may require a second execution with the solved inputs)
   - Add the solution to the worklist with `InputSource::McdcTarget`

New `InputSource` variant. The existing enum uses discriminant values for `Ord`-based worklist priority:

```rust
// Existing (do not renumber):
//   Seed = 0, Fuzzed = 1, BoundarySearch = 2, Drilled = 3,
//   Z3Solved = 4, UserProvided = 5

pub enum InputSource {
    Seed = 0,
    Fuzzed = 1,
    BoundarySearch = 2,
    Drilled = 3,
    /// MC/DC-targeted: Z3 solved with condition-independence constraint.
    /// Ranks between Drilled and Z3Solved — MC/DC refines coverage within
    /// already-visited branches, while Z3Solved discovers new branch paths.
    McdcTarget = 4,
    Z3Solved = 5,
    UserProvided = 6,
}
```

**Renumbering note**: This shifts `Z3Solved` from 4 to 5 and `UserProvided` from 5 to 6. `InputSource` discriminants are used only for in-memory worklist ordering (via `Ord` derive), never serialized to disk or wire. The renumbering is safe. Verify no code depends on specific discriminant values.

### 4.3 Solver extension (`shatter-core/src/solver.rs`)

Add a new function:

```rust
/// Solve for a condition-independence witness.
///
/// Given a decision's constraints and a target condition index, find inputs
/// where the target condition flips while all other conditions remain at
/// their observed values.
///
/// `prefix` — path constraints leading up to this decision (asserted as-is)
/// `conditions` — per-condition SymExprs for the compound decision
/// `observed` — observed truth values from a prior execution
/// `target_index` — which condition to flip
/// `solver_timeout_ms` — per-query Z3 timeout
/// `param_infos` — parameter type information
pub fn solve_for_mcdc_independence(
    prefix: &[SymExpr],
    conditions: &[SymExpr],
    observed: &[Option<bool>],
    target_index: usize,
    solver_timeout_ms: Option<u64>,
    param_infos: &[ParamInfo],
) -> Result<SolveResult, SolverError>
```

**Algorithm**:
1. Assert all prefix constraints
2. For each condition `j != target_index` where `observed[j]` is `Some(val)`:
   - If `val` is true: assert `condition_j` (as-is)
   - If `val` is false: assert `NOT condition_j`
3. For `target_index` where `observed[target_index]` is `Some(val)`:
   - Assert `NOT condition_target` if val was true (flip it)
   - Assert `condition_target` if val was false (flip it)
4. Call Z3, extract solution

This is a straightforward extension of the existing `solve_for_new_path` — same Z3 machinery, different constraint assembly.

**Handling `Unknown` conditions**: If `conditions[j]` has `SymConstraint::Unknown`, skip it (do not assert in Z3). If the target condition is `Unknown`, skip the query entirely — MC/DC analysis is not possible for opaque conditions. Report as `opaque` in metrics.

**Verification requirement**: A SAT result from Z3 does not guarantee the decision outcome actually flips — the solver constrains individual conditions but the runtime decision depends on the full expression including short-circuit semantics. The orchestrator **must re-execute** with the solved inputs and verify that (a) the target condition actually flipped, and (b) the decision outcome actually changed. Only then is the independence pair recorded in `McdcTable`. Failed verifications are logged at debug level but do not count as budget waste (the execution still contributes to branch coverage).

### 4.4 Termination changes

MC/DC convergence is tracked alongside branch convergence:

```rust
pub enum TerminationReason {
    MaxIterations,
    MaxExecutions,
    CoveragePlateau,
    WorklistExhausted,
    TimeoutExplore,
    /// All MC/DC independence pairs satisfied (100% MC/DC).
    McdcComplete,
}
```

The orchestrator checks MC/DC completeness after each observation. If all conditions in all compound decisions have independence pairs, terminate early with `McdcComplete` (even if branch budget remains).

The plateau counter resets when either a new branch path is found **or** a new MC/DC independence pair is satisfied. This prevents premature plateau termination when the solver is making progress on MC/DC goals.

---

## 5. Coverage Metrics and Reporting

### Metrics extension (`shatter-core/src/coverage_metrics.rs`)

```rust
/// MC/DC coverage metrics for a function.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct McdcMetrics {
    /// Total compound decisions (branches with 2+ conditions).
    pub total_decisions: usize,
    /// Total leaf conditions across all compound decisions.
    pub total_conditions: usize,
    /// Conditions with independence pairs satisfied.
    pub independent_conditions: usize,
    /// Conditions in Unknown constraints (can't analyze).
    pub opaque_conditions: usize,
    /// Conditions masked in all observations (never independently evaluable).
    pub always_masked: usize,
    /// MC/DC percentage: independent / (total - opaque - always_masked).
    pub mcdc_percentage: f64,
}
```

Add `mcdc_metrics: Option<McdcMetrics>` to `CoverageMetrics`.

### Report output

When MC/DC is enabled, `format_exploration_report` adds a section after branch coverage:

```
  MC/DC Coverage
  ├─ Decisions: 4 compound (of 8 total branches)
  ├─ Conditions: 11 total, 2 opaque, 1 always masked
  ├─ Independent: 7/8 analyzable (87.5%)
  │  ├─ line 15: if (a > 0 && b < 10) — 2/2 ✓
  │  ├─ line 28: if (x || y && z) — 2/3 (missing: z)
  │  └─ line 42: if (enabled && count > 0) — 2/2 ✓
  └─ [████████░] 87.5%
```

For decisions with unsatisfied conditions, show which condition lacks an independence witness and why (UNSAT, timeout, opaque).

### Discovery method attribution

Add `DiscoveryMethod::McdcTarget` to track how many MC/DC pairs were found by targeted solving vs. accidentally by regular exploration:

```rust
pub enum DiscoveryMethod {
    Z3,
    Random,
    UserProvided,
    Drilled,
    BoundarySearch,
    /// Found by MC/DC-targeted Z3 query.
    McdcTarget,
}
```

Update `CoverageMetrics::from_exploration()` to handle the new variant. `McdcTarget` counts toward `z3_solved` (it is a Z3-based discovery, just with a different constraint formulation). Alternatively, add a dedicated `mcdc_targeted: usize` counter to `CoverageMetrics` for finer attribution.

---

## 6. Export Changes

### Test annotation (`shatter-core/src/export.rs`)

When MC/DC is enabled, exported tests include annotations documenting which MC/DC pairs they satisfy:

```typescript
// MC/DC: condition 0 (a > 0) independently affects decision at line 15
// Pair: {a: 5, b: 3} → true vs {a: -1, b: 3} → false
test("classify: a > 0 independence", () => {
  expect(classify(5, 3)).toBe("can drive");
});
```

This is informational only — the test itself is a normal assertion. The annotation helps users understand why the test exists.

---

## 7. Environment Variable

| Variable | Purpose | Set by |
|---|---|---|
| `SHATTER_MCDC` | `1` to enable MC/DC in frontends | CLI, before spawning frontend |

Follows the existing pattern of `SHATTER_EXEC_TIMEOUT`, `SHATTER_LOG_LEVEL`.

---

## 8. What This Does NOT Include

- **Path coverage**: MC/DC is strictly about condition independence within decisions, not all possible paths through the function.
- **Coupled conditions**: When two conditions share a variable (e.g., `x > 0 && x < 10`), some MC/DC pairs may be UNSAT. The report flags these as "infeasible" rather than "missing."
- **Non-boolean conditions**: Expressions like `if (foo)` where `foo` is a non-boolean (truthy/falsy in JS) are treated as single-condition decisions (no MC/DC decomposition needed — they already have branch coverage).
- **Ternary operator decomposition**: `a ? b : c` is treated as a decision on `a` only. The ternary branches are separate decisions.

---

## 9. Testing Strategy

### Unit tests

- **MC/DC table**: Independence pair detection with known truth tables. Test masking, infeasible pairs, and edge cases (single-condition decisions, deeply nested `&&`/`||`).
- **Targeted solver**: `solve_for_mcdc_independence` with known-solvable and UNSAT cases.
- **Budget multipliers**: CLI applies correct defaults with and without `--mcdc`.
- **Protocol roundtrip**: `ConditionOutcome` serialization in all three frontends.

### Property tests

- **proptest (Rust)**: For arbitrary `McdcTable`, independence detection is monotonic (adding observations never decreases independent count). For arbitrary SymExpr trees with `And`/`Or`, `flattenConditions` preserves logical equivalence.
- **fast-check (TS)**: `flattenConditions` output, when reconstructed with `&&`/`||`, is logically equivalent to the original expression. Masking is consistent with short-circuit semantics.
- **rapid (Go)**: Same properties as TypeScript.

### E2E tests

Add known-answer functions with compound decisions:

```typescript
// examples/typescript/src/13-mcdc-compound.ts
//
// Expected MC/DC analysis:
//   Decision at line 3: if (a > 0 && b < 10)
//     Conditions: [a > 0, b < 10]
//     Independence pairs needed: 2
//     Expected witnesses:
//       a > 0: {a: 1, b: 5} (T,T→T) vs {a: -1, b: 5} (F,T→F)
//       b < 10: {a: 1, b: 5} (T,T→T) vs {a: 1, b: 15} (T,F→F)
export function compoundAnd(a: number, b: number): string {
  if (a > 0 && b < 10) {
    return "both";
  }
  return "neither";
}

// Decision at line 14: if (x || y)
//   Conditions: [x, y]
//   Expected witnesses:
//     x: {x: true, y: false} (T,masked→T) vs {x: false, y: false} (F,F→F)
//     y: {x: false, y: true} (F,T→T) vs {x: false, y: false} (F,F→F)
export function compoundOr(x: boolean, y: boolean): string {
  if (x || y) {
    return "either";
  }
  return "none";
}
```

E2E test in `shatter-core/tests/e2e_concolic.rs` verifies that `--mcdc` achieves 100% MC/DC on these functions and that the report includes the expected MC/DC percentages.

### Walkthrough

Add `--mcdc` to one `explore` invocation in `demo/walkthrough.sh` to exercise the full MC/DC reporting path.

---

## 10. Implementation Order

| Phase | Scope | Dependencies |
|---|---|---|
| **1. Data model** | `ConditionOutcome` type, `McdcTable`, `McdcMetrics`, protocol schema, fixtures | None |
| **2. CLI flag + budgets** | `--mcdc` flag, budget multipliers, env var propagation | Phase 1 |
| **3. TS frontend decomposition** | `flattenConditions`, `__shatter_condition`, `__shatter_branch_mcdc`, masking | Phase 1 |
| **4. Core MC/DC tracking** | `McdcTable` updates in orchestrator, independence detection | Phase 1, 3 |
| **5. Targeted solver** | `solve_for_mcdc_independence`, worklist integration, `McdcTarget` source | Phase 4 |
| **6. Reporting** | MC/DC section in `format_exploration_report`, `McdcMetrics` computation | Phase 4 |
| **7. Go frontend** | Same decomposition as TS, recording format changes | Phase 1 |
| **8. Export annotations** | MC/DC pair documentation in generated tests | Phase 4, 6 |
| **9. E2E + walkthrough** | Known-answer MC/DC tests, walkthrough update | Phase 3, 5, 6 |

Phases 2 and 3 can proceed in parallel. Phase 7 (Go) can proceed in parallel with phases 4-6.

---

## 11. Risks and Mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Eager condition evaluation changes semantics for side-effectful conditions | Correctness | Use masking MC/DC: don't evaluate masked conditions eagerly. Record `masked: true` instead. Only evaluate conditions that the language runtime would evaluate. |
| Exponential blowup for deeply nested `&&`/`||` chains | Performance | Cap condition count per decision at 16. Beyond that, treat as a single decision (branch coverage only) and log a warning. |
| MC/DC queries are UNSAT due to coupled conditions | Completeness | Track infeasible pairs separately. Report "7/8 analyzable, 1 infeasible" rather than penalizing the percentage. |
| Protocol message size increases | Network/perf | `conditions` is `Option` — only present in MC/DC mode. Typical functions have 2-4 conditions per compound decision; the overhead is minimal. |
| Frontend parity burden | Implementation effort | TS first as reference implementation. Go follows the same pattern. Rust frontend is deferred (execute unimplemented). |
