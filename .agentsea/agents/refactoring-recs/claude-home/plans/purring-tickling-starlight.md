# Plan: str-ncs — Call-graph-aware fingerprint composition

## Context

Deep fingerprints currently compose callee FPs only within a single file (`compute_deep_fingerprints()` in `fingerprint.rs`). Cross-file callees are filtered as "out of scope" and ignored. This means if `file_b.ts::helper()` changes, `file_a.ts::main()` (which calls `helper`) is NOT marked stale. The call graph (`call_graph.rs`) knows about cross-file edges but fingerprinting doesn't use them.

This issue bridges that gap: use the `CallGraph` to compose fingerprints across file boundaries and propagate staleness transitively.

## Files to Modify

| File | Changes |
|------|---------|
| `shatter-core/src/call_graph.rs` | Add `transitive_callers_of()` |
| `shatter-core/src/fingerprint.rs` | Add `FingerprintRegistry`, `compute_cross_file_deep_fingerprints()`, `StalenessReason`, `CrossFileIncrementalPlan`, `compute_cross_file_staleness()` |
| `shatter-core/src/test_arbitraries.rs` | Add `arb_fingerprint_registry()` if needed |

## Implementation Steps

### Step 1: Add `transitive_callers_of()` to `CallGraph` (`call_graph.rs`)

BFS on `self.rev` from seed nodes. Returns `HashSet<String>` of all transitive callers (including seeds).

```rust
pub fn transitive_callers_of(&self, seeds: &[&str]) -> HashSet<String> {
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    for &s in seeds {
        if let Some(&idx) = self.node_index.get(s) {
            if visited.insert(idx) { queue.push_back(idx); }
        }
    }
    while let Some(idx) = queue.pop_front() {
        for &caller_idx in &self.rev[idx] {
            if visited.insert(caller_idx) { queue.push_back(caller_idx); }
        }
    }
    visited.into_iter().map(|i| self.nodes[i].clone()).collect()
}
```

**Tests**: linear chain, diamond, cycle, empty seeds, unknown seed name, disjoint subgraphs.

### Step 2: Add `FingerprintRegistry` to `fingerprint.rs`

Stores shallow + deep FPs keyed by qualified name, plus dependency edges.

```rust
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FingerprintRegistry {
    shallow: HashMap<String, String>,
    deep: HashMap<String, String>,
    dependencies: HashMap<String, HashSet<String>>,
}
```

Public accessors: `set_shallow()`, `set_deep()`, `set_dependencies()`, `shallow()`, `deep()`, `dependencies()`, `names()`, `len()`, `is_empty()`.

**Tests**: basic CRUD, empty registry.

### Step 3: Add `compute_cross_file_deep_fingerprints()` to `fingerprint.rs`

Uses `CallGraph::topological_layers()` to process leaves first across all files. For each function, gets callees via `CallGraph::callees_of()`, looks up their deep FPs from the registry, calls existing `compute_deep_fingerprint()`.

```rust
pub fn compute_cross_file_deep_fingerprints(
    shallow_fps: &HashMap<String, String>,
    call_graph: &crate::call_graph::CallGraph,
) -> FingerprintRegistry
```

1. Initialize registry with all shallow FPs
2. Process `call_graph.topological_layers()` in order
3. For each function in layer, call `compute_deep_fingerprint(shallow, accumulated_deep, callees)`
4. Cycle remnants: process remaining with partial callee FPs (same as single-file)
5. Record dependency edges

**Tests**:
- Two-file scenario: callee in file A, caller in file B → cross-file FP composition
- Diamond across files
- Callee change propagates cross-file
- Cycle spanning files
- Single file matches existing `compute_deep_fingerprints()` behavior

**Proptests**:
- Determinism (same inputs → same registry)
- Completeness (every node in graph gets a deep FP)
- Length invariant (all deep FPs are 64-char hex)

### Step 4: Add staleness types and `compute_cross_file_staleness()` to `fingerprint.rs`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StalenessReason {
    SourceChanged,
    CalleeChanged(String),  // qualified name of changed callee
    New,
    NoPreviousFingerprint,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CrossFileIncrementalPlan {
    pub stale: Vec<String>,
    pub fresh: Vec<String>,
    pub removed: Vec<String>,
    pub stale_reasons: HashMap<String, StalenessReason>,
}

pub fn compute_cross_file_staleness(
    current: &FingerprintRegistry,
    previous: &FingerprintRegistry,
    call_graph: &crate::call_graph::CallGraph,
) -> CrossFileIncrementalPlan
```

Logic:
1. Find directly changed functions (deep FP differs, new, or no previous FP)
2. Use `call_graph.transitive_callers_of()` on changed set
3. Mark transitive callers as stale with `CalleeChanged` reason
4. Everything else is fresh
5. Detect removed (in previous but not current)

**Tests**:
- All fresh (no changes)
- Direct source change → stale
- Callee change → transitive callers stale
- New function → stale
- Removed function → removed
- Diamond: change leaf → all ancestors stale

**Proptests**:
- Staleness propagation matches `transitive_callers_of` oracle
- No changes → all fresh
- Fresh + stale + removed partition covers all known functions

## Verification

1. `cargo test -p shatter-core` — all new + existing tests pass
2. `cargo clippy -p shatter-core -- -D warnings` — no warnings
3. Existing `compute_deep_fingerprints()` per-file API unchanged — no regressions

## Scope Boundary

This issue adds the **core data structures and algorithms** for cross-file fingerprint composition. Integration into `scan_orchestrator.rs` and CLI `stale` command is deferred to `str-1fr` (cache invalidation) and `str-3lob` (revalidation CLI). The new public API is designed for those consumers.
