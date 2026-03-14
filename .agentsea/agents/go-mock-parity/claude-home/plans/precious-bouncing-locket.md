# str-xmtw: Symbolic Triage Integration Assembly

## Context

Issue str-xmtw is the integration/assembly task for symbolic triage. All 5 sub-tasks are complete:
- str-awmq: concrete evaluator (`evaluate_constraint`, `predict_branch`)
- str-pvqc: triage state/verdict types (`TriageState`, `TriageVerdict`, `TriageDisableReason`)
- str-v0a2: adaptive self-disabling (`record_verdict`, `record_sample`, disable checks)
- str-2wxm: orchestrator wiring (full integration in `explore()` loop)
- str-7qw5: tests (74 unit tests + 1 integration test)

The module is fully implemented (1,854 lines in `triage.rs`) and wired into the orchestrator. This task verifies everything works together and fills the remaining quality gap: **missing proptest coverage**.

## Gap Analysis

| Requirement | Status |
|---|---|
| triage.rs implemented | Done |
| Declared in lib.rs | Done |
| Orchestrator wired | Done |
| Unit tests (74) | Done |
| Integration test (1) | Done |
| cargo test passes | Verify |
| cargo clippy clean | Verify |
| E2E concolic tests pass | Verify |
| **Proptest coverage** | **Missing** |
| Proptest generators in test_arbitraries.rs | **Missing** |

Per CLAUDE.md: "Every component uses PBT" and "every module that handles untrusted input... should have PBT coverage of its core invariants."

## Plan

### Step 1: Setup worktree and verify current state
- Create worktree on branch `str-xmtw`
- Run `cargo test` in shatter-core
- Run `cargo clippy -- -D warnings`
- Run `cargo test --test e2e_concolic`

### Step 2: Add proptest generators for triage types
**File:** `shatter-core/src/test_arbitraries.rs`

Add `Arbitrary` implementations for:
- `BranchPrediction` (3 variants)
- `TriageVerdict` (3 variants with fields)
- `TriageDisableReason` (2 variants)

These reuse existing generators (e.g., `arb_sym_expr()` from test_arbitraries.rs).

### Step 3: Add proptest properties to triage.rs
**File:** `shatter-core/src/triage.rs` (add to existing `#[cfg(test)]` module)

Priority properties:
1. **evaluate_constraint roundtrip consistency** — evaluating the same expression with same inputs always returns the same result (determinism)
2. **predict_branch consistency** — prediction matches manual evaluate_constraint + is_truthy
3. **TriageState state machine invariants:**
   - `trace_count() <= MAX_TRACES` always holds
   - `update()` is idempotent for duplicate traces (dedup)
   - `observed_direction_count()` monotonically increases
   - After disable, `is_disabled()` stays true regardless of further operations
4. **Verdict invariants:**
   - With empty traces, verdict is always `Indeterminate`
   - Skip verdict requires all predicted directions to be already observed
5. **Adaptive disabling:**
   - Recording only `Execute` verdicts (never Skip) eventually disables triage

### Step 4: Final verification
- `cargo test` (all pass including new proptests)
- `cargo clippy -- -D warnings` (clean)
- `cargo test --test e2e_concolic` (E2E pass)

### Step 5: Commit, push, close issue
- Commit with descriptive message
- Push branch
- `bd close str-xmtw`

## Key Files
- `shatter-core/src/triage.rs` — add proptest block
- `shatter-core/src/test_arbitraries.rs` — add triage type generators
- `shatter-core/src/orchestrator.rs` — read-only verification

## Verification
1. `cargo test -p shatter-core` — all tests pass
2. `cargo clippy -p shatter-core -- -D warnings` — no warnings
3. `cargo test --test e2e_concolic` — E2E pipeline works
4. New proptests exercise core invariants (not just serialization)
