# Plan: str-kab3 — Revalidation Loop

## Context

The revalidation data model (RevalidationVerdict, RevalidationReport, classify_verdict) already exists in `shatter-core/src/revalidation.rs`. This task adds the **revalidation loop**: re-execute pool entries against current code via frontend, compare observed vs. recorded behavior using fingerprints and nondeterminism masks, and classify each result.

## Implementation

### File: `shatter-core/src/revalidation.rs` (extend existing)

#### 1. Add `revalidate_behaviors` async function

```rust
pub async fn revalidate_behaviors(
    frontend: &mut Frontend,
    behavior_map: &BehaviorMap,
    current_fingerprint: Option<&str>,
) -> Result<Vec<RevalidationReport>, FrontendError>
```

Logic:
- Determine `code_changed` by comparing `behavior_map.fingerprint` vs `current_fingerprint` (changed if either is None or they differ)
- Extract `nondeterministic_fields` from `behavior_map.nondeterministic_fields`
- For each `Behavior` in `behavior_map.behaviors`:
  1. Send `Command::Execute { function, inputs: behavior.input_args, mocks: vec![], setup_context: None }` via `frontend.send()`
  2. Extract observed `branch_path` and `thrown_error` from the `ExecuteResult`
  3. Compute `observed_severity` using `classify_severity(thrown_error, is_crash=false)`. If the response is an error (frontend error, not thrown_error), set `observed_severity = None`
  4. Compare branch paths using a masking-aware comparison (see step 2 below)
  5. Call `classify_verdict(code_changed, path_matches, expected_severity, observed_severity)`
  6. Build `RevalidationReport` with all fields

#### 2. Add `branch_paths_match` helper

```rust
fn branch_paths_match(
    expected: &[BranchDecision],
    observed: &[BranchDecision],
    nondeterministic_fields: &[NondeterministicField],
) -> bool
```

- Compare branch paths structurally (branch_id, taken)
- If lengths differ, return false
- For each pair, compare `branch_id` and `taken` (ignore constraint text — it's symbolic, not behavioral)
- Nondeterministic fields with path prefix `"branch."` could cause path divergence — if the only differing branches correspond to nondeterministic field paths, treat as matching

#### 3. Derive `expected_severity` from `Behavior`

Add a helper to derive severity from a `Behavior`:
```rust
fn severity_from_behavior(behavior: &Behavior) -> Severity
```
Uses `classify_severity(behavior.thrown_error.as_ref(), false)`.

### New imports needed

- `crate::frontend::{Frontend, FrontendError}`
- `crate::protocol::Command as ProtoCommand`
- `crate::behavior::{Behavior, BehaviorMap}`
- `crate::nondeterminism::NondeterministicField`
- `crate::interesting_pool::{classify_severity, Severity}`

### Testing

#### Unit tests (no real frontend needed)

1. **`branch_paths_match` tests**: matching paths, differing paths, different lengths, nondeterministic masking
2. **`severity_from_behavior` tests**: all four severity levels
3. **Proptest**: `classify_verdict` with arbitrary combinations (already has unit tests, add proptest for completeness)

#### Integration test (with noop-frontend.sh)

4. **`revalidate_behaviors` with noop frontend**: spawn noop-frontend.sh, create a BehaviorMap with known behaviors, revalidate, verify all come back as either Confirmed or the appropriate drift verdict (noop returns empty branch_path, so most will be Flaky or ExpectedDrift)

### Files to modify

| File | Change |
|------|--------|
| `shatter-core/src/revalidation.rs` | Add `revalidate_behaviors`, `branch_paths_match`, `severity_from_behavior`, tests |

### Verification

1. `cargo test -p shatter-core -- revalidation` — all tests pass
2. `cargo clippy -p shatter-core -- -D warnings` — clean
3. Proptest coverage for branch_paths_match and verdict classification
