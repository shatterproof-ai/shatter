# Static Budget Allocation, Child A Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a config-gated static execution-budget allocator to `shatter scan` (issue str-03mfx.1): a pure allocator module, the `defaults.exploration.budget_allocation` knob plumbed through the CLI into the scan, per-layer allocation of `max_executions` for the concolic path (including replica splitting), and the `BudgetSurplus`/`ClaimPolicy` module move. Under the default `flat`, behavior is identical to today.

**Architecture:** `shatter-core/src/budget_alloc.rs` holds pure functions (`features`, `score`, `allocate`) plus the moved surplus types. The scan orchestrator gets a `BudgetSettings` on `ScanConfig`; after a layer's runnable tasks are built it calls a pure `apply_static_allocation` over them, which sets each task's `max_iterations` and a new `max_executions_override` on the scan-level explorer config; the concolic path honours the override. The CLI resolves `defaults.exploration` (with `--set`) into `BudgetSettings` the same way it resolves LLM config.

**Tech Stack:** Rust 2024, serde, proptest (already dev-deps of shatter-core), Taskfile gates.

**Spec:** `docs/superpowers/specs/2026-09-23-static-budget-allocation-design.md` §1–3, §7 (child A rows), Compatibility contract.

## Global Constraints

- Work only on branch `str-03mfx.1-budget-alloc` in its linked worktree; never edit the primary checkout.
- Under `flat` (the default) nothing in this change may alter scan behavior or output. Every new code path is behind `BudgetAllocation::Static`.
- `static` applies to the concolic scan path only; the random explorer keeps interpreting `max_iterations` as executions and is not modified except for the new optional field it ignores.
- No new clap flags. The knob is reachable via `--set defaults.exploration.budget_allocation=static`.
- Score weights live in one `const` table; the benchmark (child D) is the only thing that tunes them.
- `allocate` conserves `Σ per_function == total` whenever `Σ floor ≤ total ≤ Σ ceiling`, never panics for any non-negative input, and is deterministic.
- Commit with `-c core.hooksPath=/dev/null` (the repo's hook test suite corrupts worktrees; project memory) and run `task test-quick` before each commit instead. Do not run `cargo fmt` on whole crates (the tree is not fmt-clean); write formatted code.
- Each `orchestrator::ExploreConfig` / scan-level `ExploreConfig` literal that lacks `..Default::default()` must gain any new field explicitly; the compiler lists them.

---

### Task 1: Allocator module: features, score, allocate

**Files:**
- Create: `shatter-core/src/budget_alloc.rs`
- Modify: `shatter-core/src/lib.rs` (add `pub mod budget_alloc;` in alphabetical position after `pub mod branch_profile;`)
- Test: inline `#[cfg(test)] mod tests` in `budget_alloc.rs`

**Interfaces:**
- Consumes: `crate::protocol::{FunctionAnalysis, DependencyKind}`, `crate::types::TypeInfo`.
- Produces:

```rust
pub struct BudgetFeatures { pub branches: u32, pub opaque_branches: u32, pub loops: u32, pub deps: u32, pub unmocked_deps: u32, pub complex_params: u32, pub param_nesting: u32, pub lines: u32 }
pub fn features(analysis: &FunctionAnalysis) -> BudgetFeatures
pub fn score(f: &BudgetFeatures) -> f64                                  // >= 1.0
pub struct Demand { pub score: f64, pub floor: u32, pub ceiling: u32 }
pub enum Infeasible { FloorsScaled { requested: u64, total: u32 }, CeilingsBind { dropped: u32 } }
pub struct Allocation { pub per_function: Vec<u32>, pub total: u32, pub infeasible: Option<Infeasible> }
pub fn allocate(demands: &[Demand], total: u32) -> Allocation
pub const WEIGHTS: Weights  // { branches, opaque, loops, deps, unmocked, complex_params, nesting, lines_per_20 }
```

- [ ] **Step 1: Write the failing tests**

Create `shatter-core/src/budget_alloc.rs` containing only the test module for now:

```rust
//! Static execution-budget allocation for scans (str-03mfx.1).

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{BranchInfo, BranchType, DependencyKind, ExternalDependency, FunctionAnalysis, InvocationModel, LoopInfo};
    use crate::types::{ParamInfo, TypeInfo};
    use proptest::prelude::*;

    fn int() -> TypeInfo {
        TypeInfo::Int { int_width: None, int_signed: None }
    }

    fn analysis(branches: Vec<BranchInfo>, params: Vec<TypeInfo>, deps: Vec<ExternalDependency>, loops: usize, lines: u32) -> FunctionAnalysis {
        FunctionAnalysis {
            name: "f".into(),
            exported: true,
            params: params.into_iter().enumerate().map(|(i, typ)| ParamInfo { name: format!("p{i}"), typ, type_name: None }).collect(),
            branches,
            dependencies: deps,
            return_type: int(),
            start_line: 1,
            end_line: lines,
            literals: vec![],
            crypto_boundaries: vec![],
            loops: (0..loops).map(|i| LoopInfo { loop_id: i as u32, line: 2 + i as u32, induction_var: Default::default() }).collect(),
            source_file: None,
            adapter_hints: vec![],
            invocation_model: InvocationModel::Direct,
        }
    }

    fn branch(id: u32, opaque: bool) -> BranchInfo {
        BranchInfo { id, line: id + 1, condition_text: format!("c{id}"), condition: if opaque { None } else { Some(crate::sym_expr::SymExpr::Bool(true)) }, branch_type: BranchType::If }
    }

    fn dep(kind: DependencyKind) -> ExternalDependency {
        ExternalDependency { kind, symbol: "s".into(), source_module: "m".into(), return_type: int(), param_types: vec![], call_sites: vec![] }
    }

    #[test]
    fn features_counts_every_dimension() {
        let a = analysis(
            vec![branch(0, false), branch(1, true), branch(2, true)],
            vec![int(), TypeInfo::Str, TypeInfo::Array { element: Box::new(TypeInfo::Object { fields: vec![("k".into(), TypeInfo::Str)] }) }, TypeInfo::Nullable { inner: Box::new(int()) }],
            vec![dep(DependencyKind::FunctionCall), dep(DependencyKind::UnmockedImport)],
            2,
            41,
        );
        let f = features(&a);
        assert_eq!(f.branches, 3);
        assert_eq!(f.opaque_branches, 2);
        assert_eq!(f.loops, 2);
        assert_eq!(f.deps, 2);
        assert_eq!(f.unmocked_deps, 1);
        assert_eq!(f.complex_params, 2, "Str and Array; Nullable<Int> is not complex");
        assert_eq!(f.param_nesting, 2, "Array -> Object");
        assert_eq!(f.lines, 41);
    }

    #[test]
    fn score_is_at_least_one_and_monotone_in_each_feature() {
        let base = BudgetFeatures { branches: 0, opaque_branches: 0, loops: 0, deps: 0, unmocked_deps: 0, complex_params: 0, param_nesting: 0, lines: 0 };
        assert!((score(&base) - 1.0).abs() < 1e-9);
        let bumps: Vec<Box<dyn Fn(&mut BudgetFeatures)>> = vec![
            Box::new(|f| f.branches += 1), Box::new(|f| f.opaque_branches += 1), Box::new(|f| f.loops += 1), Box::new(|f| f.deps += 1),
            Box::new(|f| f.unmocked_deps += 1), Box::new(|f| f.complex_params += 1), Box::new(|f| f.param_nesting += 1), Box::new(|f| f.lines += 20),
        ];
        for bump in bumps {
            let mut f = base.clone();
            bump(&mut f);
            assert!(score(&f) > score(&base), "{f:?}");
        }
    }

    fn d(score: f64, floor: u32, ceiling: u32) -> Demand {
        Demand { score, floor, ceiling }
    }

    #[test]
    fn allocate_uniform_for_equal_scores() {
        let a = allocate(&[d(1.0, 10, 1000), d(1.0, 10, 1000), d(1.0, 10, 1000)], 300);
        assert_eq!(a.per_function, vec![100, 100, 100]);
        assert_eq!(a.total, 300);
        assert!(a.infeasible.is_none());
    }

    #[test]
    fn allocate_proportional_and_conserving() {
        let a = allocate(&[d(1.0, 0, 1000), d(3.0, 0, 1000)], 100);
        assert_eq!(a.per_function, vec![25, 75]);
    }

    #[test]
    fn allocate_respects_ceiling_and_redistributes() {
        // cross-check finding 2: scores [1, 99], total 100, floor 20, ceiling 100 must not exceed 100.
        let a = allocate(&[d(1.0, 20, 100), d(99.0, 20, 100)], 100);
        assert_eq!(a.per_function.iter().sum::<u32>(), 100);
        assert_eq!(a.per_function, vec![20, 80]);
        let b = allocate(&[d(1.0, 0, 30), d(9.0, 0, 30), d(1.0, 0, 1000)], 100);
        assert_eq!(b.per_function.iter().sum::<u32>(), 100);
        assert_eq!(b.per_function[1], 30, "capped at ceiling");
        assert!(b.per_function[2] > b.per_function[0], "residue goes to the open function with the higher share");
    }

    #[test]
    fn allocate_scales_floors_when_infeasible() {
        let a = allocate(&[d(1.0, 60, 100), d(1.0, 60, 100)], 100);
        assert_eq!(a.per_function.iter().sum::<u32>(), 100);
        assert!(matches!(a.infeasible, Some(Infeasible::FloorsScaled { requested: 120, total: 100 })));
    }

    #[test]
    fn allocate_reports_binding_ceilings() {
        let a = allocate(&[d(1.0, 0, 10), d(1.0, 0, 10)], 100);
        assert_eq!(a.per_function, vec![10, 10]);
        assert_eq!(a.total, 20);
        assert!(matches!(a.infeasible, Some(Infeasible::CeilingsBind { dropped: 80 })));
    }

    #[test]
    fn allocate_degenerate_inputs_do_not_panic() {
        assert!(allocate(&[], 100).per_function.is_empty());
        assert_eq!(allocate(&[d(1.0, 5, 3)], 100).per_function, vec![5], "ceiling below floor is raised to floor");
        assert_eq!(allocate(&[d(1.0, 0, 10)], 0).per_function, vec![0]);
        assert_eq!(allocate(&[d(0.0, 0, 10), d(0.0, 0, 10)], 10).per_function.iter().sum::<u32>(), 10, "zero scores split evenly");
    }

    #[test]
    fn allocate_remainder_goes_by_descending_score_then_index() {
        let a = allocate(&[d(1.0, 0, 100), d(2.0, 0, 100), d(2.0, 0, 100)], 7);
        // proportional floors: 1.4, 2.8, 2.8 -> 1, 2, 2 = 5; remainder 2 -> index 1 then index 2 (score ties by index)
        assert_eq!(a.per_function, vec![1, 3, 3]);
    }

    proptest! {
        #[test]
        fn allocate_conserves_total_when_feasible(
            demands in prop::collection::vec((0.0f64..100.0, 0u32..50, 50u32..500), 1..12),
            total_frac in 0.0f64..=1.0,
        ) {
            let ds: Vec<Demand> = demands.iter().map(|(s, f, c)| d(*s, *f, *c)).collect();
            let lo: u32 = ds.iter().map(|x| x.floor).sum();
            let hi: u32 = ds.iter().map(|x| x.ceiling).sum();
            let total = lo + ((hi - lo) as f64 * total_frac) as u32;
            let a = allocate(&ds, total);
            prop_assert_eq!(a.per_function.iter().sum::<u32>(), total);
            prop_assert_eq!(a.total, total);
            prop_assert!(a.infeasible.is_none());
            for (share, dm) in a.per_function.iter().zip(&ds) {
                prop_assert!(*share >= dm.floor && *share <= dm.ceiling, "{share} not in [{}, {}]", dm.floor, dm.ceiling);
            }
            // monotone among open functions (strictly inside their bounds)
            let open: Vec<(f64, u32)> = a.per_function.iter().zip(&ds).filter(|(s, dm)| **s > dm.floor && **s < dm.ceiling).map(|(s, dm)| (dm.score, *s)).collect();
            for (i, (si, ai)) in open.iter().enumerate() {
                for (sj, aj) in open.iter().skip(i + 1) {
                    if si > sj { prop_assert!(ai >= aj); }
                    if sj > si { prop_assert!(aj >= ai); }
                }
            }
            prop_assert_eq!(allocate(&ds, total).per_function, a.per_function, "deterministic");
        }

        #[test]
        fn allocate_never_panics(
            demands in prop::collection::vec((0.0f64..1000.0, 0u32..1000, 0u32..1000), 0..12),
            total in 0u32..100_000,
        ) {
            let ds: Vec<Demand> = demands.iter().map(|(s, f, c)| d(*s, *f, *c)).collect();
            let a = allocate(&ds, total);
            prop_assert_eq!(a.per_function.len(), ds.len());
            prop_assert!(a.total <= total);
        }
    }
}
```

Check `LoopInfo`'s fields before compiling: `grep -n "pub struct LoopInfo" -A 8 shatter-core/src/protocol.rs`. If `InductionVar` does not implement `Default`, construct it with its actual fields (`grep -n "pub struct InductionVar" -A 8 shatter-core/src/protocol.rs`) in the helper. Likewise check `SymExpr::Bool` exists (`grep -n "Bool(" shatter-core/src/sym_expr.rs`); if the variant is named differently use any constructor that yields a `SymExpr`.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p shatter-core --lib budget_alloc::` (after adding `pub mod budget_alloc;` to `lib.rs`)
Expected: compile errors: `features`, `score`, `allocate`, `Demand`, `BudgetFeatures`, `Infeasible` not found.

- [ ] **Step 3: Implement**

Above the test module in `budget_alloc.rs`:

```rust
use crate::protocol::{DependencyKind, FunctionAnalysis};
use crate::types::TypeInfo;

/// Integer weights of the static score. These are the only tunable; the
/// budget-allocation benchmark (`task bench-budget-alloc`, child D of
/// str-03mfx) is what justifies changing them.
#[derive(Debug, Clone, Copy)]
pub struct Weights {
    pub branches: u32,
    pub opaque: u32,
    pub loops: u32,
    pub deps: u32,
    pub unmocked: u32,
    pub complex_params: u32,
    pub nesting: u32,
    /// Applied per 20 source lines.
    pub lines_per_20: u32,
}

pub const WEIGHTS: Weights = Weights {
    branches: 1,
    opaque: 2,
    loops: 3,
    deps: 1,
    unmocked: 2,
    complex_params: 2,
    nesting: 1,
    lines_per_20: 1,
};

/// Static features of a function that predict how much exploration it can absorb.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BudgetFeatures {
    pub branches: u32,
    /// Branches with no symbolic condition (opaque to Z3).
    pub opaque_branches: u32,
    pub loops: u32,
    pub deps: u32,
    pub unmocked_deps: u32,
    /// Parameters typed `Str`, `Array`, `Object`, `Union`, or `Nullable` of one of those.
    pub complex_params: u32,
    /// Maximum container depth over all parameters (scalar = 0).
    pub param_nesting: u32,
    pub lines: u32,
}

fn is_complex(t: &TypeInfo) -> bool {
    match t {
        TypeInfo::Str | TypeInfo::Array { .. } | TypeInfo::Object { .. } | TypeInfo::Union { .. } => true,
        TypeInfo::Nullable { inner } => is_complex(inner),
        _ => false,
    }
}

fn nesting(t: &TypeInfo) -> u32 {
    match t {
        TypeInfo::Array { element } => 1 + nesting(element),
        TypeInfo::Nullable { inner } => 1 + nesting(inner),
        TypeInfo::Object { fields } => 1 + fields.iter().map(|(_, f)| nesting(f)).max().unwrap_or(0),
        TypeInfo::Union { variants, .. } => 1 + variants.iter().map(nesting).max().unwrap_or(0),
        _ => 0,
    }
}

pub fn features(analysis: &FunctionAnalysis) -> BudgetFeatures {
    BudgetFeatures {
        branches: analysis.branches.len() as u32,
        opaque_branches: analysis.branches.iter().filter(|b| b.condition.is_none()).count() as u32,
        loops: analysis.loops.len() as u32,
        deps: analysis.dependencies.len() as u32,
        unmocked_deps: analysis.dependencies.iter().filter(|d| matches!(d.kind, DependencyKind::UnmockedImport)).count() as u32,
        complex_params: analysis.params.iter().filter(|p| is_complex(&p.typ)).count() as u32,
        param_nesting: analysis.params.iter().map(|p| nesting(&p.typ)).max().unwrap_or(0),
        lines: analysis.end_line.saturating_sub(analysis.start_line) + 1,
    }
}

/// `1 + Σ weight × feature`; always at least 1.0 so every function keeps a
/// positive share.
pub fn score(f: &BudgetFeatures) -> f64 {
    let w = WEIGHTS;
    1.0 + f64::from(w.branches * f.branches)
        + f64::from(w.opaque * f.opaque_branches)
        + f64::from(w.loops * f.loops)
        + f64::from(w.deps * f.deps)
        + f64::from(w.unmocked * f.unmocked_deps)
        + f64::from(w.complex_params * f.complex_params)
        + f64::from(w.nesting * f.param_nesting)
        + f64::from(w.lines_per_20) * (f.lines as f64 / 20.0)
}

/// One function's demand: relative score and absolute bounds in executions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Demand {
    pub score: f64,
    pub floor: u32,
    pub ceiling: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Infeasible {
    /// Floors summed past the total and were scaled down proportionally.
    FloorsScaled { requested: u64, total: u32 },
    /// Ceilings summed below the total; `dropped` executions were not allocated.
    CeilingsBind { dropped: u32 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Allocation {
    pub per_function: Vec<u32>,
    /// Executions actually allocated (equals the requested total unless ceilings bind).
    pub total: u32,
    pub infeasible: Option<Infeasible>,
}

/// Water-filling split of `total` executions across `demands`.
///
/// 1. Floors are honoured first (scaled down proportionally if they exceed
///    `total`); ceilings below their floor are raised to the floor.
/// 2. The remainder is distributed proportionally to score among functions
///    not yet at their ceiling, rounding down; any function that would exceed
///    its ceiling is fixed there and the pass repeats.
/// 3. Rounding leftovers go one at a time to open functions in descending
///    score, ties by ascending index.
///
/// Conserves the total whenever `Σ floor ≤ total ≤ Σ ceiling`, never panics,
/// and is deterministic.
pub fn allocate(demands: &[Demand], total: u32) -> Allocation {
    let n = demands.len();
    if n == 0 {
        return Allocation { per_function: vec![], total: 0, infeasible: None };
    }
    let mut floors: Vec<u32> = demands.iter().map(|d| d.floor).collect();
    let ceilings: Vec<u32> = demands.iter().map(|d| d.ceiling.max(d.floor)).collect();
    let mut infeasible = None;

    let floor_sum: u64 = floors.iter().map(|&f| u64::from(f)).sum();
    if floor_sum > u64::from(total) {
        for f in floors.iter_mut() {
            *f = ((u64::from(*f) * u64::from(total)) / floor_sum) as u32;
        }
        infeasible = Some(Infeasible::FloorsScaled { requested: floor_sum, total });
    }
    let ceiling_sum: u64 = ceilings.iter().map(|&c| u64::from(c)).sum();
    if ceiling_sum < u64::from(total) {
        let allocated = ceiling_sum as u32;
        return Allocation { per_function: ceilings, total: allocated, infeasible: Some(Infeasible::CeilingsBind { dropped: total - allocated }) };
    }

    let mut shares = floors.clone();
    let mut open: Vec<bool> = (0..n).map(|i| shares[i] < ceilings[i]).collect();
    let mut remaining: u32 = total - shares.iter().sum::<u32>();

    loop {
        if remaining == 0 || !open.iter().any(|&o| o) {
            break;
        }
        let score_sum: f64 = (0..n).filter(|&i| open[i]).map(|i| demands[i].score.max(0.0)).sum();
        let mut closed_any = false;
        let mut handed: u32 = 0;
        for i in 0..n {
            if !open[i] {
                continue;
            }
            let want = if score_sum > 0.0 {
                (f64::from(remaining) * demands[i].score.max(0.0) / score_sum).floor() as u32
            } else {
                remaining / open.iter().filter(|&&o| o).count() as u32
            };
            let room = ceilings[i] - shares[i];
            if want >= room {
                shares[i] = ceilings[i];
                handed += room;
                open[i] = false;
                closed_any = true;
            } else {
                shares[i] += want;
                handed += want;
            }
        }
        remaining -= handed;
        if !closed_any {
            break;
        }
    }

    // Rounding leftovers: descending score, then ascending index, one unit each, cycling.
    let mut order: Vec<usize> = (0..n).filter(|&i| open[i]).collect();
    order.sort_by(|&a, &b| demands[b].score.partial_cmp(&demands[a].score).unwrap_or(std::cmp::Ordering::Equal).then(a.cmp(&b)));
    while remaining > 0 && !order.is_empty() {
        let mut progressed = false;
        for &i in &order {
            if remaining == 0 {
                break;
            }
            if shares[i] < ceilings[i] {
                shares[i] += 1;
                remaining -= 1;
                progressed = true;
            }
        }
        if !progressed {
            break;
        }
        order.retain(|&i| shares[i] < ceilings[i]);
    }

    let allocated: u32 = shares.iter().sum();
    Allocation { per_function: shares, total: allocated, infeasible }
}
```

Add `pub mod budget_alloc;` to `shatter-core/src/lib.rs`.

- [ ] **Step 4: Run tests**

Run: `cargo test -p shatter-core --lib budget_alloc::`
Expected: all unit tests and both proptests PASS. If `allocate_remainder_goes_by_descending_score_then_index` disagrees by one unit, re-derive the expected vector by hand from the algorithm above (it is the spec) and fix whichever side is wrong; do not loosen the assertion.

- [ ] **Step 5: Commit**

```bash
git add shatter-core/src/budget_alloc.rs shatter-core/src/lib.rs
git -c core.hooksPath=/dev/null commit -m "str-03mfx.1: budget_alloc module — features, score, water-filling allocate"
```

---

### Task 2: Move BudgetSurplus and ClaimPolicy into budget_alloc

**Files:**
- Modify: `shatter-core/src/scan_orchestrator.rs:52-175` (remove the two types and their impls), add `pub use crate::budget_alloc::{BudgetSurplus, ClaimPolicy};` where they were
- Modify: `shatter-core/src/budget_alloc.rs` (append the types verbatim)
- Test: the four `budget_surplus_*` tests in `scan_orchestrator.rs` (~13520-13560) stay where they are and must still pass through the re-export; add one test in `budget_alloc.rs` for `ClaimPolicy::should_claim`/`max_claimable`

**Interfaces:**
- Produces: `crate::budget_alloc::BudgetSurplus` (`new`, `donate(u32)`, `try_claim(requested: u32, min_claim: u32) -> u32`, `available() -> u32`) and `crate::budget_alloc::ClaimPolicy { min_hit_rate: f64, window: u32, max_claim_fraction: f64 }` (`should_claim(recent_new_paths: u32) -> bool`, `max_claimable(surplus_available: u32) -> u32`), unchanged behavior. `explorer.rs` keeps referring to `crate::scan_orchestrator::BudgetSurplus`/`ClaimPolicy` via the re-export.

- [ ] **Step 1: Write the failing test**

Append to the `tests` module in `budget_alloc.rs`:

```rust
    #[test]
    fn claim_policy_thresholds() {
        let p = ClaimPolicy { min_hit_rate: 0.1, window: 10, max_claim_fraction: 0.5 };
        assert!(!p.should_claim(0));
        assert!(p.should_claim(1));
        assert_eq!(p.max_claimable(50), 25);
        assert!(!ClaimPolicy { window: 0, ..p }.should_claim(5));
        let s = BudgetSurplus::new();
        s.donate(7);
        assert_eq!(s.try_claim(10, 1), 7);
        assert_eq!(s.available(), 0);
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p shatter-core --lib budget_alloc::tests::claim_policy_thresholds`
Expected: compile error, `ClaimPolicy`/`BudgetSurplus` not in scope.

- [ ] **Step 3: Move the types**

Cut lines 52–175 of `scan_orchestrator.rs` (from the `/// Shared budget surplus within a topological layer.` doc comment through the end of `impl ClaimPolicy`) and paste them into `budget_alloc.rs` above the test module, adding at the top of `budget_alloc.rs`:

```rust
use std::sync::atomic::{AtomicU32, Ordering};
```

At the cut site in `scan_orchestrator.rs` insert:

```rust
pub use crate::budget_alloc::{BudgetSurplus, ClaimPolicy};
```

Remove `AtomicU32` from `scan_orchestrator.rs`'s imports only if the compiler reports it unused (`Ordering` is likely still used elsewhere; leave whatever is still used).

- [ ] **Step 4: Run tests**

Run: `cargo test -p shatter-core --lib -- budget_alloc:: budget_surplus_ claim`
Expected: PASS, including the four existing `budget_surplus_*` tests in `scan_orchestrator.rs`. Then `cargo build -p shatter-core -p shatter-cli` to confirm `explorer.rs` and the CLI still resolve the re-exported paths.

- [ ] **Step 5: Commit**

```bash
git add shatter-core/src/budget_alloc.rs shatter-core/src/scan_orchestrator.rs
git -c core.hooksPath=/dev/null commit -m "str-03mfx.1: move BudgetSurplus/ClaimPolicy to budget_alloc (re-exported)"
```

---

### Task 3: Config knob on `defaults.exploration`

**Files:**
- Modify: `shatter-core/src/config.rs:778-810` (`ExplorationConfig`), its `Default` impl (~857), and the tests module near `exploration_config_yaml_roundtrip_all_fields` (~3835)
- Test: inline tests in `config.rs`

**Interfaces:**
- Produces:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum BudgetAllocation { #[default] Flat, Static }
// on ExplorationConfig:
pub budget_allocation: BudgetAllocation,   // default Flat
pub budget_floor: u32,                     // default 20
pub budget_ceiling_factor: f64,            // default 4.0
pub fn ExplorationConfig::validate_budget(&self) -> Result<(), ConfigError>
pub const DEFAULT_BUDGET_FLOOR: u32 = 20; pub const DEFAULT_BUDGET_CEILING_FACTOR: f64 = 4.0;
```

and a new `ConfigError::InvalidBudgetSetting { key: String, reason: String }` variant.

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `config.rs`, next to `exploration_config_absent_means_none`:

```rust
    #[test]
    fn budget_allocation_defaults_to_flat() {
        let yaml = "defaults:\n  exploration:\n    adaptive: true\n";
        let config: ShatterConfig = serde_yaml::from_str(yaml).unwrap();
        let exp = config.defaults.exploration.unwrap();
        assert_eq!(exp.budget_allocation, BudgetAllocation::Flat);
        assert_eq!(exp.budget_floor, DEFAULT_BUDGET_FLOOR);
        assert!((exp.budget_ceiling_factor - DEFAULT_BUDGET_CEILING_FACTOR).abs() < f64::EPSILON);
        assert_eq!(ExplorationConfig::default().budget_allocation, BudgetAllocation::Flat);
    }

    #[test]
    fn budget_allocation_parses_static_and_bounds() {
        let yaml = "defaults:\n  exploration:\n    budget_allocation: static\n    budget_floor: 5\n    budget_ceiling_factor: 2.5\n";
        let config: ShatterConfig = serde_yaml::from_str(yaml).unwrap();
        let exp = config.defaults.exploration.unwrap();
        assert_eq!(exp.budget_allocation, BudgetAllocation::Static);
        assert_eq!(exp.budget_floor, 5);
        assert!((exp.budget_ceiling_factor - 2.5).abs() < f64::EPSILON);
        assert!(exp.validate_budget().is_ok());
    }

    #[test]
    fn budget_allocation_reachable_via_set_override() {
        let set = parse_set_overrides(&["defaults.exploration.budget_allocation=static".to_string()]).unwrap();
        let merged = merge_configs(&[set]);
        assert_eq!(merged.defaults.exploration.unwrap().budget_allocation, BudgetAllocation::Static);
    }

    #[test]
    fn budget_allocation_rejects_bad_bounds() {
        let bad_floor = ExplorationConfig { budget_floor: 0, ..ExplorationConfig::default() };
        let err = bad_floor.validate_budget().unwrap_err().to_string();
        assert!(err.contains("budget_floor"), "{err}");
        let bad_factor = ExplorationConfig { budget_ceiling_factor: 0.5, ..ExplorationConfig::default() };
        let err = bad_factor.validate_budget().unwrap_err().to_string();
        assert!(err.contains("budget_ceiling_factor"), "{err}");
    }

    #[test]
    fn budget_allocation_unknown_value_is_an_error() {
        let yaml = "defaults:\n  exploration:\n    budget_allocation: magic\n";
        assert!(serde_yaml::from_str::<ShatterConfig>(yaml).is_err());
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p shatter-core --lib -- config::tests::budget_allocation`
Expected: compile errors for `BudgetAllocation`, `budget_floor`, `validate_budget`, `DEFAULT_BUDGET_*`.

- [ ] **Step 3: Implement**

Near the other `DEFAULT_EXPLORATION_*` constants in `config.rs` add:

```rust
/// Default minimum executions per function under static budget allocation.
pub const DEFAULT_BUDGET_FLOOR: u32 = 20;
/// Default per-function ceiling as a multiple of the flat per-function budget.
pub const DEFAULT_BUDGET_CEILING_FACTOR: f64 = 4.0;
```

Above `ExplorationConfig`:

```rust
/// How `shatter scan` splits execution budget across a layer's functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum BudgetAllocation {
    /// Every function gets the same budget (today's behavior).
    #[default]
    Flat,
    /// Split the layer's fixed total by a static score of each function's
    /// analysis (str-03mfx). Concolic path only.
    Static,
}
```

Add to `ExplorationConfig` after `strategy_weights`:

```rust
    /// Scan-level execution-budget allocation policy. Default: `flat`.
    #[serde(default)]
    pub budget_allocation: BudgetAllocation,

    /// Minimum executions any function receives under `static`. Default: 20.
    #[serde(default = "ExplorationConfig::default_budget_floor")]
    pub budget_floor: u32,

    /// Per-function ceiling under `static`, as a multiple of the flat
    /// per-function budget. Default: 4.0.
    #[serde(default = "ExplorationConfig::default_budget_ceiling_factor")]
    pub budget_ceiling_factor: f64,
```

In `impl ExplorationConfig` add:

```rust
    fn default_budget_floor() -> u32 {
        DEFAULT_BUDGET_FLOOR
    }
    fn default_budget_ceiling_factor() -> f64 {
        DEFAULT_BUDGET_CEILING_FACTOR
    }

    /// Reject budget settings that would make static allocation meaningless.
    pub fn validate_budget(&self) -> Result<(), ConfigError> {
        if self.budget_floor == 0 {
            return Err(ConfigError::InvalidBudgetSetting {
                key: "defaults.exploration.budget_floor".into(),
                reason: "must be at least 1".into(),
            });
        }
        if !(self.budget_ceiling_factor >= 1.0) {
            return Err(ConfigError::InvalidBudgetSetting {
                key: "defaults.exploration.budget_ceiling_factor".into(),
                reason: format!("must be >= 1.0, got {}", self.budget_ceiling_factor),
            });
        }
        Ok(())
    }
```

In `impl Default for ExplorationConfig` add the three fields (`budget_allocation: BudgetAllocation::Flat`, `budget_floor: Self::default_budget_floor()`, `budget_ceiling_factor: Self::default_budget_ceiling_factor()`). Add to the `ConfigError` enum (find it with `grep -n "pub enum ConfigError" shatter-core/src/config.rs`):

```rust
    #[error("invalid budget setting {key}: {reason}")]
    InvalidBudgetSetting { key: String, reason: String },
```

Fix every `ExplorationConfig { .. }` literal without `..Default::default()` that the compiler reports (there are two in `config.rs` tests near lines 3886 and 3899) by adding `..ExplorationConfig::default()`.

- [ ] **Step 4: Run tests**

Run: `cargo test -p shatter-core --lib -- config::`
Expected: PASS, including all pre-existing config tests (the serde defaults keep old YAML parsing).

- [ ] **Step 5: Commit**

```bash
git add shatter-core/src/config.rs
git -c core.hooksPath=/dev/null commit -m "str-03mfx.1: defaults.exploration.budget_allocation knob with floor/ceiling and validation"
```

---

### Task 4: Plumb the knob through the CLI into ScanConfig

**Files:**
- Modify: `shatter-core/src/scan_orchestrator.rs` (`ScanConfig` struct ~176-260; every `ScanConfig { .. }` literal the compiler reports)
- Modify: `shatter-cli/src/helpers.rs` (new `resolve_scan_exploration`)
- Modify: `shatter-cli/src/main.rs` (~676-700 where `yaml_defaults` is built for scan; the `run_scan(` call at ~851)
- Modify: `shatter-cli/src/commands/scan.rs` (`run_scan` signature ~255-270; `ScanConfig` literals at ~1034 and ~1227)
- Test: `shatter-cli/src/helpers.rs` inline tests (next to `resolve_llm_config_reads_explicit_config_path`)

**Interfaces:**
- Produces in shatter-core:

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BudgetSettings { pub allocation: BudgetAllocation, pub floor: u32, pub ceiling_factor: f64 }
impl Default for BudgetSettings  // Flat, 20, 4.0
impl From<&ExplorationConfig> for BudgetSettings
// on ScanConfig:
pub budget: BudgetSettings,
```

- Produces in shatter-cli: `pub(crate) fn resolve_scan_exploration(config_dir: &Path, set_overrides: &[String]) -> Result<ExplorationConfig, ConfigError>`; `run_scan` gains a `budget: BudgetSettings` parameter placed right after `genetic_config`.

- [ ] **Step 1: Write the failing test**

In `shatter-cli/src/helpers.rs` tests, next to `resolve_llm_config_reads_explicit_config_path`:

```rust
    #[test]
    fn resolve_scan_exploration_applies_set_override_over_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".shatter")).unwrap();
        std::fs::write(
            dir.path().join(".shatter/config.yaml"),
            "defaults:\n  exploration:\n    budget_allocation: flat\n    budget_floor: 7\n",
        )
        .unwrap();
        let exp = resolve_scan_exploration(dir.path(), &[]).unwrap();
        assert_eq!(exp.budget_allocation, shatter_core::config::BudgetAllocation::Flat);
        assert_eq!(exp.budget_floor, 7);

        let exp = resolve_scan_exploration(dir.path(), &["defaults.exploration.budget_allocation=static".to_string()]).unwrap();
        assert_eq!(exp.budget_allocation, shatter_core::config::BudgetAllocation::Static);
        assert_eq!(exp.budget_floor, 7, "file value survives when --set touches another key");

        let settings = shatter_core::scan_orchestrator::BudgetSettings::from(&exp);
        assert_eq!(settings.allocation, shatter_core::config::BudgetAllocation::Static);
        assert_eq!(settings.floor, 7);
    }

    #[test]
    fn resolve_scan_exploration_rejects_bad_bounds() {
        let dir = tempfile::tempdir().unwrap();
        let err = resolve_scan_exploration(dir.path(), &["defaults.exploration.budget_ceiling_factor=0.5".to_string()]).unwrap_err();
        assert!(err.to_string().contains("budget_ceiling_factor"), "{err}");
    }
```

Check `tempfile` is a dev-dependency of shatter-cli (`grep -n tempfile shatter-cli/Cargo.toml`); add `tempfile = "3"` under `[dev-dependencies]` if missing.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p shatter-cli resolve_scan_exploration`
Expected: compile error, function and `BudgetSettings` missing.

- [ ] **Step 3: Implement**

In `scan_orchestrator.rs`, next to `ScanConfig`:

```rust
/// Scan-level budget policy resolved from `defaults.exploration` (str-03mfx).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BudgetSettings {
    pub allocation: crate::config::BudgetAllocation,
    /// Minimum executions per function under `Static`.
    pub floor: u32,
    /// Per-function ceiling as a multiple of the flat per-function budget.
    pub ceiling_factor: f64,
}

impl Default for BudgetSettings {
    fn default() -> Self {
        Self {
            allocation: crate::config::BudgetAllocation::Flat,
            floor: crate::config::DEFAULT_BUDGET_FLOOR,
            ceiling_factor: crate::config::DEFAULT_BUDGET_CEILING_FACTOR,
        }
    }
}

impl From<&crate::config::ExplorationConfig> for BudgetSettings {
    fn from(e: &crate::config::ExplorationConfig) -> Self {
        Self { allocation: e.budget_allocation, floor: e.budget_floor, ceiling_factor: e.budget_ceiling_factor }
    }
}
```

Add to `ScanConfig` (after `genetic_config`):

```rust
    /// Execution-budget allocation policy (str-03mfx). Default: flat.
    pub budget: BudgetSettings,
```

and `budget: BudgetSettings::default(),` in every `ScanConfig { .. }` literal the compiler reports (scan_orchestrator tests, shatter-cli `scan.rs` ~1034 and ~1227, `bench.rs`, any others).

In `shatter-cli/src/helpers.rs`, below `resolve_llm_config`:

```rust
/// Resolve `defaults.exploration` for a scan: the hierarchical
/// `.shatter/config.yaml` stack at `config_dir` with `--set` overrides as the
/// highest-priority layer, validated for the budget settings.
pub(crate) fn resolve_scan_exploration(
    config_dir: &Path,
    set_overrides: &[String],
) -> Result<shatter_core::config::ExplorationConfig, shatter_core::config::ConfigError> {
    let mut configs = shatter_core::config::discover_configs(config_dir)?;
    if !set_overrides.is_empty() {
        configs.insert(0, shatter_core::config::parse_set_overrides(set_overrides)?);
    }
    let exploration = shatter_core::config::merge_configs(&configs)
        .defaults
        .exploration
        .unwrap_or_default();
    exploration.validate_budget()?;
    Ok(exploration)
}
```

In `main.rs`, right after `yaml_defaults`/`genetic_config` for the scan command (~676-692), add:

```rust
            let scan_exploration =
                crate::helpers::resolve_scan_exploration(directory_for_resolution, &cli.set_overrides)?;
            let budget = shatter_core::scan_orchestrator::BudgetSettings::from(&scan_exploration);
```

(`directory_for_resolution` is the same path `yaml_defaults` uses; if `?` does not convert `ConfigError` into the arm's error type, follow whatever the neighbouring code does, e.g. `.map_err(|e| e.to_string())?`.) Pass `budget` to `run_scan(` right after `&genetic_config`. In `scan.rs`, add `budget: shatter_core::scan_orchestrator::BudgetSettings,` to `run_scan`'s parameters after `genetic_config`, and set `budget,` in both `ScanConfig` literals.

- [ ] **Step 4: Run tests**

Run: `cargo test -p shatter-cli resolve_scan_exploration && cargo test -p shatter-core --lib scan_orchestrator:: && cargo build -p shatter-cli`
Expected: PASS / builds.

- [ ] **Step 5: Commit**

```bash
git add shatter-core/src/scan_orchestrator.rs shatter-cli/src/helpers.rs shatter-cli/src/main.rs shatter-cli/src/commands/scan.rs shatter-cli/Cargo.toml Cargo.lock
git -c core.hooksPath=/dev/null commit -m "str-03mfx.1: resolve defaults.exploration budget settings into ScanConfig"
```

(Add `shatter-cli/src/commands/bench.rs` or other files to the `git add` if the compiler made you touch their `ScanConfig` literals.)

---

### Task 5: Per-layer allocation in the scan orchestrator

**Files:**
- Modify: `shatter-core/src/explorer.rs:101-180` (scan-level `ExploreConfig`: add `max_executions_override: Option<usize>`)
- Modify: `shatter-core/src/scan_orchestrator.rs`: concolic config derivation (~3129-3160), layer loop after tasks are built (just before the `expanded_tasks` block ~4660), replica expansion (~4665-4685), plus a new pure `apply_static_allocation`
- Test: `scan_orchestrator.rs` inline tests

**Interfaces:**
- Consumes: `budget_alloc::{features, score, allocate, Demand}`, `BudgetSettings`, `concolic_scan_max_executions(max_iterations, has_custom_generators)`.
- Produces:

```rust
// explorer::ExploreConfig
pub max_executions_override: Option<usize>,   // None = derive as today
// scan_orchestrator
pub(crate) struct StaticShare { pub func_name: String, pub score: f64, pub executions: u32 }
pub(crate) fn apply_static_allocation(tasks: &mut [ExploreTask], settings: &BudgetSettings, default_max_iterations: u32) -> Vec<StaticShare>
```

- [ ] **Step 1: Write the failing tests**

In `scan_orchestrator.rs`'s tests module (near the `budget_surplus_*` tests), using the `make_analysis` pattern from `batch_analyze.rs`:

```rust
    fn alloc_task(name: &str, branch_count: usize, loops: usize, custom_generators: bool) -> ExploreTask {
        use crate::protocol::{BranchInfo, BranchType, FunctionAnalysis, InvocationModel, LoopInfo};
        use crate::types::{ParamInfo, TypeInfo};
        let analysis = FunctionAnalysis {
            name: name.to_string(),
            exported: true,
            params: vec![ParamInfo { name: "x".into(), typ: TypeInfo::Int { int_width: None, int_signed: None }, type_name: None }],
            branches: (0..branch_count).map(|i| BranchInfo { id: i as u32, line: i as u32 + 1, condition_text: String::new(), condition: None, branch_type: BranchType::If }).collect(),
            dependencies: vec![],
            return_type: TypeInfo::Int { int_width: None, int_signed: None },
            start_line: 1,
            end_line: 10,
            literals: vec![],
            crypto_boundaries: vec![],
            loops: (0..loops).map(|i| LoopInfo { loop_id: i as u32, line: 5, induction_var: Default::default() }).collect(),
            source_file: None,
            adapter_hints: vec![],
            invocation_model: InvocationModel::Direct,
        };
        let mut explore_config = ExploreConfig { max_iterations: Some(100), ..test_explore_config() };
        if custom_generators {
            explore_config.value_sources.push(ValueSource::CustomGenerator { type_name: "T".into(), generator_path: std::path::PathBuf::from("g") });
        }
        ExploreTask {
            func_name: name.to_string(),
            analysis,
            explore_config,
            file_path: "f.ts".into(),
            mocks_used: vec![],
            callees: Default::default(),
            deep_fp: None,
            progress_index: 0,
            known_targets: Default::default(),
        }
    }

    #[test]
    fn static_allocation_conserves_layer_total_and_orders_by_score() {
        let settings = BudgetSettings { allocation: crate::config::BudgetAllocation::Static, floor: 20, ceiling_factor: 4.0 };
        let mut tasks = vec![alloc_task("trivial", 1, 0, false), alloc_task("loopy", 8, 3, false)];
        let shares = apply_static_allocation(&mut tasks, &settings, 100);
        let total: u32 = shares.iter().map(|s| s.executions).sum();
        assert_eq!(total, 1000, "2 functions × 500 default executions");
        assert!(shares[0].executions < shares[1].executions);
        assert_eq!(tasks[0].explore_config.max_executions_override, Some(shares[0].executions as usize));
        assert_eq!(tasks[0].explore_config.max_iterations, Some((shares[0].executions / 5).max(1)));
        assert_eq!(tasks[1].explore_config.max_iterations, Some((shares[1].executions / 5).max(1)));
        for s in &shares {
            assert!(s.executions >= 20 && s.executions <= 2000);
        }
    }

    #[test]
    fn static_allocation_custom_generator_uses_one_to_one_ratio() {
        let settings = BudgetSettings { allocation: crate::config::BudgetAllocation::Static, floor: 20, ceiling_factor: 4.0 };
        let mut tasks = vec![alloc_task("gen", 3, 0, true), alloc_task("plain", 3, 0, false)];
        let shares = apply_static_allocation(&mut tasks, &settings, 100);
        assert_eq!(shares.iter().map(|s| s.executions).sum::<u32>(), 600, "100 + 500");
        assert_eq!(tasks[0].explore_config.max_iterations, Some(shares[0].executions));
        assert!(shares[0].executions <= 400, "ceiling 4 × 100");
    }

    #[test]
    fn flat_allocation_leaves_tasks_untouched() {
        let mut tasks = vec![alloc_task("a", 1, 0, false), alloc_task("b", 9, 2, false)];
        let before: Vec<_> = tasks.iter().map(|t| (t.explore_config.max_iterations, t.explore_config.max_executions_override)).collect();
        let shares = apply_static_allocation(&mut tasks, &BudgetSettings::default(), 100);
        assert!(shares.is_empty());
        let after: Vec<_> = tasks.iter().map(|t| (t.explore_config.max_iterations, t.explore_config.max_executions_override)).collect();
        assert_eq!(before, after);
        assert!(after.iter().all(|(_, o)| o.is_none()));
    }

    #[test]
    fn concolic_scan_max_executions_honours_override() {
        assert_eq!(effective_concolic_max_executions(100, false, None), 500);
        assert_eq!(effective_concolic_max_executions(100, true, None), 100);
        assert_eq!(effective_concolic_max_executions(100, false, Some(37)), 37);
    }

    #[test]
    fn replica_split_divides_execution_override() {
        let mut task = alloc_task("r", 4, 0, false);
        task.explore_config.max_iterations = Some(10);
        task.explore_config.max_executions_override = Some(53);
        let replicas = split_task_across_replicas(task, 0, 4);
        assert_eq!(replicas.len(), 4);
        let iters: Vec<u32> = replicas.iter().map(|r| r.explore_config.max_iterations.unwrap()).collect();
        assert_eq!(iters, vec![2, 2, 2, 2], "max_iterations split as today (floor, min 1)");
        let execs: Vec<usize> = replicas.iter().map(|r| r.explore_config.max_executions_override.unwrap()).collect();
        assert_eq!(execs.iter().sum::<usize>(), 53, "execution shares sum to the function's share");
        assert_eq!(execs, vec![14, 13, 13, 13], "remainder to the first replica");
    }
```

`test_explore_config()` refers to whatever helper the tests module already uses to build a scan-level `ExploreConfig` (find with `grep -n "fn test_explore_config\|fn default_explore_config\|ExploreConfig {" shatter-core/src/scan_orchestrator.rs | head`); if there is none, write one that fills every field with defaults and `budget_surplus: None`. Check `ValueSource::CustomGenerator`'s fields with `grep -n "CustomGenerator" shatter-core/src/config.rs | head -3` and use the real field names. `LoopInfo`/`InductionVar` construction: same note as Task 1.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p shatter-core --lib -- scan_orchestrator::tests::static_allocation scan_orchestrator::tests::flat_allocation scan_orchestrator::tests::concolic_scan_max scan_orchestrator::tests::replica_split`
Expected: compile errors for `max_executions_override`, `apply_static_allocation`, `effective_concolic_max_executions`, `split_task_across_replicas`.

- [ ] **Step 3: Implement**

In `explorer.rs`, add to the scan-level `ExploreConfig` (near `max_iterations`):

```rust
    /// Concolic-path execution cap set by static budget allocation
    /// (str-03mfx). `None` derives the cap from `max_iterations` as before.
    /// The random explorer ignores this field.
    pub max_executions_override: Option<usize>,
```

and `max_executions_override: None` in every literal the compiler reports (explorer.rs tests ~5017, ~5261; scan_orchestrator; shatter-cli `explore.rs`, `observe.rs`, `run.rs`, `properties.rs`, `bench.rs` where they build this type).

In `scan_orchestrator.rs`, replace the call at ~3134 and add the helper next to `concolic_scan_max_executions`:

```rust
/// Execution cap for the concolic scan path: the static-allocation override
/// when present, else the flat derivation.
fn effective_concolic_max_executions(max_iterations: usize, has_custom_generators: bool, override_executions: Option<usize>) -> usize {
    override_executions.unwrap_or_else(|| concolic_scan_max_executions(max_iterations, has_custom_generators))
}
```

```rust
    let max_executions = effective_concolic_max_executions(
        max_iterations,
        has_custom_generators,
        explore_config.max_executions_override,
    );
```

Add the allocation helper near `ExploreTask`:

```rust
/// One function's static share, for logging and tests.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StaticShare {
    pub func_name: String,
    pub score: f64,
    pub executions: u32,
}

fn task_has_custom_generators(task: &ExploreTask) -> bool {
    task.explore_config
        .value_sources
        .iter()
        .any(|s| matches!(s, ValueSource::CustomGenerator { .. }))
}

/// Under `Static`, split the layer's flat execution total across `tasks` by
/// static score and write each share into the task's config
/// (`max_executions_override` and a derived `max_iterations`). Under `Flat`
/// this is a no-op and returns an empty vec.
pub(crate) fn apply_static_allocation(
    tasks: &mut [ExploreTask],
    settings: &BudgetSettings,
    default_max_iterations: u32,
) -> Vec<StaticShare> {
    use crate::budget_alloc::{Demand, allocate, features, score};
    use crate::config::BudgetAllocation;
    if settings.allocation != BudgetAllocation::Static || tasks.is_empty() {
        return Vec::new();
    }
    let flat: Vec<(u32, bool)> = tasks
        .iter()
        .map(|t| {
            let custom = task_has_custom_generators(t);
            (concolic_scan_max_executions(default_max_iterations as usize, custom) as u32, custom)
        })
        .collect();
    let total: u32 = flat.iter().map(|(f, _)| *f).sum();
    let scores: Vec<f64> = tasks.iter().map(|t| score(&features(&t.analysis))).collect();
    let demands: Vec<Demand> = flat
        .iter()
        .zip(&scores)
        .map(|((flat_execs, _), s)| Demand {
            score: *s,
            floor: settings.floor,
            ceiling: ((*flat_execs as f64) * settings.ceiling_factor).floor().max(1.0) as u32,
        })
        .collect();
    let allocation = allocate(&demands, total);
    if let Some(inf) = allocation.infeasible {
        log::warn!("budget: layer allocation infeasible: {inf:?}");
    }
    let mut shares = Vec::with_capacity(tasks.len());
    for ((task, share), ((_, custom), s)) in tasks.iter_mut().zip(&allocation.per_function).zip(flat.iter().zip(&scores)) {
        let execs = *share;
        task.explore_config.max_executions_override = Some(execs as usize);
        task.explore_config.max_iterations = Some(if *custom { execs } else { (execs / 5).max(1) });
        log::debug!("budget: {} score={:.1} executions={}", task.func_name, s, execs);
        shares.push(StaticShare { func_name: task.func_name.clone(), score: *s, executions: execs });
    }
    let mut sorted: Vec<u32> = shares.iter().map(|s| s.executions).collect();
    sorted.sort_unstable();
    log::info!(
        "budget: layer: {} functions, total {}, share min/median/max {}/{}/{}",
        shares.len(),
        allocation.total,
        sorted[0],
        sorted[sorted.len() / 2],
        sorted[sorted.len() - 1]
    );
    shares
}
```

Call it in the layer loop after `tasks` is complete and before the `expanded_tasks` block:

```rust
            let _shares = apply_static_allocation(&mut tasks, &config.budget, config.max_iterations_per_function);
```

(`tasks` must be `let mut`; the serial/pooled branch at ~4640 also consumes `tasks`, so place the call before the `if` that chooses between them.)

Extract the replica expansion body into a function and split the override too:

```rust
/// Expand one task into `wpf` replicas with derived seeds. `max_iterations`
/// is split evenly (floor, min 1) as before; a static-allocation
/// `max_executions_override` is split evenly with the remainder on the first
/// replica so the replicas' executions sum to the function's share.
fn split_task_across_replicas(task: ExploreTask, fn_idx: usize, wpf: usize) -> Vec<ExploreTask> {
    let per_replica_iters = task.explore_config.max_iterations.map(|m| (m / wpf as u32).max(1));
    let exec_shares: Option<Vec<usize>> = task.explore_config.max_executions_override.map(|total| {
        let base = total / wpf;
        let rem = total % wpf;
        (0..wpf).map(|r| base + usize::from(r < rem)).collect()
    });
    let mut out = Vec::with_capacity(wpf);
    for replica in 0..wpf {
        let mut replica_config = task.explore_config.clone();
        replica_config.seed = derive_replica_seed(task.explore_config.seed, fn_idx, replica);
        replica_config.max_iterations = per_replica_iters;
        replica_config.max_executions_override = exec_shares.as_ref().map(|v| v[replica]);
        out.push(ExploreTask {
            func_name: task.func_name.clone(),
            analysis: task.analysis.clone(),
            explore_config: replica_config,
            file_path: task.file_path.clone(),
            mocks_used: task.mocks_used.clone(),
            callees: task.callees.clone(),
            deep_fp: task.deep_fp.clone(),
            progress_index: task.progress_index,
            known_targets: task.known_targets.clone(),
        });
    }
    out
}
```

and replace the inline loop at ~4665-4685 with `out.extend(split_task_across_replicas(task, fn_idx, wpf));`. Leave the batch path (`batch_explore_config.max_iterations = Some(batch_config.batch_size)`) as is: the override is cloned along with the config and still caps executions for the batch.

- [ ] **Step 4: Run tests**

Run: `cargo test -p shatter-core --lib -- scan_orchestrator:: explorer::` then `cargo build -p shatter-cli`
Expected: PASS; all pre-existing scan and explorer tests unchanged.

- [ ] **Step 5: Commit**

```bash
git add shatter-core/src/explorer.rs shatter-core/src/scan_orchestrator.rs shatter-cli/src
git -c core.hooksPath=/dev/null commit -m "str-03mfx.1: per-layer static allocation of concolic execution budget; replica split of the override"
```

---

### Task 6: Fixture-ordering test on real analyses and flat-parity check

**Files:**
- Modify: `shatter-core/tests/e2e_concolic.rs` (append one test; this file is run by `task e2e-ts` with `--include-ignored`)
- Test: the new test itself

**Interfaces:**
- Consumes: `shatter_core::budget_alloc::{features, score}`; the file's existing helpers `spawn_ts_frontend`, `analyze_function` (check their exact names at the top of `e2e_concolic.rs`; use the ones present).

- [ ] **Step 1: Write the test**

Append to `shatter-core/tests/e2e_concolic.rs`:

```rust
/// str-03mfx.1: the static budget score must rank a trivial fixture below a
/// parser-shaped one on real TS analyses. Pins the ordering child C's e2e
/// test relies on; if it fails, adjust `budget_alloc::WEIGHTS` here, not in C.
#[tokio::test]
#[ignore = "subprocess E2E; run via task e2e-ts or core:test-ignored"]
async fn budget_score_ranks_classify_number_below_parse_cron() {
    use shatter_core::budget_alloc::{features, score};
    let dir = examples_dir();
    let mut frontend = spawn_ts_frontend().await;
    let simple = analyze_function(&mut frontend, &dir.join("01-arithmetic.ts").to_string_lossy(), "classifyNumber").await;
    let complex = analyze_function(&mut frontend, &dir.join("16-cron-parser.ts").to_string_lossy(), "parseCron").await;
    let (fs, fc) = (features(&simple), features(&complex));
    assert!(
        score(&fs) < score(&fc),
        "classifyNumber {fs:?} scored {:.1}, parseCron {fc:?} scored {:.1}",
        score(&fs),
        score(&fc)
    );
}
```

Match helper names to the file: `grep -n "^async fn \|^fn examples_dir" shatter-core/tests/e2e_concolic.rs`.

- [ ] **Step 2: Run it**

Run: `cd shatter-ts && npm run build && cd .. && python3 scripts/examples_checkout.py && SHATTER_ALLOW_HOST_WRITES=1 cargo test -p shatter-core --test e2e_concolic budget_score -- --include-ignored`
Expected: PASS. If it fails, print both feature structs (they are in the assertion message), decide which weight is wrong by comparing the two structs against the spec's intent (opaque branches, loops and string params are what make a parser expensive), change `WEIGHTS` in `budget_alloc.rs`, rerun Task 1's tests, and rerun this test.

- [ ] **Step 3: Flat parity check**

Run the full TS e2e suite and the scan-report snapshot tests, which exercise scans under the default `flat`:

```bash
SHATTER_ALLOW_HOST_WRITES=1 cargo test -p shatter-core --test e2e_concolic -- --include-ignored
cargo test -p shatter-core --test html_snapshots --test outcome_md_snapshots --test source_set_summary_snapshots
```

Expected: all PASS with no snapshot updates needed (proof that `flat` output is unchanged).

- [ ] **Step 4: Commit**

```bash
git add shatter-core/tests/e2e_concolic.rs shatter-core/src/budget_alloc.rs
git -c core.hooksPath=/dev/null commit -m "str-03mfx.1: pin budget score ordering on real TS analyses"
```

---

### Task 7: Document the knob and run the gates

**Files:**
- Modify: whichever doc describes the `defaults.exploration` block (find it: `grep -rln "score_window\|strategy_floor" README.md docs/ | head`); add the three keys with one line each and the sentence "`static` applies to the concolic scan path only and is off by default; see `docs/superpowers/specs/2026-09-23-static-budget-allocation-design.md`."
- Test: gates

- [ ] **Step 1: Add the doc lines** (in the file found above, next to `strategy_weights`):

```yaml
    budget_allocation: flat      # flat | static — split each scan layer's execution total by a static score of each function (str-03mfx); concolic path only
    budget_floor: 20             # static only: minimum executions per function
    budget_ceiling_factor: 4.0   # static only: per-function ceiling as a multiple of the flat budget
```

- [ ] **Step 2: Run the gates**

```bash
cargo clippy -p shatter-core -p shatter-cli --all-targets -- -D warnings
SHATTER_ALLOW_HOST_WRITES=1 task affected
SHATTER_ALLOW_HOST_WRITES=1 task e2e-go
```

Expected: clippy clean; `task affected` exit 0 (record its `Gates selected` list); `task e2e-go` green (Go scan path must be unaffected under `flat`).

- [ ] **Step 3: Commit and push**

```bash
git add -A docs README.md
git -c core.hooksPath=/dev/null commit -m "str-03mfx.1: document budget_allocation keys"
git -c core.hooksPath=/dev/null push -u origin str-03mfx.1-budget-alloc
```

Then invoke `bento:land-work` for str-03mfx.1 (prepare, independent code review of `merge-base..HEAD`, `land.py`, close with gate evidence, tear down the worktree).

---

## Self-review

**Spec coverage (child A rows).** §1 allocator with water-filling and invariants: Task 1. Module move with re-export: Task 2. §2 knob on `defaults.exploration`, validation, `--set` reachability test: Tasks 3–4. §3 allocation after the runnable set is known, `max_iterations = max(1, share/5)`, 1:1 for custom generators, debug/info logging, infeasible warning, replica split: Task 5. §7 real-analysis ordering test: Task 6. Compatibility contract (flat unchanged): Task 5's `flat_allocation_leaves_tasks_untouched` plus Task 6 Step 3 snapshots. Not in this child by design: donation fix, claiming, report fields, benchmark, e2e allocation test (children B–E).

**Placeholders.** None. Where a helper's exact name is unknown (`test_explore_config`, `analyze_function`, `LoopInfo` fields), the step gives the grep to find it and what to do in each case.

**Type consistency.** `BudgetSettings { allocation, floor, ceiling_factor }` defined in Task 4 and consumed in Task 5. `apply_static_allocation(&mut [ExploreTask], &BudgetSettings, u32) -> Vec<StaticShare>` used identically in tests and wiring. `max_executions_override: Option<usize>` on the scan-level `ExploreConfig` (Task 5) is read by `effective_concolic_max_executions(usize, bool, Option<usize>)`. `Demand { score, floor, ceiling }`, `allocate(&[Demand], u32) -> Allocation { per_function, total, infeasible }` match between Task 1 and Task 5.
